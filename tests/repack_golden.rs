//! Edge cases of the `repack` stages on the SYNTHETIC exports in `tests/fixtures/repack/synthetic/`
//! (see the README there): the 50 kg boundary, KC/SC deletion, the label plant with and without
//! pallet, alias customers, blank plants, PAL rows, the minimum of two yellow rows. The expected
//! tables come from an independent implementation of spec 9.5 (a one-off script).
mod common;

use common::{assert_tables_equal, load_fixture, load_synthetic_in};
use sap_automation::abc::relabel::{self, effective_plants, RelabelRules};
use sap_automation::abc::repack::{self, *};
use sap_automation::abc::table::Table;

fn del() -> Table {
    load_synthetic_in("repack", "zmdesnr", "raw_zmdesnr_del.txt")
}

fn hist() -> Table {
    load_synthetic_in("repack", "zmdesnr", "raw_zmdesnr_hist.txt")
}

fn rules() -> RepackRules {
    // the defaults: label plant PL55, alias plant AL55, patterns ALIAS_A* / ALIAS_B* / ALIAS_C*
    RepackRules::default()
}

fn serials(f: &Table, idx: &[usize]) -> Vec<String> {
    let s = f.col("lm_serial_number").unwrap();
    idx.iter().map(|&i| f.rows[i][s].clone()).collect()
}

#[test]
fn blocks_of_the_synthetic_export() {
    let f = del();
    // 21 rows with a serial and one without: the stages drop that one
    assert_eq!(f.nrows(), 22);
    let r = rules();
    let eff = effective_plants(&f, &r.plants).unwrap();
    let b = select_blocks(&f, &eff, &r).unwrap();
    // 50.000 (900000002) is in neither R1 nor R2; 49.999 / 50.001 are
    assert_eq!(
        serials(&f, &b.r1),
        [
            "900000001",
            "900000005",
            "900000006",
            "900000009",
            "900000015",
            "900000021"
        ]
    );
    assert_eq!(serials(&f, &b.r2), ["900000003", "900000013", "900000020"]);
    assert_eq!(
        serials(&f, &b.r3),
        ["900000013", "900000014", "900000015", "900000021"]
    );
    // label plant without pallet; the container-less row passes, KC / pallet / alias / AGM* rows
    assert_eq!(serials(&f, &b.r4), ["900000007", "900000011", "900000018"]);
    assert_eq!(
        history_serials(&f, &b).unwrap(),
        ["900000003", "900000013", "900000020"]
    );
}

#[test]
fn snr_repack_and_pivot_equal_the_expected_tables() {
    let (f, h) = (del(), hist());
    let r = rules();
    let eff = effective_plants(&f, &r.plants).unwrap();
    let b = select_blocks(&f, &eff, &r).unwrap();
    let idx = history_rows(&h, &r).unwrap();
    // UNP rows of WH01 only: 3 (1 of 3), 13 (2), 20 (2, one with a KC container)
    assert_eq!(idx, [0, 3, 4, 5, 6]);
    let s = build_snr_repack(&f, &b, Some((&h, &idx)), "2026-11-05").unwrap();
    assert_eq!((s.r1, s.history, s.r3, s.deleted, s.r4), (6, 5, 4, 5, 3));
    assert_tables_equal(
        &load_fixture("repack", "synthetic/expected_snr_repack.tsv"),
        &s.table,
    );
    let p = repack::pivot_customer_plant(&s.table).unwrap();
    assert_tables_equal(&load_fixture("repack", "synthetic/expected_pivot.tsv"), &p);
    assert_eq!(relabel::pivot_total(&p).unwrap(), 13);
    // alias rows print their real plant; no alias plant anywhere in the output
    let pl = s.table.col("plant").unwrap();
    assert!(s.table.rows.iter().all(|r| r[pl] != "AL55"));
}

#[test]
fn nothing_is_sorted_and_duplicates_stay() {
    let (f, h) = (del(), hist());
    let r = rules();
    let eff = effective_plants(&f, &r.plants).unwrap();
    let b = select_blocks(&f, &eff, &r).unwrap();
    let idx = history_rows(&h, &r).unwrap();
    let s = build_snr_repack(&f, &b, Some((&h, &idx)), "d").unwrap();
    let ser: Vec<&str> = s.table.rows.iter().map(|r| r[0].as_str()).collect();
    // R1: 1, 9, 21 (5, 6, 15 deleted); history: 3, 13, 13, 20; R3: 13, 14, 21; R4: 7, 11, 18
    assert_eq!(
        ser,
        [
            "900000001",
            "900000009",
            "900000021",
            "900000003",
            "900000013",
            "900000013",
            "900000020",
            "900000013",
            "900000014",
            "900000021",
            "900000007",
            "900000011",
            "900000018"
        ]
    );
}

#[test]
fn empty_r2_means_no_history_call() {
    // lower every weight: no heavy row, so no serial for the History call
    let mut f = del();
    let n = f.col("net_weight").unwrap();
    for r in &mut f.rows {
        r[n] = "1".into();
    }
    let r = rules();
    let eff = effective_plants(&f, &r.plants).unwrap();
    let b = select_blocks(&f, &eff, &r).unwrap();
    assert!(b.r2.is_empty());
    assert!(history_serials(&f, &b).unwrap().is_empty());
    let s = build_snr_repack(&f, &b, None, "d").unwrap();
    assert_eq!(s.history, 0);
    assert!(s.table.nrows() > 0);
}

#[test]
fn the_label_plant_and_alias_customers_come_from_the_rules() {
    let f = del();
    let r = RepackRules {
        plants: RelabelRules {
            label_plant: "PL56".into(),
            alias_plant: "ZZ56".into(),
            alias_customers: vec!["CUSTOMER 201*".into()],
        },
        ..RepackRules::default()
    };
    let eff = effective_plants(&f, &r.plants).unwrap();
    let b = select_blocks(&f, &eff, &r).unwrap();
    // PL56 is now the label plant, but all its rows are of an alias customer: effective plant
    // ZZ56, so none is a label-plant row (R4 empty); PL55 is an ordinary plant now, so its
    // heavy row 900000007 goes to R2
    assert!(b.r4.is_empty());
    assert!(serials(&f, &b.r2).contains(&"900000007".to_string()));
    assert!(serials(&f, &b.r1).contains(&"900000001".to_string()));
}

#[test]
fn history_filter_follows_the_warehouse_rule() {
    let h = hist();
    let idx = history_rows(
        &h,
        &RepackRules {
            history_warehouse: "WH02".into(),
            ..RepackRules::default()
        },
    )
    .unwrap();
    // the only UNP row of WH02
    assert_eq!(idx, [2]);
    assert!(history_rows(
        &h,
        &RepackRules {
            history_warehouse: "NOPE".into(),
            ..RepackRules::default()
        }
    )
    .unwrap()
    .is_empty());
}

fn combined(name: &str) -> Table {
    load_fixture("repack", &format!("synthetic/{name}"))
}

#[test]
fn exclusion_list_needs_two_yellow_rows() {
    let two = load_fixture("repack", "relabel_combined_stand_in.tsv");
    let (s, n) = exclusion_serials(&two).unwrap();
    assert_eq!(n, 5);
    assert_eq!(
        s,
        [
            "130007102",
            "130007101",
            "130007100",
            "130007099",
            "130007098"
        ]
    );
    let (s, n) = exclusion_serials(&combined("combined_one_yellow.tsv")).unwrap();
    assert!(s.is_empty());
    assert_eq!(n, 1);
    let (s, n) = exclusion_serials(&combined("combined_no_yellow.tsv")).unwrap();
    assert!(s.is_empty() && n == 0);
    let e = exclusion_serials(&combined("combined_no_yellow_column.tsv")).unwrap_err();
    assert!(e.to_string().contains("yellow"), "{e}");
}
