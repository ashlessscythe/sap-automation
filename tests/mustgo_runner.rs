//! `run_mustgo` against a fake SAP seam serving the REAL fixtures.
mod common;

use anyhow::{anyhow, Result};
use chrono::NaiveDate;
use common::{assert_tables_equal, load_fixture, load_real};
use sap_automation::abc::raw::RawOptions;
use sap_automation::abc::run::RunDate;
use sap_automation::abc::runner::*;
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
    ships: Vec<String>,
    dels: Vec<String>,
    tos: Vec<String>,
    last_dels: Vec<String>,
    last_tos: Vec<String>,
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
        Ok(StepOutput::Table(load_real(tcode, file)))
    }
}

impl SapSteps for Fake {
    fn vt11(&mut self, date: &RunDate) -> Result<StepOutput> {
        self.dates.push(*date);
        self.serve("vt11", "vt11", "raw_vt11.txt")
    }
    fn vl06o(&mut self, s: &[String]) -> Result<StepOutput> {
        self.ships = s.to_vec();
        self.serve("vl06o", "vl06o", "raw_vl06o.txt")
    }
    fn zmdesnr_by_deliveries(&mut self, d: &[String]) -> Result<StepOutput> {
        self.dels = d.to_vec();
        self.serve("zmdesnr_by_deliveries", "zmdesnr", "raw_zmdesnr_del.txt")
    }
    fn lt23(&mut self, t: &[String]) -> Result<StepOutput> {
        self.tos = t.to_vec();
        self.serve("lt23", "lt23", "raw_lt23.txt")
    }
    fn zmdesnr_by_deliveries_and_tos(&mut self, d: &[String], t: &[String]) -> Result<StepOutput> {
        self.last_dels = d.to_vec();
        self.last_tos = t.to_vec();
        self.serve(
            "zmdesnr_by_deliveries_and_tos",
            "zmdesnr",
            "raw_zmdesnr_del_to.txt",
        )
    }
}

fn day() -> RunDate {
    RunDate {
        date1: NaiveDate::from_ymd_opt(2026, 11, 5).unwrap(),
        date2: None,
    }
}

fn temp_out(tag: &str) -> OutputSpec {
    let dir = std::env::temp_dir().join(format!("mustgo_runner_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    OutputSpec {
        reports_dir: dir,
        today: NaiveDate::from_ymd_opt(2026, 11, 9).unwrap(),
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

fn col_set(t: &Table, c: &str) -> HashSet<String> {
    let i = t.col(c).unwrap();
    t.rows.iter().map(|r| r[i].clone()).collect()
}

fn distinct(v: &[String]) -> bool {
    v.iter().collect::<HashSet<_>>().len() == v.len()
}

#[test]
fn full_run_on_real_fixtures() {
    let out = temp_out("full");
    let mut f = Fake::default();
    let s = run_mustgo(&mut f, &day(), &out).unwrap();

    assert_eq!(
        f.calls,
        vec![
            "vt11",
            "vl06o",
            "zmdesnr_by_deliveries",
            "lt23",
            "zmdesnr_by_deliveries_and_tos"
        ]
    );
    assert_eq!(f.dates, vec![day()]);
    // VL06O gets exactly the distinct TPDC shipments
    assert_eq!(f.ships.len(), 54);
    assert!(distinct(&f.ships));
    let vt11 = load_real("vt11", "raw_vt11.txt");
    let tp = vt11.col("transportplanningpt").unwrap();
    let sh = vt11.col("shipment_number").unwrap();
    let want: HashSet<String> = vt11
        .rows
        .iter()
        .filter(|r| r[tp] == "TPDC")
        .map(|r| r[sh].clone())
        .collect();
    assert_eq!(f.ships.iter().cloned().collect::<HashSet<_>>(), want);
    // ZMDESNR by deliveries gets the 107 distinct VL06O deliveries
    assert_eq!(f.dels.len(), 107);
    assert!(distinct(&f.dels));
    // LT23 gets the 87 distinct TOs
    assert_eq!(f.tos.len(), 87);
    assert!(distinct(&f.tos));

    // Step 19 lists: spec 5.1 rows 17-19 use sn_del AFTER the row-18 deletion (128 rows, 10
    // deliveries, 15 TOs). These are exactly the deliveries and TOs of the real 2a file, and
    // that file is precisely what those lists select from the unfiltered sn_del (476 rows).
    let to_file = load_real("zmdesnr", "raw_zmdesnr_del_to.txt");
    assert_eq!(f.last_dels.len(), 10);
    assert_eq!(f.last_tos.len(), 15);
    assert!(distinct(&f.last_dels) && distinct(&f.last_tos));
    assert_eq!(
        f.last_dels.iter().cloned().collect::<HashSet<_>>(),
        col_set(&to_file, "delivery")
    );
    assert_eq!(
        f.last_tos.iter().cloned().collect::<HashSet<_>>(),
        col_set(&to_file, "transfer_order_number")
    );
    let orig = load_real("zmdesnr", "raw_zmdesnr_del.txt");
    let (d, t) = (
        orig.col("delivery").unwrap(),
        orig.col("transfer_order_number").unwrap(),
    );
    let (ds, ts): (HashSet<_>, HashSet<_>) = (
        f.last_dels.iter().cloned().collect(),
        f.last_tos.iter().cloned().collect(),
    );
    let selected = orig
        .rows
        .iter()
        .filter(|r| ds.contains(&r[d]) && ts.contains(&r[t]))
        .count();
    assert_eq!(selected, 476);
    assert_eq!(to_file.nrows(), 476);

    // outputs
    assert_eq!(s.export_date, "2026-11-05");
    assert_eq!(
        (s.shipments, s.deliveries, s.transfer_orders),
        (54, 107, 87)
    );
    assert_eq!((s.lt23_rows, s.snr_copy_rows, s.pivot_rows), (15, 92, 6));
    assert_eq!(s.outputs.len(), 3);
    let read =
        |p: &PathBuf, n: &str| Table::from_tsv(n, &std::fs::read_to_string(p).unwrap()).unwrap();
    assert_tables_equal(
        &load_fixture("mustgo", "expected_lt23.tsv"),
        &read(&s.outputs[0], "lt23"),
    );
    assert_tables_equal(
        &load_fixture("mustgo", "expected_snr_copy.tsv"),
        &read(&s.outputs[1], "snr"),
    );
    let pivot = read(&s.outputs[2], "pivot");
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
    let _ = std::fs::remove_dir_all(&out.reports_dir);
}

#[test]
fn range_run_labels_export_date() {
    let out = temp_out("range");
    let mut f = Fake::default();
    let rd = RunDate {
        date1: NaiveDate::from_ymd_opt(2026, 11, 1).unwrap(),
        date2: Some(NaiveDate::from_ymd_opt(2026, 11, 7).unwrap()),
    };
    let s = run_mustgo(&mut f, &rd, &out).unwrap();
    assert_eq!(s.export_date, "WK_45");
    assert_eq!(f.dates, vec![rd], "VT11 gets the whole range");
    let snr = Table::from_tsv("s", &std::fs::read_to_string(&s.outputs[1]).unwrap()).unwrap();
    assert!(snr.rows.iter().all(|r| r.last().unwrap() == "WK_45"));
    assert!(s.outputs[0].to_string_lossy().contains("WK_45_MUSTGO_LT23"));
    let _ = std::fs::remove_dir_all(&out.reports_dir);
}

fn run_fail(tag: &str, over: Vec<(&'static str, Ov)>) -> (anyhow::Error, Fake, OutputSpec) {
    let out = temp_out(tag);
    let mut f = Fake {
        over,
        ..Fake::default()
    };
    let e = run_mustgo(&mut f, &day(), &out).unwrap_err();
    assert_eq!(files_under(&out.reports_dir), 0, "no partial output");
    (e, f, out)
}

#[test]
fn vt11_no_data_stops_the_block() {
    let (e, f, _) = run_fail("vt11nd", vec![("vt11", Ov::NoData)]);
    let nd = e.downcast_ref::<NoDataError>().expect("NoDataError");
    assert_eq!(nd.step, "vt11");
    assert_eq!(f.calls, vec!["vt11"]);
}

#[test]
fn no_tpdc_rows() {
    let mut vt11 = load_real("vt11", "raw_vt11.txt");
    let c = vt11.col("transportplanningpt").unwrap();
    vt11.rows.retain(|r| r[c] != "TPDC");
    vt11.visible = vec![true; vt11.rows.len()];
    let (e, f, _) = run_fail("notpdc", vec![("vt11", Ov::Table(vt11))]);
    assert!(format!("{e:#}").contains("TPDC"), "{e:#}");
    assert_eq!(f.calls, vec!["vt11"]);
}

#[test]
fn vl06o_no_data() {
    let (e, f, _) = run_fail("vlnd", vec![("vl06o", Ov::NoData)]);
    assert_eq!(e.downcast_ref::<NoDataError>().unwrap().step, "vl06o");
    assert_eq!(f.calls, vec!["vt11", "vl06o"]);
}

#[test]
fn empty_lt23_result() {
    // a single TO row: post-process leaves nothing
    let mut lt = load_real("lt23", "raw_lt23.txt");
    lt.rows.truncate(1);
    lt.visible = vec![true];
    let (e, f, _) = run_fail("ltempty", vec![("lt23", Ov::Table(lt))]);
    assert_eq!(
        e.downcast_ref::<NoDataError>().unwrap().step,
        "lt23 post-process"
    );
    assert_eq!(f.calls.len(), 4, "final ZMDESNR call is skipped");
}

#[test]
fn error_mid_flow_and_missing_columns() {
    let (e, f, _) = run_fail("err", vec![("lt23", Ov::Err)]);
    assert!(format!("{e:#}").contains("sap exploded"), "{e:#}");
    assert!(e.downcast_ref::<NoDataError>().is_none());
    assert_eq!(f.calls.last(), Some(&"lt23"));

    let mut vl = load_real("vl06o", "raw_vl06o.txt");
    vl.headers[0] = "something_else".into();
    let (e, _, _) = run_fail("cols", vec![("vl06o", Ov::Table(vl))]);
    let m = format!("{e:#}");
    assert!(
        m.contains("missing column") && m.contains("delivery"),
        "{m}"
    );
}

#[test]
fn outcome_adapter_loads_real_file() {
    let p: PathBuf = [
        env!("CARGO_MANIFEST_DIR"),
        "tests",
        "fixtures",
        "mustgo",
        "real",
        "raw_lt23.txt",
    ]
    .iter()
    .collect();
    let out = outcome_to_output("lt23", ExportOutcome::File(p), &RawOptions::default()).unwrap();
    let StepOutput::Table(t) = out else {
        panic!("expected a table")
    };
    assert_tables_equal(&load_real("lt23", "raw_lt23.txt"), &t);
}
