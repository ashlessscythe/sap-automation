//! Pure stages of the `relabel` report (spec 5.6, verified in 9.5 against the run of 2026-11-05;
//! where 5.6 and 9.5 differ, 9.5 wins).
//!
//! Same rules as `mustgo.rs` / `inv.rs` / `k2xx.rs`: tables with sanitised headers in, tables
//! out, columns matched by name, no SAP access. The two ZMDESNR calls that feed these stages
//! (by deliveries, and by serial / History) live in the seam of [`super::relabel_runner`].
//!
//! What the report does to the delivery export `F` (`sn_del`, original row order), for the run
//! date `D` and the window `W = [D, D+1]` on `document_date` (spec 9.5):
//! 1. **Effective plant.** A row with `plant = PL55` and a `customer` matching `REDACTED*`, `GM*` or
//!    `GENERAL*` is treated as plant `AL55` for the rest of the stage. The macro writes that mark
//!    into the sheet and replaces it back at the end; here the marks only live in a side vector
//!    ([`effective_plants`]), so nothing leaks into any output (spec 9.5 "observed side effects",
//!    items 33-34: owner decision, no marks, no extra row).
//! 2. **Blocks** (all in the original row order of `F`, never sorted, a row can be in several):
//!    B1 `plant <> PL55` (not blank), `spv_scan_copies = 2`, `document_date` in `W`;
//!    B2 the same plant rule, `serial_number_2nd_party_verification` blank, in `W`;
//!    B3 the same plant rule, `status = PAL`, first row per `parent_serial_number`;
//!    B4 (not appended) `plant = PL55`, `pallet = 1`: its serials go to ZMDESNR by serial /
//!    History; the returned rows with `storage_location = 11` and `status = PRI` mark their
//!    serials **yellow**; B5 `pallet` in {1,2} and `plant = PL55`; B6 `plant = PL55`, `pallet`
//!    blank, `parent_serial_number` blank; B7 `parent_serial_number` and `kanban_number` both
//!    non-blank and `kanban_number` not starting with `0`.
//!    `combined` = B1, B2, B3, B5, B6, B7. Plant comparisons use the effective plant.
//! 3. **`_of_labels`**: `4` for a row whose effective plant is `PL55`, with blank
//!    `kanban_number`, `container` not matching `KC*`, `pallet` in {1,2} and not yellow, provided
//!    at least two such rows exist; `2` for every other row.
//! 4. **`export_date`** = the run date; the pivot sums `_of_labels` by `customer` x `plant`.
//!
//! The macro's yellow cell fill is a data channel (spec 9.5, decision 35); here it is the
//! explicit column `yellow` (`1` or blank) written last, so that `repack` can read it back
//! from the TSV. Rows whose `lm_serial_number` is blank are dropped (the macro deletes empty
//! rows by that column).
//!
//! Not reproduced on purpose (owner decision): the temporary `AL55` marks left in the saved
//! `sn_del` sheet and the extra last row with only `plant = AL55`. `save full_label` (A!62) was
//! not ticked in the verified run and is left out. See "Design notes: `relabel`" in
//! `docs/abc_headless_plan.md`.
#![allow(dead_code)]

use super::ops;
use super::table::Table;
use crate::utils::export_outcome::paste_value_list;
use anyhow::{anyhow, Result};
use chrono::{Duration, NaiveDate};
use std::collections::{BTreeMap, HashSet};

/// Plant a marked row is treated as during the stage.
pub const AL55: &str = "AL55";
/// The plant of the label rows.
pub const PL55: &str = "PL55";
/// Customer patterns that move an `PL55` row to `AL55` (`rework_relabel_al55`, three rows).
pub const AL55_CUSTOMERS: [&str; 3] = ["REDACTED*", "GM*", "GENERAL*"];
/// Labels per row of the label rule and of every other row.
pub const LABELS_SPECIAL: u32 = 4;
pub const LABELS_DEFAULT: u32 = 2;
/// Added columns, in order, after the columns of the delivery export.
pub const COL_LABELS: &str = "_of_labels";
pub const COL_EXPORT_DATE: &str = "export_date";
pub const COL_YELLOW: &str = "yellow";

/// Columns the delivery export (`sn_del`) must have.
pub const SN_DEL_COLUMNS: [&str; 11] = [
    "lm_serial_number",
    "status",
    "serial_number_2nd_party_verification",
    "spv_scan_copies",
    "pallet",
    "parent_serial_number",
    "plant",
    "document_date",
    "kanban_number",
    "container",
    "customer",
];
/// Columns the History export must have.
pub const HISTORY_COLUMNS: [&str; 3] = ["lm_serial_number", "storage_location", "status"];

/// `[D, D+1]`, both ends inclusive (spec 9.5).
pub fn window(d: NaiveDate) -> (NaiveDate, NaiveDate) {
    (d, d + Duration::days(1))
}

fn blank(s: &str) -> bool {
    s.trim().is_empty()
}

fn num(s: &str) -> Option<f64> {
    s.trim().replace(',', "").parse::<f64>().ok()
}

/// Numeric (or, failing that, case-insensitive text) equality, like an Excel filter `=n`.
fn eq_num(cell: &str, n: f64) -> bool {
    num(cell) == Some(n)
}

fn in_window(cell: &str, w: (NaiveDate, NaiveDate)) -> bool {
    let first = cell.split_whitespace().next().unwrap_or("");
    NaiveDate::parse_from_str(first, "%Y-%m-%d").is_ok_and(|d| d >= w.0 && d <= w.1)
}

/// Effective plant of every row of `f` (step 1): `AL55` for `PL55` rows of a `REDACTED*` / `GM*` /
/// `GENERAL*` customer, else the plant as it is (trimmed).
pub fn effective_plants(f: &Table) -> Result<Vec<String>> {
    let (p, c) = (f.col("plant")?, f.col("customer")?);
    Ok(f.rows
        .iter()
        .map(|r| {
            let plant = r[p].trim();
            if plant.eq_ignore_ascii_case(PL55)
                && AL55_CUSTOMERS
                    .iter()
                    .any(|pat| ops::wildcard_match(pat, r[c].trim()))
            {
                AL55.to_string()
            } else {
                plant.to_string()
            }
        })
        .collect())
}

/// Row indexes (into `f`) of each block; `b4` is not appended to `combined`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Blocks {
    pub b1: Vec<usize>,
    pub b2: Vec<usize>,
    pub b3: Vec<usize>,
    pub b4: Vec<usize>,
    pub b5: Vec<usize>,
    pub b6: Vec<usize>,
    pub b7: Vec<usize>,
}

impl Blocks {
    /// Row indexes of `combined` in order: B1, B2, B3, B5, B6, B7.
    pub fn combined_rows(&self) -> Vec<usize> {
        [&self.b1, &self.b2, &self.b3, &self.b5, &self.b6, &self.b7]
            .into_iter()
            .flatten()
            .copied()
            .collect()
    }
}

/// Step 2: select the blocks. `eff` is [`effective_plants`] of `f`, `d` the run date.
pub fn select_blocks(f: &Table, eff: &[String], d: NaiveDate) -> Result<Blocks> {
    let c = |n: &str| f.col(n);
    let (ser, st, ver, spv, pal, par, dd, kan) = (
        c("lm_serial_number")?,
        c("status")?,
        c("serial_number_2nd_party_verification")?,
        c("spv_scan_copies")?,
        c("pallet")?,
        c("parent_serial_number")?,
        c("document_date")?,
        c("kanban_number")?,
    );
    let w = window(d);
    let mut b = Blocks::default();
    let mut b3_seen = HashSet::new();
    for (i, r) in f.rows.iter().enumerate() {
        if blank(&r[ser]) {
            continue; // an empty row: the macro deletes those by lm_serial_number
        }
        let plant = eff[i].as_str();
        let other = !plant.is_empty() && !plant.eq_ignore_ascii_case(PL55);
        let pl55 = plant.eq_ignore_ascii_case(PL55);
        let win = in_window(&r[dd], w);
        if other && eq_num(&r[spv], 2.0) && win {
            b.b1.push(i);
        }
        if other && blank(&r[ver]) && win {
            b.b2.push(i);
        }
        if other
            && r[st].trim().eq_ignore_ascii_case("PAL")
            && b3_seen.insert(r[par].trim().to_lowercase())
        {
            b.b3.push(i);
        }
        if pl55 && eq_num(&r[pal], 1.0) {
            b.b4.push(i);
        }
        if pl55 && (eq_num(&r[pal], 1.0) || eq_num(&r[pal], 2.0)) {
            b.b5.push(i);
        }
        if pl55 && blank(&r[pal]) && blank(&r[par]) {
            b.b6.push(i);
        }
        if !blank(&r[par]) && !blank(&r[kan]) && !r[kan].trim().starts_with('0') {
            b.b7.push(i);
        }
    }
    Ok(b)
}

/// Distinct, blank-free serials of block B4, normalised for the SAP paste (the input of the
/// History call; empty = no call).
pub fn history_serials(f: &Table, blocks: &Blocks) -> Result<Vec<String>> {
    let s = f.col("lm_serial_number")?;
    let raw: Vec<&str> = blocks.b4.iter().map(|&i| f.rows[i][s].as_str()).collect();
    Ok(paste_value_list(&raw, false))
}

/// The serials the History export marks yellow: `lm_serial_number` of the rows with
/// `storage_location = 11` and `status = PRI`.
pub fn yellow_serials(history: &Table) -> Result<HashSet<String>> {
    let (s, l, st) = (
        history.col("lm_serial_number")?,
        history.col("storage_location")?,
        history.col("status")?,
    );
    Ok(history
        .rows
        .iter()
        .filter(|r| eq_num(&r[l], 11.0) && r[st].trim().eq_ignore_ascii_case("PRI"))
        .map(|r| r[s].trim().to_string())
        .filter(|v| !v.is_empty())
        .collect())
}

/// Result of [`build_combined`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Combined {
    /// Columns of `f`, then `_of_labels`, `export_date`, `yellow`.
    pub table: Table,
    /// Rows that got 4 labels (0 when fewer than two qualified).
    pub rows_with_4: usize,
    /// Sum of `_of_labels`.
    pub label_total: u32,
    /// Rows flagged yellow in `combined`.
    pub yellow_rows: usize,
}

/// Steps 2-4: append the blocks, set `_of_labels`, `export_date` and the `yellow` flag.
/// `yellow` is the set of serials from [`yellow_serials`]; `export_date` the text of the run date.
pub fn build_combined(
    f: &Table,
    eff: &[String],
    blocks: &Blocks,
    yellow: &HashSet<String>,
    export_date: &str,
) -> Result<Combined> {
    for added in [COL_LABELS, COL_EXPORT_DATE, COL_YELLOW] {
        if f.col(added).is_ok() {
            return Err(anyhow!(
                "sn_del already has a column '{added}'; cannot add it to combined"
            ));
        }
    }
    let (ser, pal, kan, con) = (
        f.col("lm_serial_number")?,
        f.col("pallet")?,
        f.col("kanban_number")?,
        f.col("container")?,
    );
    let idx = blocks.combined_rows();
    let is_yellow: Vec<bool> = idx
        .iter()
        .map(|&i| yellow.contains(f.rows[i][ser].trim()))
        .collect();
    // the 4-label rule (spec 9.5 step 3)
    let special: Vec<bool> = idx
        .iter()
        .zip(&is_yellow)
        .map(|(&i, &y)| {
            let r = &f.rows[i];
            eff[i].eq_ignore_ascii_case(PL55)
                && blank(&r[kan])
                && !ops::wildcard_match("KC*", r[con].trim())
                && (eq_num(&r[pal], 1.0) || eq_num(&r[pal], 2.0))
                && !y
        })
        .collect();
    let n4 = special.iter().filter(|s| **s).count();
    let four = n4 >= 2;
    let mut headers = f.headers.clone();
    headers.extend([COL_LABELS, COL_EXPORT_DATE, COL_YELLOW].map(String::from));
    let mut label_total = 0;
    let mut rows = Vec::with_capacity(idx.len());
    for (k, &i) in idx.iter().enumerate() {
        let labels = if four && special[k] {
            LABELS_SPECIAL
        } else {
            LABELS_DEFAULT
        };
        label_total += labels;
        let mut row = f.rows[i].clone();
        row.push(labels.to_string());
        row.push(export_date.to_string());
        row.push(if is_yellow[k] { "1" } else { "" }.to_string());
        rows.push(row);
    }
    Ok(Combined {
        table: Table::new("combined", headers, rows),
        rows_with_4: if four { n4 } else { 0 },
        label_total,
        yellow_rows: is_yellow.iter().filter(|y| **y).count(),
    })
}

/// Label of the first column of the pivot's header and of its last row / column (as in Excel).
pub const PIVOT_TOTAL_LABEL: &str = "Grand Total";

/// Pivot of `combined` in the macro's cross-tab layout: header `customer`, one column per plant,
/// `Grand Total`; one row per customer, then a `Grand Total` row. Customers and plants ascending
/// (case-insensitive), a blank one is `(blank)` and sorts last; a cell without rows is blank.
pub fn pivot_customer_plant(combined: &Table) -> Result<Table> {
    let (c, p, l) = (
        combined.col("customer")?,
        combined.col("plant")?,
        combined.col(COL_LABELS)?,
    );
    type Key = (bool, String);
    let key = |v: &str| -> (Key, String) {
        let v = v.trim();
        if v.is_empty() {
            ((true, String::new()), super::inv::BLANK_LABEL.to_string())
        } else {
            ((false, v.to_lowercase()), v.to_string())
        }
    };
    let mut custs: BTreeMap<Key, String> = BTreeMap::new();
    let mut plants: BTreeMap<Key, String> = BTreeMap::new();
    let mut cells: BTreeMap<(Key, Key), i64> = BTreeMap::new();
    for r in &combined.rows {
        let ((kc, nc), (kp, np)) = (key(&r[c]), key(&r[p]));
        custs.entry(kc.clone()).or_insert(nc);
        plants.entry(kp.clone()).or_insert(np);
        *cells.entry((kc, kp)).or_insert(0) += r[l].trim().parse::<i64>().unwrap_or(0);
    }
    let mut headers = vec!["customer".to_string()];
    headers.extend(plants.values().cloned());
    headers.push(PIVOT_TOTAL_LABEL.to_string());
    let show = |n: Option<i64>| n.map(|n| n.to_string()).unwrap_or_default();
    let mut rows = Vec::new();
    for (kc, name) in &custs {
        let mut row = vec![name.clone()];
        let mut total = 0;
        for kp in plants.keys() {
            let n = cells.get(&(kc.clone(), kp.clone())).copied();
            total += n.unwrap_or(0);
            row.push(show(n));
        }
        row.push(total.to_string());
        rows.push(row);
    }
    let mut last = vec![PIVOT_TOTAL_LABEL.to_string()];
    let mut grand = 0;
    for kp in plants.keys() {
        let n: i64 = cells
            .iter()
            .filter(|((_, k), _)| k == kp)
            .map(|(_, n)| *n)
            .sum();
        grand += n;
        last.push(n.to_string());
    }
    last.push(grand.to_string());
    rows.push(last);
    Ok(Table::new("pivot", headers, rows))
}

/// The grand total of the pivot (bottom right cell), for the run summary.
pub fn pivot_total(pivot: &Table) -> Result<i64> {
    let l = pivot
        .headers
        .len()
        .checked_sub(1)
        .ok_or_else(|| anyhow!("empty pivot"))?;
    let r = pivot.rows.last().ok_or_else(|| anyhow!("empty pivot"))?;
    Ok(r[l].trim().parse::<i64>().unwrap_or(0))
}

#[cfg(test)]
mod tests {
    use super::*;

    const H: [&str; 12] = [
        "lm_serial_number",
        "status",
        "serial_number_2nd_party_verification",
        "spv_scan_copies",
        "pallet",
        "parent_serial_number",
        "plant",
        "document_date",
        "kanban_number",
        "container",
        "customer",
        "delivery",
    ];

    fn d() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, 5).unwrap()
    }

    fn tbl(rows: &[[&str; 12]]) -> Table {
        Table::new(
            "sn_del",
            H.iter().map(|s| s.to_string()).collect(),
            rows.iter()
                .map(|r| r.iter().map(|s| s.to_string()).collect())
                .collect(),
        )
    }

    /// serial, status, verification, spv, pallet, parent, plant, date, kanban, container, customer
    fn row(v: [&str; 11]) -> [&str; 12] {
        [
            v[0], v[1], v[2], v[3], v[4], v[5], v[6], v[7], v[8], v[9], v[10], "1",
        ]
    }

    #[test]
    fn al55_rule() {
        let f = tbl(&[
            row(["1", "", "", "", "", "", "PL55", "", "", "", "GM REDACTED"]),
            row(["2", "", "", "", "", "", "PL55", "", "", "", "REDACTED corp"]),
            row([
                "3",
                "",
                "",
                "",
                "",
                "",
                "PL55",
                "",
                "",
                "",
                "General Motors",
            ]),
            row(["4", "", "", "", "", "", "PL55", "", "", "", "REDACTED"]),
            row(["5", "", "", "", "", "", "PL33", "", "", "", "GM REDACTED"]),
            row(["6", "", "", "", "", "", "PL55", "", "", "", ""]),
        ]);
        assert_eq!(
            effective_plants(&f).unwrap(),
            ["AL55", "AL55", "AL55", "PL55", "PL33", "PL55"]
        );
    }

    #[test]
    fn blocks_and_order() {
        let f = tbl(&[
            // 0: B1 and B2 (spv 2, verification blank, in window)
            row([
                "10",
                "SHP",
                "",
                "2",
                "",
                "",
                "PL33",
                "2026-11-05",
                "",
                "",
                "A",
            ]),
            // 1: B2 only; window end is inclusive
            row([
                "11",
                "SHP",
                "",
                "0",
                "",
                "",
                "PL33",
                "2026-11-06",
                "",
                "",
                "A",
            ]),
            // 2: out of window
            row([
                "12",
                "SHP",
                "",
                "2",
                "",
                "",
                "PL33",
                "2026-11-07",
                "",
                "",
                "A",
            ]),
            // 3: PAL, parent P1 (B3), verified so no B2
            row([
                "13",
                "PAL",
                "X",
                "0",
                "",
                "P1",
                "PL33",
                "2026-10-01",
                "",
                "",
                "A",
            ]),
            // 4: PAL, same parent: dropped by the dedup
            row([
                "14",
                "PAL",
                "X",
                "0",
                "",
                "P1",
                "PL33",
                "2026-10-01",
                "",
                "",
                "A",
            ]),
            // 5: PL55 pallet 1 (B4, B5)
            row(["15", "SHP", "X", "", "1", "", "PL55", "", "", "", "REDACTED"]),
            // 6: PL55 pallet 2 (B5)
            row(["16", "SHP", "X", "", "2", "", "PL55", "", "", "", "REDACTED"]),
            // 7: PL55 pallet blank, parent blank (B6)
            row(["17", "SHP", "X", "", "", "", "PL55", "", "", "", "REDACTED"]),
            // 8: AL55 customer: counts as a non-PL55 plant (B2), not B5/B6
            row([
                "18",
                "SHP",
                "",
                "",
                "1",
                "",
                "PL55",
                "2026-11-05",
                "",
                "",
                "GM X",
            ]),
            // 9: B7 (parent and kanban set), not starting 0
            row([
                "19", "SHP", "X", "", "", "P2", "PL55", "", "K7", "", "REDACTED",
            ]),
            // 10: kanban "0": not B7
            row(["20", "SHP", "X", "", "", "P2", "PL33", "", "0", "", "A"]),
            // 11: blank serial: dropped; blank plant: no plant block
            row([
                "",
                "SHP",
                "",
                "2",
                "",
                "",
                "PL33",
                "2026-11-05",
                "",
                "",
                "A",
            ]),
            row(["21", "SHP", "", "2", "", "", "", "2026-11-05", "", "", "A"]),
        ]);
        let eff = effective_plants(&f).unwrap();
        let b = select_blocks(&f, &eff, d()).unwrap();
        assert_eq!(b.b1, [0]);
        assert_eq!(b.b2, [0, 1, 8]);
        assert_eq!(b.b3, [3]);
        assert_eq!(b.b4, [5]);
        assert_eq!(b.b5, [5, 6]);
        assert_eq!(b.b6, [7]);
        assert_eq!(b.b7, [9]);
        // B1, B2, B3, B5, B6, B7; rows repeat across blocks
        assert_eq!(b.combined_rows(), [0, 0, 1, 8, 3, 5, 6, 7, 9]);
        assert_eq!(history_serials(&f, &b).unwrap(), ["15"]);
    }

    #[test]
    fn labels_need_two_candidates_and_yellow_is_excluded() {
        fn mk<'a>(
            ser: &'a str,
            pal: &'a str,
            kan: &'a str,
            con: &'a str,
            cust: &'a str,
        ) -> [&'a str; 12] {
            row([ser, "SHP", "X", "", pal, "", "PL55", "", kan, con, cust])
        }
        let f = tbl(&[
            mk("1", "1", "", "", "REDACTED"),    // yellow
            mk("2", "2", "", "SC1", "REDACTED"), // 4
            mk("3", "2", "", "kc9", "REDACTED"), // KC*: 2
            mk("4", "2", "9", "", "REDACTED"),   // kanban: 2
            mk("5", "1", "", "", "GM X"),     // AL55: not in B5
            mk("6", "2", "", "", "REDACTED"),    // 4
        ]);
        let eff = effective_plants(&f).unwrap();
        let b = select_blocks(&f, &eff, d()).unwrap();
        let yellow: HashSet<String> = ["1".to_string()].into();
        let c = build_combined(&f, &eff, &b, &yellow, "2026-11-05").unwrap();
        let l = c.table.col(COL_LABELS).unwrap();
        let labels: Vec<&str> = c.table.rows.iter().map(|r| r[l].as_str()).collect();
        assert_eq!(labels, ["2", "4", "2", "2", "4"]);
        assert_eq!((c.rows_with_4, c.label_total, c.yellow_rows), (2, 14, 1));
        // the same table with one candidate less: nobody gets 4
        let yellow2: HashSet<String> = ["1".to_string(), "6".to_string()].into();
        let c = build_combined(&f, &eff, &b, &yellow2, "2026-11-05").unwrap();
        assert!(c.table.rows.iter().all(|r| r[l] == "2"));
        assert_eq!((c.rows_with_4, c.label_total, c.yellow_rows), (0, 10, 2));
        // header order and the yellow column
        assert_eq!(
            c.table.headers[12..],
            [COL_LABELS, COL_EXPORT_DATE, COL_YELLOW]
        );
        assert_eq!(c.table.rows[0][13], "2026-11-05");
        assert_eq!(c.table.rows[0][14], "1");
        assert_eq!(c.table.rows[1][14], "");
    }

    #[test]
    fn yellow_needs_storage_location_11_and_pri() {
        let h = Table::new(
            "history",
            vec![
                "lm_serial_number".into(),
                "storage_location".into(),
                "status".into(),
            ],
            vec![
                vec!["1".into(), "11".into(), "PRI".into()],
                vec!["2".into(), "11".into(), "SHP".into()],
                vec!["3".into(), "3".into(), "PRI".into()],
                vec!["4".into(), "11".into(), "pri".into()],
            ],
        );
        let y = yellow_serials(&h).unwrap();
        assert_eq!(y.len(), 2);
        assert!(y.contains("1") && y.contains("4"));
    }

    #[test]
    fn pivot_is_a_cross_tab_with_totals() {
        let rows: Vec<Vec<String>> = [
            ["b corp", "PL55", "4"],
            ["A corp", "PL33", "2"],
            ["a corp", "PL33", "2"],
            ["A corp", "", "2"],
            ["", "PL33", "2"],
            ["A corp", "PL50", "2"],
        ]
        .iter()
        .map(|r| r.iter().map(|s| s.to_string()).collect())
        .collect();
        let t = Table::new(
            "combined",
            ["customer", "plant", COL_LABELS]
                .iter()
                .map(|s| s.to_string())
                .collect(),
            rows,
        );
        let p = pivot_customer_plant(&t).unwrap();
        assert_eq!(
            p.headers,
            ["customer", "PL33", "PL50", "PL55", "(blank)", "Grand Total"]
        );
        assert_eq!(
            p.rows,
            vec![
                vec!["A corp", "4", "2", "", "2", "8"],
                vec!["b corp", "", "", "4", "", "4"],
                vec!["(blank)", "2", "", "", "", "2"],
                vec!["Grand Total", "6", "2", "4", "2", "14"],
            ]
        );
        assert_eq!(pivot_total(&p).unwrap(), 14);
    }

    #[test]
    fn missing_column_is_named() {
        let mut f = tbl(&[]);
        f.headers[6] = "plnt".into();
        let e = effective_plants(&f).unwrap_err().to_string();
        assert!(e.contains("plant"), "{e}");
    }
}
