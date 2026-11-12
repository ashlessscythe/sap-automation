//! `merge_report` / `merge_chain` over per-day TSVs built from the anonymised fixtures.
mod common;

use calamine::{open_workbook, Data, Reader, Xlsx};
use chrono::NaiveDate;
use sap_automation::abc::merge::{merge_chain, merge_report};
use sap_automation::abc::registry::{self, output_path};
use sap_automation::abc::run::RunDate;
use sap_automation::abc::table::Table;
use std::path::Path;

fn d(day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 11, day).unwrap()
}

fn today() -> NaiveDate {
    d(12)
}

fn range() -> RunDate {
    RunDate {
        date1: d(1),
        date2: Some(d(7)),
    }
}

fn week() -> Vec<NaiveDate> {
    range().days()
}

/// Write `table` (first `keep` data rows) as the per-day TSV of `rname`/`table_name` for `day`.
fn put(dir: &Path, rname: &str, day: u32, table_name: &str, t: &Table, keep: usize) {
    let def = registry::find_report(rname).unwrap();
    let mut t = t.clone();
    t.rows.truncate(keep);
    t.visible.truncate(keep);
    let p = output_path(dir, def, today(), &RunDate::single(d(day)), table_name);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, t.to_tsv()).unwrap();
}

fn fixture(dir: &str, file: &str) -> Table {
    common::load_fixture(dir, file)
}

/// Fixture per (rname, table); tables without a fixture get a tiny synthetic one.
fn source(rname: &str, table: &str) -> Table {
    match (rname, table) {
        ("xdock", "vt11") => fixture("xdock", "expected_vt11.tsv"),
        ("xdock", "vl06o") => fixture("xdock", "expected_vl06o.tsv"),
        ("xdock", "pivot") => fixture("xdock", "expected_pivot.tsv"),
        ("k2xx", "zvt11") => fixture("k2xx", "expected_real_zvt11.tsv"),
        ("k2xx", "vt11") => fixture("k2xx", "expected_real_vt11.tsv"),
        ("k2xx", "vl06o") => fixture("k2xx", "expected_real_vl06o.tsv"),
        ("k2xx", "pivot") => fixture("k2xx", "expected_real_pivot.tsv"),
        ("toyota", "vl06o") => fixture("toyota", "expected_vl06o.tsv"),
        ("misc", "vl06o") => fixture("misc", "expected_vl06o.tsv"),
        ("mustgo", "snr_copy") => fixture("mustgo", "expected_snr_copy.tsv"),
        ("relabel", "combined") => fixture("relabel", "expected_real_combined.tsv"),
        ("relabel", "sn_del") => fixture("relabel", "expected_sn_del.tsv"),
        ("relabel", "pivot") => fixture("relabel", "expected_real_pivot.tsv"),
        ("repack", "snr_repack") => fixture("repack", "expected_real_snr_repack.tsv"),
        ("repack", "pivot") => fixture("repack", "expected_real_pivot.tsv"),
        _ => Table::new(
            table,
            vec!["a".into(), "b".into()],
            vec![
                vec!["1".into(), "x".into()],
                vec!["2".into(), "y".into()],
                vec!["3".into(), "z".into()],
            ],
        ),
    }
}

/// Write the per-day files for `rname` on `day`, `keep` rows per table.
fn put_report(dir: &Path, rname: &str, day: u32, keep: usize) {
    for t in registry::tables(rname) {
        put(dir, rname, day, t, &source(rname, t), keep);
    }
}

fn sheet_rows(path: &Path, sheet: &str) -> Vec<Vec<String>> {
    let mut wb: Xlsx<_> = open_workbook(path).unwrap();
    let range = wb.worksheet_range(sheet).unwrap();
    range
        .rows()
        .map(|r| {
            r.iter()
                .map(|c| match c {
                    Data::String(s) => s.clone(),
                    Data::Empty => String::new(),
                    Data::Float(f) if f.fract() == 0.0 => format!("{}", *f as i64),
                    other => other.to_string(),
                })
                .collect()
        })
        .collect()
}

fn sheet_names(path: &Path) -> Vec<String> {
    let wb: Xlsx<_> = open_workbook(path).unwrap();
    wb.sheet_names()
}

#[test]
fn xdock_two_days_with_gaps() {
    let tmp = tempfile::tempdir().unwrap();
    let def = registry::find_report("xdock").unwrap();
    let np = source("xdock", "pivot").nrows();
    put_report(tmp.path(), "xdock", 2, 100);
    put_report(tmp.path(), "xdock", 4, 40);
    let out = merge_report(tmp.path(), def, today(), &range(), &week()).unwrap();

    assert_eq!(
        out.path,
        registry::merged_path(tmp.path(), def, today(), &range())
    );
    assert!(out.path.is_file());
    assert!(out
        .path
        .to_string_lossy()
        .contains("WK_45_XDOCK_MERGED.xlsx"));
    assert_eq!(out.days_merged, vec![d(2), d(4)]);
    assert_eq!(out.days_missing, vec![d(1), d(3), d(5), d(6), d(7)]);
    assert_eq!(
        out.tables,
        vec![
            ("vt11".to_string(), 91 + 40),
            ("vl06o".to_string(), 140),
            ("pivot".to_string(), np.min(100) + np.min(40)),
            ("weekly_pivot".to_string(), 3), // two days + Total
        ]
    );
    assert_eq!(
        sheet_names(&out.path),
        ["vt11", "vl06o", "pivot", "weekly_pivot"]
    );

    // pivot sheet: each day's pivot rows tagged with the right run_date
    let rows = sheet_rows(&out.path, "pivot");
    let pivot = source("xdock", "pivot");
    assert_eq!(rows[0], ["run_date", "Row Labels", "Sum of total"]);
    assert_eq!(rows.len(), 1 + 2 * np);
    for (i, r) in pivot.rows.iter().enumerate() {
        assert_eq!(rows[1 + i][0], "2026-11-02");
        assert_eq!(rows[1 + i][1], r[0]);
        assert_eq!(rows[1 + i][2], r[1]);
        assert_eq!(rows[1 + np + i][0], "2026-11-04");
        assert_eq!(rows[1 + np + i][1], r[0]);
        assert_eq!(rows[1 + np + i][2], r[1]);
    }

    // vl06o: 100 rows tagged day 2, then 40 tagged day 4
    let rows = sheet_rows(&out.path, "vl06o");
    assert_eq!(rows.len(), 141);
    assert_eq!(rows[0][0], "run_date");
    assert!(rows[1..=100].iter().all(|r| r[0] == "2026-11-02"));
    assert!(rows[101..].iter().all(|r| r[0] == "2026-11-04"));
}

#[test]
fn header_only_table_on_all_days_still_gets_a_sheet() {
    let tmp = tempfile::tempdir().unwrap();
    let def = registry::find_report("xdock").unwrap();
    put_report(tmp.path(), "xdock", 2, 5);
    put_report(tmp.path(), "xdock", 3, 5);
    for day in [2, 3] {
        put(
            tmp.path(),
            "xdock",
            day,
            "vt11",
            &source("xdock", "vt11"),
            0,
        );
    }
    let out = merge_report(tmp.path(), def, today(), &range(), &week()).unwrap();
    assert_eq!(out.tables[0], ("vt11".to_string(), 0));
    let rows = sheet_rows(&out.path, "vt11");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0][0], "run_date");
}

#[test]
fn incomplete_day_is_an_error_naming_day_and_table() {
    let tmp = tempfile::tempdir().unwrap();
    let def = registry::find_report("xdock").unwrap();
    put_report(tmp.path(), "xdock", 2, 5);
    put_report(tmp.path(), "xdock", 3, 5);
    let p = output_path(tmp.path(), def, today(), &RunDate::single(d(3)), "pivot");
    std::fs::remove_file(p).unwrap();
    let e = merge_report(tmp.path(), def, today(), &range(), &week()).unwrap_err();
    let msg = format!("{e:#}");
    assert!(msg.contains("11-03-2026") && msg.contains("pivot"), "{msg}");
    assert!(!registry::merged_path(tmp.path(), def, today(), &range()).exists());
}

#[test]
fn header_mismatch_is_an_error_naming_day_and_table() {
    let tmp = tempfile::tempdir().unwrap();
    let def = registry::find_report("xdock").unwrap();
    put_report(tmp.path(), "xdock", 2, 5);
    put_report(tmp.path(), "xdock", 3, 5);
    let mut bad = source("xdock", "vl06o");
    bad.headers[1] = "renamed_column".into();
    put(tmp.path(), "xdock", 3, "vl06o", &bad, 5);
    let e = merge_report(tmp.path(), def, today(), &range(), &week()).unwrap_err();
    let msg = format!("{e:#}");
    assert!(msg.contains("vl06o"), "{msg}");
    assert!(msg.contains("2026-11-03"), "{msg}");
    assert!(!registry::merged_path(tmp.path(), def, today(), &range()).exists());
}

#[test]
fn nothing_to_merge_writes_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let def = registry::find_report("xdock").unwrap();
    let e = merge_report(tmp.path(), def, today(), &range(), &week()).unwrap_err();
    assert!(format!("{e}").contains("nothing to merge"));
    assert!(!registry::merged_path(tmp.path(), def, today(), &range()).exists());
}

#[test]
fn repack_chain_one_workbook_per_report() {
    let tmp = tempfile::tempdir().unwrap();
    for rname in ["k2xx", "relabel", "repack"] {
        put_report(tmp.path(), rname, 2, 10);
        put_report(tmp.path(), rname, 3, 20);
    }
    let res = merge_chain(tmp.path(), "repack", today(), &range(), &week()).unwrap();
    let names: Vec<&str> = res.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names, ["k2xx", "relabel", "repack"]);
    for (name, r) in &res {
        let o = r.as_ref().unwrap();
        assert_eq!(o.report, *name);
        assert_eq!(o.days_merged, vec![d(2), d(3)]);
        let mut expect: Vec<String> = registry::tables(name)
            .iter()
            .map(|s| s.to_string())
            .collect();
        expect.push("weekly_pivot".into());
        assert_eq!(sheet_names(&o.path), expect);
        assert!(o
            .path
            .to_string_lossy()
            .ends_with(&format!("WK_45_{}_MERGED.xlsx", name.to_ascii_uppercase())));
    }
    let k2 = res[0].1.as_ref().unwrap();
    assert_eq!(k2.tables[2], ("vl06o".to_string(), 30));
    // relabel's combined table: 10 + 20 rows, run_date column first
    let rl = res[1].1.as_ref().unwrap();
    assert_eq!(rl.tables[0], ("combined".to_string(), 30));
    let rows = sheet_rows(&rl.path, "combined");
    assert_eq!(rows[1][0], "2026-11-02");
    assert_eq!(rows[30][0], "2026-11-03");
}

#[test]
fn chain_reports_failures_without_aborting_the_others() {
    let tmp = tempfile::tempdir().unwrap();
    // k2xx has no output at all; relabel and repack do
    for rname in ["relabel", "repack"] {
        put_report(tmp.path(), rname, 2, 5);
    }
    let res = merge_chain(tmp.path(), "repack", today(), &range(), &week()).unwrap();
    assert_eq!(res.len(), 3);
    let err = res[0].1.as_ref().unwrap_err();
    assert!(format!("{err}").contains("nothing to merge"));
    assert!(res[1].1.is_ok());
    assert!(res[2].1.is_ok());
    assert!(merge_chain(tmp.path(), "nope", today(), &range(), &week()).is_err());
}

/// Wide cross-tab pivot with `plants` as columns (synthetic, tiny).
fn wide_pivot(plants: &[&str], cust: &str, n: u32) -> Table {
    let mut headers = vec!["customer".to_string()];
    headers.extend(plants.iter().map(|p| p.to_string()));
    let mut row = vec![cust.to_string()];
    row.extend(plants.iter().map(|_| n.to_string()));
    Table::new("pivot", headers, vec![row])
}

#[test]
fn wide_pivot_columns_are_unioned_across_days() {
    let tmp = tempfile::tempdir().unwrap();
    let def = registry::find_report("relabel").unwrap();
    put_report(tmp.path(), "relabel", 10, 5);
    put_report(tmp.path(), "relabel", 11, 5);
    let r = RunDate {
        date1: d(10),
        date2: Some(d(11)),
    };
    put(
        tmp.path(),
        "relabel",
        10,
        "pivot",
        &wide_pivot(&["P1", "P2"], "C1", 4),
        5,
    );
    put(
        tmp.path(),
        "relabel",
        11,
        "pivot",
        &wide_pivot(&["P2", "P3"], "C2", 7),
        5,
    );
    let out = merge_report(tmp.path(), def, today(), &r, &r.days()).unwrap();
    let rows = sheet_rows(&out.path, "pivot");
    assert_eq!(rows[0], ["run_date", "customer", "P1", "P2", "P3"]);
    assert_eq!(rows[1], ["2026-11-10", "C1", "4", "4", ""]);
    assert_eq!(rows[2], ["2026-11-11", "C2", "", "7", "7"]);

    // a non-pivot table with a header mismatch still fails, nothing is written
    let _ = std::fs::remove_file(&out.path);
    let mut bad = source("relabel", "combined");
    bad.headers[1] = "renamed_column".into();
    put(tmp.path(), "relabel", 11, "combined", &bad, 5);
    let e = merge_report(tmp.path(), def, today(), &r, &r.days()).unwrap_err();
    let msg = format!("{e:#}");
    assert!(
        msg.contains("combined") && msg.contains("2026-11-11"),
        "{msg}"
    );
    assert!(!out.path.exists());
}

fn num(s: &str) -> f64 {
    s.trim().replace(',', "").parse().unwrap_or(0.0)
}

fn col_sum(t: &Table, col: &str) -> f64 {
    let c = t.col(col).unwrap();
    t.rows.iter().map(|r| num(&r[c])).sum()
}

#[test]
fn xdock_weekly_pivot_totals_match_source_and_day_pivots() {
    let tmp = tempfile::tempdir().unwrap();
    let def = registry::find_report("xdock").unwrap();
    // days given out of order on disk (4 before 2 is irrelevant: rows follow the ISO sort)
    put_report(tmp.path(), "xdock", 4, 1000);
    put_report(tmp.path(), "xdock", 2, 1000);
    let out = merge_report(tmp.path(), def, today(), &range(), &week()).unwrap();
    assert_eq!(sheet_names(&out.path).last().unwrap(), "weekly_pivot");
    let rows = sheet_rows(&out.path, "weekly_pivot");
    let hdr = &rows[0];
    assert_eq!(hdr[0], "run_date");
    assert_eq!(hdr.last().unwrap(), "Total");
    let plants = &hdr[1..hdr.len() - 1];
    // A-Z, the "(blank)" column last
    let mut sorted = plants.to_vec();
    sorted.sort_by_key(|p| (p == "(blank)", p.to_lowercase()));
    assert_eq!(plants, sorted.as_slice());
    assert_eq!(rows.len(), 4);
    assert_eq!(rows[1][0], "2026-11-02");
    assert_eq!(rows[2][0], "2026-11-04");
    assert_eq!(rows[3][0], "Total");
    let vl = source("xdock", "vl06o");
    let src_total = col_sum(&vl, "total");
    let grand = num(rows[3].last().unwrap());
    assert_eq!(grand, 2.0 * src_total);
    // equals the sum of the per-day pivots (without their own Grand Total rows)
    let pv = source("xdock", "pivot");
    let day_pivot: f64 = pv
        .rows
        .iter()
        .filter(|r| r[0] != "Grand Total")
        .map(|r| num(&r[1]))
        .sum();
    assert_eq!(grand, 2.0 * day_pivot);
    // per plant column = sum of `total` of that plant over both days
    let pc = vl.col("plant").unwrap();
    let tc = vl.col("total").unwrap();
    for (j, p) in plants.iter().enumerate() {
        let want: f64 = vl
            .rows
            .iter()
            .filter(|r| r[pc].trim() == p || (p == "(blank)" && r[pc].trim().is_empty()))
            .map(|r| num(&r[tc]))
            .sum();
        assert_eq!(num(&rows[3][1 + j]), 2.0 * want, "plant {p}");
        assert_eq!(num(&rows[1][1 + j]), want, "plant {p} day 1");
    }
}

#[test]
fn weekly_pivot_for_every_rname_with_fixtures() {
    let tmp = tempfile::tempdir().unwrap();
    for (rname, table, plant, measure) in [
        ("mustgo", "snr_copy", "plant", None),
        ("toyota", "vl06o", "plant", Some("total")),
        (
            "misc",
            "vl06o",
            "shipping_pointreceiving_pt",
            Some("number_of_packages"),
        ),
        ("k2xx", "vl06o", "plant", Some("total")),
        ("relabel", "combined", "plant", Some("_of_labels")),
        ("repack", "snr_repack", "plant", None),
    ] {
        put_report(tmp.path(), rname, 2, 1000);
        put_report(tmp.path(), rname, 3, 1000);
        let def = registry::find_report(rname).unwrap();
        let out = merge_report(tmp.path(), def, today(), &range(), &week()).unwrap();
        assert_eq!(out.tables.last().unwrap().0, "weekly_pivot", "{rname}");
        assert!(
            out.notes.iter().any(|n| n.starts_with("weekly_pivot:")),
            "{rname}"
        );
        assert_eq!(sheet_names(&out.path).last().unwrap(), "weekly_pivot");
        let rows = sheet_rows(&out.path, "weekly_pivot");
        let src = source(rname, table);
        let want = match measure {
            Some(c) => col_sum(&src, c),
            None => {
                let c = src.col("lm_serial_number").unwrap();
                src.rows.iter().filter(|r| !r[c].trim().is_empty()).count() as f64
            }
        };
        let datecol = if src.headers.iter().any(|h| h == "export_date") {
            "export_date"
        } else {
            "run_date"
        };
        assert_eq!(rows[0][0], datecol, "{rname}");
        assert_eq!(rows[0].last().unwrap(), "Total");
        assert_eq!(rows.last().unwrap()[0], "Total");
        // export_date of the fixtures is one fixed date, so one day row for those
        let ndays = if datecol == "export_date" { 1 } else { 2 };
        assert_eq!(rows.len(), 1 + ndays + 1, "{rname}");
        assert_eq!(
            num(rows.last().unwrap().last().unwrap()),
            2.0 * want,
            "{rname}"
        );
        let pc = src.col(plant).unwrap();
        let mut names: Vec<String> = src
            .rows
            .iter()
            .map(|r| r[pc].trim().to_string())
            .filter(|p| !p.is_empty())
            .collect();
        names.sort_by_key(|p| p.to_lowercase());
        names.dedup();
        assert_eq!(rows[0][1..1 + names.len()], names[..], "{rname}");
    }
}

#[test]
fn weekly_pivot_skipped_with_note_when_source_is_not_usable() {
    let tmp = tempfile::tempdir().unwrap();
    let def = registry::find_report("xdock").unwrap();
    // synthetic a/b tables: no plant / total columns
    for t in registry::tables("xdock") {
        let syn = source("none", t);
        put(tmp.path(), "xdock", 2, t, &syn, 3);
    }
    let out = merge_report(tmp.path(), def, today(), &range(), &week()).unwrap();
    assert_eq!(sheet_names(&out.path), ["vt11", "vl06o", "pivot"]);
    assert!(
        out.notes
            .iter()
            .any(|n| n.starts_with("weekly_pivot skipped")),
        "{:?}",
        out.notes
    );
    // header-only source: skipped too, merge still fine
    put(
        tmp.path(),
        "xdock",
        2,
        "vl06o",
        &source("xdock", "vl06o"),
        0,
    );
    let out = merge_report(tmp.path(), def, today(), &range(), &week()).unwrap();
    assert!(out.notes.iter().any(|n| n.contains("skipped")));
}
