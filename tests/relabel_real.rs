//! `relabel` on the REAL exports of the live run of 2026-11-05 (`tests/fixtures/relabel/real/`,
//! byte-exact, `-text`): ZMDESNR by deliveries (6939 rows, layout `site_relabel`) and by serial /
//! History (5 rows), loaded through the production loader. The expected tables come from the
//! macro's own `11-05-2026_RELABEL.xlsx` of that run (sheets `combined` and `pivot`; the yellow
//! rows are the 5 rows with a yellow cell fill; the pivot is the macro cross-tab: customer rows x plant columns, Grand Total row and column).
//!
//! One deliberate difference to the xlsx: Excel shows a blank `Scan Cpy` as `0`; the SAP text
//! (and so the TSV) has it blank. No rule looks at `spv_scan_copies` other than `= 2`.
mod common;

use anyhow::Result;
use chrono::NaiveDate;
use common::{assert_tables_equal, load_fixture, load_real_in};
use sap_automation::abc::raw::{load_export_canonical, RawOptions};
use sap_automation::abc::relabel::{self, *};
use sap_automation::abc::relabel_runner::*;
use sap_automation::abc::run::RunDate;
use sap_automation::abc::runner::{OutputSpec, StepOutput};
use sap_automation::abc::table::Table;
use std::collections::HashSet;

fn day() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 11, 5).unwrap()
}

fn del() -> Table {
    load_real_in("relabel", "zmdesnr", "raw_zmdesnr_del.txt")
}

fn hist() -> Table {
    load_real_in("relabel", "zmdesnr", "raw_zmdesnr_hist.txt")
}

#[test]
fn real_headers_map_to_the_canonical_columns() {
    // the failing run: these canonical names were missing (found: sr_no_verf, scan_cpy,
    // doc_date, kanban_num, plant_doc ...)
    for t in [del(), hist()] {
        for c in SN_DEL_COLUMNS {
            assert!(t.col(c).is_ok(), "missing {c}: {:?}", t.headers);
        }
        assert_eq!(
            t.headers,
            [
                "lm_serial_number",
                "status",
                "serial_number_2nd_party_verification",
                "spv_scan_copies",
                "pallet",
                "parent_serial_number",
                "material",
                "customer_part_",
                "delivery",
                "customer",
                "plant",
                "storage_location",
                "warehouse_number",
                "document_date",
                "created_on",
                "created_by",
                "plant_doc_line",
                "dloc",
                "kanban_number",
                "container"
            ]
        );
    }
    assert_eq!(del().nrows(), 6939);
    assert_eq!(hist().nrows(), 5);
}

/// The exact header line of the error message (what SAP wrote, short titles), alone.
#[test]
fn the_failing_header_set_loads() {
    let txt = "Serial Number History\r\nNo. of records:        1\r\n\r\n\tLM Serial\tStatus\tSr No Verf\tScan Cpy\tPallet\tParent no\tMaterial\tCust. Part\tDelivery\t\tPlnt\tSLoc\tWhN\tDoc. Date\tCreated On\tCreated By\tPlant Doc\tDLOC\tKanban Num\tContainer\r\n\r\n\tREDACTED\tPRI\t2\t\t1\t\tREDACTED\t32156T6Z AL10\tREDACTED\tHONDA C/O REDACTED\tFV55\t0011\tE01\t2026-11-04\t2026-11-04\tPLACEHOLDER_SPMDE054\tREDACTED\t\t\tC0015210\r\n";
    let t = load_export_canonical("zmdesnr", "t", txt.as_bytes(), &RawOptions::default()).unwrap();
    for c in SN_DEL_COLUMNS {
        assert!(t.col(c).is_ok(), "missing {c}: {:?}", t.headers);
    }
}

/// The second export of the same run (rerun, other time stamp) is the same table.
#[test]
fn the_rerun_export_loads_identically() {
    let rerun = load_real_in("relabel", "zmdesnr", "raw_zmdesnr_del_rerun.txt");
    assert_tables_equal(&del(), &rerun);
}

struct Real;
impl RelabelSteps for Real {
    fn zmdesnr_by_deliveries(&mut self, _: &[String], _: &str, _: &str) -> Result<StepOutput> {
        Ok(StepOutput::Table(del()))
    }
    fn zmdesnr_history(&mut self, s: &[String], _: &str, _: &str) -> Result<StepOutput> {
        // the serials the History call was made for are the 5 PL55 pallet-1 rows
        assert_eq!(
            s,
            [
                "373170875",
                "373170768",
                "373170103",
                "373169636",
                "373169211"
            ]
        );
        Ok(StepOutput::Table(hist()))
    }
}

fn read(p: &std::path::Path, n: &str) -> Table {
    Table::from_tsv(n, &std::fs::read_to_string(p).unwrap()).unwrap()
}

#[test]
fn stages_reproduce_the_macro_output() {
    let f = del();
    let eff = effective_plants(&f).unwrap();
    assert_eq!(eff.iter().filter(|p| *p == AL55).count(), 619);
    let b = select_blocks(&f, &eff, day()).unwrap();
    assert_eq!(
        [
            b.b1.len(),
            b.b2.len(),
            b.b3.len(),
            b.b4.len(),
            b.b5.len(),
            b.b6.len(),
            b.b7.len()
        ],
        [87, 262, 48, 5, 9, 15, 0]
    );
    let y = yellow_serials(&hist()).unwrap();
    assert_eq!(y.len(), 5);
    let c = build_combined(&f, &eff, &b, &y, "2026-11-05").unwrap();
    assert_eq!((c.rows_with_4, c.label_total, c.yellow_rows), (4, 850, 5));
    assert_tables_equal(
        &load_fixture("relabel", "expected_real_combined.tsv"),
        &c.table,
    );
    let p = relabel::pivot_customer_plant(&c.table).unwrap();
    assert_tables_equal(&load_fixture("relabel", "expected_real_pivot.tsv"), &p);
    assert_eq!(pivot_total(&p).unwrap(), 850);
}

#[test]
fn run_relabel_on_the_real_exports() {
    let dir = std::env::temp_dir().join(format!("relabel_real_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let out = OutputSpec {
        reports_dir: dir.clone(),
        today: NaiveDate::from_ymd_opt(2026, 11, 11).unwrap(),
    };
    let date = RunDate {
        date1: day(),
        date2: None,
    };
    // the k2xx delivery list of the run: the distinct deliveries of the export
    let f = del();
    let c = f.col("delivery").unwrap();
    let dels: HashSet<&str> = f
        .rows
        .iter()
        .map(|r| r[c].as_str())
        .filter(|v| !v.is_empty())
        .collect();
    let p = k2xx_vl06o_path(&out, &date);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    let mut text = String::from("delivery\n");
    for d in &dels {
        text.push_str(d);
        text.push('\n');
    }
    std::fs::write(&p, text).unwrap();

    let s = run_relabel(&mut Real, &date, &RelabelSelection::default(), &out).unwrap();
    assert_eq!(s.deliveries, dels.len());
    assert_eq!(s.sn_del_rows, 6939);
    assert_eq!(s.blocks, [87, 262, 48, 5, 9, 15, 0]);
    assert_eq!((s.history_serials, s.history_rows), (5, 5));
    assert_eq!((s.combined_rows, s.yellow_rows, s.rows_with_4), (421, 5, 4));
    assert_eq!((s.labels.as_str(), s.pivot_rows), ("850", 9));
    let base = dir.join("K2XX").join("2026-11-11").join("relabel");
    assert_eq!(s.outputs[0], base.join("11-05-2026_RELABEL_COMBINED.tsv"));
    assert_tables_equal(
        &load_fixture("relabel", "expected_real_combined.tsv"),
        &read(&s.outputs[0], "combined"),
    );
    assert_tables_equal(
        &load_fixture("relabel", "expected_real_pivot.tsv"),
        &read(&s.outputs[3], "pivot"),
    );
    assert_tables_equal(&f, &read(&s.outputs[1], "sn_del"));
    let _ = std::fs::remove_dir_all(&dir);
}
