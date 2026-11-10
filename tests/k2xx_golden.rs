//! `k2xx` stages on the SYNTHETIC stand-in exports (tests/fixtures/k2xx/synthetic/, derived from
//! the real xdock/mustgo exports, see the README there), loaded through the production loader
//! `abc::raw`, compared with expected tables that were computed from the spec text (5.5 / 9.5)
//! by a one-off script, independently of the code under test.
//!
//! No real k2xx export and no macro xlsx were available: these tests pin the rules of spec 9.5,
//! they do not prove equality with the macro output.
mod common;

use common::{assert_tables_equal, load_fixture, load_real_in, load_synthetic_in};
use sap_automation::abc::inv_runner::shipment_list;
use sap_automation::abc::k2xx;
use sap_automation::abc::table::Table;
use std::collections::BTreeSet;

const RUN_DATE: &str = "2026-11-05";

/// Deliveries of the stand-in data without a ZMDETPC row (`8012734818` is the one of spec 9.5).
const NO_ZMDETPC: [&str; 3] = ["85650310", "85655488", "8012734818"];

fn vt11() -> Table {
    load_synthetic_in("k2xx", "vt11_inv", "raw_vt11.txt")
}
fn vl06o_raw() -> Table {
    load_synthetic_in("k2xx", "vl06o", "raw_vl06o.txt")
}
fn zvt11() -> Table {
    load_synthetic_in("k2xx", "zvt11", "raw_zvt11.txt")
}
fn zmdetpc() -> Table {
    load_synthetic_in("k2xx", "zmdetpc", "raw_zmdetpc.txt")
}

fn merged() -> Table {
    let mut vl = vl06o_raw();
    k2xx::prepare_vl06o(&mut vl, RUN_DATE).unwrap();
    k2xx::merge_zmdetpc(&vl, &zmdetpc()).unwrap()
}

fn col(t: &Table, name: &str) -> Vec<String> {
    let c = t.col(name).unwrap();
    t.rows.iter().map(|r| r[c].clone()).collect()
}

#[test]
fn loader_shapes() {
    let vt = vt11();
    assert_eq!((vt.nrows(), vt.headers.len()), (54, 13));
    assert!(col(&vt, "transportplanningpt").iter().all(|v| v == "TPDC"));
    assert_eq!(shipment_list(&vt).unwrap().len(), 54);

    // layout `pallets` (spec 3.3): 8 columns in this order
    let vl = vl06o_raw();
    assert_eq!(
        vl.headers,
        [
            "delivery",
            "shipto_party",
            "name_of_the_shipto_party",
            "deliv_datefromto",
            "total_gds_mvt_stat",
            "plant",
            "external_delivery_id",
            "number_of_packages"
        ]
    );
    assert_eq!(vl.nrows(), 834);
    assert_eq!(
        col(&vl, "delivery").iter().collect::<BTreeSet<_>>().len(),
        107
    );

    // layout `add_del` (spec 3.2 / 8.2); ` Date` is trimmed before sanitising (spec 9.5)
    let zv = zvt11();
    assert_eq!(
        zv.headers,
        [
            "date",
            "trailer_number",
            "shipment_number",
            "delivery_number",
            "plant",
            "ship_tofrom_name",
            "shipment_end_date"
        ]
    );
    assert_eq!(zv.nrows(), 108);

    let zm = zmdetpc();
    assert_eq!(zm.headers, ["delivery", "pallet", "child"]);
    assert_eq!(zm.nrows(), 104);
}

/// The VT11 stand-in is the TPDC part of the real xdock export: same rows, same order.
#[test]
fn vt11_stand_in_is_the_tpdc_part_of_the_real_xdock_export() {
    let real = load_real_in("xdock", "vt11_inv", "raw_vt11.txt");
    let c = real.col("transportplanningpt").unwrap();
    let rows: Vec<Vec<String>> = real
        .rows
        .iter()
        .filter(|r| r[c] == "TPDC")
        .cloned()
        .collect();
    let vt = vt11();
    assert_eq!(vt.headers, real.headers);
    assert_eq!(vt.rows, rows);
}

#[test]
fn merged_vl06o_equals_the_expected_table() {
    let m = merged();
    assert_tables_equal(&load_fixture("k2xx", "expected_vl06o.tsv"), &m);
}

#[test]
fn merged_vl06o_follows_spec_9_5() {
    let m = merged();
    // one row per delivery, ascending (the 10-digit delivery last), date = run date
    assert_eq!(m.nrows(), 107);
    let d = col(&m, "delivery");
    let nums: Vec<u64> = d.iter().map(|v| v.parse().unwrap()).collect();
    assert!(nums.windows(2).all(|w| w[0] < w[1]), "ascending, distinct");
    assert_eq!(d.last().unwrap(), "8012734818");
    assert!(col(&m, "date").iter().all(|v| v == RUN_DATE));
    assert_eq!(
        m.headers[8..],
        ["date", "delivery_", "pallet", "child", "total"]
    );

    // total = pallet + child; a delivery without a ZMDETPC row keeps all four blank
    let (dz, p, c, t) = (
        col(&m, "delivery_"),
        col(&m, "pallet"),
        col(&m, "child"),
        col(&m, "total"),
    );
    let mut blank = Vec::new();
    for i in 0..m.nrows() {
        if dz[i].is_empty() {
            assert!(p[i].is_empty() && c[i].is_empty() && t[i].is_empty());
            blank.push(d[i].as_str());
        } else {
            assert_eq!(dz[i], d[i]);
            let sum: u64 = p[i].parse::<u64>().unwrap() + c[i].parse::<u64>().unwrap();
            assert_eq!(t[i], sum.to_string(), "row {i}");
        }
    }
    assert_eq!(blank, NO_ZMDETPC);
    assert_eq!(k2xx::total_sum(&m).unwrap(), 1144.0);
}

/// The dedup keeps the FIRST row of a delivery: `85650066` has two plants in the item rows.
#[test]
fn dedup_keeps_the_first_row() {
    let raw = vl06o_raw();
    let (dc, pc) = (raw.col("delivery").unwrap(), raw.col("plant").unwrap());
    let plants: Vec<&str> = raw
        .rows
        .iter()
        .filter(|r| r[dc] == "85650066")
        .map(|r| r[pc].as_str())
        .collect();
    assert!(plants.iter().collect::<BTreeSet<_>>().len() > 1);
    let m = merged();
    let row = m
        .rows
        .iter()
        .find(|r| r[m.col("delivery").unwrap()] == "85650066")
        .unwrap();
    assert_eq!(row[m.col("plant").unwrap()], plants[0]);
}

/// `zvt11` is written as exported: the loaded table equals the expected one cell by cell.
#[test]
fn zvt11_is_passed_through() {
    assert_tables_equal(&load_fixture("k2xx", "expected_zvt11.tsv"), &zvt11());
}

/// Unlike xdock/toyota: no `number_of_packages` fallback and no `0 -> 1`.
#[test]
fn differs_from_the_inv_merge_only_where_the_rules_say() {
    let mut vl = vl06o_raw();
    k2xx::prepare_vl06o(&mut vl, RUN_DATE).unwrap();
    let inv = sap_automation::abc::inv::merge_zmdetpc(&vl, &zmdetpc()).unwrap();
    let m = merged();
    assert_eq!(inv.headers, m.headers);
    let (d, t, dz) = (
        m.col("delivery").unwrap(),
        m.col("total").unwrap(),
        m.col("delivery_").unwrap(),
    );
    let mut differing = Vec::new();
    for (a, b) in inv.rows.iter().zip(&m.rows) {
        if a != b {
            differing.push(b[d].as_str());
            assert!(b[t].is_empty() && b[dz].is_empty());
            assert!(!a[t].is_empty() && a[dz] == a[d]);
        }
    }
    assert_eq!(differing, NO_ZMDETPC);
}

/// Pivot (owner decision of 2026-11-10, overrides spec 7.0 item 1): plant rows, sum of `total`
/// over the merged table. `expected_pivot.tsv` was computed by a one-off script straight from
/// the raw stand-in exports (first row per delivery, pallet + child of its ZMDETPC row),
/// independently of the code under test.
#[test]
fn pivot_equals_the_expected_table() {
    let p = k2xx::pivot_plant_total(&merged()).unwrap();
    let x = load_fixture("k2xx", "expected_pivot.tsv");
    assert_eq!(x.headers, ["plant", "total"]);
    assert_eq!(x.nrows(), 12);
    assert_tables_equal(&x, &p);

    // long form: ascending by plant, no grand-total row, the sum is that of the vl06o table
    let plants = col(&p, "plant");
    assert!(plants.windows(2).all(|w| w[0] < w[1]), "ascending");
    assert!(!plants.iter().any(|v| v.to_lowercase().contains("total")));
    let sum: u64 = col(&p, "total")
        .iter()
        .map(|v| v.parse::<u64>().unwrap())
        .sum();
    assert_eq!(sum, 1144);
    // the deliveries without a ZMDETPC row (blank total) add nothing to their plants
    let row = |plant: &str| p.rows.iter().find(|r| r[0] == plant).unwrap()[1].clone();
    assert_eq!(
        (row("PL58"), row("PL63")),
        ("158".to_string(), "2".to_string())
    );
}

/// Pivot conventions on hand-made rows (the stand-in data has no blank plant): `(blank)` last,
/// labels grouped case-insensitively, `total` parsed numerically (thousands separator, blank
/// and text cells add nothing), no grand-total row, columns found by name wherever they are.
#[test]
fn pivot_edge_cases() {
    let rows: &[[&str; 4]] = &[
        ["9", "PL61", "x", "1,200"],
        ["1", "", "x", "4"],
        ["2", "PL55", "x", "2.5"],
        ["3", "pl55", "x", "3"],
        ["4", "PL33", "x", ""],
        ["5", " ", "x", "1"],
        ["6", "PL33", "x", "n/a"],
        ["7", "PL50", "x", "10"],
        ["8", "PL50", "x", "0"],
    ];
    let m = Table::new(
        "vl06o",
        ["delivery", "plant", "other", "total"]
            .map(String::from)
            .to_vec(),
        rows.iter().map(|r| r.map(String::from).to_vec()).collect(),
    );
    let p = k2xx::pivot_plant_total(&m).unwrap();
    assert_eq!(p.headers, ["plant", "total"]);
    assert_eq!(
        p.rows,
        vec![
            vec!["PL33", "0"],
            vec!["PL50", "10"],
            vec!["PL55", "5.5"],
            vec!["PL61", "1200"],
            vec!["(blank)", "5"],
        ]
    );

    // columns are matched by name, not by position
    let swapped = Table::new(
        "vl06o",
        ["total", "other", "plant"].map(String::from).to_vec(),
        vec![["7", "x", "PL55"].map(String::from).to_vec()],
    );
    assert_eq!(
        k2xx::pivot_plant_total(&swapped).unwrap().rows,
        vec![vec!["PL55", "7"]]
    );
    let no_plant = Table::new("vl06o", vec!["total".to_string()], Vec::new());
    assert!(k2xx::pivot_plant_total(&no_plant).is_err());
}
