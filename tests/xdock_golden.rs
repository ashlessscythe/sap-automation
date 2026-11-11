//! `xdock` stages on the REAL SAP text exports (tests/fixtures/xdock/real/), loaded through the
//! production loader `abc::raw`, compared with the macro output `11-05-2026_XDOCK.xlsx`
//! converted to TSV (tests/fixtures/xdock/expected_*.tsv, see the README there).
//!
//! The xlsx and our output differ in the places listed in the doc comments below; every one is
//! asserted exactly, so a change in either direction fails the test.
mod common;

use common::{diff_tables, load_fixture, load_real_in};
use sap_automation::abc::inv;
use sap_automation::abc::table::Table;
use std::collections::BTreeSet;

const RUN_DATE: &str = "2026-11-05";

/// The 8 deliveries without a ZMDETPC row (spec 9.6).
const NO_ZMDETPC: [&str; 8] = [
    "13000801",
    "13000802",
    "13000803",
    "13000825",
    "13000834",
    "13000914",
    "13000948",
    "1300000000",
];

fn vt11() -> Table {
    load_real_in("xdock", "vt11_inv", "raw_vt11.txt")
}
fn vl06o_raw() -> Table {
    load_real_in("xdock", "vl06o", "raw_vl06o.txt")
}
fn zmdetpc() -> Table {
    load_real_in("xdock", "zmdetpc", "raw_zmdetpc.txt")
}

fn merged() -> Table {
    let mut vl = vl06o_raw();
    inv::prepare_vl06o(&mut vl, RUN_DATE).unwrap();
    inv::merge_zmdetpc(&vl, &zmdetpc()).unwrap()
}

fn col(t: &Table, name: &str) -> Vec<String> {
    let c = t.col(name).unwrap();
    t.rows.iter().map(|r| r[c].clone()).collect()
}

/// A real week VT11 export (WK43, 424 shipments) is not UTF-8: it has non-breaking spaces
/// (byte 0xA0, Windows-1252) in "Seal Number 2". It used to stop the run with "export is not
/// valid UTF-8"; it must load, with all rows and unchanged shipment numbers.
#[test]
fn vt11_week_export_in_windows_1252_loads() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/xdock/real/raw_vt11_wk43_cp1252.txt");
    let bytes = std::fs::read(&path).unwrap();
    assert!(
        std::str::from_utf8(&bytes).is_err(),
        "fixture must really be non-UTF-8"
    );
    let vt = common::load_real_in("xdock", "vt11_inv", "raw_vt11_wk43_cp1252.txt");
    assert_eq!(vt.nrows(), 424, "list header says Entries: 424");
    assert!(vt.col("shipment_number").is_ok() && vt.col("transportplanningpt").is_ok());
    let sn = col(&vt, "shipment_number");
    assert!(sn
        .iter()
        .all(|s| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit())));
    // the cell with the NBSP is kept (interior NBSP), not dropped or mangled
    let seals = col(&vt, "seal_number_2");
    assert!(
        seals.iter().any(|s| s.contains('\u{a0}')),
        "expected a seal text with a non-breaking space"
    );
}

#[test]
fn loader_shapes() {
    let vt = vt11();
    assert_eq!((vt.nrows(), vt.headers.len()), (91, 13));
    assert_eq!(
        vt.headers,
        [
            "shipment_number",
            "ovtransport_status",
            "shipment_type",
            "transportplanningpt",
            "seal_number_2",
            "shipping_type",
            "service_agent",
            "trailer_number",
            "seal_number_1",
            "actualshipmentend",
            "actshipmnt_end_time",
            "descripof_shipment",
            "forwarding_agent_tracking_id"
        ]
    );
    let vl = vl06o_raw();
    assert_eq!((vl.nrows(), vl.headers.len()), (1068, 7));
    assert_eq!(
        vl.headers,
        [
            "delivery",
            "shipto_party",
            "name_of_the_shipto_party",
            "external_delivery_id",
            "total_gds_mvt_stat",
            "plant",
            "number_of_packages"
        ]
    );
    let z = zmdetpc();
    assert_eq!((z.nrows(), z.headers.len()), (140, 3));
    assert_eq!(z.headers, ["delivery", "pallet", "child"]);
    // zero padded in the raw file, normalised by the loader; 10 digits stay as they are
    assert_eq!(z.rows[0], ["13000804", "1", "0"]);
}

#[test]
fn spec_96_counts() {
    let vt = vt11();
    assert_eq!(inv::delivery_key("0013000804"), "13000804");
    let mut shipments: Vec<String> = col(&vt, "shipment_number");
    shipments.sort();
    shipments.dedup();
    assert_eq!(shipments.len(), 91);
    let mut vl = vl06o_raw();
    inv::prepare_vl06o(&mut vl, RUN_DATE).unwrap();
    assert_eq!(vl.nrows(), 148);
    let d = inv::delivery_values(&vl).unwrap();
    assert_eq!(d.len(), 148);
    let z: BTreeSet<String> = col(&zmdetpc(), "delivery")
        .iter()
        .map(|v| inv::delivery_key(v))
        .collect();
    let missing: BTreeSet<&str> = d
        .iter()
        .filter(|x| !z.contains(&inv::delivery_key(x)))
        .map(|s| s.as_str())
        .collect();
    assert_eq!(missing, NO_ZMDETPC.iter().copied().collect::<BTreeSet<_>>());
}

/// VT11: our table equals the xlsx sheet. Differences handled by the comparison: the real
/// text export truncates the three free-text columns to the list width (our value is a prefix of
/// the xlsx one, which may also end in a line break), and Excel turned date-only
/// `descripof_shipment` texts into dates (the converter wrote them back as `MM/DD/YYYY`).
#[test]
fn vt11_matches_xlsx_sheet() {
    let ours = vt11();
    let x = load_fixture("xdock", "expected_vt11.tsv");
    assert_eq!(x.headers, ours.headers);
    assert_eq!(x.nrows(), ours.nrows());
    for name in [
        "seal_number_2",
        "descripof_shipment",
        "forwarding_agent_tracking_id",
    ] {
        let (a, b) = (ours.col(name).unwrap(), x.col(name).unwrap());
        for (o, e) in ours.rows.iter().zip(&x.rows) {
            let e = e[b].trim_end_matches(['\n', '\r']);
            assert!(e.starts_with(&o[a]), "{name}: {:?} vs {:?}", o[a], e);
        }
    }
    for name in [
        "shipment_number",
        "ovtransport_status",
        "shipment_type",
        "transportplanningpt",
        "shipping_type",
        "service_agent",
        "trailer_number",
        "seal_number_1",
        "actualshipmentend",
        "actshipmnt_end_time",
    ] {
        let (a, b) = (ours.col(name).unwrap(), x.col(name).unwrap());
        let o: Vec<&str> = ours.rows.iter().map(|r| r[a].as_str()).collect();
        let e: Vec<&str> = x.rows.iter().map(|r| r[b].as_str()).collect();
        assert_eq!(o, e, "column {name}");
    }
}

/// KNOWN DIFFERENCES to the xlsx `XDOCK_VL06O` sheet (15 columns):
/// 1. The sheet has 3 extra trailing columns (`delivery_`, `pallet`, `child` again, columns
///    M:O): the unaligned ZMDETPC rows left behind by the macro's positional append. Not part of
///    the data; ignored here (the first 12 columns are compared).
/// 2. For the 8 deliveries without a ZMDETPC row the sheet has `total` = 0 (the Excel formula
///    `SUM(pallet:child)` of two blank cells is 0, not blank, so the macro's "blank total takes
///    number_of_packages" never fired and the `0 -> 1` replace did not touch it either). Spec
///    4.12 step 5 and 9.6 say `total` = `number_of_packages` for them, which is what we do.
///    The rows are identical in every other cell.
#[test]
fn vl06o_matches_xlsx_sheet_except_documented_differences() {
    let ours = merged();
    let x = load_fixture("xdock", "expected_vl06o.tsv");
    assert_eq!(x.headers.len(), 15);
    assert_eq!(x.nrows(), 148);
    assert_eq!(ours.nrows(), 148);
    assert_eq!(ours.headers, x.headers[..12]);
    let trimmed = Table::new(
        "expected",
        x.headers[..12].to_vec(),
        x.rows.iter().map(|r| r[..12].to_vec()).collect(),
    );
    let report = diff_tables(&trimmed, &ours, 200).unwrap_err();
    // exactly the 8 rows differ, only in `total`
    let diff_rows: Vec<usize> = (0..148)
        .filter(|&i| trimmed.rows[i] != ours.rows[i])
        .collect();
    assert_eq!(diff_rows.len(), 8, "{report}");
    let dc = ours.col("delivery").unwrap();
    let tc = ours.col("total").unwrap();
    let nc = ours.col("number_of_packages").unwrap();
    let mut got = BTreeSet::new();
    for i in diff_rows {
        let (e, a) = (&trimmed.rows[i], &ours.rows[i]);
        for c in 0..12 {
            if c != tc {
                assert_eq!(e[c], a[c], "row {i} col {c} must be identical");
            }
        }
        assert_eq!(e[tc], "0", "xlsx total of a delivery without ZMDETPC row");
        assert_eq!(a[tc], a[nc], "ours: total = number_of_packages");
        assert_eq!(a[dc], a[ours.col("delivery_").unwrap()]);
        got.insert(a[dc].clone());
    }
    assert_eq!(got, NO_ZMDETPC.iter().map(|s| s.to_string()).collect());
    // the date column carries the run date
    assert!(col(&ours, "date").iter().all(|d| d == RUN_DATE));
}

/// KNOWN DIFFERENCES to the xlsx pivot (plant rows, sum of `total`):
/// 1. The sheet ends with a `Grand Total` row; ours has none (a long table of groups). The
///    sheet header is `Row Labels` / `Sum of total`, ours `plant` / `total`.
/// 2. Because of difference 2 of the vl06o sheet, every plant that has a delivery without a
///    ZMDETPC row is higher in ours by those deliveries' `number_of_packages` (the `(blank)`
///    plant: 0 in the sheet, 4 in ours); the grand total is 1328 in the sheet and 1493 in ours
///    (spec 9.6 says 1493). Plants without such a delivery are identical.
#[test]
fn pivot_matches_xlsx_except_documented_differences() {
    let m = merged();
    let p = inv::pivot_sum(&m, "plant", "total").unwrap();
    assert_eq!(p.headers, ["plant", "total"]);
    let x = load_fixture("xdock", "expected_pivot.tsv");
    assert_eq!(x.headers, ["Row Labels", "Sum of total"]);
    let (grand, groups) = x.rows.split_last().unwrap();
    assert_eq!(grand, &["Grand Total", "1328"]);
    assert_eq!(p.nrows(), groups.len());

    // adjustment per plant label: number_of_packages of the deliveries without a ZMDETPC row
    let (dc, pc, nc) = (
        m.col("delivery").unwrap(),
        m.col("plant").unwrap(),
        m.col("number_of_packages").unwrap(),
    );
    let mut adj: std::collections::BTreeMap<String, i64> = Default::default();
    for r in &m.rows {
        if NO_ZMDETPC.contains(&r[dc].as_str()) {
            let label = if r[pc].is_empty() { "(blank)" } else { &r[pc] };
            *adj.entry(label.to_string()).or_default() += r[nc].parse::<i64>().unwrap();
        }
    }
    assert_eq!(adj.values().sum::<i64>(), 1493 - 1328);
    assert_eq!(adj["(blank)"], 4);

    let mut ours_total = 0i64;
    for (o, e) in p.rows.iter().zip(groups) {
        assert_eq!(o[0], e[0], "same plant order, (blank) last");
        let (ov, ev) = (o[1].parse::<i64>().unwrap(), e[1].parse::<i64>().unwrap());
        assert_eq!(
            ov,
            ev + adj.get(&o[0]).copied().unwrap_or(0),
            "plant {}",
            o[0]
        );
        ours_total += ov;
    }
    assert_eq!(e_blank(groups), "0");
    assert_eq!(p.rows.last().unwrap()[0], "(blank)");
    assert_eq!(ours_total, 1493);
    let sheet: i64 = groups.iter().map(|r| r[1].parse::<i64>().unwrap()).sum();
    assert_eq!(sheet, 1328);
}

fn e_blank(groups: &[Vec<String>]) -> &str {
    &groups.last().unwrap()[1]
}
