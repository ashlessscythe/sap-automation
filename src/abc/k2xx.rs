//! Pure post-processing stages of the `k2xx` report (spec 5.5, verified in 9.5 against the run
//! of 2026-11-05).
//!
//! Same rules as `mustgo.rs` / `inv.rs`: tables with sanitised headers in, tables out, columns
//! matched by name, no SAP access. The calls that feed these stages (VT11, VL06O, ZVT11 Closed,
//! ZMDETPC) live in the seam of [`super::k2xx_runner`].
//!
//! What the report does to its tables (spec 9.5):
//! * `vl06o`: one row per delivery (**first row kept**), a `date` column (run date), then the
//!   ZMDETPC merge of spec 4.12 **without** the xdock/toyota rules: sorted ascending by
//!   delivery, columns `..., date, delivery_, pallet, child, total`, `total = pallet + child`;
//!   a delivery without a ZMDETPC row keeps `delivery_`, `pallet`, `child` and `total` blank.
//! * `vt11` and `zvt11` are written as exported (`zvt11` is the unformatted export).
//! * `pivot`: plant rows, sum of `total` over the merged `vl06o` table, in the shape of the INV
//!   pivots ([`inv::pivot_sum`]: long form, ascending, `(blank)` plant last, no grand-total
//!   row). **Owner decision of 2026-11-10; it overrides spec 7.0 item 1** ("k2xx produces no
//!   pivot"): the macro workbook has no such sheet, so there is nothing to compare it with.
//!
//! Assumptions that are NOT confirmed by data (no raw k2xx export was available when this was
//! written; see "Design notes: `k2xx`" in `docs/abc_headless_plan.md`):
//! * the owner's INV `total` decision (fallback to `number_of_packages`, `0 -> 1`) is **not**
//!   applied here, because spec 9.5 shows the macro output with a blank `total`;
//! * the ZVT11 window of a range run is `start - 100` .. `end + 100` (the spec only defines
//!   `date1 +- 100` for a single date).
#![allow(dead_code)]

use super::inv::{self, MergeRules};
use super::run::RunDate;
use super::table::Table;
use anyhow::Result;
use chrono::NaiveDate;

/// Days on either side of the run date for the ZVT11 Closed selection (spec 3.2 / 5.5 row 55).
pub const ZVT11_WINDOW_DAYS: i64 = crate::zvt11::CLOSED_WINDOW_DAYS;

/// VL06O prepare (spec 5.5 rows 53-54 and the dedup of 9.5): one row per delivery (first kept)
/// and a `date` column holding `date_text` on every row.
pub fn prepare_vl06o(vl06o: &mut Table, date_text: &str) -> Result<()> {
    inv::prepare_vl06o(vl06o, date_text)
}

/// Spec 5.5 row 56: merge ZMDETPC into `vl06o` with the plain rules ([`MergeRules::Plain`]).
pub fn merge_zmdetpc(vl06o: &Table, zmdetpc: &Table) -> Result<Table> {
    inv::merge_zmdetpc_with(vl06o, zmdetpc, MergeRules::Plain)
}

/// `(from, to)` of the ZVT11 Closed selection: the run date range widened by `days` on both
/// sides (negative counts as 0). The delivery list is the real filter, the dates only have to
/// be wide enough.
pub fn zvt11_window(date: &RunDate, days: i64) -> (NaiveDate, NaiveDate) {
    let (start, end) = date.as_range();
    crate::zvt11::closed_window(start, end, days)
}

/// Canonical headers of the ZVT11 Closed list (layout `add_del`), for the header-only output of
/// an empty list.
pub const ZVT11_HEADERS: [&str; 7] = [
    "date",
    "trailer_number",
    "shipment_number",
    "delivery",
    "plant",
    "ship_tofrom_name",
    "shipment_end_date",
];

/// Header-only ZVT11 table (owner decision: an empty ZVT11 list does not stop `k2xx`).
pub fn empty_zvt11() -> Table {
    Table::new(
        "zvt11",
        ZVT11_HEADERS.iter().map(|s| s.to_string()).collect(),
        Vec::new(),
    )
}

/// Spec 7.0 item 30: `date` = `shipment_end_date` on every row. **Deliberately differs from the
/// macro sheet** (and from the raw export): there 4 rows have a blank `Date` and 78 rows a
/// `Date` one day after the shipment end.
pub fn apply_zvt11_end_date(zvt11: &mut Table) -> Result<()> {
    let (d, e) = (zvt11.col("date")?, zvt11.col("shipment_end_date")?);
    for r in &mut zvt11.rows {
        r[d] = r[e].clone();
    }
    Ok(())
}

/// Pivot of the merged table (owner decision, departs from spec 7.0 item 1): long table
/// `plant`, `total` with the sum of the numeric `total` cells per plant; a blank `total` (no
/// ZMDETPC row) adds nothing, a plant with only such rows shows `0`. Conventions of
/// [`inv::pivot_sum`]: ascending, `(blank)` plant last, no grand-total row.
pub fn pivot_plant_total(merged: &Table) -> Result<Table> {
    inv::pivot_sum(merged, "plant", "total")
}

/// Sum of the numeric `total` cells of the merged table (blank cells add nothing), for the
/// run summary.
pub fn total_sum(merged: &Table) -> Result<f64> {
    let c = merged.col("total")?;
    Ok(merged
        .rows
        .iter()
        .filter_map(|r| r[c].trim().replace(',', "").parse::<f64>().ok())
        .sum())
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

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn prepare_then_merge_matches_spec_9_5_shape() {
        let mut v = t(
            "vl06o",
            &["delivery", "plant", "number_of_packages"],
            &[
                &["85650002", "FV55", "4"],
                &["85650002", "", "4"],
                &["8012734818", "FW61", "7"],
                &["85650001", "FV33", "0"],
            ],
        );
        prepare_vl06o(&mut v, "2026-11-05").unwrap();
        assert_eq!(v.nrows(), 3, "first row per delivery");
        let z = t(
            "zmdetpc",
            &["delivery", "pallet", "child"],
            &[&["0085650001", "0", "0"], &["0085650002", "1", "2"]],
        );
        let m = merge_zmdetpc(&v, &z).unwrap();
        assert_eq!(
            m.headers,
            vec![
                "delivery",
                "plant",
                "number_of_packages",
                "date",
                "delivery_",
                "pallet",
                "child",
                "total"
            ]
        );
        assert_eq!(
            m.rows,
            vec![
                // pallet + child = 0 stays 0: no packages fallback, no `0 -> 1`
                vec![
                    "85650001",
                    "FV33",
                    "0",
                    "2026-11-05",
                    "0085650001",
                    "0",
                    "0",
                    "0"
                ],
                vec![
                    "85650002",
                    "FV55",
                    "4",
                    "2026-11-05",
                    "0085650002",
                    "1",
                    "2",
                    "3"
                ],
                // the 10-digit delivery sorts last and has no ZMDETPC row: all four blank
                vec!["8012734818", "FW61", "7", "2026-11-05", "", "", "", ""],
            ]
        );
        assert_eq!(total_sum(&m).unwrap(), 3.0);
        // every plant of the merged table gets a row, also with a blank or zero total
        let p = pivot_plant_total(&m).unwrap();
        assert_eq!(p.headers, ["plant", "total"]);
        assert_eq!(
            p.rows,
            vec![vec!["FV33", "0"], vec!["FV55", "3"], vec!["FW61", "0"]]
        );
    }

    #[test]
    fn merge_does_not_need_number_of_packages() {
        let v = t("vl06o", &["delivery"], &[&["1"]]);
        let z = t("z", &["delivery", "pallet", "child"], &[&["1", "2", "3"]]);
        assert_eq!(
            merge_zmdetpc(&v, &z).unwrap().rows[0],
            ["1", "1", "2", "3", "5"]
        );
        assert!(merge_zmdetpc(&v, &t("z", &["delivery", "pallet"], &[])).is_err());
        // the pivot needs `plant` (matched by name)
        let m = merge_zmdetpc(&v, &z).unwrap();
        assert!(pivot_plant_total(&m).is_err());
    }

    #[test]
    fn zvt11_window_single_date_and_range() {
        let single = RunDate {
            date1: d(2026, 11, 5),
            date2: None,
        };
        assert_eq!(
            zvt11_window(&single, ZVT11_WINDOW_DAYS),
            (d(2026, 7, 28), d(2027, 2, 13))
        );
        let week = RunDate {
            date1: d(2026, 11, 1),
            date2: Some(d(2026, 11, 7)),
        };
        assert_eq!(
            zvt11_window(&week, ZVT11_WINDOW_DAYS),
            (d(2026, 7, 24), d(2027, 2, 15))
        );
        assert_eq!(zvt11_window(&single, -3), (d(2026, 11, 5), d(2026, 11, 5)));
    }
}
