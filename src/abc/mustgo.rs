//! Pure post-processing stages of the `mustgo` report (spec 5.1 rows 10-22, verified in 9.3).
//!
//! Every stage works on [`Table`]s with sanitised headers; columns are always matched by
//! name. No SAP access and no raw-export parsing happens here: the SAP steps (later) hand the
//! tables in, and these functions do the selecting, de-duplicating and computing between them.
#![allow(dead_code)]

use super::ops::{self, FilterOpts, LookupSpec, Which};
use super::registry::{self, MUSTGO};
use super::run::RunDate;
use super::table::{ColPos, Table};
use crate::utils::export_outcome::paste_value_list;
use anyhow::{anyhow, Context, Result};
use chrono::NaiveDate;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Row 10: `Sel(vt11, transportplanningpt = TPDC)` (non-destructive) and the distinct
/// shipment numbers of the selected rows, ready to paste into VL06O. Errors when no row matches.
pub fn select_tpdc_shipments(vt11: &mut Table) -> Result<Vec<String>> {
    ops::filter(vt11, "transportplanningpt", "TPDC", FilterOpts::default())?;
    let list = paste_value_list(&vt11.visible_column("shipment_number")?, false);
    if list.is_empty() {
        return Err(anyhow!(
            "vt11: no shipments with transportplanningpt = TPDC (no data for mustgo)"
        ));
    }
    Ok(list)
}

/// Row 12 input: distinct deliveries of `vl06o` (visible rows), ready to paste into ZMDESNR.
pub fn delivery_list(vl06o: &Table) -> Result<Vec<String>> {
    let list = paste_value_list(&vl06o.visible_column("delivery")?, false);
    if list.is_empty() {
        return Err(anyhow!("vl06o: no deliveries"));
    }
    Ok(list)
}

/// Rows 13-15: select `parent_serial_number` non-blank AND `status <> PAL`, then dedup by
/// parent among the selected rows (first kept). Hidden rows stay in the table, hidden.
pub fn filter_sn_del(sn_del: &mut Table) -> Result<()> {
    ops::filter(sn_del, "parent_serial_number", "<>", FilterOpts::default())?;
    ops::filter(
        sn_del,
        "status",
        "<>PAL",
        FilterOpts {
            keep_prev: true,
            delete: false,
        },
    )?;
    ops::dedup(sn_del, &["parent_serial_number"], true)
}

/// Distinct transfer orders of the visible rows (row 16 input for LT23, row 19 input).
pub fn transfer_order_list(t: &Table) -> Result<Vec<String>> {
    Ok(paste_value_list(
        &t.visible_column("transfer_order_number")?,
        false,
    ))
}

/// Row 16 post-process `mustgo` on the LT23 export, steps (a)-(f). The `datetime` column
/// goes after `creation_time`, `bdup` becomes the first column.
pub fn lt23_postprocess(lt23: &mut Table) -> Result<()> {
    lt23.reset_selection();
    // (a)
    ops::dedup(lt23, &["transfer_order_number"], false)?;
    // (b)
    ops::morph_datetime(
        lt23,
        "creation_date",
        "creation_time",
        "datetime",
        &ColPos::After("creation_time".to_string()),
    )?;
    // (c)
    ops::sort_by(
        lt23,
        &[
            ("delivery", true),
            ("datetime", true),
            ("source_storage_bin", true),
        ],
    )?;
    // (d)
    ops::count_same(lt23, "delivery", "bdup", &ColPos::First, "unique")?;
    // (e)
    ops::filter(
        lt23,
        "bdup",
        "<>unique",
        FilterOpts {
            keep_prev: false,
            delete: true,
        },
    )?;
    // (f)
    ops::remove_one(lt23, "delivery", Which::Last)
}

/// Rows 17-18: add `count` (`1` when the row's TO is in `lt23`, else `x`) to `sn_del` and
/// delete the `x` rows.
pub fn mark_sn_del_by_lt23(sn_del: &mut Table, lt23: &Table) -> Result<()> {
    ops::lookup(
        sn_del,
        lt23,
        &LookupSpec {
            src_key: "transfer_order_number",
            src_val: "transfer_order_number",
            dst_col: "transfer_order_number",
            new_col: "count",
            pos: ColPos::Last,
            not_found: "x",
            found_value: Some("1"),
            skip_hidden: false,
        },
    )?;
    ops::filter(
        sn_del,
        "count",
        "<>x",
        FilterOpts {
            keep_prev: false,
            delete: true,
        },
    )
}

/// Excel `IFNA(WEEKNUM(created_on),"-")`: `-` when the text is not a date.
pub fn week_of(created_on: &str) -> String {
    let first = created_on.split_whitespace().next().unwrap_or("");
    match NaiveDate::parse_from_str(first, "%Y-%m-%d") {
        Ok(d) => registry::excel_weeknum(d).to_string(),
        Err(_) => "-".to_string(),
    }
}

/// Rows 19(b)-21: copy of `sn_to`, dedup by `parent_serial_number` among non-blank parents
/// (blank-parent rows stay in place), `week` at position 19, `export_date` last.
pub fn snr_copy(sn_to: &Table, run_date: NaiveDate) -> Result<Table> {
    let mut t = ops::copy_table(sn_to, "snr_copy");
    ops::filter(&mut t, "parent_serial_number", "<>", FilterOpts::default())?;
    ops::dedup(&mut t, &["parent_serial_number"], true)?;
    t.reset_selection();
    let c = t.col("created_on")?;
    let weeks = t.rows.iter().map(|r| week_of(&r[c])).collect();
    t.insert_column("week", &ColPos::Index(19), weeks)?;
    let n = t.nrows();
    t.insert_column(
        "export_date",
        &ColPos::Last,
        vec![run_date.format("%Y-%m-%d").to_string(); n],
    )?;
    Ok(t)
}

/// Row 22 pivot as a long table: `plant`, `export_date`, `count` (non-blank
/// `lm_serial_number`), sorted by plant then date. A blank plant is labelled `(blank)`.
pub fn pivot_plant_by_date(snr_copy: &Table) -> Result<Table> {
    let (p, d, s) = (
        snr_copy.col("plant")?,
        snr_copy.col("export_date")?,
        snr_copy.col("lm_serial_number")?,
    );
    let mut counts: BTreeMap<(String, String, String), usize> = BTreeMap::new();
    for r in &snr_copy.rows {
        if r[s].trim().is_empty() {
            continue;
        }
        let plant = if r[p].trim().is_empty() {
            "(blank)".to_string()
        } else {
            r[p].trim().to_string()
        };
        let key = (plant.to_lowercase(), r[d].trim().to_string(), plant);
        *counts.entry(key).or_default() += 1;
    }
    let rows = counts
        .into_iter()
        .map(|((_, date, plant), n)| vec![plant, date, n.to_string()])
        .collect();
    Ok(Table::new(
        "pivot",
        vec!["plant".into(), "export_date".into(), "count".into()],
        rows,
    ))
}

/// Write the three output tables as TSV under the registry's `output_path`s. Returns the paths
/// in the order `lt23`, `snr_copy`, `pivot`.
pub fn write_outputs(
    reports_dir: &Path,
    today: NaiveDate,
    run_date: &RunDate,
    lt23: &Table,
    snr_copy: &Table,
    pivot: &Table,
) -> Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    for (name, t) in [("lt23", lt23), ("snr_copy", snr_copy), ("pivot", pivot)] {
        let path = registry::output_path(reports_dir, &MUSTGO, today, run_date, name);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).with_context(|| format!("create {}", dir.display()))?;
        }
        std::fs::write(&path, t.to_tsv()).with_context(|| format!("write {}", path.display()))?;
        paths.push(path);
    }
    Ok(paths)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(name: &str, headers: &[&str], rows: &[&[&str]]) -> Table {
        Table::new(
            name,
            headers.iter().map(|s| s.to_string()).collect(),
            rows.iter()
                .map(|r| r.iter().map(|s| s.to_string()).collect())
                .collect(),
        )
    }

    #[test]
    fn tpdc_selects_distinct_and_is_non_destructive() {
        let mut v = t(
            "vt11",
            &["shipment_number", "transportplanningpt"],
            &[
                &["1", "TPDC"],
                &["2", "TV33"],
                &["1", "TPDC"],
                &["", "TPDC"],
            ],
        );
        assert_eq!(select_tpdc_shipments(&mut v).unwrap(), vec!["1"]);
        assert_eq!(v.nrows(), 4);
        assert_eq!(v.visible_count(), 3);
    }

    #[test]
    fn tpdc_none_is_a_no_data_error() {
        let mut v = t(
            "vt11",
            &["shipment_number", "transportplanningpt"],
            &[&["2", "TV33"]],
        );
        let e = select_tpdc_shipments(&mut v).unwrap_err().to_string();
        assert!(e.contains("TPDC"), "{e}");
        let mut empty = t("vt11", &["shipment_number", "transportplanningpt"], &[]);
        assert!(select_tpdc_shipments(&mut empty).is_err());
    }

    #[test]
    fn missing_column_names_the_column() {
        let mut v = t("vt11", &["shipment_number"], &[&["1"]]);
        let e = select_tpdc_shipments(&mut v).unwrap_err().to_string();
        assert!(e.contains("transportplanningpt"), "{e}");
        let e = delivery_list(&v).unwrap_err().to_string();
        assert!(e.contains("delivery"), "{e}");
    }

    #[test]
    fn delivery_list_distinct_and_empty() {
        let v = t(
            "vl",
            &["delivery", "item"],
            &[&["5", "10"], &["5", "20"], &["6", "10"]],
        );
        assert_eq!(delivery_list(&v).unwrap(), vec!["5", "6"]);
        assert!(delivery_list(&t("vl", &["delivery"], &[])).is_err());
    }

    #[test]
    fn sn_del_filter_drops_pal_blank_parent_and_dup_parents() {
        let h = ["parent_serial_number", "status", "transfer_order_number"];
        let mut s = t(
            "sn",
            &h,
            &[
                &["P1", "SHP", "T1"],
                &["P1", "SHP", "T2"], // dup parent, removed
                &["P2", "PAL", "T3"], // PAL, hidden
                &["", "SHP", "T4"],   // blank parent, hidden
                &["P3", "SPV", "T5"],
            ],
        );
        filter_sn_del(&mut s).unwrap();
        assert_eq!(s.visible_count(), 2);
        assert_eq!(s.nrows(), 4);
        assert_eq!(transfer_order_list(&s).unwrap(), vec!["T1", "T5"]);
    }

    fn lt23(rows: &[&[&str]]) -> Table {
        t(
            "lt23",
            &[
                "delivery",
                "transfer_order_number",
                "creation_date",
                "creation_time",
                "source_storage_bin",
            ],
            rows,
        )
    }

    #[test]
    fn lt23_single_to_delivery_is_dropped_and_earliest_removed() {
        let mut l = lt23(&[
            &["10", "1", "2026-11-05", "08:00:00", "A"], // single-TO delivery
            &["20", "2", "2026-11-05", "08:00:00", "A"],
            &["20", "3", "2026-11-05", "09:00:00", "A"],
            &["20", "3", "2026-11-05", "09:00:00", "A"], // dup TO
            &["20", "4", "2026-11-05", "10:00:00", "A"],
        ]);
        lt23_postprocess(&mut l).unwrap();
        assert_eq!(l.headers[0], "bdup");
        assert_eq!(l.headers[5], "datetime");
        let to = l.col("transfer_order_number").unwrap();
        let got: Vec<&str> = l.rows.iter().map(|r| r[to].as_str()).collect();
        assert_eq!(got, vec!["4", "3"], "earliest TO (2) removed, newest first");
    }

    #[test]
    fn lt23_ties_keep_input_order_and_bin_breaks_ties_descending() {
        let mut l = lt23(&[
            &["20", "1", "2026-11-05", "08:00:00", "A"],
            &["20", "2", "2026-11-05", "08:00:00", "B"],
            &["20", "3", "2026-11-05", "08:00:00", "B"],
        ]);
        lt23_postprocess(&mut l).unwrap();
        let to = l.col("transfer_order_number").unwrap();
        // sorted: B(2), B(3) (stable), A(1); last (1) removed
        let got: Vec<&str> = l.rows.iter().map(|r| r[to].as_str()).collect();
        assert_eq!(got, vec!["2", "3"]);
    }

    #[test]
    fn lt23_empty_gives_empty() {
        let mut l = lt23(&[]);
        lt23_postprocess(&mut l).unwrap();
        assert_eq!(l.nrows(), 0);
    }

    #[test]
    fn mark_deletes_rows_without_matching_to() {
        let l = t("lt23", &["transfer_order_number"], &[&["1"], &["3"]]);
        let mut s = t(
            "sn",
            &["transfer_order_number", "delivery"],
            &[&["1", "a"], &["2", "b"], &["3", "c"]],
        );
        mark_sn_del_by_lt23(&mut s, &l).unwrap();
        assert_eq!(s.nrows(), 2);
        let c = s.col("count").unwrap();
        assert!(s.rows.iter().all(|r| r[c] == "1"));
    }

    #[test]
    fn week_parse() {
        assert_eq!(week_of("2026-11-04"), "45");
        assert_eq!(week_of("2026-11-05 20:15:01"), "45");
        assert_eq!(week_of(""), "-");
        assert_eq!(week_of("garbage"), "-");
    }

    #[test]
    fn snr_copy_keeps_blank_parents_and_inserts_columns() {
        let mut headers = vec!["lm_serial_number", "parent_serial_number", "plant"];
        headers.extend((4..=17).map(|_| "x"));
        headers.push("created_on");
        let mk = |a: &str, p: &str, d: &str| {
            let mut r = vec![a.to_string(), p.to_string(), "FV52".to_string()];
            r.extend((4..=17).map(|_| String::new()));
            r.push(d.to_string());
            r
        };
        let s = Table::new(
            "sn_to",
            headers.iter().map(|s| s.to_string()).collect(),
            vec![
                mk("1", "P", "2026-11-04"),
                mk("2", "P", "2026-11-04"),
                mk("3", "", "bad"),
                mk("4", "", "2026-01-01"),
            ],
        );
        let d = NaiveDate::from_ymd_opt(2026, 11, 5).unwrap();
        let c = snr_copy(&s, d).unwrap();
        assert_eq!(c.nrows(), 3);
        assert_eq!(c.headers[18], "week");
        assert_eq!(c.headers.last().unwrap(), "export_date");
        let w = c.col("week").unwrap();
        let weeks: Vec<&str> = c.rows.iter().map(|r| r[w].as_str()).collect();
        assert_eq!(weeks, vec!["45", "-", "1"]);
        assert!(c.rows.iter().all(|r| r.last().unwrap() == "2026-11-05"));
        let p = pivot_plant_by_date(&c).unwrap();
        assert_eq!(p.rows, vec![vec!["FV52", "2026-11-05", "3"]]);
    }

    #[test]
    fn pivot_orders_plants_and_labels_blank() {
        let s = t(
            "snr",
            &["lm_serial_number", "plant", "export_date"],
            &[
                &["1", "FW61", "d"],
                &["2", "FV52", "d"],
                &["3", "", "d"],
                &["", "FV52", "d"],
                &["4", "FV52", "d"],
            ],
        );
        let p = pivot_plant_by_date(&s).unwrap();
        let plants: Vec<&str> = p.rows.iter().map(|r| r[0].as_str()).collect();
        assert_eq!(plants, vec!["(blank)", "FV52", "FW61"]);
        assert_eq!(p.rows[1][2], "2");
        assert!(pivot_plant_by_date(&t("x", &["plant"], &[])).is_err());
    }
}
