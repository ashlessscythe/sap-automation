//! `run_xdock` against a fake SAP seam serving the REAL fixtures.
mod common;

use anyhow::{anyhow, Result};
use chrono::NaiveDate;
use common::{load_fixture, load_real_in};
use sap_automation::abc::inv_runner::*;
use sap_automation::abc::raw::RawOptions;
use sap_automation::abc::run::RunDate;
use sap_automation::abc::runner::{outcome_to_output, NoDataError, OutputSpec, StepOutput};
use sap_automation::abc::table::Table;
use sap_automation::utils::export_outcome::ExportOutcome;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[derive(Clone)]
enum Ov {
    NoData,
    Err,
    Table(Table),
}

#[derive(Default)]
struct Fake {
    calls: Vec<&'static str>,
    dates: Vec<RunDate>,
    vt11_args: Vec<(String, String)>,
    ships: Vec<String>,
    vl_layout: String,
    dels: Vec<String>,
    zm_variant: String,
    zm_date: Option<RunDate>,
    over: Vec<(&'static str, Ov)>,
}

impl Fake {
    fn serve(&mut self, step: &'static str, tcode: &str, file: &str) -> Result<StepOutput> {
        self.calls.push(step);
        if let Some((_, o)) = self.over.iter().find(|(s, _)| *s == step) {
            return match o.clone() {
                Ov::NoData => Ok(StepOutput::NoData("nothing".into())),
                Ov::Err => Err(anyhow!("sap exploded")),
                Ov::Table(t) => Ok(StepOutput::Table(t)),
            };
        }
        Ok(StepOutput::Table(load_real_in("xdock", tcode, file)))
    }
}

impl InvSteps for Fake {
    fn vt11(&mut self, date: &RunDate, variant: &str, layout: &str) -> Result<StepOutput> {
        self.dates.push(*date);
        self.vt11_args.push((variant.into(), layout.into()));
        self.serve("vt11", "vt11_inv", "raw_vt11.txt")
    }
    fn vl06o_by_shipments(&mut self, s: &[String], layout: &str) -> Result<StepOutput> {
        self.ships = s.to_vec();
        self.vl_layout = layout.into();
        self.serve("vl06o", "vl06o", "raw_vl06o.txt")
    }
    fn zmdetpc_summarised(
        &mut self,
        d: &[String],
        date: &RunDate,
        variant: &str,
    ) -> Result<StepOutput> {
        self.dels = d.to_vec();
        self.zm_date = Some(*date);
        self.zm_variant = variant.into();
        self.serve("zmdetpc", "zmdetpc", "raw_zmdetpc.txt")
    }
}

fn day() -> RunDate {
    RunDate {
        date1: NaiveDate::from_ymd_opt(2026, 11, 5).unwrap(),
        date2: None,
    }
}

fn temp_out(tag: &str) -> OutputSpec {
    let dir = std::env::temp_dir().join(format!("xdock_runner_{tag}_{}", std::process::id()));
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

#[test]
fn full_run_on_real_fixtures() {
    let out = temp_out("full");
    let mut f = Fake::default();
    let sel = InvSelection::xdock();
    let s = run_xdock(&mut f, &day(), &sel, &out).unwrap();

    assert_eq!(f.calls, vec!["vt11", "vl06o", "zmdetpc"]);
    assert_eq!(f.dates, vec![day()]);
    assert_eq!(
        f.vt11_args,
        vec![("XD_ALL_EPDC_".to_string(), "scorecard".to_string())]
    );
    // VL06O gets all 91 distinct shipments with layout xdock
    assert_eq!(f.ships.len(), 91);
    assert_eq!(f.ships.iter().collect::<HashSet<_>>().len(), 91);
    assert_eq!(f.vl_layout, "xdock");
    // ZMDETPC gets the 148 distinct deliveries, the run date and the variant
    assert_eq!(f.dels.len(), 148);
    assert_eq!(f.dels.iter().collect::<HashSet<_>>().len(), 148);
    assert_eq!(f.zm_date, Some(day()));
    assert_eq!(f.zm_variant, "EPDC_TONY");

    assert_eq!(s.export_date, "2026-11-05");
    assert_eq!(
        (s.shipments, s.deliveries, s.zmdetpc_rows, s.without_zmdetpc),
        (91, 148, 140, 8)
    );
    assert_eq!((s.vl06o_rows, s.pivot_rows), (148, 13));
    assert_eq!(s.total, "1493");

    // outputs: <reports_dir>\INV\<today>\xdock\<date>_XDOCK_<TABLE>.tsv
    assert_eq!(s.outputs.len(), 3);
    let base = out.reports_dir.join("INV").join("2026-11-10").join("xdock");
    assert_eq!(s.outputs[0], base.join("11-05-2026_XDOCK_VT11.tsv"));
    assert_eq!(s.outputs[1], base.join("11-05-2026_XDOCK_VL06O.tsv"));
    assert_eq!(s.outputs[2], base.join("11-05-2026_XDOCK_PIVOT.tsv"));
    let vt = read(&s.outputs[0], "vt11");
    assert_eq!((vt.nrows(), vt.headers.len()), (91, 13));
    let vl = read(&s.outputs[1], "vl06o");
    let x = load_fixture("xdock", "expected_vl06o.tsv");
    assert_eq!(vl.headers, x.headers[..12]);
    assert_eq!(vl.nrows(), 148);
    let pivot = read(&s.outputs[2], "pivot");
    assert_eq!(pivot.headers, ["plant", "total"]);
    assert_eq!(pivot.rows.last().unwrap(), &["(blank)", "4"]);
    assert_eq!(pivot.rows[0], ["FV33", "80"]);
    let _ = std::fs::remove_dir_all(&out.reports_dir);
}

#[test]
fn range_run_labels_date() {
    let out = temp_out("range");
    let mut f = Fake::default();
    let rd = RunDate {
        date1: NaiveDate::from_ymd_opt(2026, 11, 1).unwrap(),
        date2: Some(NaiveDate::from_ymd_opt(2026, 11, 7).unwrap()),
    };
    let s = run_xdock(&mut f, &rd, &InvSelection::xdock(), &out).unwrap();
    assert_eq!(s.export_date, "WK_45");
    assert_eq!(f.dates, vec![rd], "VT11 gets the whole range");
    assert_eq!(f.zm_date, Some(rd), "ZMDETPC gets the whole range");
    let vl = read(&s.outputs[1], "vl06o");
    let c = vl.col("date").unwrap();
    assert!(vl.rows.iter().all(|r| r[c] == "WK_45"));
    assert!(s.outputs[1].to_string_lossy().contains("WK_45_XDOCK_VL06O"));
    let _ = std::fs::remove_dir_all(&out.reports_dir);
}

fn run_fail(tag: &str, over: Vec<(&'static str, Ov)>) -> (anyhow::Error, Fake) {
    let out = temp_out(tag);
    let mut f = Fake {
        over,
        ..Fake::default()
    };
    let e = run_xdock(&mut f, &day(), &InvSelection::xdock(), &out).unwrap_err();
    assert_eq!(files_under(&out.reports_dir), 0, "no partial output");
    (e, f)
}

#[test]
fn no_data_stops_the_run_at_each_step() {
    for (step, calls) in [
        ("vt11", vec!["vt11"]),
        ("vl06o", vec!["vt11", "vl06o"]),
        ("zmdetpc", vec!["vt11", "vl06o", "zmdetpc"]),
    ] {
        let (e, f) = run_fail(&format!("nd_{step}"), vec![(step, Ov::NoData)]);
        let nd = e.downcast_ref::<NoDataError>().expect("NoDataError");
        assert_eq!(nd.step, step);
        assert_eq!(f.calls, calls);
    }
}

#[test]
fn empty_lists_stop_the_run() {
    let mut vt = load_real_in("xdock", "vt11_inv", "raw_vt11.txt");
    vt.rows.clear();
    vt.visible.clear();
    let (e, f) = run_fail("novt", vec![("vt11", Ov::Table(vt))]);
    assert_eq!(e.downcast_ref::<NoDataError>().unwrap().step, "vt11");
    assert_eq!(f.calls, vec!["vt11"]);

    let mut vl = load_real_in("xdock", "vl06o", "raw_vl06o.txt");
    vl.rows.clear();
    vl.visible.clear();
    let (e, f) = run_fail("novl", vec![("vl06o", Ov::Table(vl))]);
    assert_eq!(e.downcast_ref::<NoDataError>().unwrap().step, "vl06o");
    assert_eq!(f.calls, vec!["vt11", "vl06o"], "ZMDETPC is skipped");
}

#[test]
fn errors_and_missing_columns() {
    let (e, f) = run_fail("err", vec![("zmdetpc", Ov::Err)]);
    assert!(format!("{e:#}").contains("sap exploded"), "{e:#}");
    assert!(e.downcast_ref::<NoDataError>().is_none());
    assert_eq!(f.calls.last(), Some(&"zmdetpc"));

    let mut z = load_real_in("xdock", "zmdetpc", "raw_zmdetpc.txt");
    z.headers[1] = "something_else".into();
    let (e, _) = run_fail("cols", vec![("zmdetpc", Ov::Table(z))]);
    let m = format!("{e:#}");
    assert!(m.contains("missing column") && m.contains("pallet"), "{m}");

    let mut vl = load_real_in("xdock", "vl06o", "raw_vl06o.txt");
    vl.headers[5] = "something_else".into();
    let (e, _) = run_fail("cols2", vec![("vl06o", Ov::Table(vl))]);
    assert!(format!("{e:#}").contains("plant"), "{e:#}");
}

/// The production file loader (`outcome_to_output`) reads the real export of each tcode.
#[test]
fn outcome_adapter_loads_real_files() {
    for (tc, file) in [
        ("vt11_inv", "raw_vt11.txt"),
        ("vl06o", "raw_vl06o.txt"),
        ("zmdetpc", "raw_zmdetpc.txt"),
    ] {
        let p: PathBuf = [
            env!("CARGO_MANIFEST_DIR"),
            "tests",
            "fixtures",
            "xdock",
            "real",
            file,
        ]
        .iter()
        .collect();
        let out = outcome_to_output(tc, ExportOutcome::File(p), &RawOptions::default()).unwrap();
        let StepOutput::Table(t) = out else {
            panic!("expected a table")
        };
        assert_eq!(t.headers, load_real_in("xdock", tc, file).headers);
        assert!(t.nrows() > 0);
    }
}
