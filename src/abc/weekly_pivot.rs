//! Computed weekly pivot of a merged report (`weekly_pivot` sheet, plan phase 6).
//!
//! Native Excel PivotTables cannot be written, so this is a plain table of computed values:
//! rows = one per day, columns = the plants of the report, cells = the same measure the report's
//! per-day `pivot` uses, grand-total column (right) and row (bottom) both labelled `Total`.
//!
//! - Day column: `export_date` when the source table has it (mustgo `snr_copy`, relabel
//!   `combined`, repack `snr_repack`), else the merge's ISO `run_date`.
//! - Day order: ascending when every non-blank day text is ISO `yyyy-mm-dd`; otherwise (e.g.
//!   `WK_46` or `MM-DD-YYYY`) the first-appearance order of the days. Blank day = `(blank)`, last.
//! - Plants: distinct trimmed values, case-insensitive then ordinal ascending; a blank plant is
//!   the `(blank)` column, last (same convention as the per-day pivots, see `inv::pivot_sum`).
//! - Cells inside the grid with no rows are `0`.
//! - Per rName: `weekly_pivot_spec`; `None` for an rName without one (none at present).
#![allow(dead_code)]

use super::inv::{fmt_num, parse_num, BLANK_LABEL};
use super::table::Table;
use anyhow::{anyhow, Result};
use chrono::NaiveDate;
use std::collections::BTreeMap;

/// Name of the extra sheet.
pub const SHEET: &str = "weekly_pivot";
/// Label of the grand-total column and row.
pub const TOTAL_LABEL: &str = "Total";

/// What a cell of the weekly pivot aggregates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Measure {
    /// Sum of the numeric cells of the column (blank / text add nothing).
    Sum(&'static str),
    /// Number of rows whose column is non-blank.
    CountNonBlank(&'static str),
}

/// Where the weekly pivot of one report comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WeeklyPivotSpec {
    /// Output table of the report (merged, `run_date` first) the pivot is built from.
    pub source_table: &'static str,
    /// Column whose distinct values become the pivot columns.
    pub plant_col: &'static str,
    pub measure: Measure,
}

/// The weekly pivot spec of `rname` (same source table and measure as its per-day pivot, minus
/// the customer dimension for relabel / repack). Every current rName has one.
pub fn weekly_pivot_spec(rname: &str) -> Option<WeeklyPivotSpec> {
    let s = |table, plant, measure| {
        Some(WeeklyPivotSpec {
            source_table: table,
            plant_col: plant,
            measure,
        })
    };
    match rname.trim().to_ascii_lowercase().as_str() {
        "mustgo" => s(
            "snr_copy",
            "plant",
            Measure::CountNonBlank("lm_serial_number"),
        ),
        "xdock" | "toyota" | "k2xx" => s("vl06o", "plant", Measure::Sum("total")),
        "misc" => s(
            "vl06o",
            "shipping_pointreceiving_pt",
            Measure::Sum("number_of_packages"),
        ),
        "relabel" => s("combined", "plant", Measure::Sum("_of_labels")),
        "repack" => s(
            "snr_repack",
            "plant",
            Measure::CountNonBlank("lm_serial_number"),
        ),
        _ => None,
    }
}

fn is_iso(s: &str) -> bool {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").is_ok()
}

/// Build the weekly pivot of `src` (a merged table, hidden rows ignored). Errors when a needed
/// column is missing or when there are no rows.
pub fn build(src: &Table, spec: &WeeklyPivotSpec) -> Result<Table> {
    let date_name = if src.headers.iter().any(|h| h == "export_date") {
        "export_date"
    } else {
        "run_date"
    };
    let (dc, pc) = (src.col(date_name)?, src.col(spec.plant_col)?);
    let vc = match spec.measure {
        Measure::Sum(c) | Measure::CountNonBlank(c) => src.col(c)?,
    };
    // (blank?, lowercase) keys: ordinal order = case-insensitive, blank last
    type Key = (bool, String);
    let mut plants: BTreeMap<Key, String> = BTreeMap::new();
    let mut days: Vec<(String, bool)> = Vec::new(); // first-appearance order (text, blank)
    let mut cells: BTreeMap<(String, Key), f64> = BTreeMap::new();
    let mut any = false;
    for (i, r) in src.rows.iter().enumerate() {
        if !src.visible[i] {
            continue;
        }
        any = true;
        let day = r[dc].trim();
        let (day, dblank) = if day.is_empty() {
            (BLANK_LABEL.to_string(), true)
        } else {
            (day.to_string(), false)
        };
        if !days.iter().any(|(d, _)| *d == day) {
            days.push((day.clone(), dblank));
        }
        let p = r[pc].trim();
        let (pk, pn) = if p.is_empty() {
            ((true, String::new()), BLANK_LABEL.to_string())
        } else {
            ((false, p.to_lowercase()), p.to_string())
        };
        plants.entry(pk.clone()).or_insert(pn);
        let v = match spec.measure {
            Measure::Sum(_) => parse_num(&r[vc]).unwrap_or(0.0),
            Measure::CountNonBlank(_) => f64::from(u8::from(!r[vc].trim().is_empty())),
        };
        *cells.entry((day, pk)).or_insert(0.0) += v;
    }
    if !any {
        return Err(anyhow!("no rows in '{}'", spec.source_table));
    }
    if days.iter().filter(|(_, b)| !*b).all(|(d, _)| is_iso(d)) {
        days.sort_by(|a, b| (a.1, &a.0).cmp(&(b.1, &b.0)));
    }
    let mut headers = vec![date_name.to_string()];
    headers.extend(plants.values().cloned());
    headers.push(TOTAL_LABEL.to_string());
    let mut rows = Vec::with_capacity(days.len() + 1);
    let mut col_totals = vec![0.0; plants.len()];
    for (day, _) in &days {
        let mut row = vec![day.clone()];
        let mut total = 0.0;
        for (j, pk) in plants.keys().enumerate() {
            let v = cells
                .get(&(day.clone(), pk.clone()))
                .copied()
                .unwrap_or(0.0);
            total += v;
            col_totals[j] += v;
            row.push(fmt_num(v));
        }
        row.push(fmt_num(total));
        rows.push(row);
    }
    let mut last = vec![TOTAL_LABEL.to_string()];
    last.extend(col_totals.iter().map(|v| fmt_num(*v)));
    last.push(fmt_num(col_totals.iter().sum()));
    rows.push(last);
    Ok(Table::new(SHEET, headers, rows))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(headers: &[&str], rows: &[&[&str]]) -> Table {
        Table::new(
            "src",
            headers.iter().map(|s| s.to_string()).collect(),
            rows.iter()
                .map(|r| r.iter().map(|s| s.to_string()).collect())
                .collect(),
        )
    }

    const SUM: WeeklyPivotSpec = WeeklyPivotSpec {
        source_table: "x",
        plant_col: "plant",
        measure: Measure::Sum("total"),
    };

    fn row(t: &Table, i: usize) -> Vec<&str> {
        t.rows[i].iter().map(String::as_str).collect()
    }

    #[test]
    fn sum_run_date_sorted_plants_totals() {
        let src = t(
            &["run_date", "plant", "total"],
            &[
                &["2026-11-03", "b2", "2"],
                &["2026-11-02", "B1", "5"],
                &["2026-11-03", "A9", "1.5"],
                &["2026-11-03", "b2", "3"],
                &["2026-11-02", "", "4"],
                &["2026-11-02", "A9", ""],
            ],
        );
        let p = build(&src, &SUM).unwrap();
        assert_eq!(
            p.headers,
            ["run_date", "A9", "B1", "b2", "(blank)", "Total"]
        );
        assert_eq!(row(&p, 0), ["2026-11-02", "0", "5", "0", "4", "9"]);
        assert_eq!(row(&p, 1), ["2026-11-03", "1.5", "0", "5", "0", "6.5"]);
        assert_eq!(row(&p, 2), ["Total", "1.5", "5", "5", "4", "15.5"]);
        assert_eq!(p.nrows(), 3);
    }

    #[test]
    fn count_variant_counts_non_blank() {
        let spec = WeeklyPivotSpec {
            measure: Measure::CountNonBlank("sn"),
            ..SUM
        };
        let src = t(
            &["run_date", "plant", "sn"],
            &[
                &["2026-11-02", "P", "1"],
                &["2026-11-02", "P", ""],
                &["2026-11-02", "Q", "2"],
            ],
        );
        let p = build(&src, &spec).unwrap();
        assert_eq!(row(&p, 0), ["2026-11-02", "1", "1", "2"]);
        assert_eq!(row(&p, 1), ["Total", "1", "1", "2"]);
    }

    #[test]
    fn export_date_wins_over_run_date() {
        let src = t(
            &["run_date", "export_date", "plant", "total"],
            &[&["2026-11-02", "2026-11-09", "P", "1"]],
        );
        let p = build(&src, &SUM).unwrap();
        assert_eq!(p.headers[0], "export_date");
        assert_eq!(p.rows[0][0], "2026-11-09");
    }

    #[test]
    fn non_iso_days_keep_first_appearance_order() {
        let src = t(
            &["run_date", "export_date", "plant", "total"],
            &[
                &["a", "WK_46", "P", "1"],
                &["b", "WK_45", "P", "2"],
                &["c", "WK_46", "P", "3"],
            ],
        );
        let p = build(&src, &SUM).unwrap();
        assert_eq!(p.rows[0][0], "WK_46");
        assert_eq!(p.rows[1][0], "WK_45");
        assert_eq!(row(&p, 0), ["WK_46", "4", "4"]);
    }

    #[test]
    fn blank_day_goes_last_and_errors() {
        let src = t(
            &["run_date", "plant", "total"],
            &[&["", "P", "1"], &["2026-11-02", "P", "2"]],
        );
        let p = build(&src, &SUM).unwrap();
        assert_eq!(p.rows[0][0], "2026-11-02");
        assert_eq!(p.rows[1][0], "(blank)");
        assert!(build(&t(&["run_date", "plant"], &[&["x", "P"]]), &SUM).is_err());
        assert!(build(&t(&["run_date", "plant", "total"], &[]), &SUM).is_err());
    }

    #[test]
    fn every_rname_has_a_spec() {
        for n in super::super::registry::report_names() {
            let s = weekly_pivot_spec(n).unwrap();
            assert!(super::super::registry::tables(n).contains(&s.source_table));
        }
        assert!(weekly_pivot_spec("nope").is_none());
    }
}
