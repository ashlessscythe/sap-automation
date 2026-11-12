//! `repack` on the REAL exports of the live run of 2026-11-05 (`tests/fixtures/repack/real/`,
//! byte-exact, `-text`, anonymised): ZMDESNR by deliveries excluding parents (5839 rows) and by
//! serial / History (380 rows), loaded through the production loader. The expected tables come
//! from the macro's own `11-05-2026_REPACK.xlsx` of that run (sheets `SNR_REPACK` and `pivot`;
//! same anonymisation): 280 rows in the order R1, history rows, R3, R4, and the macro cross-tab.
mod common;

use anyhow::Result;
use chrono::NaiveDate;
use common::{assert_tables_equal, load_fixture, load_real_in, real_repack_rules};
use sap_automation::abc::relabel::{self, effective_plants};
use sap_automation::abc::repack::{self, *};
use sap_automation::abc::repack_runner::*;
use sap_automation::abc::run::RunDate;
use sap_automation::abc::runner::{OutputSpec, StepOutput};
use sap_automation::abc::table::Table;
use std::path::PathBuf;

fn del() -> Table {
    load_real_in("repack", "zmdesnr", "raw_zmdesnr_del.txt")
}

fn hist() -> Table {
    load_real_in("repack", "zmdesnr", "raw_zmdesnr_hist.txt")
}

const HEADERS: [&str; 15] = [
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
];

#[test]
fn real_headers_map_to_the_canonical_columns() {
    // `Created` (by deliveries) and `Created By` (history), `Net wt`: all canonical after the aliases
    for t in [del(), hist()] {
        assert_eq!(t.headers, HEADERS);
        for c in REPACK_SN_COLUMNS {
            assert!(t.col(c).is_ok(), "missing {c}: {:?}", t.headers);
        }
    }
    assert_eq!(del().nrows(), 5839);
    assert_eq!(hist().nrows(), 380);
    for c in HISTORY_COLUMNS {
        assert!(hist().col(c).is_ok());
    }
}

#[test]
fn stages_reproduce_the_macro_output() {
    let rules = real_repack_rules();
    let f = del();
    let eff = effective_plants(&f, &rules.plants).unwrap();
    assert_eq!(
        eff.iter()
            .filter(|p| **p == rules.plants.alias_plant)
            .count(),
        525
    );
    let b = select_blocks(&f, &eff, &rules).unwrap();
    assert_eq!(
        [b.r1.len(), b.r2.len(), b.r3.len(), b.r4.len()],
        [26, 36, 228, 22]
    );
    let serials = history_serials(&f, &b).unwrap();
    assert_eq!(serials.len(), 36);
    let h = hist();
    // the History export is exactly the 36 serials
    let hs = h.col("lm_serial_number").unwrap();
    let got: std::collections::HashSet<&str> = h.rows.iter().map(|r| r[hs].as_str()).collect();
    assert_eq!(got.len(), 36);
    assert!(serials.iter().all(|s| got.contains(s.as_str())));
    let idx = history_rows(&h, &rules).unwrap();
    assert_eq!(idx.len(), 4);
    let s = build_snr_repack(&f, &b, Some((&h, &idx)), "2026-11-05").unwrap();
    // the real run has no KC/SC container among R1, the history rows and R3
    assert_eq!(
        (s.r1, s.history, s.r3, s.deleted, s.r4),
        (26, 4, 228, 0, 22)
    );
    assert_eq!(s.table.nrows(), 280);
    assert_tables_equal(
        &load_fixture("repack", "expected_real_snr_repack.tsv"),
        &s.table,
    );
    let p = repack::pivot_customer_plant(&s.table).unwrap();
    assert_tables_equal(&load_fixture("repack", "expected_real_pivot.tsv"), &p);
    assert_eq!(relabel::pivot_total(&p).unwrap(), 280);
}

struct Real {
    exclude: Vec<String>,
    deliveries: Vec<String>,
}
impl RepackSteps for Real {
    fn zmdesnr_by_deliveries_excluding(
        &mut self,
        d: &[String],
        e: &[String],
        _: &str,
        _: &str,
    ) -> Result<StepOutput> {
        self.deliveries = d.to_vec();
        self.exclude = e.to_vec();
        Ok(StepOutput::Table(del()))
    }
    fn zmdesnr_history(&mut self, s: &[String], _: &str, _: &str) -> Result<StepOutput> {
        // the 36 serials of block R2, in the order of the export
        assert_eq!(s.len(), 36);
        Ok(StepOutput::Table(hist()))
    }
}

fn fixture_text(name: &str) -> String {
    let p: PathBuf = [
        env!("CARGO_MANIFEST_DIR"),
        "tests",
        "fixtures",
        "repack",
        name,
    ]
    .iter()
    .collect();
    std::fs::read_to_string(p).unwrap()
}

fn read(p: &std::path::Path, n: &str) -> Table {
    Table::from_tsv(n, &std::fs::read_to_string(p).unwrap()).unwrap()
}

#[test]
fn run_repack_on_the_real_exports() {
    let dir = std::env::temp_dir().join(format!("repack_real_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let out = OutputSpec {
        reports_dir: dir.clone(),
        today: NaiveDate::from_ymd_opt(2026, 11, 11).unwrap(),
    };
    let date = RunDate {
        date1: NaiveDate::from_ymd_opt(2026, 11, 5).unwrap(),
        date2: None,
    };
    let vl = sap_automation::abc::relabel_runner::k2xx_vl06o_path(&out, &date);
    let cb = relabel_combined_path(&out, &date);
    for (p, name) in [
        (&vl, "k2xx_vl06o_stand_in.tsv"),
        (&cb, "relabel_combined_stand_in.tsv"),
    ] {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, fixture_text(name)).unwrap();
    }

    let mut steps = Real {
        exclude: vec![],
        deliveries: vec![],
    };
    let sel = RepackSelection {
        rules: real_repack_rules(),
        ..RepackSelection::default()
    };
    let s = run_repack(&mut steps, &date, &sel, &out).unwrap();
    // the exclusion list = the 5 yellow serials of the relabel stand-in; deliveries = vl06o file
    assert_eq!(
        steps.exclude,
        [
            "130007102",
            "130007101",
            "130007100",
            "130007099",
            "130007098"
        ]
    );
    assert_eq!(steps.deliveries.len(), 104);
    assert_eq!((s.deliveries, s.yellow_rows, s.excluded), (104, 5, 5));
    assert_eq!((s.repack_sn_rows, s.alias_rows), (5839, 525));
    assert_eq!(s.blocks, [26, 36, 4, 228, 0, 22]);
    assert_eq!((s.history_serials, s.history_rows), (36, 380));
    assert_eq!(
        (s.snr_rows, s.total.as_str(), s.pivot_rows),
        (280, "280", 9)
    );
    let base = dir.join("K2XX").join("2026-11-11").join("repack");
    assert_eq!(s.outputs[0], base.join("11-05-2026_REPACK_SNR_REPACK.tsv"));
    assert_eq!(s.outputs[1], base.join("11-05-2026_REPACK_REPACK_SN.tsv"));
    assert_eq!(s.outputs[2], base.join("11-05-2026_REPACK_PIVOT.tsv"));
    assert_tables_equal(
        &load_fixture("repack", "expected_real_snr_repack.tsv"),
        &read(&s.outputs[0], "snr_repack"),
    );
    assert_tables_equal(
        &load_fixture("repack", "expected_real_pivot.tsv"),
        &read(&s.outputs[2], "pivot"),
    );
    // repack_sn is the export as loaded: no alias marks, no extra row
    assert_tables_equal(&del(), &read(&s.outputs[1], "repack_sn"));
    let _ = std::fs::remove_dir_all(&dir);
}

/// Without the alias customer patterns the alias rows are label-plant rows: the result differs
/// (guards that the rule is really applied and configurable).
#[test]
fn the_alias_patterns_matter() {
    let f = del();
    let rules = RepackRules::default(); // placeholder patterns: no real customer matches
    let eff = effective_plants(&f, &rules.plants).unwrap();
    assert_eq!(
        eff.iter()
            .filter(|p| **p == rules.plants.alias_plant)
            .count(),
        0
    );
    let b = select_blocks(&f, &eff, &rules).unwrap();
    assert_ne!(
        [b.r1.len(), b.r2.len(), b.r3.len(), b.r4.len()],
        [26, 36, 228, 22]
    );
}
