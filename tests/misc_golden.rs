//! `misc` on the REAL SAP text exports (tests/fixtures/misc/real/, tiny: 3 shipments, 3
//! deliveries, no EPDC shipping point) compared with the macro output `11-05-2026_MISC.xlsx`
//! converted to TSV (tests/fixtures/misc/expected_*.tsv, see the README there), plus edge cases
//! (EPDC rows with and without parentheses, blank shipping point, empty results) through
//! `run_misc` with a fake SAP seam.
mod common;

use anyhow::{anyhow, Result};
use chrono::NaiveDate;
use common::{load_fixture, load_real_in};
use sap_automation::abc::inv_runner::*;
use sap_automation::abc::run::RunDate;
use sap_automation::abc::runner::{NoDataError, OutputSpec, StepOutput};
use sap_automation::abc::table::Table;
use std::path::{Path, PathBuf};

fn vt11() -> Table {
    load_real_in("misc", "vt11_inv", "raw_vt11.txt")
}
fn vl06o() -> Table {
    load_real_in("misc", "vl06o", "raw_vl06o.txt")
}

fn t(headers: &[&str], rows: &[&[&str]]) -> Table {
    Table::new(
        "t",
        headers.iter().map(|s| s.to_string()).collect(),
        rows.iter()
            .map(|r| r.iter().map(|s| s.to_string()).collect())
            .collect(),
    )
}

const VL_HEADERS: [&str; 7] = [
    "delivery",
    "shipto_party",
    "name_of_the_shipto_party",
    "external_delivery_id",
    "total_gds_mvt_stat",
    "shipping_pointreceiving_pt",
    "number_of_packages",
];

#[derive(Clone)]
enum Ov {
    NoData,
    Table(Table),
}

#[derive(Default)]
struct Fake {
    calls: Vec<&'static str>,
    vt11_args: Vec<(String, String)>,
    ships: Vec<String>,
    vl_layout: String,
    vt: Option<Ov>,
    vl: Option<Ov>,
}

impl MiscSteps for Fake {
    fn vt11(&mut self, _d: &RunDate, variant: &str, layout: &str) -> Result<StepOutput> {
        self.calls.push("vt11");
        self.vt11_args.push((variant.into(), layout.into()));
        match &self.vt {
            Some(Ov::NoData) => Ok(StepOutput::NoData("nothing".into())),
            Some(Ov::Table(t)) => Ok(StepOutput::Table(t.clone())),
            None => Ok(StepOutput::Table(vt11())),
        }
    }
    fn vl06o_by_shipments(&mut self, s: &[String], layout: &str) -> Result<StepOutput> {
        self.calls.push("vl06o");
        self.ships = s.to_vec();
        self.vl_layout = layout.into();
        match &self.vl {
            Some(Ov::NoData) => Ok(StepOutput::NoData("nothing".into())),
            Some(Ov::Table(t)) => Ok(StepOutput::Table(t.clone())),
            None => Ok(StepOutput::Table(vl06o())),
        }
    }
}

fn day() -> RunDate {
    RunDate {
        date1: NaiveDate::from_ymd_opt(2026, 11, 5).unwrap(),
        date2: None,
    }
}

fn temp_out(tag: &str) -> OutputSpec {
    let dir = std::env::temp_dir().join(format!("misc_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    OutputSpec {
        reports_dir: dir,
        today: NaiveDate::from_ymd_opt(2026, 11, 10).unwrap(),
    }
}

fn files_under(dir: &Path) -> usize {
    fn walk(d: &Path, n: &mut usize) {
        if let Ok(rd) = std::fs::read_dir(d) {
            for e in rd.flatten() {
                let p = e.path();
                if p.is_dir() {
                    walk(&p, n)
                } else {
                    *n += 1
                }
            }
        }
    }
    let mut n = 0;
    walk(dir, &mut n);
    n
}

fn read(p: &PathBuf, n: &str) -> Table {
    Table::from_tsv(n, &std::fs::read_to_string(p).unwrap()).unwrap()
}

/// Run with a custom VL06O table; returns the vl06o and pivot outputs and the summary.
fn run_with(tag: &str, vl: Table) -> (Table, Table, InvSummary) {
    let out = temp_out(tag);
    let mut f = Fake {
        vl: Some(Ov::Table(vl)),
        ..Fake::default()
    };
    let s = run_misc(&mut f, &day(), &InvSelection::misc(), &out).unwrap();
    let v = read(&s.outputs[1], "vl06o");
    let p = read(&s.outputs[2], "pivot");
    let _ = std::fs::remove_dir_all(&out.reports_dir);
    (v, p, s)
}

fn pairs(p: &Table) -> Vec<(&str, &str)> {
    p.rows
        .iter()
        .map(|r| (r[0].as_str(), r[1].as_str()))
        .collect()
}

#[test]
fn loader_shapes() {
    let vt = vt11();
    assert_eq!((vt.nrows(), vt.headers.len()), (3, 13));
    let vl = vl06o();
    // 6 item rows of 3 deliveries; the header `ShPt` maps to shipping_pointreceiving_pt
    assert_eq!((vl.nrows(), vl.headers.len()), (6, 7));
    assert_eq!(vl.headers, VL_HEADERS);
    // the first row of 85623472 has no `No.Pk` cell (short row): blank
    assert_eq!(vl.rows[0][6], "");
}

/// Differences to the xlsx VT11 sheet: only the 14-character list truncation of the free-text
/// columns (our value is a prefix of the xlsx one); dates were written back as text.
#[test]
fn vt11_matches_xlsx_sheet() {
    let ours = vt11();
    let x = load_fixture("misc", "expected_vt11.tsv");
    assert_eq!(x.headers, ours.headers);
    assert_eq!(x.nrows(), 3);
    let trunc = [
        "seal_number_2",
        "descripof_shipment",
        "forwarding_agent_tracking_id",
    ];
    for (c, name) in ours.headers.iter().enumerate() {
        for (o, e) in ours.rows.iter().zip(&x.rows) {
            if trunc.contains(&name.as_str()) {
                assert!(e[c].starts_with(&o[c]), "{name}: {:?} vs {:?}", o[c], e[c]);
            } else {
                assert_eq!(o[c], e[c], "column {name}");
            }
        }
    }
}

/// Real data through the runner vs the xlsx sheets. KNOWN DIFFERENCES:
/// 1. `number_of_packages` of 85623472 is blank in the text export (SAP leaves the cell out)
///    and 0 in the xlsx export; both count as 0 in the pivot.
/// 2. The xlsx pivot has the header `Row Labels` / `Sum of number_of_packages` and a `Grand
///    Total` row; ours is the long table `shipping_pointreceiving_pt`, `number_of_packages`
///    without a grand total (same shape as the xdock pivot: group column + summed column,
///    named after the source columns).
/// 3. No row has an `EPDC` shipping point, so the regex step changes nothing (not verified
///    against the macro on real data: no such row in this export).
#[test]
fn real_run_matches_xlsx_except_documented_differences() {
    let out = temp_out("real");
    let mut f = Fake::default();
    let s = run_misc(&mut f, &day(), &InvSelection::misc(), &out).unwrap();
    assert_eq!(f.calls, ["vt11", "vl06o"]);
    assert_eq!(
        f.vt11_args,
        [("MISC_EPDC_".to_string(), "scorecard".to_string())]
    );
    assert_eq!(f.ships, ["24602236", "24605562", "24611870"]);
    assert_eq!(f.vl_layout, "inv");
    assert_eq!(
        (s.shipments, s.deliveries, s.vl06o_rows, s.pivot_rows),
        (3, 3, 3, 2)
    );
    assert_eq!((s.total.as_str(), s.regex_replaced), ("2", 0));

    let base = out.reports_dir.join("INV").join("2026-11-10").join("misc");
    assert_eq!(s.outputs.len(), 3);
    for (p, n) in s.outputs.iter().zip(["VT11", "VL06O", "PIVOT"]) {
        assert_eq!(p, &base.join(format!("11-05-2026_MISC_{n}.tsv")));
    }

    let ours = read(&s.outputs[1], "vl06o");
    let x = load_fixture("misc", "expected_vl06o.tsv");
    assert_eq!(ours.headers, x.headers);
    assert_eq!(ours.nrows(), 3);
    for (i, (o, e)) in ours.rows.iter().zip(&x.rows).enumerate() {
        for c in 0..8 {
            if (i, c) == (0, 6) {
                assert_eq!((o[c].as_str(), e[c].as_str()), ("", "0"), "difference 1");
            } else {
                assert_eq!(o[c], e[c], "row {i} col {c}");
            }
        }
    }

    let pivot = read(&s.outputs[2], "pivot");
    assert_eq!(
        pivot.headers,
        ["shipping_pointreceiving_pt", "number_of_packages"]
    );
    let xp = load_fixture("misc", "expected_pivot.tsv");
    assert_eq!(xp.headers, ["Row Labels", "Sum of number_of_packages"]);
    let (grand, groups) = xp.rows.split_last().unwrap();
    assert_eq!(grand, &["Grand Total", "2"]);
    assert_eq!(pivot.rows, groups);
    let _ = std::fs::remove_dir_all(&out.reports_dir);
}

#[test]
fn edge_epdc_with_parentheses_is_replaced_and_labels_sort() {
    let vl = t(
        &VL_HEADERS,
        &[
            &["1", "S", "ACME (EPDC-1) CORP (X)", "", "C", "EPDC", "2"],
            &["2", "S", "OTHER", "", "C", "FV55", "3"],
            &["3", "S", "BETA (EPDC-2)", "", "C", "EPDC", "4"],
            &["4", "S", "NO SHIP POINT", "", "C", "", "5"],
            &["5", "S", "GAMMA (EPDC-1)", "", "C", "EPDC", "6"],
        ],
    );
    let (v, p, s) = run_with("epdc", vl);
    // first parenthesised group, parentheses kept; other rows untouched
    let pc = v.col("shipping_pointreceiving_pt").unwrap();
    let col: Vec<&str> = v.rows.iter().map(|r| r[pc].as_str()).collect();
    assert_eq!(col, ["(EPDC-1)", "FV55", "(EPDC-2)", "", "(EPDC-1)"]);
    assert_eq!(s.regex_replaced, 3);
    // '(' sorts before letters; the blank label is last
    assert_eq!(
        pairs(&p),
        [
            ("(EPDC-1)", "8"),
            ("(EPDC-2)", "4"),
            ("FV55", "3"),
            ("(blank)", "5")
        ]
    );
}

#[test]
fn edge_epdc_without_parentheses_and_lowercase_stay() {
    let vl = t(
        &VL_HEADERS,
        &[
            &["1", "S", "NO PARENS HERE", "", "C", "EPDC", "1"],
            &["2", "S", "ACME (X)", "", "C", "epdc", "1"],
        ],
    );
    let (v, p, s) = run_with("noparens", vl);
    let pc = v.col("shipping_pointreceiving_pt").unwrap();
    // no group: unchanged; `contains("EPDC")` is case sensitive
    assert_eq!(v.rows[0][pc], "EPDC");
    assert_eq!(v.rows[1][pc], "epdc");
    assert_eq!(s.regex_replaced, 0);
    // the pivot groups labels case-insensitively, like Excel (first spelling kept)
    assert_eq!(pairs(&p), [("EPDC", "2")]);
}

#[test]
fn edge_no_epdc_rows_and_blank_shipping_point_only() {
    let vl = t(
        &VL_HEADERS,
        &[
            &["1", "S", "A (B)", "", "C", "", "2"],
            &["2", "S", "C", "", "C", "", ""],
        ],
    );
    let (_, p, s) = run_with("blank", vl);
    assert_eq!(s.regex_replaced, 0);
    // a single `(blank)` group; the blank package cell adds nothing
    assert_eq!(pairs(&p), [("(blank)", "2")]);
}

#[test]
fn edge_dedup_keeps_first_row_of_a_delivery() {
    let vl = t(
        &VL_HEADERS,
        &[
            &["85", "S", "A", "", "C", "FV1", "1"],
            &["85", "S", "A", "", "C", "FV2", "9"],
            &["86", "S", "B", "", "C", "FV2", "2"],
        ],
    );
    let (v, p, s) = run_with("dedup", vl);
    assert_eq!(v.nrows(), 2);
    assert_eq!(s.deliveries, 2);
    assert_eq!(pairs(&p), [("FV1", "1"), ("FV2", "2")]);
}

#[test]
fn empty_results_stop_the_run_and_write_nothing() {
    for (vt, vl, calls, step) in [
        (Some(Ov::NoData), None, vec!["vt11"], "vt11"),
        (None, Some(Ov::NoData), vec!["vt11", "vl06o"], "vl06o"),
        (
            None,
            Some(Ov::Table(t(&VL_HEADERS, &[]))),
            vec!["vt11", "vl06o"],
            "vl06o",
        ),
        (
            Some(Ov::Table(Table::new(
                "e",
                vec!["shipment_number".into()],
                vec![],
            ))),
            None,
            vec!["vt11"],
            "vt11",
        ),
    ] {
        let out = temp_out("empty");
        let mut f = Fake {
            vt,
            vl,
            ..Fake::default()
        };
        let e = run_misc(&mut f, &day(), &InvSelection::misc(), &out).unwrap_err();
        assert_eq!(e.downcast_ref::<NoDataError>().unwrap().step, step);
        assert_eq!(f.calls, calls);
        assert_eq!(files_under(&out.reports_dir), 0, "no partial output");
    }
}

#[test]
fn missing_columns_name_the_report_and_step() {
    let out = temp_out("cols");
    let mut f = Fake {
        // no shipping point column (e.g. the xdock layout with `plant` was used by mistake)
        vl: Some(Ov::Table(load_real_in("misc", "vl06o", "raw_vl06o.txt"))),
        ..Fake::default()
    };
    f.vl = f.vl.take().map(|o| match o {
        Ov::Table(mut t) => {
            t.headers[5] = "plant".into();
            Ov::Table(t)
        }
        other => other,
    });
    let e = run_misc(&mut f, &day(), &InvSelection::misc(), &out).unwrap_err();
    let m = format!("{e:#}");
    assert!(
        m.contains("misc step 'vl06o'") && m.contains("shipping_pointreceiving_pt"),
        "{m}"
    );
    assert_eq!(files_under(&out.reports_dir), 0);
    let _ = anyhow!("unused");
}
