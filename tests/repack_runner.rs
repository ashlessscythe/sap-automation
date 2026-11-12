//! `run_repack` against a fake SAP seam serving the SYNTHETIC exports
//! (tests/fixtures/repack/synthetic/, see the README there). The delivery list is read from a
//! k2xx `vl06o` TSV and the exclusion list from a relabel `combined` TSV, both written to the
//! paths of the same run (the stand-ins of tests/fixtures/repack/).
mod common;

use anyhow::{anyhow, Result};
use chrono::NaiveDate;
use common::{assert_tables_equal, load_fixture, load_synthetic_in};
use sap_automation::abc::relabel_runner::k2xx_vl06o_path;
use sap_automation::abc::repack_runner::*;
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
    dels: Vec<String>,
    exclude: Vec<String>,
    del_args: (String, String),
    serials: Vec<String>,
    hist_args: (String, String),
    over: Vec<(&'static str, Ov)>,
}

impl Fake {
    fn serve(&mut self, step: &'static str, file: &str) -> Result<StepOutput> {
        self.calls.push(step);
        if let Some((_, o)) = self.over.iter().find(|(s, _)| *s == step) {
            return match o.clone() {
                Ov::NoData => Ok(StepOutput::NoData("nothing".into())),
                Ov::Err => Err(anyhow!("sap exploded")),
                Ov::Table(t) => Ok(StepOutput::Table(t)),
            };
        }
        Ok(StepOutput::Table(load_synthetic_in(
            "repack", "zmdesnr", file,
        )))
    }
}

impl RepackSteps for Fake {
    fn zmdesnr_by_deliveries_excluding(
        &mut self,
        d: &[String],
        e: &[String],
        variant: &str,
        layout: &str,
    ) -> Result<StepOutput> {
        self.dels = d.to_vec();
        self.exclude = e.to_vec();
        self.del_args = (variant.into(), layout.into());
        self.serve("by_deliveries", "raw_zmdesnr_del.txt")
    }
    fn zmdesnr_history(&mut self, s: &[String], variant: &str, layout: &str) -> Result<StepOutput> {
        self.serials = s.to_vec();
        self.hist_args = (variant.into(), layout.into());
        self.serve("history", "raw_zmdesnr_hist.txt")
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

fn fixture_text(rel: &str) -> String {
    let mut p: PathBuf = [env!("CARGO_MANIFEST_DIR"), "tests", "fixtures", "repack"]
        .iter()
        .collect();
    p.push(rel);
    std::fs::read_to_string(p).unwrap()
}

fn temp_out(tag: &str) -> OutputSpec {
    let dir = std::env::temp_dir().join(format!("repack_runner_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    OutputSpec {
        reports_dir: dir,
        today: ymd(2026, 11, 10),
    }
}

fn put(p: &Path, text: &str) {
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, text).unwrap();
}

/// Both inputs of the run in place (k2xx stand-in with 104 deliveries, relabel stand-in with 5
/// yellow rows).
fn prepared(tag: &str) -> OutputSpec {
    let out = temp_out(tag);
    put(
        &k2xx_vl06o_path(&out, &day()),
        &fixture_text("k2xx_vl06o_stand_in.tsv"),
    );
    put(
        &relabel_combined_path(&out, &day()),
        &fixture_text("relabel_combined_stand_in.tsv"),
    );
    out
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

fn sel() -> RepackSelection {
    RepackSelection::default()
}

fn cleanup(out: &OutputSpec) {
    let _ = std::fs::remove_dir_all(&out.reports_dir);
}

#[test]
fn happy_path_writes_the_three_tables() {
    let out = prepared("ok");
    let mut f = Fake::default();
    let s = run_repack(&mut f, &day(), &sel(), &out).unwrap();
    assert_eq!(f.calls, ["by_deliveries", "history"]);
    assert_eq!(f.dels.len(), 104);
    assert_eq!(
        f.exclude,
        [
            "130007102",
            "130007101",
            "130007100",
            "130007099",
            "130007098"
        ]
    );
    assert_eq!(
        f.del_args,
        (
            "ZMDESNR_REPACK_VARIANT".into(),
            "ZMDESNR_REPACK_LAYOUT".into()
        )
    );
    assert_eq!(f.hist_args, f.del_args);
    assert_eq!(f.serials, ["900000003", "900000013", "900000020"]);
    assert_eq!(s.export_date, "2026-11-05");
    assert_eq!((s.deliveries, s.yellow_rows, s.excluded), (104, 5, 5));
    assert_eq!((s.repack_sn_rows, s.alias_rows), (22, 2));
    assert_eq!(s.blocks, [6, 3, 5, 4, 5, 3]);
    assert_eq!((s.history_serials, s.history_rows), (3, 7));
    assert_eq!((s.snr_rows, s.total.as_str(), s.pivot_rows), (13, "13", 6));
    let base = out
        .reports_dir
        .join("K2XX")
        .join("2026-11-10")
        .join("repack");
    assert_eq!(s.outputs.len(), 3);
    assert_eq!(s.outputs[0], base.join("11-05-2026_REPACK_SNR_REPACK.tsv"));
    assert_tables_equal(
        &load_fixture("repack", "synthetic/expected_snr_repack.tsv"),
        &read(&s.outputs[0], "snr_repack"),
    );
    assert_tables_equal(
        &load_synthetic_in("repack", "zmdesnr", "raw_zmdesnr_del.txt"),
        &read(&s.outputs[1], "repack_sn"),
    );
    assert_tables_equal(
        &load_fixture("repack", "synthetic/expected_pivot.tsv"),
        &read(&s.outputs[2], "pivot"),
    );
    cleanup(&out);
}

#[test]
fn rules_and_names_are_passed_through() {
    let out = prepared("sel");
    let mut f = Fake::default();
    let sel = RepackSelection {
        zmdesnr_variant: "V1".into(),
        zmdesnr_layout: "L1".into(),
        ..RepackSelection::default()
    };
    run_repack(&mut f, &day(), &sel, &out).unwrap();
    assert_eq!(f.del_args, ("V1".into(), "L1".into()));
    assert_eq!(f.hist_args, ("V1".into(), "L1".into()));
    cleanup(&out);
}

#[test]
fn fewer_than_two_yellow_rows_run_without_exclusion() {
    for (tag, file, yellow) in [
        ("one", "synthetic/combined_one_yellow.tsv", 1),
        ("none", "synthetic/combined_no_yellow.tsv", 0),
    ] {
        let out = prepared(tag);
        put(&relabel_combined_path(&out, &day()), &fixture_text(file));
        let mut f = Fake::default();
        let s = run_repack(&mut f, &day(), &sel(), &out).unwrap();
        assert!(f.exclude.is_empty(), "no exclusion list below the minimum");
        assert_eq!((s.yellow_rows, s.excluded), (yellow, 0));
        assert_eq!(s.snr_rows, 13);
        cleanup(&out);
    }
}

#[test]
fn no_heavy_rows_means_no_history_call() {
    let out = prepared("nor2");
    let mut f = Fake::default();
    let mut t = load_synthetic_in("repack", "zmdesnr", "raw_zmdesnr_del.txt");
    let n = t.col("net_weight").unwrap();
    for r in &mut t.rows {
        r[n] = "1".into();
    }
    f.over.push(("by_deliveries", Ov::Table(t)));
    let s = run_repack(&mut f, &day(), &sel(), &out).unwrap();
    assert_eq!(f.calls, ["by_deliveries"]);
    assert_eq!((s.history_serials, s.history_rows, s.blocks[1]), (0, 0, 0));
    assert!(s.snr_rows > 0);
    cleanup(&out);
}

#[test]
fn missing_or_empty_k2xx_output_says_run_k2xx_first() {
    let out = temp_out("nok2xx");
    put(
        &relabel_combined_path(&out, &day()),
        &fixture_text("relabel_combined_stand_in.tsv"),
    );
    let mut f = Fake::default();
    let e = run_repack(&mut f, &day(), &sel(), &out)
        .unwrap_err()
        .to_string();
    assert!(e.contains("Run k2xx first") && e.contains("repack"), "{e}");
    assert!(e.contains("--run-date=2026-11-05"), "{e}");
    assert!(f.calls.is_empty());
    put(&k2xx_vl06o_path(&out, &day()), "");
    let e = run_repack(&mut f, &day(), &sel(), &out)
        .unwrap_err()
        .to_string();
    assert!(e.contains("Run k2xx first") && e.contains("empty"), "{e}");
    put(&k2xx_vl06o_path(&out, &day()), "delivery\n");
    let e = run_repack(&mut f, &day(), &sel(), &out)
        .unwrap_err()
        .to_string();
    assert!(e.contains("Run k2xx first"), "{e}");
    assert!(f.calls.is_empty());
    cleanup(&out);
}

#[test]
fn missing_or_empty_relabel_output_says_run_relabel_first() {
    let out = temp_out("norel");
    put(
        &k2xx_vl06o_path(&out, &day()),
        &fixture_text("k2xx_vl06o_stand_in.tsv"),
    );
    let mut f = Fake::default();
    let e = run_repack(&mut f, &day(), &sel(), &out)
        .unwrap_err()
        .to_string();
    assert!(
        e.contains("Run relabel first") && e.contains("not found"),
        "{e}"
    );
    assert!(e.contains("--rname=relabel --run-date=2026-11-05"), "{e}");
    let p = relabel_combined_path(&out, &day());
    put(&p, "");
    let e = run_repack(&mut f, &day(), &sel(), &out)
        .unwrap_err()
        .to_string();
    assert!(
        e.contains("Run relabel first") && e.contains("empty"),
        "{e}"
    );
    // header only
    let header = fixture_text("relabel_combined_stand_in.tsv");
    put(&p, &format!("{}\n", header.lines().next().unwrap()));
    let e = run_repack(&mut f, &day(), &sel(), &out)
        .unwrap_err()
        .to_string();
    assert!(e.contains("Run relabel first"), "{e}");
    // an output of an older relabel without the yellow column
    put(&p, &fixture_text("synthetic/combined_no_yellow_column.tsv"));
    let e = run_repack(&mut f, &day(), &sel(), &out)
        .unwrap_err()
        .to_string();
    assert!(
        e.contains("yellow") && e.contains("run relabel again"),
        "{e}"
    );
    assert!(
        f.calls.is_empty(),
        "no SAP call before the inputs are there"
    );
    assert_eq!(
        files_under(&out.reports_dir),
        2,
        "only the two inputs exist"
    );
    cleanup(&out);
}

#[test]
fn a_range_or_week_is_rejected() {
    let out = prepared("range");
    let mut f = Fake::default();
    let range = RunDate {
        date1: ymd(2026, 11, 1),
        date2: Some(ymd(2026, 11, 7)),
    };
    let e = run_repack(&mut f, &range, &sel(), &out)
        .unwrap_err()
        .to_string();
    assert!(e.contains("single run date"), "{e}");
    assert!(f.calls.is_empty());
    cleanup(&out);
}

fn assert_stopped(out: &OutputSpec, r: Result<RepackSummary>, step: &str) {
    let e = r.unwrap_err();
    let nd = e
        .downcast_ref::<NoDataError>()
        .unwrap_or_else(|| panic!("not a no-data error: {e:#}"));
    assert_eq!(nd.step, step);
    // the two inputs are the only files: nothing was written
    assert_eq!(files_under(&out.reports_dir), 2);
}

#[test]
fn no_data_or_empty_lists_stop_the_run_and_write_nothing() {
    for (step, ov, expect) in [
        ("by_deliveries", Ov::NoData, "zmdesnr_by_deliveries"),
        (
            "by_deliveries",
            Ov::Table(Table::new("x", repack_headers(), vec![])),
            "zmdesnr_by_deliveries",
        ),
        ("history", Ov::NoData, "zmdesnr_history"),
        (
            "history",
            Ov::Table(Table::new("x", repack_headers(), vec![])),
            "zmdesnr_history",
        ),
    ] {
        let out = prepared("nodata");
        let mut f = Fake::default();
        f.over.push((step, ov));
        let r = run_repack(&mut f, &day(), &sel(), &out);
        assert_stopped(&out, r, expect);
        cleanup(&out);
    }
}

fn repack_headers() -> Vec<String> {
    [
        "lm_serial_number",
        "status",
        "pallet",
        "parent_serial_number",
        "material",
        "customer_part_",
        "delivery",
        "customer",
        "plant",
        "storage_location",
        "warehouse_number",
        "created_by",
        "created_on",
        "net_weight",
        "container",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

#[test]
fn a_step_error_stops_the_run_and_writes_nothing() {
    for step in ["by_deliveries", "history"] {
        let out = prepared("err");
        let mut f = Fake::default();
        f.over.push((step, Ov::Err));
        let e = run_repack(&mut f, &day(), &sel(), &out).unwrap_err();
        assert!(format!("{e:#}").contains("sap exploded"), "{e:#}");
        assert!(e.downcast_ref::<NoDataError>().is_none());
        assert_eq!(files_under(&out.reports_dir), 2);
        cleanup(&out);
    }
}

#[test]
fn a_missing_column_is_named() {
    // by deliveries without net_weight
    let out = prepared("col1");
    let mut f = Fake::default();
    let mut t = load_synthetic_in("repack", "zmdesnr", "raw_zmdesnr_del.txt");
    let i = t.col("net_weight").unwrap();
    t.headers.remove(i);
    for r in &mut t.rows {
        r.remove(i);
    }
    f.over.push(("by_deliveries", Ov::Table(t)));
    let e = format!(
        "{:#}",
        run_repack(&mut f, &day(), &sel(), &out).unwrap_err()
    );
    assert!(e.contains("net_weight"), "{e}");
    assert_eq!(files_under(&out.reports_dir), 2);
    cleanup(&out);

    // history without warehouse_number
    let out = prepared("col2");
    let mut f = Fake::default();
    let mut t = load_synthetic_in("repack", "zmdesnr", "raw_zmdesnr_hist.txt");
    let i = t.col("warehouse_number").unwrap();
    t.headers.remove(i);
    for r in &mut t.rows {
        r.remove(i);
    }
    f.over.push(("history", Ov::Table(t)));
    let e = format!(
        "{:#}",
        run_repack(&mut f, &day(), &sel(), &out).unwrap_err()
    );
    assert!(e.contains("warehouse_number"), "{e}");
    assert_eq!(files_under(&out.reports_dir), 2);
    cleanup(&out);
}

#[test]
fn no_rows_in_any_block_is_no_data() {
    let out = prepared("norows");
    let mut f = Fake::default();
    // only blank-plant rows: no block has a row
    let mut t = load_synthetic_in("repack", "zmdesnr", "raw_zmdesnr_del.txt");
    let p = t.col("plant").unwrap();
    for r in &mut t.rows {
        r[p].clear();
    }
    f.over.push(("by_deliveries", Ov::Table(t)));
    let r = run_repack(&mut f, &day(), &sel(), &out);
    assert_stopped(&out, r, "snr_repack");
    assert_eq!(f.calls, ["by_deliveries"]);
    cleanup(&out);
}
