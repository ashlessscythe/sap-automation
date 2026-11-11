//! `run_k2xx` against a fake SAP seam serving the REAL exports of 2026-11-05
//! (tests/fixtures/k2xx/real/, see the README there).
mod common;

use anyhow::{anyhow, Result};
use chrono::NaiveDate;
use common::{assert_tables_equal, load_fixture, load_real_in};
use sap_automation::abc::k2xx_runner::*;
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
    vt11_args: Vec<(String, String, bool)>,
    ships: Vec<String>,
    vl_layout: String,
    zv_dels: Vec<String>,
    zv_window: Option<(NaiveDate, NaiveDate)>,
    zv_args: (String, String),
    zm_dels: Vec<String>,
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
        Ok(StepOutput::Table(load_real_in("k2xx", tcode, file)))
    }
}

impl K2xxSteps for Fake {
    fn vt11(
        &mut self,
        date: &RunDate,
        variant: &str,
        layout: &str,
        monday_include_sunday: bool,
    ) -> Result<StepOutput> {
        self.dates.push(*date);
        self.vt11_args
            .push((variant.into(), layout.into(), monday_include_sunday));
        self.serve("vt11", "vt11_inv", "raw_vt11.txt")
    }
    fn vl06o_by_shipments(&mut self, s: &[String], layout: &str) -> Result<StepOutput> {
        self.ships = s.to_vec();
        self.vl_layout = layout.into();
        self.serve("vl06o", "vl06o", "raw_vl06o.txt")
    }
    fn zvt11_closed(
        &mut self,
        d: &[String],
        from: NaiveDate,
        to: NaiveDate,
        variant: &str,
        layout: &str,
    ) -> Result<StepOutput> {
        self.zv_dels = d.to_vec();
        self.zv_window = Some((from, to));
        self.zv_args = (variant.into(), layout.into());
        self.serve("zvt11", "zvt11", "raw_zvt11.txt")
    }
    fn zmdetpc_summarised(
        &mut self,
        d: &[String],
        date: &RunDate,
        variant: &str,
    ) -> Result<StepOutput> {
        self.zm_dels = d.to_vec();
        self.zm_date = Some(*date);
        self.zm_variant = variant.into();
        self.serve("zmdetpc", "zmdetpc", "raw_zmdetpc.txt")
    }
}

fn ymd(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).unwrap()
}

fn day() -> RunDate {
    RunDate {
        date1: ymd(2026, 11, 5),
        date2: None,
    }
}

fn temp_out(tag: &str) -> OutputSpec {
    let dir = std::env::temp_dir().join(format!("k2xx_runner_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    OutputSpec {
        reports_dir: dir,
        today: ymd(2026, 11, 10),
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
fn full_run_on_the_stand_in_fixtures() {
    let out = temp_out("full");
    let mut f = Fake::default();
    let sel = K2xxSelection::default();
    let s = run_k2xx(&mut f, &day(), &sel, &out).unwrap();

    // spec 5.5 order: VT11, VL06O, ZVT11 Closed, ZMDETPC
    assert_eq!(f.calls, vec!["vt11", "vl06o", "zvt11", "zmdetpc"]);
    assert_eq!(f.dates, vec![day()]);
    assert_eq!(
        f.vt11_args,
        vec![("VT11_K2XX_VARIANT".to_string(), "scorecard".to_string(), true)]
    );
    assert_eq!(f.ships.len(), 53);
    assert_eq!(f.ships.iter().collect::<HashSet<_>>().len(), 53);
    assert_eq!(f.vl_layout, "pallets");
    // ZVT11 Closed and ZMDETPC get the same 107 distinct deliveries
    assert_eq!(f.zv_dels.len(), 107);
    assert_eq!(f.zv_dels.iter().collect::<HashSet<_>>().len(), 107);
    assert_eq!(f.zm_dels, f.zv_dels);
    // run date -/+ 100 days
    assert_eq!(f.zv_window, Some((ymd(2026, 7, 28), ymd(2027, 2, 13))));
    assert_eq!(f.zv_args, ("k2xx_".to_string(), "add_del".to_string()));
    assert_eq!(f.zm_date, Some(day()));
    assert_eq!(f.zm_variant, "ZMDETPC_VARIANT");

    assert_eq!(s.export_date, "2026-11-05");
    assert_eq!((s.shipments, s.deliveries), (53, 107));
    assert_eq!(
        (s.zvt11_rows, s.zmdetpc_rows, s.without_zmdetpc),
        (409, 106, 1)
    );
    assert_eq!((s.vl06o_rows, s.pivot_rows), (107, 12));
    assert_eq!(s.total, "1148");

    // outputs: <reports_dir>\K2XX\<today>\k2xx\<date>_K2XX_<TABLE>.tsv, macro sheet order, then
    // the pivot (owner decision, not a sheet of the macro workbook)
    assert_eq!(s.outputs.len(), 4);
    let base = out.reports_dir.join("K2XX").join("2026-11-10").join("k2xx");
    assert_eq!(s.outputs[0], base.join("11-05-2026_K2XX_ZVT11.tsv"));
    assert_eq!(s.outputs[1], base.join("11-05-2026_K2XX_VT11.tsv"));
    assert_eq!(s.outputs[2], base.join("11-05-2026_K2XX_VL06O.tsv"));
    assert_eq!(s.outputs[3], base.join("11-05-2026_K2XX_PIVOT.tsv"));
    assert!(s.outputs.iter().all(|p| p.is_file()));
    assert_eq!(files_under(&out.reports_dir), 4);
    assert_tables_equal(
        &load_fixture("k2xx", "expected_real_zvt11.tsv"),
        &read(&s.outputs[0], "expected_real_zvt11.tsv"),
    );
    let vt = read(&s.outputs[1], "vt11");
    assert_eq!((vt.nrows(), vt.headers.len()), (53, 13));
    assert_tables_equal(
        &load_fixture("k2xx", "expected_real_vl06o.tsv"),
        &read(&s.outputs[2], "expected_real_vl06o.tsv"),
    );
    // pivot: plant / sum of total of the vl06o output, long form, no grand-total row
    let pivot = read(&s.outputs[3], "expected_real_pivot.tsv");
    assert_eq!(pivot.headers, ["plant", "total"]);
    assert_tables_equal(&load_fixture("k2xx", "expected_real_pivot.tsv"), &pivot);
    assert_eq!(pivot.rows[0], ["PL33", "32"]);
    let _ = std::fs::remove_dir_all(&out.reports_dir);
}

fn pivot_path(out: &OutputSpec) -> PathBuf {
    out.reports_dir
        .join("K2XX")
        .join("2026-11-10")
        .join("k2xx")
        .join("11-05-2026_K2XX_PIVOT.tsv")
}

/// The pivot is written with the other outputs, only after the last SAP step (ZMDETPC) and the
/// merge succeeded: a failure of the last step leaves no pivot (and no other file) behind.
#[test]
fn pivot_is_written_only_after_the_last_step_succeeds() {
    // a ZMDETPC table without `child` passes the SAP call but fails the column check
    let mut bad = load_real_in("k2xx", "zmdetpc", "raw_zmdetpc.txt");
    bad.headers[2] = "something_else".into();
    for (tag, ov) in [
        ("piv_nd", Ov::NoData),
        ("piv_err", Ov::Err),
        ("piv_cols", Ov::Table(bad)),
    ] {
        let out = temp_out(tag);
        let mut f = Fake {
            over: vec![("zmdetpc", ov)],
            ..Fake::default()
        };
        assert!(run_k2xx(&mut f, &day(), &K2xxSelection::default(), &out).is_err());
        assert_eq!(f.calls, vec!["vt11", "vl06o", "zvt11", "zmdetpc"]);
        assert!(!pivot_path(&out).exists(), "{tag}: no pivot");
        assert_eq!(files_under(&out.reports_dir), 0, "{tag}: no partial output");
    }

    // the same seam with a working last step writes it, as the last output
    let out = temp_out("piv_ok");
    let mut f = Fake::default();
    let s = run_k2xx(&mut f, &day(), &K2xxSelection::default(), &out).unwrap();
    assert_eq!(f.calls.last(), Some(&"zmdetpc"));
    assert!(pivot_path(&out).is_file());
    assert_eq!(s.outputs.last().unwrap(), &pivot_path(&out));
    let _ = std::fs::remove_dir_all(&out.reports_dir);
}

/// A blank plant becomes the `(blank)` row, last; a delivery without a ZMDETPC row adds nothing.
#[test]
fn pivot_puts_a_blank_plant_last() {
    let out = temp_out("piv_blank");
    let mut vl = load_real_in("k2xx", "vl06o", "raw_vl06o.txt");
    let (dc, pc) = (vl.col("delivery").unwrap(), vl.col("plant").unwrap());
    // 85626834 (PL50, total 2) and 8012734818 (PL63, no ZMDETPC row) lose their plant
    for r in vl.rows.iter_mut() {
        if r[dc] == "85626834" || r[dc] == "8012734818" {
            r[pc].clear();
        }
    }
    let mut f = Fake {
        over: vec![("vl06o", Ov::Table(vl))],
        ..Fake::default()
    };
    let s = run_k2xx(&mut f, &day(), &K2xxSelection::default(), &out).unwrap();
    let pivot = read(&s.outputs[3], "pivot");
    assert_eq!(s.pivot_rows, 13);
    assert_eq!(pivot.rows.last().unwrap(), &["(blank)", "2"]);
    let row = |plant: &str| pivot.rows.iter().find(|r| r[0] == plant).unwrap()[1].clone();
    assert_eq!(
        (row("PL50"), row("PL63")),
        ("92".to_string(), "2".to_string())
    );
    assert_eq!(s.total, "1148");
    let _ = std::fs::remove_dir_all(&out.reports_dir);
}

#[test]
fn selection_is_passed_through() {
    let out = temp_out("sel");
    let mut f = Fake::default();
    let sel = K2xxSelection {
        vt11_variant: "V".into(),
        vt11_layout: "L".into(),
        monday_include_sunday: false,
        vl06o_layout: "P".into(),
        zvt11_variant: "ZV".into(),
        zvt11_layout: "ZL".into(),
        zvt11_days: 10,
        zmdetpc_variant: "ZM".into(),
    };
    run_k2xx(&mut f, &day(), &sel, &out).unwrap();
    assert_eq!(f.vt11_args, vec![("V".into(), "L".into(), false)]);
    assert_eq!(f.vl_layout, "P");
    assert_eq!(f.zv_args, ("ZV".to_string(), "ZL".to_string()));
    assert_eq!(f.zv_window, Some((ymd(2026, 10, 26), ymd(2026, 11, 15))));
    assert_eq!(f.zm_variant, "ZM");
    let _ = std::fs::remove_dir_all(&out.reports_dir);
}

#[test]
fn range_run_labels_date_and_widens_both_ends() {
    let out = temp_out("range");
    let mut f = Fake::default();
    let rd = RunDate {
        date1: ymd(2026, 11, 1),
        date2: Some(ymd(2026, 11, 7)),
    };
    let s = run_k2xx(&mut f, &rd, &K2xxSelection::default(), &out).unwrap();
    assert_eq!(s.export_date, "WK_45");
    assert_eq!(f.dates, vec![rd], "VT11 gets the whole range");
    assert_eq!(f.zm_date, Some(rd), "ZMDETPC gets the whole range");
    assert_eq!(f.zv_window, Some((ymd(2026, 7, 24), ymd(2027, 2, 15))));
    let vl = read(&s.outputs[2], "vl06o");
    let c = vl.col("date").unwrap();
    assert!(vl.rows.iter().all(|r| r[c] == "WK_45"));
    assert!(s.outputs[2].to_string_lossy().contains("WK_45_K2XX_VL06O"));
    let _ = std::fs::remove_dir_all(&out.reports_dir);
}

fn run_fail(tag: &str, over: Vec<(&'static str, Ov)>) -> (anyhow::Error, Fake) {
    let out = temp_out(tag);
    let mut f = Fake {
        over,
        ..Fake::default()
    };
    let e = run_k2xx(&mut f, &day(), &K2xxSelection::default(), &out).unwrap_err();
    assert_eq!(files_under(&out.reports_dir), 0, "no partial output");
    (e, f)
}

#[test]
fn no_data_stops_the_run_at_each_step() {
    for (step, calls) in [
        ("vt11", vec!["vt11"]),
        ("vl06o", vec!["vt11", "vl06o"]),
        ("zmdetpc", vec!["vt11", "vl06o", "zvt11", "zmdetpc"]),
    ] {
        let (e, f) = run_fail(&format!("nd_{step}"), vec![(step, Ov::NoData)]);
        let nd = e.downcast_ref::<NoDataError>().expect("NoDataError");
        assert_eq!(nd.step, step);
        assert_eq!(f.calls, calls);
    }
}

#[test]
fn empty_lists_stop_the_run() {
    let empty = |tcode: &str, file: &str| {
        let mut t = load_real_in("k2xx", tcode, file);
        t.rows.clear();
        t.visible.clear();
        t
    };
    let (e, f) = run_fail(
        "novt",
        vec![("vt11", Ov::Table(empty("vt11_inv", "raw_vt11.txt")))],
    );
    assert_eq!(e.downcast_ref::<NoDataError>().unwrap().step, "vt11");
    assert_eq!(f.calls, vec!["vt11"]);

    let (e, f) = run_fail(
        "novl",
        vec![("vl06o", Ov::Table(empty("vl06o", "raw_vl06o.txt")))],
    );
    assert_eq!(e.downcast_ref::<NoDataError>().unwrap().step, "vl06o");
    assert_eq!(f.calls, vec!["vt11", "vl06o"], "ZVT11 is skipped");
    // an empty ZVT11 list does NOT stop the run: see `empty_zvt11_does_not_stop_the_run`
}

#[test]
fn errors_and_missing_columns() {
    let (e, f) = run_fail("err", vec![("zvt11", Ov::Err)]);
    assert!(format!("{e:#}").contains("sap exploded"), "{e:#}");
    assert!(e.downcast_ref::<NoDataError>().is_none());
    assert_eq!(f.calls.last(), Some(&"zvt11"));

    let mut z = load_real_in("k2xx", "zmdetpc", "raw_zmdetpc.txt");
    z.headers[1] = "something_else".into();
    let (e, _) = run_fail("cols", vec![("zmdetpc", Ov::Table(z))]);
    let m = format!("{e:#}");
    assert!(m.contains("missing column") && m.contains("pallet"), "{m}");

    let mut vl = load_real_in("k2xx", "vl06o", "raw_vl06o.txt");
    vl.headers[0] = "something_else".into();
    let (e, _) = run_fail("cols2", vec![("vl06o", Ov::Table(vl))]);
    assert!(format!("{e:#}").contains("delivery"), "{e:#}");

    // the pivot needs `plant`: a VL06O list without it stops the run before ZVT11
    let mut vl = load_real_in("k2xx", "vl06o", "raw_vl06o.txt");
    let pc = vl.col("plant").unwrap();
    vl.headers[pc] = "something_else".into();
    let (e, f) = run_fail("cols3", vec![("vl06o", Ov::Table(vl))]);
    let m = format!("{e:#}");
    assert!(m.contains("missing column") && m.contains("plant"), "{m}");
    assert_eq!(f.calls, vec!["vt11", "vl06o"]);
}

/// ZVT11 is written as exported, but its `delivery_number` / `shipment_number` columns must be
/// there (a wrong layout stops the run, nothing is written); VL06O only needs `delivery` and
/// `plant`, extra columns are not checked.
#[test]
fn zvt11_needs_its_key_columns_and_extra_vl06o_columns_are_not_checked() {
    let out = temp_out("anyhdr");
    let mut zv = load_real_in("k2xx", "zvt11", "raw_zvt11.txt");
    let dc = zv.col("delivery_number").unwrap();
    zv.headers[dc] = "c3".into();
    let mut f = Fake {
        over: vec![("zvt11", Ov::Table(zv))],
        ..Fake::default()
    };
    assert!(run_k2xx(&mut f, &day(), &K2xxSelection::default(), &out).is_err());
    assert_eq!(files_under(&out.reports_dir), 0);
    let mut vl = load_real_in("k2xx", "vl06o", "raw_vl06o.txt");
    let pk = vl.col("number_of_packages").unwrap();
    vl.headers[pk] = "nopk_renamed".into();
    let mut f = Fake {
        over: vec![("vl06o", Ov::Table(vl))],
        ..Fake::default()
    };
    let s = run_k2xx(&mut f, &day(), &K2xxSelection::default(), &out).unwrap();
    assert_eq!(read(&s.outputs[0], "zvt11").headers[0], "date");
    assert_eq!(s.total, "1148");
    let _ = std::fs::remove_dir_all(&out.reports_dir);
}

/// The production file loader (`outcome_to_output`) reads each real export.
#[test]
fn outcome_adapter_loads_the_files() {
    for (tc, file) in [
        ("vt11_inv", "raw_vt11.txt"),
        ("vl06o", "raw_vl06o.txt"),
        ("zvt11", "raw_zvt11.txt"),
        ("zmdetpc", "raw_zmdetpc.txt"),
    ] {
        let p: PathBuf = [
            env!("CARGO_MANIFEST_DIR"),
            "tests",
            "fixtures",
            "k2xx",
            "real",
            file,
        ]
        .iter()
        .collect();
        let out = outcome_to_output(tc, ExportOutcome::File(p), &RawOptions::default()).unwrap();
        let StepOutput::Table(t) = out else {
            panic!("expected a table")
        };
        assert_eq!(t.headers, load_real_in("k2xx", tc, file).headers);
        assert!(t.nrows() > 0);
    }
}

/// Owner decision: an empty ZVT11 list does not stop k2xx. The ZVT11 output is header-only
/// (the 7 `add_del` columns), the other outputs and the summary are unchanged.
#[test]
fn empty_zvt11_does_not_stop_the_run() {
    let ok_empty = {
        let mut t = load_real_in("k2xx", "zvt11", "raw_zvt11.txt");
        t.rows.clear();
        t
    };
    for (tag, ov) in [
        ("zv_nodata", Ov::NoData),
        ("zv_empty", Ov::Table(ok_empty)),
        (
            "zv_notitles",
            Ov::Table(Table::new("z", Vec::new(), Vec::new())),
        ),
    ] {
        let out = temp_out(tag);
        let mut f = Fake {
            over: vec![("zvt11", ov)],
            ..Fake::default()
        };
        let s = run_k2xx(&mut f, &day(), &K2xxSelection::default(), &out).unwrap();
        assert_eq!(f.calls, vec!["vt11", "vl06o", "zvt11", "zmdetpc"], "{tag}");
        assert_eq!(s.zvt11_rows, 0, "{tag}");
        assert_eq!(s.outputs.len(), 4, "{tag}");
        let zv = read(&s.outputs[0], "zvt11");
        assert_eq!(
            zv.headers,
            sap_automation::abc::k2xx::ZVT11_HEADERS,
            "{tag}"
        );
        assert_eq!(zv.nrows(), 0, "{tag}");
        assert_tables_equal(
            &load_fixture("k2xx", "expected_real_vl06o.tsv"),
            &read(&s.outputs[2], "vl06o"),
        );
        assert_eq!(s.total, "1148");
        let _ = std::fs::remove_dir_all(&out.reports_dir);
    }
}
