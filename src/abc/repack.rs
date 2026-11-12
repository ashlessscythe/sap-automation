//! Pure stages of the `repack` report (spec 5.7, verified in 9.5 against the run of 2026-11-05;
//! where 5.7 and 9.5 differ, 9.5 wins).
//!
//! Same rules as `relabel.rs` / `mustgo.rs` / `inv.rs`: tables with sanitised headers in, tables
//! out, columns matched by name, no SAP access. The two ZMDESNR calls that feed these stages
//! (by deliveries excluding parents, and by serial / History) live in the seam of
//! [`super::repack_runner`].
//!
//! What the report does to the export `F` (`repack_sn`, original row order), spec 9.5:
//! 1. **Effective plant.** Exactly the relabel rule ([`relabel::effective_plants`]): a row of the
//!    label plant whose `customer` matches an alias customer pattern counts as the alias plant
//!    for the whole stage. The macro writes that mark into the sheet and replaces it back at the
//!    end; here the marks only live in a side vector, so nothing leaks into any output (spec 9.5
//!    "observed side effects", items 33-34: owner decision, no marks, no extra last row).
//! 2. **Blocks** (all in the original row order of `F`, never sorted):
//!    * R1: plant (effective) not blank and not the label plant, `parent_serial_number` blank,
//!      `net_weight` < 50;
//!    * R2 (not appended): the same with `net_weight` > 50. A row with exactly 50, or a blank or
//!      unparsable weight, is in neither (an Excel numeric filter). Its serials go to ZMDESNR by
//!      serial / History; the returned rows with `warehouse_number` = the history warehouse
//!      (placeholder `WH01`, config key `history_warehouse`) and `status` = `UNP` are appended after R1;
//!    * R3: plant not blank and not the label plant, `status` = `PAL`.
//!    * Then every row whose `container` matches `KC*` or `SC*` (case-insensitive) is deleted from
//!      the rows collected so far (R1, history rows, R3);
//!    * R4: effective plant = the label plant, `pallet` blank, `container` not matching `KC*`
//!      (a blank container passes), appended after that deletion.
//!
//!    `snr_repack` = R1, history rows, R3, R4 (rows can be in several blocks, e.g. a PAL row of
//!    low weight is in R1 and R3; both copies stay). The macro's `Multi_Sort` calls do not change
//!    that order (decision 34: block order, no sorting). Rows whose `lm_serial_number` is blank
//!    are dropped (the macro deletes empty rows by that column).
//! 3. **`export_date`** = the run date; the pivot counts `lm_serial_number` by `customer` x `plant`.
//!
//! The exclusion list sent to ZMDESNR (spec 5.7 row 68, 9.5) is the `lm_serial_number` of the rows
//! of relabel's `combined` flagged by its explicit `yellow` column (decision 35), and it is only
//! used when at least two such rows exist ([`MIN_EXCLUSIONS`]).
//!
//! Not reproduced on purpose (owner decision): the temporary alias plant marks left in the saved
//! `repack_sn` sheet and its extra last row with only `plant` = the alias plant. See
//! "Design notes: `repack`" in `docs/private/abc_headless_plan.md`.
#![allow(dead_code)]

use super::ops;
use super::relabel::{self, RelabelRules};
use super::table::Table;
use crate::utils::export_outcome::paste_value_list;
use anyhow::{anyhow, Result};

/// Plant codes and customer patterns (the relabel rules, `[abc.repack]` has the same keys) and
/// the warehouse number of the History rows to keep. The defaults are placeholders.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepackRules {
    /// `label_plant`, `alias_plant` and `alias_customers`, as for relabel.
    pub plants: RelabelRules,
    /// History rows are kept when `warehouse_number` equals this (case-insensitive).
    pub history_warehouse: String,
}

impl Default for RepackRules {
    fn default() -> Self {
        Self {
            plants: RelabelRules::default(),
            history_warehouse: "WH01".to_string(),
        }
    }
}

/// Status of the History rows that are kept.
pub const HISTORY_STATUS: &str = "UNP";
/// Net weight limit of R1 (`<`) and R2 (`>`); a row at exactly the limit is in neither.
pub const WEIGHT_LIMIT: f64 = 50.0;
/// Container patterns deleted from R1, the history rows and R3 (`repack_filter_kc`).
pub const DELETE_CONTAINERS: [&str; 2] = ["KC*", "SC*"];
/// R4 skips containers matching this.
pub const R4_SKIP_CONTAINER: &str = "KC*";
/// Fewer yellow rows than this: no exclusion list is sent (spec 5.7 row 68).
pub const MIN_EXCLUSIONS: usize = 2;
/// Added column after the columns of the export.
pub const COL_EXPORT_DATE: &str = "export_date";

/// Columns the by-deliveries export (`repack_sn`) must have.
pub const REPACK_SN_COLUMNS: [&str; 8] = [
    "lm_serial_number",
    "status",
    "pallet",
    "parent_serial_number",
    "plant",
    "customer",
    "container",
    "net_weight",
];
/// Columns the History export must have (it is appended to `snr_repack` by column name, so it
/// also needs every column of the by-deliveries export, checked by [`build_snr_repack`]).
pub const HISTORY_COLUMNS: [&str; 3] = ["lm_serial_number", "status", "warehouse_number"];
/// Columns of relabel's `combined` TSV that `repack` reads.
pub const COMBINED_COLUMNS: [&str; 2] = ["lm_serial_number", relabel::COL_YELLOW];

fn blank(s: &str) -> bool {
    s.trim().is_empty()
}

fn weight(s: &str) -> Option<f64> {
    s.trim().replace(',', "").parse::<f64>().ok()
}

fn is_deleted_container(c: &str) -> bool {
    DELETE_CONTAINERS
        .iter()
        .any(|p| ops::wildcard_match(p, c.trim()))
}

/// The exclusion list: distinct, blank-free `lm_serial_number` of the rows of relabel's
/// `combined` whose `yellow` cell is `1`. Returns `(serials, yellow_rows)`; the list is empty
/// when fewer than [`MIN_EXCLUSIONS`] rows are yellow (the macro skips the exclusion then).
pub fn exclusion_serials(combined: &Table) -> Result<(Vec<String>, usize)> {
    let (s, y) = (
        combined.col("lm_serial_number")?,
        combined.col(relabel::COL_YELLOW)?,
    );
    let yellow: Vec<&str> = combined
        .rows
        .iter()
        .filter(|r| r[y].trim() == "1")
        .map(|r| r[s].as_str())
        .collect();
    let n = yellow.len();
    if n < MIN_EXCLUSIONS {
        return Ok((Vec::new(), n));
    }
    Ok((paste_value_list(&yellow, false), n))
}

/// Row indexes (into `F`) of each block; `r2` is not appended to `snr_repack`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Blocks {
    pub r1: Vec<usize>,
    pub r2: Vec<usize>,
    pub r3: Vec<usize>,
    pub r4: Vec<usize>,
}

/// Step 2: select the blocks. `eff` is [`relabel::effective_plants`] of `f`. R1 and R3 are
/// returned before the KC/SC deletion (see [`build_snr_repack`]).
pub fn select_blocks(f: &Table, eff: &[String], rules: &RepackRules) -> Result<Blocks> {
    let c = |n: &str| f.col(n);
    let (ser, st, pal, par, net, con) = (
        c("lm_serial_number")?,
        c("status")?,
        c("pallet")?,
        c("parent_serial_number")?,
        c("net_weight")?,
        c("container")?,
    );
    let label = &rules.plants.label_plant;
    let mut b = Blocks::default();
    for (i, r) in f.rows.iter().enumerate() {
        if blank(&r[ser]) {
            continue; // an empty row: the macro deletes those by lm_serial_number
        }
        let plant = eff[i].as_str();
        let other = !plant.is_empty() && !plant.eq_ignore_ascii_case(label);
        let light = weight(&r[net]).is_some_and(|w| w < WEIGHT_LIMIT);
        let heavy = weight(&r[net]).is_some_and(|w| w > WEIGHT_LIMIT);
        if other && blank(&r[par]) && light {
            b.r1.push(i);
        }
        if other && blank(&r[par]) && heavy {
            b.r2.push(i);
        }
        if other && r[st].trim().eq_ignore_ascii_case("PAL") {
            b.r3.push(i);
        }
        if plant.eq_ignore_ascii_case(label)
            && blank(&r[pal])
            && !ops::wildcard_match(R4_SKIP_CONTAINER, r[con].trim())
        {
            b.r4.push(i);
        }
    }
    Ok(b)
}

/// Distinct, blank-free serials of block R2, normalised for the SAP paste (the input of the
/// History call; empty = no call).
pub fn history_serials(f: &Table, blocks: &Blocks) -> Result<Vec<String>> {
    let s = f.col("lm_serial_number")?;
    let raw: Vec<&str> = blocks.r2.iter().map(|&i| f.rows[i][s].as_str()).collect();
    Ok(paste_value_list(&raw, false))
}

/// Row indexes (into the History table) that are appended: `warehouse_number` = the history
/// warehouse and `status` = `UNP`, in the order of the History export.
pub fn history_rows(history: &Table, rules: &RepackRules) -> Result<Vec<usize>> {
    let (s, w, st) = (
        history.col("lm_serial_number")?,
        history.col("warehouse_number")?,
        history.col("status")?,
    );
    Ok(history
        .rows
        .iter()
        .enumerate()
        .filter(|(_, r)| {
            !blank(&r[s])
                && r[w]
                    .trim()
                    .eq_ignore_ascii_case(rules.history_warehouse.trim())
                && r[st].trim().eq_ignore_ascii_case(HISTORY_STATUS)
        })
        .map(|(i, _)| i)
        .collect())
}

/// Result of [`build_snr_repack`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnrRepack {
    /// Columns of `F`, then `export_date`.
    pub table: Table,
    /// Rows of R1, the appended history rows, R3 (all before the deletion), the rows the KC/SC
    /// deletion removed, and the rows of R4.
    pub r1: usize,
    pub history: usize,
    pub r3: usize,
    pub deleted: usize,
    pub r4: usize,
}

/// Steps 2-3: R1, history rows, R3, the KC/SC deletion, R4, `export_date`. `history` is the
/// History table (None = no R2 rows, no call) and `history_idx` [`history_rows`] of it.
pub fn build_snr_repack(
    f: &Table,
    blocks: &Blocks,
    history: Option<(&Table, &[usize])>,
    export_date: &str,
) -> Result<SnrRepack> {
    if f.col(COL_EXPORT_DATE).is_ok() {
        return Err(anyhow!(
            "repack_sn already has a column '{COL_EXPORT_DATE}'; cannot add it to snr_repack"
        ));
    }
    let con = f.col("container")?;
    // history rows re-ordered to the columns of F (by header name, never by position)
    let mut hist_rows: Vec<Vec<String>> = Vec::new();
    if let Some((h, idx)) = history {
        let map: Vec<usize> = f
            .headers
            .iter()
            .map(|name| {
                h.col(name).map_err(|_| {
                    anyhow!(
                        "the History export has no column '{name}' (found: {})",
                        h.headers.join(", ")
                    )
                })
            })
            .collect::<Result<_>>()?;
        for &i in idx {
            hist_rows.push(map.iter().map(|&c| h.rows[i][c].clone()).collect());
        }
    }
    let mut collected: Vec<Vec<String>> = Vec::new();
    collected.extend(blocks.r1.iter().map(|&i| f.rows[i].clone()));
    collected.extend(hist_rows.iter().cloned());
    collected.extend(blocks.r3.iter().map(|&i| f.rows[i].clone()));
    let before = collected.len();
    collected.retain(|r| !is_deleted_container(&r[con]));
    let deleted = before - collected.len();
    collected.extend(blocks.r4.iter().map(|&i| f.rows[i].clone()));
    let mut headers = f.headers.clone();
    headers.push(COL_EXPORT_DATE.to_string());
    for r in &mut collected {
        r.push(export_date.to_string());
    }
    Ok(SnrRepack {
        table: Table::new("snr_repack", headers, collected),
        r1: blocks.r1.len(),
        history: hist_rows.len(),
        r3: blocks.r3.len(),
        deleted,
        r4: blocks.r4.len(),
    })
}

/// Pivot of `snr_repack` in the macro's cross-tab layout (same as relabel's, see
/// [`relabel::pivot_cross`]): rows `customer`, columns `plant`, the count of `lm_serial_number`
/// (a row with a blank serial counts 0; there are none after [`select_blocks`]).
pub fn pivot_customer_plant(snr: &Table) -> Result<Table> {
    let s = snr.col("lm_serial_number")?;
    relabel::pivot_cross(snr, |r| i64::from(!blank(&r[s])))
}

#[cfg(test)]
mod tests {
    use super::*;

    const H: [&str; 9] = [
        "lm_serial_number",
        "status",
        "pallet",
        "parent_serial_number",
        "plant",
        "customer",
        "container",
        "net_weight",
        "warehouse_number",
    ];

    fn tbl(rows: &[[&str; 9]]) -> Table {
        Table::new(
            "repack_sn",
            H.iter().map(|s| s.to_string()).collect(),
            rows.iter()
                .map(|r| r.iter().map(|s| s.to_string()).collect())
                .collect(),
        )
    }

    /// serial, status, pallet, parent, plant, customer, container, net, warehouse
    fn r(v: [&str; 9]) -> [&str; 9] {
        v
    }

    #[test]
    fn exclusions_need_two_yellow_rows() {
        let h = |rows: &[[&str; 2]]| {
            Table::new(
                "combined",
                vec!["lm_serial_number".into(), "yellow".into()],
                rows.iter()
                    .map(|r| r.iter().map(|s| s.to_string()).collect())
                    .collect(),
            )
        };
        let (s, n) =
            exclusion_serials(&h(&[["1", "1"], ["2", ""], ["3", "1"], ["3", "1"]])).unwrap();
        assert_eq!((s, n), (vec!["1".to_string(), "3".to_string()], 3));
        let (s, n) = exclusion_serials(&h(&[["1", "1"], ["2", ""]])).unwrap();
        assert!(s.is_empty());
        assert_eq!(n, 1);
        let (s, n) = exclusion_serials(&h(&[["1", ""]])).unwrap();
        assert!(s.is_empty() && n == 0);
        // two yellow rows of the same serial are two rows (the macro counts rows) but one value
        let (s, n) = exclusion_serials(&h(&[["7", "1"], ["7", "1"]])).unwrap();
        assert_eq!((s, n), (vec!["7".to_string()], 2));
        let no_col = Table::new("c", vec!["lm_serial_number".into()], vec![vec!["1".into()]]);
        assert!(exclusion_serials(&no_col).is_err());
    }

    #[test]
    fn weight_boundary_and_blocks() {
        let f = tbl(&[
            r(["1", "SHP", "", "", "PL33", "A", "C1", "49.999", ""]), // R1
            r(["2", "SHP", "", "", "PL33", "A", "C2", "50", ""]),     // neither
            r(["3", "SHP", "", "", "PL33", "A", "C3", "50.001", ""]), // R2
            r(["4", "SHP", "", "P", "PL33", "A", "C4", "10", ""]),    // parent set
            r(["5", "SHP", "", "", "", "A", "C5", "10", ""]),         // blank plant
            r(["6", "SHP", "", "", "PL33", "A", "C6", "", ""]),       // blank weight
            r(["7", "SHP", "", "", "PL33", "A", "C7", "x", ""]),      // unparsable weight
            r(["8", "PAL", "", "", "PL33", "A", "C8", "10", ""]),     // R1 and R3
            r(["", "SHP", "", "", "PL33", "A", "C9", "10", ""]),      // blank serial
        ]);
        let rules = RepackRules::default();
        let eff = relabel::effective_plants(&f, &rules.plants).unwrap();
        let b = select_blocks(&f, &eff, &rules).unwrap();
        assert_eq!(b.r1, [0, 7]);
        assert_eq!(b.r2, [2]);
        assert_eq!(b.r3, [7]);
        assert!(b.r4.is_empty());
        assert_eq!(history_serials(&f, &b).unwrap(), ["3"]);
    }

    #[test]
    fn label_plant_and_alias_rows() {
        let f = tbl(&[
            r(["1", "SHP", "", "", "PL55", "X", "C1", "10", ""]), // R4
            r(["2", "SHP", "1", "", "PL55", "X", "C2", "10", ""]), // pallet: no
            r(["3", "SHP", "", "", "PL55", "X", "kc3", "10", ""]), // KC: no
            r(["4", "SHP", "", "", "PL55", "X", "", "10", ""]),   // blank container: R4
            r(["5", "SHP", "", "", "PL55", "ALIAS_A Y", "C5", "10", ""]), // alias: R1
            r(["6", "SHP", "", "", "PL55", "ALIAS_B Y", "C6", "80", ""]), // alias: R2
            r(["7", "SHP", "", "", "PL55", "AALIAS_A", "C7", "80", ""]), // not alias: R4
        ]);
        let rules = RepackRules::default();
        let eff = relabel::effective_plants(&f, &rules.plants).unwrap();
        let b = select_blocks(&f, &eff, &rules).unwrap();
        assert_eq!(b.r1, [4]);
        assert_eq!(b.r2, [5]);
        assert_eq!(b.r4, [0, 3, 6]);
        let s = build_snr_repack(&f, &b, None, "2026-11-05").unwrap();
        // alias rows print their real plant
        assert_eq!(s.table.nrows(), 4);
        let p = s.table.col("plant").unwrap();
        assert!(s.table.rows.iter().all(|r| r[p] == "PL55"));
    }

    #[test]
    fn history_filter_and_append_order() {
        let f = tbl(&[
            r(["1", "SHP", "", "", "PL33", "A", "C1", "10", ""]),
            r(["2", "SHP", "", "", "PL33", "A", "C2", "60", ""]),
            r(["3", "PAL", "", "P", "PL34", "A", "C3", "10", ""]),
            r(["4", "SHP", "", "", "PL55", "A", "C4", "10", ""]),
        ]);
        // a history table whose columns are in another order
        let mut hh = H.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        hh.swap(0, 8);
        let h = Table::new(
            "h",
            hh,
            vec![
                vec!["WH01", "UNP", "", "", "PL33", "A", "C2", "60", "2"],
                vec!["WH01", "PRI", "", "", "PL33", "A", "C2", "60", "2"],
                vec!["WH02", "UNP", "", "", "PL33", "A", "C2", "60", "2"],
                vec!["wh01", "unp", "", "", "PL33", "A", "KC2", "60", "2"],
            ]
            .into_iter()
            .map(|r| r.into_iter().map(String::from).collect())
            .collect(),
        );
        let rules = RepackRules::default();
        let eff = relabel::effective_plants(&f, &rules.plants).unwrap();
        let b = select_blocks(&f, &eff, &rules).unwrap();
        let idx = history_rows(&h, &rules).unwrap();
        assert_eq!(idx, [0, 3]);
        let s = build_snr_repack(&f, &b, Some((&h, &idx)), "2026-11-05").unwrap();
        let ser: Vec<&str> = s.table.rows.iter().map(|r| r[0].as_str()).collect();
        // R1 (1), history 2 (the KC one is deleted), R3 (3), R4 (4)
        assert_eq!(ser, ["1", "2", "3", "4"]);
        assert_eq!((s.r1, s.history, s.r3, s.deleted, s.r4), (1, 2, 1, 1, 1));
        assert_eq!(s.table.headers.last().unwrap(), "export_date");
        assert!(s
            .table
            .rows
            .iter()
            .all(|r| r.last().unwrap() == "2026-11-05"));
        // missing column in history
        let bad = Table::new("h", vec!["lm_serial_number".into()], vec![vec!["2".into()]]);
        assert!(build_snr_repack(&f, &b, Some((&bad, &[0])), "d").is_err());
    }

    #[test]
    fn pivot_counts_rows() {
        let t = Table::new(
            "snr_repack",
            vec!["lm_serial_number".into(), "customer".into(), "plant".into()],
            vec![
                vec!["1".into(), "B".into(), "P2".into()],
                vec!["2".into(), "B".into(), "P2".into()],
                vec!["3".into(), "A".into(), "P1".into()],
                vec!["4".into(), "".into(), "P1".into()],
            ],
        );
        let p = pivot_customer_plant(&t).unwrap();
        assert_eq!(p.headers, ["customer", "P1", "P2", "Grand Total"]);
        assert_eq!(p.rows[0], ["A", "1", "", "1"]);
        assert_eq!(p.rows[1], ["B", "", "2", "2"]);
        assert_eq!(p.rows[2], ["(blank)", "1", "", "1"]);
        assert_eq!(p.rows[3], ["Grand Total", "2", "2", "4"]);
        assert_eq!(relabel::pivot_total(&p).unwrap(), 4);
    }
}
