//! Stage-by-stage golden tests of the `mustgo` post-processing (spec 9.3).
//!
//! Inputs are the raw-ish fixtures passed through `Table::from_raw` (header sanitising, blank
//! header -> `customer`, leading zeros stripped, decision 25). Expected tables come from the
//! macro output workbook and are compared as is (they already hold normalised numbers).
mod common;

use chrono::NaiveDate;
use common::{assert_tables_equal, load_fixture, load_raw_fixture};
use sap_automation::abc::mustgo::*;
use sap_automation::abc::registry::MUSTGO;
use sap_automation::abc::run::RunDate;
use sap_automation::abc::table::Table;
use std::collections::HashSet;

const DIR: &str = "mustgo";

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

fn filtered_sn_del() -> Table {
    let mut sn = load_raw_fixture(DIR, "raw_zmdesnr_del.tsv");
    filter_sn_del(&mut sn).unwrap();
    sn
}

fn lt23_result() -> Table {
    let mut lt = load_raw_fixture(DIR, "raw_lt23.tsv");
    lt23_postprocess(&mut lt).unwrap();
    lt
}

fn run_date() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 11, 5).unwrap()
}

#[test]
fn stage_a_b_vt11_and_vl06o_lists() {
    let mut vt11 = load_raw_fixture(DIR, "raw_vt11.tsv");
    // blank first header became `customer`, columns matched by name
    assert_eq!(vt11.headers[0], "customer");
    let ships = select_tpdc_shipments(&mut vt11).unwrap();
    assert_eq!(ships.len(), 54);
    assert_eq!(vt11.visible_count(), 54);
    assert_eq!(vt11.nrows(), 91, "selection is non-destructive");
    let vl = load_raw_fixture(DIR, "raw_vl06o.tsv");
    assert_eq!(vl.nrows(), 834);
    assert_eq!(delivery_list(&vl).unwrap().len(), 107);
}

#[test]
fn stage_c_filter_sn_del() {
    let raw = load_raw_fixture(DIR, "raw_zmdesnr_del.tsv");
    assert_eq!(raw.nrows(), 5796);
    let sn = filtered_sn_del();
    assert_eq!(sn.visible_count(), 1011);
    let tos = transfer_order_list(&sn).unwrap();
    assert_eq!(tos.len(), 87);
    // the 87 TOs equal the TOs of the LT23 export
    let lt23 = load_raw_fixture(DIR, "raw_lt23.tsv");
    let want = set(&lt23, "transfer_order_number");
    assert_eq!(tos.into_iter().collect::<HashSet<_>>(), want);
}

#[test]
fn stage_d_lt23_matches_macro_output() {
    let lt = lt23_result();
    let expected = load_fixture(DIR, "expected_lt23.tsv");
    assert_eq!(lt.nrows(), 15);
    assert_eq!(lt.headers.len(), 13);
    assert_tables_equal(&expected, &lt);
}

#[test]
fn stage_e_lookup_and_delete() {
    let lt = lt23_result();
    let mut sn = filtered_sn_del();
    mark_sn_del_by_lt23(&mut sn, &lt).unwrap();
    // The filtered/deduped sn_del keeps 128 rows (10 deliveries, the 15 TOs) ...
    assert_eq!(sn.nrows(), 128);
    assert_eq!(set(&sn, "delivery").len(), 10);
    assert_eq!(set(&sn, "transfer_order_number").len(), 15);
    // ... and these are exactly the (delivery, TO) pairs SAP returned for the follow-up query.
    let sn_to = load_raw_fixture(DIR, "raw_zmdesnr_del_to.tsv");
    assert_eq!(
        pairs(&sn, "delivery", "transfer_order_number"),
        pairs(&sn_to, "delivery", "transfer_order_number")
    );
    // The 476 of spec 9.3 are the rows of the UNFILTERED sn_del (the `snr_orig` copy) whose
    // delivery and TO are in those lists.
    let orig = load_raw_fixture(DIR, "raw_zmdesnr_del.tsv");
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
    assert_eq!(sn_to.nrows(), 476);
}

#[test]
fn stage_f_snr_copy_matches_macro_output() {
    let sn_to = load_raw_fixture(DIR, "raw_zmdesnr_del_to.tsv");
    let snr = snr_copy(&sn_to, run_date()).unwrap();
    assert_eq!(snr.nrows(), 92);
    assert_eq!(snr.headers[18], "week");
    assert_tables_equal(&load_fixture(DIR, "expected_snr_copy.tsv"), &snr);
}

#[test]
fn stage_g_pivot_counts() {
    let sn_to = load_raw_fixture(DIR, "raw_zmdesnr_del_to.tsv");
    let pivot = pivot_plant_by_date(&snr_copy(&sn_to, run_date()).unwrap()).unwrap();
    let got: Vec<(String, String, String)> = pivot
        .rows
        .iter()
        .map(|r| (r[0].clone(), r[1].clone(), r[2].clone()))
        .collect();
    // Expected values from the rendered PIVOT sheet (rows between header and Grand Total).
    let exp = load_fixture(DIR, "expected_pivot.tsv");
    let want: Vec<(String, String, String)> = exp
        .rows
        .iter()
        .filter(|r| r[1] != "Grand Total" && r[2].parse::<usize>().is_ok())
        .map(|r| (r[1].clone(), "2026-11-05".to_string(), r[2].clone()))
        .collect();
    assert_eq!(want.len(), 6);
    assert_eq!(got, want);
    let total: usize = got.iter().map(|g| g.2.parse::<usize>().unwrap()).sum();
    assert_eq!(total, 92);
}

#[test]
fn stage_h_write_outputs_roundtrip() {
    let lt = lt23_result();
    let sn_to = load_raw_fixture(DIR, "raw_zmdesnr_del_to.tsv");
    let snr = snr_copy(&sn_to, run_date()).unwrap();
    let pivot = pivot_plant_by_date(&snr).unwrap();
    let dir = std::env::temp_dir().join(format!("mustgo_pipeline_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let rd = RunDate {
        date1: run_date(),
        date2: None,
    };
    let today = NaiveDate::from_ymd_opt(2026, 11, 9).unwrap();
    let paths = write_outputs(&dir, today, &rd, &lt, &snr, &pivot).unwrap();
    assert_eq!(paths.len(), 3);
    let _ = &MUSTGO;
    let back = Table::from_tsv("snr", &std::fs::read_to_string(&paths[1]).unwrap()).unwrap();
    assert_tables_equal(&snr, &back);
    assert!(paths[0].ends_with("11-05-2026_MUSTGO_LT23.tsv"));
    let _ = std::fs::remove_dir_all(&dir);
}
