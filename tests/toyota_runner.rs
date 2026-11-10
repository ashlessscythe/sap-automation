//! `run_toyota` against a fake SAP seam serving the REAL toyota fixtures.
mod common;

use anyhow::{anyhow, Result};
use chrono::NaiveDate;
use common::load_real_in;
use sap_automation::abc::inv_runner::*;
use sap_automation::abc::run::RunDate;
use sap_automation::abc::runner::{NoDataError, OutputSpec, StepOutput};
use sap_automation::abc::table::Table;
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
    vt11_args: Vec<(String, String)>,
    ships: Vec<String>,
    vl_layout: String,
    dels: Vec<String>,
    zm_variant: String,
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
        Ok(StepOutput::Table(load_real_in("toyota", tcode, file)))
    }
}

impl InvSteps for Fake {
    fn vt11(&mut self, _date: &RunDate, variant: &str, layout: &str) -> Result<StepOutput> {
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
        _date: &RunDate,
        variant: &str,
    ) -> Result<StepOutput> {
        self.dels = d.to_vec();
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
    let dir = std::env::temp_dir().join(format!("toyota_runner_{tag}_{}", std::process::id()));
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
    let s = run_toyota(&mut f, &day(), &InvSelection::toyota(), &out).unwrap();

    assert_eq!(f.calls, vec!["vt11", "vl06o", "zmdetpc"]);
    assert_eq!(
        f.vt11_args,
        vec![("FG_TOYO_XD".to_string(), "scorecard".to_string())]
    );
    assert_eq!(f.ships, ["24605455", "24605498", "24605502"]);
    assert_eq!(f.vl_layout, "xdock");
    assert_eq!(
        f.dels,
        ["85628718", "85628715", "85628719"],
        "distinct, SAP order"
    );
    assert_eq!(f.zm_variant, "EPDC_TONY");

    assert_eq!(
        (s.shipments, s.deliveries, s.zmdetpc_rows, s.without_zmdetpc),
        (3, 3, 3, 0)
    );
    assert_eq!((s.vl06o_rows, s.pivot_rows, s.total.as_str()), (3, 1, "4"));
    assert_eq!(s.trailers, vec![("FV55".to_string(), "0".to_string())]);

    let base = out
        .reports_dir
        .join("INV")
        .join("2026-11-10")
        .join("toyota");
    let names = ["VT11", "VL06O", "PIVOT", "TRAILERS"];
    assert_eq!(s.outputs.len(), 4);
    for (p, n) in s.outputs.iter().zip(names) {
        assert_eq!(p, &base.join(format!("11-05-2026_TOYOTA_{n}.tsv")));
    }
    assert_eq!(read(&s.outputs[0], "vt11").nrows(), 3);
    assert_eq!(read(&s.outputs[1], "vl06o").nrows(), 3);
    let pivot = read(&s.outputs[2], "pivot");
    assert_eq!(pivot.headers, ["plant", "total"]);
    assert_eq!(pivot.rows, vec![vec!["FV55".to_string(), "4".to_string()]]);
    let tr = read(&s.outputs[3], "trailers");
    assert_eq!(tr.headers, ["plant", "trailers"]);
    assert_eq!(tr.rows, vec![vec!["FV55".to_string(), "0".to_string()]]);
    let _ = std::fs::remove_dir_all(&out.reports_dir);
}

#[test]
fn xdock_selection_is_independent() {
    // the shared flow must not write a trailers table for xdock
    let out = temp_out("xd");
    let mut f = Fake::default();
    let s = run_xdock(&mut f, &day(), &InvSelection::xdock(), &out).unwrap();
    assert_eq!(s.outputs.len(), 3);
    assert!(s.trailers.is_empty());
    assert!(s.outputs[0].to_string_lossy().contains("xdock"));
    let _ = std::fs::remove_dir_all(&out.reports_dir);
}

fn run_fail(tag: &str, over: Vec<(&'static str, Ov)>) -> (anyhow::Error, Fake) {
    let out = temp_out(tag);
    let mut f = Fake {
        over,
        ..Fake::default()
    };
    let e = run_toyota(&mut f, &day(), &InvSelection::toyota(), &out).unwrap_err();
    assert_eq!(files_under(&out.reports_dir), 0, "no partial output");
    (e, f)
}

#[test]
fn empty_results_stop_the_run() {
    for (step, calls) in [
        ("vt11", vec!["vt11"]),
        ("vl06o", vec!["vt11", "vl06o"]),
        ("zmdetpc", vec!["vt11", "vl06o", "zmdetpc"]),
    ] {
        let (e, f) = run_fail(&format!("nd_{step}"), vec![(step, Ov::NoData)]);
        assert_eq!(e.downcast_ref::<NoDataError>().unwrap().step, step);
        assert_eq!(f.calls, calls);
    }
    // header-only tables: no shipments / no deliveries
    let mut vt = load_real_in("toyota", "vt11_inv", "raw_vt11.txt");
    vt.rows.clear();
    vt.visible.clear();
    let (e, f) = run_fail("novt", vec![("vt11", Ov::Table(vt))]);
    assert_eq!(e.downcast_ref::<NoDataError>().unwrap().step, "vt11");
    assert_eq!(f.calls, vec!["vt11"]);
    let mut vl = load_real_in("toyota", "vl06o", "raw_vl06o.txt");
    vl.rows.clear();
    vl.visible.clear();
    let (e, f) = run_fail("novl", vec![("vl06o", Ov::Table(vl))]);
    assert_eq!(e.downcast_ref::<NoDataError>().unwrap().step, "vl06o");
    assert_eq!(f.calls, vec!["vt11", "vl06o"]);
}

#[test]
fn errors_and_missing_columns_name_the_report() {
    let (e, _) = run_fail("err", vec![("zmdetpc", Ov::Err)]);
    let m = format!("{e:#}");
    assert!(
        m.contains("sap exploded") && m.contains("toyota step 'zmdetpc'"),
        "{m}"
    );
    let mut z = load_real_in("toyota", "zmdetpc", "raw_zmdetpc.txt");
    z.headers[2] = "other".into();
    let (e, _) = run_fail("cols", vec![("zmdetpc", Ov::Table(z))]);
    let m = format!("{e:#}");
    assert!(m.contains("missing column") && m.contains("child"), "{m}");
}

#[test]
fn ragged_zmdetpc_without_rows_still_runs() {
    // every delivery lacks a ZMDETPC row: total = number_of_packages, pivot FV55 = 3
    let mut z = load_real_in("toyota", "zmdetpc", "raw_zmdetpc.txt");
    z.rows.clear();
    z.visible.clear();
    let out = temp_out("noz");
    let mut f = Fake {
        over: vec![("zmdetpc", Ov::Table(z))],
        ..Fake::default()
    };
    let s = run_toyota(&mut f, &day(), &InvSelection::toyota(), &out).unwrap();
    assert_eq!((s.without_zmdetpc, s.total.as_str()), (3, "3"));
    assert_eq!(s.trailers, vec![("FV55".to_string(), "0".to_string())]);
    let _ = std::fs::remove_dir_all(&out.reports_dir);
}
