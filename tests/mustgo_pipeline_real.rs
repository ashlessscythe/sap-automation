//! `mustgo` stages on the REAL SAP text exports (tests/fixtures/mustgo/real/), loaded through
//! the production loader `abc::raw`. Same expectations as the xlsx-derived pipeline tests.
mod common;

use chrono::NaiveDate;
use common::{assert_tables_equal, diff_tables, load_fixture, load_raw_fixture, load_real};
use sap_automation::abc::mustgo::*;
use sap_automation::abc::table::Table;
use std::collections::HashSet;

fn set(t: &Table, col: &str) -> HashSet<String> {
    let c = t.col(col).unwrap();
    t.rows.iter().map(|r| r[c].clone()).collect()
}

fn pairs(t: &Table, a: &str, b: &str) -> HashSet<(String, String)> {
    let (a, b) = (t.col(a).unwrap(), t.col(b).unwrap());
    t.rows
        .iter()
        .map(|r| (r[a].clone(), r[b].clone()))
        .collect()
}

fn sn_del() -> Table {
    load_real("zmdesnr", "raw_zmdesnr_del.txt")
}

fn lt23_result() -> Table {
    let mut lt = load_real("lt23", "raw_lt23.txt");
    lt23_postprocess(&mut lt).unwrap();
    lt
}

fn run_date() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 11, 5).unwrap()
}

#[test]
fn loader_shapes() {
    let vt11 = load_real("vt11", "raw_vt11.txt");
    assert_eq!((vt11.nrows(), vt11.headers.len()), (91, 17));
    let vl = load_real("vl06o", "raw_vl06o.txt");
    assert_eq!((vl.nrows(), vl.headers.len()), (834, 10));
    assert_eq!((sn_del().nrows(), sn_del().headers.len()), (5796, 19));
    let lt = load_real("lt23", "raw_lt23.txt");
    assert_eq!((lt.nrows(), lt.headers.len()), (1071, 11));
    let to = load_real("zmdesnr", "raw_zmdesnr_del_to.txt");
    assert_eq!((to.nrows(), to.headers.len()), (476, 19));
}

/// Real exports equal the xlsx-derived fixtures cell by cell after loading (vl06o, zmdesnr x2,
/// lt23). Only the VT11 column set differs (different list layout), see the next test.
#[test]
fn real_equals_xlsx_derived_except_vt11() {
    for (tc, real, xl) in [
        ("vl06o", "raw_vl06o.txt", "raw_vl06o.tsv"),
        ("zmdesnr", "raw_zmdesnr_del.txt", "raw_zmdesnr_del.tsv"),
        ("lt23", "raw_lt23.txt", "raw_lt23.tsv"),
        (
            "zmdesnr",
            "raw_zmdesnr_del_to.txt",
            "raw_zmdesnr_del_to.tsv",
        ),
    ] {
        let r = load_real(tc, real);
        let x = load_raw_fixture("mustgo", xl);
        if let Err(rep) = diff_tables(&x, &r, 5) {
            panic!("{real} differs from {xl}:\n{rep}");
        }
    }
}

#[test]
fn vt11_real_vs_xlsx_shared_columns() {
    let r = load_real("vt11", "raw_vt11.txt");
    let x = load_raw_fixture("mustgo", "raw_vt11.tsv");
    for col in [
        "shipment_number",
        "shipment_type",
        "transportplanningpt",
        "shipment_route",
        "trailer_number",
        "seal_number_1",
        "service_agent",
    ] {
        let (a, b) = (r.col(col).unwrap(), x.col(col).unwrap());
        let ra: Vec<&str> = r.rows.iter().map(|row| row[a].as_str()).collect();
        let xb: Vec<&str> = x.rows.iter().map(|row| row[b].as_str()).collect();
        assert_eq!(ra, xb, "column {col}");
    }
    // The real text export truncates these free-text columns (14 characters, list width);
    // the xlsx export has the full text, so the real value is a prefix of the xlsx one.
    for col in ["seal_number_2", "descripof_shipment"] {
        let (a, b) = (r.col(col).unwrap(), x.col(col).unwrap());
        for (rr, xr) in r.rows.iter().zip(&x.rows) {
            assert!(
                xr[b].starts_with(&rr[a]),
                "{col}: {:?} vs {:?}",
                rr[a],
                xr[b]
            );
        }
    }
}

#[test]
fn stages_on_real_data_match_spec_9_3() {
    // a, b
    let mut vt11 = load_real("vt11", "raw_vt11.txt");
    assert_eq!(select_tpdc_shipments(&mut vt11).unwrap().len(), 54);
    let vl = load_real("vl06o", "raw_vl06o.txt");
    assert_eq!(delivery_list(&vl).unwrap().len(), 107);
    // c
    let mut sn = sn_del();
    filter_sn_del(&mut sn).unwrap();
    assert_eq!(sn.visible_count(), 1011);
    let tos = transfer_order_list(&sn).unwrap();
    assert_eq!(tos.len(), 87);
    let lt_raw = load_real("lt23", "raw_lt23.txt");
    assert_eq!(
        tos.into_iter().collect::<HashSet<_>>(),
        set(&lt_raw, "transfer_order_number")
    );
    // d
    let lt = lt23_result();
    assert_eq!(lt.nrows(), 15);
    assert_tables_equal(&load_fixture("mustgo", "expected_lt23.tsv"), &lt);
    // e
    mark_sn_del_by_lt23(&mut sn, &lt).unwrap();
    assert_eq!(sn.nrows(), 128);
    assert_eq!(set(&sn, "delivery").len(), 10);
    let sn_to = load_real("zmdesnr", "raw_zmdesnr_del_to.txt");
    assert_eq!(sn_to.nrows(), 476);
    assert_eq!(
        pairs(&sn, "delivery", "transfer_order_number"),
        pairs(&sn_to, "delivery", "transfer_order_number")
    );
    let orig = sn_del();
    let (d, t) = (
        orig.col("delivery").unwrap(),
        orig.col("transfer_order_number").unwrap(),
    );
    let (dels, tos) = (set(&sn, "delivery"), set(&sn, "transfer_order_number"));
    let n = orig
        .rows
        .iter()
        .filter(|r| dels.contains(&r[d]) && tos.contains(&r[t]))
        .count();
    assert_eq!(n, 476);
}

#[test]
fn outputs_on_real_data_match_macro_output() {
    let sn_to = load_real("zmdesnr", "raw_zmdesnr_del_to.txt");
    let snr = snr_copy(&sn_to, run_date()).unwrap();
    assert_eq!(snr.nrows(), 92);
    assert_tables_equal(&load_fixture("mustgo", "expected_snr_copy.tsv"), &snr);
    let pivot = pivot_plant_by_date(&snr).unwrap();
    let got: Vec<(&str, &str)> = pivot
        .rows
        .iter()
        .map(|r| (r[0].as_str(), r[2].as_str()))
        .collect();
    assert_eq!(
        got,
        vec![
            ("PL52", "2"),
            ("PL55", "30"),
            ("PL56", "46"),
            ("PL59", "6"),
            ("PL61", "5"),
            ("PL65", "3")
        ]
    );
}
