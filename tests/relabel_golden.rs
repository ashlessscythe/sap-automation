//! `relabel` stages on the SYNTHETIC stand-in exports (tests/fixtures/relabel/synthetic/, see the
//! README there), loaded through the production loader `abc::raw`, compared with expected tables
//! that were computed from the spec text (5.6 / 9.5) by a one-off script that parses the raw
//! text itself, independently of the code under test.
//!
//! No real relabel export and no macro xlsx were available: these tests pin the rules of spec
//! 9.5, they do not prove equality with the macro output.
mod common;

use common::{assert_tables_equal, load_fixture, load_synthetic_in};
use sap_automation::abc::relabel::{self, *};
use sap_automation::abc::table::Table;
use std::collections::HashSet;

fn run_date() -> chrono::NaiveDate {
    chrono::NaiveDate::from_ymd_opt(2026, 11, 5).unwrap()
}

fn sn_del() -> Table {
    load_synthetic_in("relabel", "zmdesnr", "raw_zmdesnr_del.txt")
}

fn history() -> Table {
    load_synthetic_in("relabel", "zmdesnr", "raw_zmdesnr_hist.txt")
}

fn col(t: &Table, name: &str) -> Vec<String> {
    let c = t.col(name).unwrap();
    t.rows.iter().map(|r| r[c].clone()).collect()
}

struct Stages {
    f: Table,
    eff: Vec<String>,
    blocks: Blocks,
    yellow: HashSet<String>,
    combined: Combined,
}

fn stages() -> Stages {
    let f = sn_del();
    let eff = effective_plants(&f, &RelabelRules::default()).unwrap();
    let blocks = select_blocks(&f, &eff, run_date(), &RelabelRules::default()).unwrap();
    let yellow = yellow_serials(&history()).unwrap();
    let combined = build_combined(
        &f,
        &eff,
        &blocks,
        &yellow,
        "2026-11-05",
        &RelabelRules::default(),
    )
    .unwrap();
    Stages {
        f,
        eff,
        blocks,
        yellow,
        combined,
    }
}

#[test]
fn loader_shapes() {
    let f = sn_del();
    assert_eq!((f.nrows(), f.headers.len()), (44, 20));
    for c in SN_DEL_COLUMNS {
        assert!(f.col(c).is_ok(), "missing {c}");
    }
    // the blank list title is the customer; the other titles sanitise like the macro's
    assert_eq!(f.headers[10], "customer");
    assert_eq!(f.headers[2], "serial_number_2nd_party_verification");
    assert_eq!(f.headers[7], "customer_part_");
    assert_eq!(f.headers[19], "container");
    // leading zeros are stripped by the loader, a spaced `2.000` is `2`
    assert!(col(&f, "lm_serial_number")
        .iter()
        .all(|s| s.is_empty() || !s.starts_with('0')));
    assert!(col(&f, "spv_scan_copies").contains(&"2".to_string()));
    assert!(!col(&f, "spv_scan_copies").iter().any(|v| v.contains('.')));
    let h = history();
    assert_eq!((h.nrows(), h.headers.len()), (7, 20));
    for c in HISTORY_COLUMNS {
        assert!(h.col(c).is_ok(), "missing {c}");
    }
}

#[test]
fn sn_del_is_passed_through() {
    assert_tables_equal(&load_fixture("relabel", "expected_sn_del.tsv"), &sn_del());
}

#[test]
fn al55_is_an_effective_plant_only() {
    let s = stages();
    let n = s
        .eff
        .iter()
        .filter(|p| **p == RelabelRules::default().alias_plant)
        .count();
    assert_eq!(n, 6, "GM, REDACTED, GENERAL (any case) on PL55; not REDACTED");
    // the marks never reach a table: the plant column of F and of combined is untouched
    assert!(!col(&s.f, "plant").contains(&RelabelRules::default().alias_plant));
    assert!(!col(&s.combined.table, "plant").contains(&RelabelRules::default().alias_plant));
    // ... and every AL55 row is still PL55 in F
    let p = s.f.col("plant").unwrap();
    for (i, e) in s.eff.iter().enumerate() {
        if *e == RelabelRules::default().alias_plant {
            assert_eq!(s.f.rows[i][p], "PL55");
        }
    }
}

#[test]
fn block_sizes() {
    let s = stages();
    let b = &s.blocks;
    assert_eq!(
        [
            b.b1.len(),
            b.b2.len(),
            b.b3.len(),
            b.b4.len(),
            b.b5.len(),
            b.b6.len(),
            b.b7.len()
        ],
        [4, 7, 2, 9, 14, 3, 3]
    );
    // every block keeps the row order of F (no sorting) and combined is B1, B2, B3, B5, B6, B7
    for blk in [&b.b1, &b.b2, &b.b3, &b.b4, &b.b5, &b.b6, &b.b7] {
        assert!(blk.windows(2).all(|w| w[0] < w[1]));
    }
    assert_eq!(s.combined.table.nrows(), 4 + 7 + 2 + 14 + 3 + 3);
    assert_eq!(
        b.combined_rows().len(),
        s.combined.table.nrows(),
        "B4 is not appended"
    );
    // rows repeat across blocks (B1 and B2 share the first stand-in row)
    let ser = col(&s.combined.table, "lm_serial_number");
    let distinct: HashSet<&String> = ser.iter().collect();
    assert!(distinct.len() < ser.len());
}

#[test]
fn history_serials_are_the_pl55_pallet_1_rows() {
    let s = stages();
    let serials = history_serials(&s.f, &s.blocks).unwrap();
    assert_eq!(serials.len(), 9);
    let (ser, pal, plant) = (
        s.f.col("lm_serial_number").unwrap(),
        s.f.col("pallet").unwrap(),
        s.f.col("plant").unwrap(),
    );
    for v in &serials {
        let r = s.f.rows.iter().find(|r| &r[ser] == v).unwrap();
        assert_eq!((r[pal].as_str(), r[plant].as_str()), ("1", "PL55"));
    }
    // AL55 rows with pallet 1 are not sent
    let gm_pallet1 = s
        .eff
        .iter()
        .zip(&s.f.rows)
        .filter(|(e, r)| **e == RelabelRules::default().alias_plant && r[pal] == "1")
        .count();
    assert_eq!(gm_pallet1, 2);
}

#[test]
fn yellow_follows_storage_location_11_and_pri() {
    let s = stages();
    // 7 history rows: 4 are PRI at location 11 (`0011` and `11` both count); SHP/11 and PRI/3 do not
    assert_eq!(s.yellow.len(), 4);
    assert_eq!(s.combined.yellow_rows, 4);
    let t = &s.combined.table;
    let (ser, yc, lab) = (
        t.col("lm_serial_number").unwrap(),
        t.col("yellow").unwrap(),
        t.col("_of_labels").unwrap(),
    );
    for r in &t.rows {
        assert_eq!(
            r[yc] == "1",
            s.yellow.contains(&r[ser]),
            "serial {}",
            r[ser]
        );
        if r[yc] == "1" {
            assert_eq!(r[lab], "2", "a yellow row never gets 4 labels");
        } else {
            assert_eq!(r[yc], "");
        }
    }
}

#[test]
fn combined_equals_the_expected_table() {
    let s = stages();
    assert_tables_equal(
        &load_fixture("relabel", "expected_combined.tsv"),
        &s.combined.table,
    );
    assert_eq!(
        s.combined.table.headers[20..],
        ["_of_labels", "export_date", "yellow"]
    );
    assert!(col(&s.combined.table, "export_date")
        .iter()
        .all(|d| d == "2026-11-05"));
}

#[test]
fn labels_follow_the_four_two_rule() {
    let s = stages();
    assert_eq!(s.combined.rows_with_4, 8);
    assert_eq!(s.combined.label_total, 8 * 4 + 25 * 2);
    let t = &s.combined.table;
    let c = |n: &str| t.col(n).unwrap();
    let mut fours = 0;
    for r in &t.rows {
        let four = r[c("plant")] == "PL55"
            && r[c("kanban_number")].is_empty()
            && !r[c("container")].to_uppercase().starts_with("KC")
            && ["1", "2"].contains(&r[c("pallet")].as_str())
            && r[c("yellow")].is_empty()
            // alias-pattern customers are PL55 in the table but were AL55 for the rule
            && !["ALIAS_A", "ALIAS_B", "ALIAS_C"]
                .iter()
                .any(|p| r[c("customer")].to_uppercase().starts_with(p));
        assert_eq!(r[c("_of_labels")], if four { "4" } else { "2" }, "{r:?}");
        fours += usize::from(four);
    }
    assert_eq!(fours, 8);
}

#[test]
fn fewer_than_two_candidates_means_two_labels_everywhere() {
    let s = stages();
    // mark all but one of the 4-label serials yellow
    let t = &s.combined.table;
    let (ser, lab) = (
        t.col("lm_serial_number").unwrap(),
        t.col("_of_labels").unwrap(),
    );
    let fours: Vec<String> = t
        .rows
        .iter()
        .filter(|r| r[lab] == "4")
        .map(|r| r[ser].clone())
        .collect();
    assert_eq!(fours.len(), 8);
    let mut yellow = s.yellow.clone();
    yellow.extend(fours[1..].iter().cloned());
    let c = build_combined(
        &s.f,
        &s.eff,
        &s.blocks,
        &yellow,
        "2026-11-05",
        &RelabelRules::default(),
    )
    .unwrap();
    assert_eq!(c.rows_with_4, 0);
    assert!(col(&c.table, "_of_labels").iter().all(|v| v == "2"));
    assert_eq!(c.label_total as usize, c.table.nrows() * 2);
    // with exactly two candidates left, both get 4
    let mut yellow = s.yellow.clone();
    yellow.extend(fours[2..].iter().cloned());
    let c = build_combined(
        &s.f,
        &s.eff,
        &s.blocks,
        &yellow,
        "2026-11-05",
        &RelabelRules::default(),
    )
    .unwrap();
    assert_eq!(c.rows_with_4, 2);
}

#[test]
fn no_history_means_no_yellow_and_more_fours() {
    let s = stages();
    let c = build_combined(
        &s.f,
        &s.eff,
        &s.blocks,
        &HashSet::new(),
        "2026-11-05",
        &RelabelRules::default(),
    )
    .unwrap();
    assert_eq!(c.yellow_rows, 0);
    // the 3 yellow pallet-1 rows without kanban now qualify (the 4th has a kanban number)
    assert_eq!(c.rows_with_4, 8 + 3);
}

#[test]
fn pivot_equals_the_expected_table() {
    let s = stages();
    let p = pivot_customer_plant(&s.combined.table).unwrap();
    assert_eq!(p.headers[0], "customer");
    assert_eq!(p.headers.last().unwrap(), "Grand Total");
    assert_eq!(p.rows.last().unwrap()[0], "Grand Total");
    assert_tables_equal(&load_fixture("relabel", "expected_pivot.tsv"), &p);
    assert_eq!(pivot_total(&p).unwrap(), i64::from(s.combined.label_total));
    // `(blank)` customer sorts last, just above the Grand Total row
    assert_eq!(p.rows[p.nrows() - 2][0], "(blank)");
    assert!(col(&p, "customer").iter().all(|c| c != "AL55"));
}

#[test]
fn window_is_the_run_date_and_the_next_day() {
    let (a, b) = relabel::window(run_date());
    assert_eq!(a.to_string(), "2026-11-05");
    assert_eq!(b.to_string(), "2026-11-06");
    // another run date moves the window: only the 11-05 rows of the stand-in fall out
    let f = sn_del();
    let eff = effective_plants(&f, &RelabelRules::default()).unwrap();
    let nov4 = chrono::NaiveDate::from_ymd_opt(2026, 11, 4).unwrap();
    let b = select_blocks(&f, &eff, nov4, &RelabelRules::default()).unwrap();
    // window 11-04..11-05: the 11-06 rows drop out, the 11-04 row comes in
    assert_ne!(b.b2.len(), 7);
    assert_eq!(b.b3.len(), 2, "B3 has no date window");
}

#[test]
fn a_missing_column_is_named() {
    let mut f = sn_del();
    let i = f.col("kanban_number").unwrap();
    f.headers[i] = "kanban".into();
    let eff = effective_plants(&f, &RelabelRules::default()).unwrap();
    let e = select_blocks(&f, &eff, run_date(), &RelabelRules::default())
        .unwrap_err()
        .to_string();
    assert!(e.contains("kanban_number"), "{e}");
}
