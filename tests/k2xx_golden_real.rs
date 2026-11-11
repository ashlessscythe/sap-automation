//! `k2xx` stages on the REAL SAP text exports of 2026-11-05 (tests/fixtures/k2xx/real/, byte-exact),
//! loaded through the production loader and compared with the macro output `11-05-2026_K2XX.xlsx`
//! converted to TSV (tests/fixtures/k2xx/expected_real_*.tsv). There is no real ZVT11 export, so
//! that step is not covered (expected_real_zvt11.tsv is kept for the day one arrives).
mod common;

use common::{diff_tables, load_fixture, load_real_in};
use sap_automation::abc::{inv, k2xx};

const RUN_DATE: &str = "2026-11-05";

fn merged() -> sap_automation::abc::table::Table {
    let mut vl = load_real_in("k2xx", "vl06o", "raw_vl06o.txt");
    k2xx::prepare_vl06o(&mut vl, RUN_DATE).unwrap();
    let z = load_real_in("k2xx", "zmdetpc", "raw_zmdetpc.txt");
    k2xx::merge_zmdetpc(&vl, &z).unwrap()
}

#[test]
fn vt11_loads_with_the_scorecard_headers() {
    let vt = load_real_in("k2xx", "vt11_inv", "raw_vt11.txt");
    let x = load_fixture("k2xx", "expected_real_vt11.tsv");
    assert_eq!((vt.nrows(), vt.headers.len()), (53, 13));
    assert_eq!(vt.headers, x.headers);
    // the list truncates the free-text columns to the list width: ours is a prefix of the xlsx
    let free = [
        "seal_number_2",
        "descripof_shipment",
        "forwarding_agent_tracking_id",
    ];
    for (i, h) in vt.headers.iter().enumerate() {
        for (o, e) in vt.rows.iter().zip(&x.rows) {
            let e = e[i].trim_end();
            if free.contains(&h.as_str()) {
                assert!(e.starts_with(&o[i]), "{h}: {:?} vs {:?}", o[i], e);
            } else {
                assert_eq!(o[i], e, "{h}");
            }
        }
    }
}

#[test]
fn vl06o_merged_matches_the_xlsx_sheet() {
    let m = merged();
    let x = load_fixture("k2xx", "expected_real_vl06o.tsv");
    assert_eq!((m.nrows(), m.headers.len()), (107, 13));
    diff_tables(&x, &m, 5).unwrap();
}

#[test]
fn pivot_sums_to_the_sheet_total() {
    let m = merged();
    let p = k2xx::pivot_plant_total(&m).unwrap();
    let x = load_fixture("k2xx", "expected_real_vl06o.tsv");
    let (pc, tc) = (x.col("plant").unwrap(), x.col("total").unwrap());
    let want: f64 = x
        .rows
        .iter()
        .filter_map(|r| r[tc].parse::<f64>().ok())
        .sum();
    assert_eq!(k2xx::total_sum(&m).unwrap(), want);
    let got: f64 = p.rows.iter().map(|r| r[1].parse::<f64>().unwrap()).sum();
    assert_eq!(got, want);
    assert!(p.nrows() >= 1 && x.rows.iter().all(|r| !r[pc].is_empty()));
    let _ = inv::delivery_key("1");
}

/// The real ZVT11 Closed export (window 2026-07-28..2027-02-13, layout `add_del`, variant
/// `k2xx_`). EXPECTED = the macro sheet `K2XX_ZVT11` WITH SPEC 7.0 ITEM 30 APPLIED (owner decision
/// 2026-11-11, a deliberate deviation from the macro): `date` = `shipment_end_date` on every row.
/// The raw export (and the macro sheet) have 4 rows with a blank `Date` (PL63 shipment 24625349)
/// and 78 rows with `Date` 2026-11-06 although `shipment_end_date` is 2026-11-05.
#[test]
fn zvt11_matches_the_xlsx_sheet_with_item_30_applied() {
    let mut z = load_real_in("k2xx", "zvt11", "raw_zvt11.txt");
    let x = load_fixture("k2xx", "expected_real_zvt11.tsv");
    assert_eq!((z.nrows(), z.headers.len()), (409, 7));
    assert_eq!(z.headers, k2xx::ZVT11_HEADERS);
    let (d, e) = (z.col("date").unwrap(), z.col("shipment_end_date").unwrap());
    assert_eq!(z.rows.iter().filter(|r| r[d].is_empty()).count(), 4);
    assert_eq!(z.rows.iter().filter(|r| r[d] != r[e]).count(), 82);
    k2xx::apply_zvt11_end_date(&mut z).unwrap();
    assert!(z.rows.iter().all(|r| r[d] == r[e] && !r[d].is_empty()));
    diff_tables(&x, &z, 5).unwrap();
}
