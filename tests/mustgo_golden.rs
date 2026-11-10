//! Golden-test harness for the `mustgo` flow (spec 9.3, run date 2026-11-05).
//! Fixtures are TSV conversions of resources/mustgo.zip under tests/fixtures/mustgo/.

mod common;

use common::{assert_tables_equal, diff_tables, load_fixture};
use sap_automation::abc::table::Table;
use std::collections::HashSet;

const DIR: &str = "mustgo";

fn distinct(t: &Table, header: &str) -> usize {
    let c = t.headers.iter().position(|h| h == header).expect(header);
    t.rows
        .iter()
        .map(|r| r[c].as_str())
        .filter(|v| !v.is_empty())
        .collect::<HashSet<_>>()
        .len()
}

fn shape(t: &Table) -> (usize, usize) {
    (t.nrows(), t.headers.len())
}

#[test]
fn fixture_vt11() {
    let t = load_fixture(DIR, "raw_vt11.tsv");
    assert_eq!(shape(&t), (91, 18));
    let c = t
        .headers
        .iter()
        .position(|h| h == "TransportPlanningPt")
        .unwrap();
    let tpdc = t.rows.iter().filter(|r| r[c] == "TPDC").count();
    assert_eq!(tpdc, 54, "TPDC shipments");
}

#[test]
fn fixture_vl06o() {
    let t = load_fixture(DIR, "raw_vl06o.tsv");
    assert_eq!(shape(&t), (834, 10));
    assert_eq!(distinct(&t, "Delivery"), 107);
}

#[test]
fn fixture_zmdesnr_by_deliveries() {
    let t = load_fixture(DIR, "raw_zmdesnr_del.tsv");
    assert_eq!(shape(&t), (5796, 19));
    let c = t.headers.iter().position(|h| h == "Status").unwrap();
    let n = |s: &str| t.rows.iter().filter(|r| r[c] == s).count();
    assert_eq!((n("SHP"), n("PAL"), n("SPV")), (5546, 228, 22));
}

#[test]
fn fixture_lt23() {
    let t = load_fixture(DIR, "raw_lt23.tsv");
    assert_eq!(shape(&t), (1071, 11));
    assert_eq!(distinct(&t, "Transfer Order Number"), 87);
}

#[test]
fn fixture_zmdesnr_del_to() {
    let t = load_fixture(DIR, "raw_zmdesnr_del_to.tsv");
    assert_eq!(shape(&t), (476, 19));
}

#[test]
fn fixture_expected_outputs() {
    let lt23 = load_fixture(DIR, "expected_lt23.tsv");
    assert_eq!(lt23.nrows(), 15);
    let snr = load_fixture(DIR, "expected_snr_copy.tsv");
    assert_eq!(snr.nrows(), 92);
    assert_eq!(snr.headers[18], "week");
    let sn_to = load_fixture(DIR, "expected_sn_to.tsv");
    assert_eq!(sn_to.nrows(), 476);
    let pivot = load_fixture(DIR, "expected_pivot.tsv");
    assert_eq!(pivot.nrows(), 9);
}

#[test]
fn compare_helper_equal_and_unequal() {
    let a = load_fixture(DIR, "expected_lt23.tsv");
    let b = load_fixture(DIR, "expected_lt23.tsv");
    assert_tables_equal(&a, &b);

    let mut c = b.clone();
    c.rows[2][1] = "X".to_string();
    c.rows.pop();
    let rep = diff_tables(&a, &c, 5).unwrap_err();
    assert!(
        rep.contains("row count differs: expected 15, actual 14"),
        "{rep}"
    );
    assert!(rep.contains("col 1 'delivery'"), "{rep}");
    assert!(rep.contains("1 differing row(s)"), "{rep}");
}
