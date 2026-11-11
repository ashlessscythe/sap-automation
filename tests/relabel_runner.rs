//! `run_relabel` against a fake SAP seam serving the SYNTHETIC stand-in exports
//! (tests/fixtures/relabel/synthetic/, see the README there). The delivery list is read from a
//! k2xx `vl06o` TSV written to the path of the same run (the k2xx stand-in output).
mod common;

use anyhow::{anyhow, Result};
use chrono::NaiveDate;
use common::{assert_tables_equal, load_fixture, load_synthetic_in};
use sap_automation::abc::relabel_runner::*;
use sap_automation::abc::run::RunDate;
use sap_automation::abc::runner::{NoDataError, OutputSpec, StepOutput};
use sap_automation::abc::table::Table;
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
    dels: Vec<String>,
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
            "relabel", "zmdesnr", file,
        )))
    }
}

impl RelabelSteps for Fake {
    fn zmdesnr_by_deliveries(
        &mut self,
        d: &[String],
        variant: &str,
        layout: &str,
    ) -> Result<StepOutput> {
        self.dels = d.to_vec();
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

fn temp_out(tag: &str) -> OutputSpec {
    let dir = std::env::temp_dir().join(format!("relabel_runner_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    OutputSpec {
        reports_dir: dir,
        today: ymd(2026, 11, 10),
    }
}

/// Put `text` where k2xx writes its `vl06o` table for `day()`.
fn put_k2xx(out: &OutputSpec, text: &str) -> PathBuf {
    let p = k2xx_vl06o_path(out, &day());
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(&p, text).unwrap();
    p
}

/// The k2xx stand-in output (107 deliveries).
fn k2xx_stand_in() -> String {
    let p: PathBuf = [
        env!("CARGO_MANIFEST_DIR"),
        "tests",
        "fixtures",
        "relabel",
        "k2xx_vl06o_stand_in.tsv",
    ]
    .iter()
    .collect();
    std::fs::read_to_string(p).unwrap()
}

fn prepared(tag: &str) -> OutputSpec {
    let out = temp_out(tag);
    put_k2xx(&out, &k2xx_stand_in());
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

fn sel() -> RelabelSelection {
    RelabelSelection::default()
}

#[test]
fn full_run_on_the_stand_in_fixtures() {
    let out = prepared("full");
    let mut f = Fake::default();
    let s = run_relabel(&mut f, &day(), &sel(), &out).unwrap();

    assert_eq!(f.calls, vec!["by_deliveries", "history"]);
    // the deliveries are those of the k2xx file, distinct
    assert_eq!(f.dels.len(), 107);
    assert_eq!(f.dels.iter().collect::<HashSet<_>>().len(), 107);
    assert_eq!(f.dels.first().unwrap(), "13000815");
    assert_eq!(
        f.del_args,
        (
            "ZMDESNR_RELABEL_VARIANT".into(),
            "ZMDESNR_RELABEL_LAYOUT".into()
        )
    );
    assert_eq!(
        f.hist_args,
        (
            "ZMDESNR_RELABEL_VARIANT".into(),
            "ZMDESNR_RELABEL_LAYOUT".into()
        )
    );
    assert_eq!(f.serials.len(), 9);
    assert_eq!(f.serials.iter().collect::<HashSet<_>>().len(), 9);

    assert_eq!(s.export_date, "2026-11-05");
    assert_eq!((s.deliveries, s.sn_del_rows, s.alias_rows), (107, 44, 6));
    assert_eq!(s.blocks, [4, 7, 2, 9, 14, 3, 3]);
    assert_eq!((s.history_serials, s.history_rows), (9, 7));
    assert_eq!((s.combined_rows, s.yellow_rows, s.rows_with_4), (33, 4, 8));
    assert_eq!((s.labels.as_str(), s.pivot_rows), ("82", 9));

    // outputs: <reports_dir>\K2XX\<today>\relabel\<date>_RELABEL_<TABLE>.tsv
    assert_eq!(s.outputs.len(), 4);
    let base = out
        .reports_dir
        .join("K2XX")
        .join("2026-11-10")
        .join("relabel");
    assert_eq!(s.outputs[0], base.join("11-05-2026_RELABEL_COMBINED.tsv"));
    assert_eq!(s.outputs[1], base.join("11-05-2026_RELABEL_SN_DEL.tsv"));
    assert_eq!(s.outputs[2], base.join("11-05-2026_RELABEL_SNR_ORIG.tsv"));
    assert_eq!(s.outputs[3], base.join("11-05-2026_RELABEL_PIVOT.tsv"));
    assert!(s.outputs.iter().all(|p| p.is_file()));
    // the four outputs plus the k2xx file that was read (and is left alone)
    assert_eq!(files_under(&out.reports_dir), 5);
    assert_tables_equal(
        &load_fixture("relabel", "expected_combined.tsv"),
        &read(&s.outputs[0], "combined"),
    );
    // no AL55 marks and no extra last row in sn_del / snr_orig (macro side effects not reproduced)
    let sn = read(&s.outputs[1], "sn_del");
    assert_tables_equal(&load_fixture("relabel", "expected_sn_del.tsv"), &sn);
    assert_eq!(sn.nrows(), 44);
    assert!(!sn.rows.iter().flatten().any(|c| c == "AL55"));
    assert_tables_equal(&sn, &read(&s.outputs[2], "snr_orig"));
    assert_tables_equal(
        &load_fixture("relabel", "expected_pivot.tsv"),
        &read(&s.outputs[3], "pivot"),
    );
    let _ = std::fs::remove_dir_all(&out.reports_dir);
}

#[test]
fn custom_variant_and_layout_reach_both_calls() {
    let out = prepared("custom");
    let mut f = Fake::default();
    let sel = RelabelSelection {
        zmdesnr_variant: "V".into(),
        zmdesnr_layout: "L".into(),
        rules: Default::default(),
    };
    run_relabel(&mut f, &day(), &sel, &out).unwrap();
    assert_eq!(f.del_args, ("V".into(), "L".into()));
    assert_eq!(f.hist_args, ("V".into(), "L".into()));
    let _ = std::fs::remove_dir_all(&out.reports_dir);
}

#[test]
fn missing_or_empty_k2xx_output_stops_with_run_k2xx_first() {
    // no file at all
    let out = temp_out("nofile");
    let mut f = Fake::default();
    let e = run_relabel(&mut f, &day(), &sel(), &out)
        .unwrap_err()
        .to_string();
    assert!(
        e.contains("Run k2xx first") && e.contains("file not found"),
        "{e}"
    );
    assert!(e.contains("11-05-2026_K2XX_VL06O.tsv"), "{e}");
    assert!(f.calls.is_empty(), "SAP is not touched");
    assert_eq!(files_under(&out.reports_dir), 0);

    // empty, whitespace only, header only, no delivery column, blank deliveries
    for (tag, text, want) in [
        ("empty", "", "file is empty"),
        ("ws", "\n \n", "file is empty"),
        ("hdr", "delivery\tplant\n", "no deliveries"),
        ("blank", "delivery\tplant\n\t\n", "no deliveries"),
    ] {
        let out = temp_out(tag);
        put_k2xx(&out, text);
        let mut f = Fake::default();
        let e = run_relabel(&mut f, &day(), &sel(), &out)
            .unwrap_err()
            .to_string();
        assert!(
            e.contains("Run k2xx first") && e.contains(want),
            "{tag}: {e}"
        );
        assert!(f.calls.is_empty(), "{tag}");
        assert_eq!(files_under(&out.reports_dir), 1, "{tag}: nothing written");
        let _ = std::fs::remove_dir_all(&out.reports_dir);
    }
    let out = temp_out("nocol");
    put_k2xx(&out, "plant\ttotal\nPL33\t1\n");
    let mut f = Fake::default();
    let e = run_relabel(&mut f, &day(), &sel(), &out)
        .unwrap_err()
        .to_string();
    assert!(e.contains("no column 'delivery'"), "{e}");
    assert!(f.calls.is_empty());
    let _ = std::fs::remove_dir_all(&out.reports_dir);
}

#[test]
fn the_k2xx_file_of_another_day_or_folder_is_not_used() {
    // a file for another run date and one under another `today` folder do not count
    let out = temp_out("other");
    let other = RunDate {
        date1: ymd(2026, 11, 4),
        date2: None,
    };
    let p = k2xx_vl06o_path(&out, &other);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(&p, k2xx_stand_in()).unwrap();
    let yesterday = OutputSpec {
        reports_dir: out.reports_dir.clone(),
        today: ymd(2026, 11, 9),
    };
    let p2 = k2xx_vl06o_path(&yesterday, &day());
    std::fs::create_dir_all(p2.parent().unwrap()).unwrap();
    std::fs::write(&p2, k2xx_stand_in()).unwrap();
    let mut f = Fake::default();
    let e = run_relabel(&mut f, &day(), &sel(), &out)
        .unwrap_err()
        .to_string();
    assert!(e.contains("Run k2xx first"), "{e}");
    let _ = std::fs::remove_dir_all(&out.reports_dir);
}

#[test]
fn a_range_or_week_is_rejected_before_anything_runs() {
    let out = prepared("range");
    for d in [
        RunDate {
            date1: ymd(2026, 11, 1),
            date2: Some(ymd(2026, 11, 7)),
        },
        RunDate {
            date1: ymd(2026, 11, 5),
            date2: Some(ymd(2026, 11, 6)),
        },
    ] {
        let mut f = Fake::default();
        let e = run_relabel(&mut f, &d, &sel(), &out)
            .unwrap_err()
            .to_string();
        assert!(e.contains("single run date"), "{e}");
        assert!(f.calls.is_empty());
    }
    // a range whose ends are equal is a single date
    let same = RunDate {
        date1: ymd(2026, 11, 5),
        date2: Some(ymd(2026, 11, 5)),
    };
    let mut f = Fake::default();
    assert!(run_relabel(&mut f, &same, &sel(), &out).is_ok());
    let _ = std::fs::remove_dir_all(&out.reports_dir);
}

/// Any failure of a SAP step, or a table that cannot be used, stops the run and writes nothing.
#[test]
fn failures_write_nothing() {
    let mut no_kanban = load_synthetic_in("relabel", "zmdesnr", "raw_zmdesnr_del.txt");
    let i = no_kanban.col("kanban_number").unwrap();
    no_kanban.headers[i] = "kanban".into();
    let mut empty_del = load_synthetic_in("relabel", "zmdesnr", "raw_zmdesnr_del.txt");
    empty_del.rows.clear();
    empty_del.visible.clear();
    let mut no_status = load_synthetic_in("relabel", "zmdesnr", "raw_zmdesnr_hist.txt");
    let i = no_status.col("status").unwrap();
    no_status.headers[i] = "st".into();
    let mut empty_hist = load_synthetic_in("relabel", "zmdesnr", "raw_zmdesnr_hist.txt");
    empty_hist.rows.clear();
    empty_hist.visible.clear();

    let cases: Vec<(&str, &'static str, Ov, &str)> = vec![
        ("del_nd", "by_deliveries", Ov::NoData, "no data"),
        ("del_err", "by_deliveries", Ov::Err, "sap exploded"),
        (
            "del_cols",
            "by_deliveries",
            Ov::Table(no_kanban),
            "kanban_number",
        ),
        (
            "del_empty",
            "by_deliveries",
            Ov::Table(empty_del),
            "empty list",
        ),
        ("hist_nd", "history", Ov::NoData, "no data"),
        ("hist_err", "history", Ov::Err, "sap exploded"),
        ("hist_cols", "history", Ov::Table(no_status), "status"),
        ("hist_empty", "history", Ov::Table(empty_hist), "empty list"),
    ];
    for (tag, step, ov, want) in cases {
        let out = prepared(tag);
        let mut f = Fake {
            over: vec![(step, ov)],
            ..Fake::default()
        };
        let e = run_relabel(&mut f, &day(), &sel(), &out).unwrap_err();
        let msg = format!("{e:#}");
        assert!(msg.to_lowercase().contains(want), "{tag}: {msg}");
        if let Some(nd) = e.downcast_ref::<NoDataError>() {
            assert!(nd.step.contains(if step == "history" {
                "history"
            } else {
                "deliveries"
            }));
        }
        let expect_calls = if step == "history" {
            vec!["by_deliveries", "history"]
        } else {
            vec!["by_deliveries"]
        };
        assert_eq!(f.calls, expect_calls, "{tag}");
        assert_eq!(
            files_under(&out.reports_dir),
            1,
            "{tag}: only the k2xx file"
        );
        let _ = std::fs::remove_dir_all(&out.reports_dir);
    }
}

/// History NO DATA is a NoDataError at its own step.
#[test]
fn history_no_data_names_its_step() {
    let out = prepared("hist_step");
    let mut f = Fake {
        over: vec![("history", Ov::NoData)],
        ..Fake::default()
    };
    let e = run_relabel(&mut f, &day(), &sel(), &out).unwrap_err();
    let nd = e.downcast_ref::<NoDataError>().expect("no-data error");
    assert_eq!(nd.step, "zmdesnr_history");
    let _ = std::fs::remove_dir_all(&out.reports_dir);
}

/// Without any PL55 pallet-1 row there is nothing to look up: no history call, nothing yellow.
#[test]
fn no_pallet_1_rows_means_no_history_call() {
    let out = prepared("nohist");
    let mut t = load_synthetic_in("relabel", "zmdesnr", "raw_zmdesnr_del.txt");
    let (p, plant) = (t.col("pallet").unwrap(), t.col("plant").unwrap());
    for r in &mut t.rows {
        if r[plant] == "PL55" && r[p] == "1" {
            r[p] = "2".into();
        }
    }
    let mut f = Fake {
        over: vec![("by_deliveries", Ov::Table(t))],
        ..Fake::default()
    };
    let s = run_relabel(&mut f, &day(), &sel(), &out).unwrap();
    assert_eq!(f.calls, vec!["by_deliveries"]);
    assert_eq!(
        (s.history_serials, s.history_rows, s.yellow_rows),
        (0, 0, 0)
    );
    let c = read(&s.outputs[0], "combined");
    assert!(c
        .rows
        .iter()
        .all(|r| r[c.col("yellow").unwrap()].is_empty()));
    let _ = std::fs::remove_dir_all(&out.reports_dir);
}

/// A history result without a PRI/11 row marks nothing yellow (not an error).
#[test]
fn history_without_a_hit_marks_nothing() {
    let out = prepared("nohit");
    let mut h = load_synthetic_in("relabel", "zmdesnr", "raw_zmdesnr_hist.txt");
    let st = h.col("status").unwrap();
    for r in &mut h.rows {
        r[st] = "SHP".into();
    }
    let mut f = Fake {
        over: vec![("history", Ov::Table(h))],
        ..Fake::default()
    };
    let s = run_relabel(&mut f, &day(), &sel(), &out).unwrap();
    assert_eq!((s.yellow_rows, s.rows_with_4), (0, 8 + 3));
    let _ = std::fs::remove_dir_all(&out.reports_dir);
}

/// When no block selects a row there is nothing to write: NO DATA at `combined`.
#[test]
fn an_empty_combined_stops_the_run() {
    let out = prepared("nocomb");
    let mut t = load_synthetic_in("relabel", "zmdesnr", "raw_zmdesnr_del.txt");
    t.rows.truncate(1); // A9 PAL row only: B3 has it, so empty it differently
    let (st, pl) = (t.col("status").unwrap(), t.col("plant").unwrap());
    t.rows[0][st] = "SHP".into();
    t.rows[0][pl] = "PL33".into();
    let d = t.col("document_date").unwrap();
    t.rows[0][d] = "2026-01-01".into();
    let mut f = Fake {
        over: vec![("by_deliveries", Ov::Table(t))],
        ..Fake::default()
    };
    let e = run_relabel(&mut f, &day(), &sel(), &out).unwrap_err();
    let nd = e.downcast_ref::<NoDataError>().expect("no-data error");
    assert_eq!(nd.step, "combined");
    assert_eq!(files_under(&out.reports_dir), 1);
    let _ = std::fs::remove_dir_all(&out.reports_dir);
}
