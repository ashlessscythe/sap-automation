//! Integration tests for `SapConfig::load_from_path` + `apply_overrides_with`.
//!
//! These tests:
//!
//! 1. Write a `config.toml` into a per-test `tempfile::TempDir` (NEVER touch
//!    the workspace `config.toml`).
//! 2. Load it with `SapConfig::load_from_path`.
//! 3. Build a hand-rolled `CliOverrides` and call `apply_overrides_with`
//!    directly, bypassing the process-wide `OnceLock` singleton.
//! 4. Assert the resulting `get_tcode_config` map and `TcodeConfig` fields.
//!
//! The most important coverage here is the regression for the
//! `by_delivery` / `by_date` clobber bug — when the file said `by_delivery
//! = "true"` and the CLI said `--by-delivery=false`, the additional_params
//! catch-all used to overwrite the first-class field with the file value.

use chrono::NaiveDate;
use sap_automation::utils::cli_overrides::CliOverrides;
use sap_automation::utils::config_types::SapConfig;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// Write `content` into `<tmp>/config.toml` and return both the TempDir
/// (so it stays alive for the duration of the test) and the path.
fn write_temp_config(content: &str) -> (TempDir, PathBuf) {
    let dir = TempDir::new().expect("create tempdir");
    let path = dir.path().join("config.toml");
    fs::write(&path, content).expect("write temp config");
    (dir, path)
}

fn load(path: &Path) -> SapConfig {
    SapConfig::load_from_path(path.to_str().unwrap()).expect("load_from_path")
}

// ---------- Regression: by_delivery / by_date CLI override must win over file ----------

#[test]
fn by_delivery_cli_false_overrides_file_true_loop_run() {
    let (_tmp, path) = write_temp_config(
        r#"
[global]
reports_dir = "C:\\Test\\Reports"

[tcode.VT11]
variant = "TEST_VARIANT"
layout = "TEST_LAYOUT"
by_delivery = "true"
"#,
    );

    let mut cfg = load(&path);
    let overrides = CliOverrides {
        tcode: Some("VT11".into()),
        by_delivery: Some(false),
        ..Default::default()
    };
    cfg.apply_overrides_with(&overrides);

    let merged = cfg
        .get_tcode_config("VT11", Some(true))
        .expect("VT11 config present");
    assert_eq!(merged.get("by_delivery").map(String::as_str), Some("false"));
}

#[test]
fn by_delivery_cli_false_overrides_file_true_normal_run() {
    let (_tmp, path) = write_temp_config(
        r#"
[tcode.VT11]
by_delivery = "true"
"#,
    );

    let mut cfg = load(&path);
    let overrides = CliOverrides {
        tcode: Some("VT11".into()),
        by_delivery: Some(false),
        ..Default::default()
    };
    cfg.apply_overrides_with(&overrides);

    let merged = cfg
        .get_tcode_config("VT11", Some(false))
        .expect("VT11 config present");
    assert_eq!(merged.get("by_delivery").map(String::as_str), Some("false"));
}

#[test]
fn by_date_cli_false_overrides_file_true() {
    let (_tmp, path) = write_temp_config(
        r#"
[tcode.VT11]
by_date = "true"
"#,
    );

    let mut cfg = load(&path);
    let overrides = CliOverrides {
        tcode: Some("VT11".into()),
        by_date: Some(false),
        ..Default::default()
    };
    cfg.apply_overrides_with(&overrides);

    let merged = cfg.get_tcode_config("VT11", Some(true)).unwrap();
    assert_eq!(merged.get("by_date").map(String::as_str), Some("false"));
}

// ---------- Missing config.toml → CLI flags still populate SapConfig ----------

#[test]
fn missing_config_with_cli_flags_yields_overrides_only() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("does-not-exist.toml");

    let mut cfg = SapConfig::load_from_path(path.to_str().unwrap())
        .expect("load_from_path tolerates missing files");

    let overrides = CliOverrides {
        tcode: Some("VT11".into()),
        variant: Some("v1".into()),
        layout: Some("l1".into()),
        by_delivery: Some(false),
        by_date: Some(true),
        ..Default::default()
    };
    cfg.apply_overrides_with(&overrides);

    let merged = cfg.get_tcode_config("VT11", Some(true)).unwrap();
    assert_eq!(merged.get("variant").map(String::as_str), Some("v1"));
    assert_eq!(merged.get("layout").map(String::as_str), Some("l1"));
    assert_eq!(merged.get("by_delivery").map(String::as_str), Some("false"));
    assert_eq!(merged.get("by_date").map(String::as_str), Some("true"));
}

// ---------- Per-tcode plain field overrides ----------

#[test]
fn variant_layout_export_type_win_over_file() {
    let (_tmp, path) = write_temp_config(
        r#"
[tcode.VT11]
variant = "FILE_VARIANT"
layout = "FILE_LAYOUT"
export_type = 0
"#,
    );

    let mut cfg = load(&path);
    let overrides = CliOverrides {
        tcode: Some("VT11".into()),
        variant: Some("CLI_VARIANT".into()),
        layout: Some("CLI_LAYOUT".into()),
        export_type: Some(2),
        ..Default::default()
    };
    cfg.apply_overrides_with(&overrides);

    let merged = cfg.get_tcode_config("VT11", Some(true)).unwrap();
    assert_eq!(
        merged.get("variant").map(String::as_str),
        Some("CLI_VARIANT")
    );
    assert_eq!(merged.get("layout").map(String::as_str), Some("CLI_LAYOUT"));
    assert_eq!(merged.get("export_type").map(String::as_str), Some("2"));
}

// ---------- Global overrides ----------

#[test]
fn global_overrides_win_over_file() {
    let (_tmp, path) = write_temp_config(
        r#"
[global]
reports_dir = "C:\\From\\File"
date_format = "mm/dd/yyyy"
timezone = "UTC"
"#,
    );

    let mut cfg = load(&path);
    let overrides = CliOverrides {
        reports_dir: Some("D:\\From\\CLI".into()),
        date_format: Some("yyyy-mm-dd".into()),
        timezone: Some("America/Denver".into()),
        ..Default::default()
    };
    cfg.apply_overrides_with(&overrides);

    let g = cfg.global.as_ref().expect("global present");
    // reports_dir is stored with backslashes doubled to match the file-load
    // path. CLI input "D:\From\CLI" becomes "D:\\From\\CLI" on disk-style.
    assert_eq!(g.reports_dir, "D:\\\\From\\\\CLI");
    assert_eq!(g.date_format, "yyyy-mm-dd");
    assert_eq!(g.timezone, "America/Denver");
}

#[test]
fn global_overrides_create_global_section_when_absent() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("missing.toml");
    let mut cfg = SapConfig::load_from_path(path.to_str().unwrap()).unwrap();

    let overrides = CliOverrides {
        reports_dir: Some("E:\\Reports".into()),
        ..Default::default()
    };
    cfg.apply_overrides_with(&overrides);

    assert!(cfg.global.is_some());
    assert_eq!(cfg.global.unwrap().reports_dir, "E:\\\\Reports");
}

// ---------- limiter, date range ----------

#[test]
fn limiter_writes_into_additional_params() {
    let (_tmp, path) = write_temp_config(
        r#"
[tcode.VT11]
variant = "v"
"#,
    );

    let mut cfg = load(&path);
    let overrides = CliOverrides {
        tcode: Some("VT11".into()),
        limiter: Some("date_range".into()),
        ..Default::default()
    };
    cfg.apply_overrides_with(&overrides);

    let entry = cfg.tcode.as_ref().unwrap().get("VT11").unwrap();
    assert_eq!(
        entry.additional_params.get("limiter").map(String::as_str),
        Some("date_range")
    );
}

#[test]
fn date_range_writes_iso_strings() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("missing.toml");
    let mut cfg = SapConfig::load_from_path(path.to_str().unwrap()).unwrap();

    let overrides = CliOverrides {
        tcode: Some("VT11".into()),
        date_start: NaiveDate::from_ymd_opt(2026, 5, 1),
        date_end: NaiveDate::from_ymd_opt(2026, 5, 31),
        ..Default::default()
    };
    cfg.apply_overrides_with(&overrides);

    let entry = cfg.tcode.as_ref().unwrap().get("VT11").unwrap();
    assert_eq!(entry.date_range_start.as_deref(), Some("2026-05-01"));
    assert_eq!(entry.date_range_end.as_deref(), Some("2026-05-31"));
}

// ---------- by_shipment + column_name auto-fill ----------

#[test]
fn by_shipment_true_on_vl06o_autofills_column_name() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("missing.toml");
    let mut cfg = SapConfig::load_from_path(path.to_str().unwrap()).unwrap();

    let overrides = CliOverrides {
        tcode: Some("VL06O".into()),
        by_shipment: Some(true),
        ..Default::default()
    };
    cfg.apply_overrides_with(&overrides);

    let entry = cfg.tcode.as_ref().unwrap().get("VL06O").unwrap();
    assert_eq!(entry.column_name.as_deref(), Some("Shipment Number"));
    assert_eq!(
        entry
            .additional_params
            .get("by_shipment")
            .map(String::as_str),
        Some("true")
    );
}

#[test]
fn shipment_col_overrides_default_column_name() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("missing.toml");
    let mut cfg = SapConfig::load_from_path(path.to_str().unwrap()).unwrap();

    let overrides = CliOverrides {
        tcode: Some("VL06O".into()),
        by_shipment: Some(true),
        shipment_col: Some("Custom Header".into()),
        ..Default::default()
    };
    cfg.apply_overrides_with(&overrides);

    let entry = cfg.tcode.as_ref().unwrap().get("VL06O").unwrap();
    assert_eq!(entry.column_name.as_deref(), Some("Custom Header"));
    assert_eq!(
        entry
            .additional_params
            .get("cli_shipment_col")
            .map(String::as_str),
        Some("Custom Header")
    );
}

#[test]
fn by_shipment_false_does_not_autofill_column_name() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("missing.toml");
    let mut cfg = SapConfig::load_from_path(path.to_str().unwrap()).unwrap();

    let overrides = CliOverrides {
        tcode: Some("VL06O".into()),
        by_shipment: Some(false),
        ..Default::default()
    };
    cfg.apply_overrides_with(&overrides);

    let entry = cfg.tcode.as_ref().unwrap().get("VL06O").unwrap();
    assert!(entry.column_name.is_none());
}

// ---------- ZMDESNR-only knobs ----------

#[test]
fn pre_export_back_lands_only_on_zmdesnr() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("missing.toml");
    let mut cfg = SapConfig::load_from_path(path.to_str().unwrap()).unwrap();

    let overrides = CliOverrides {
        tcode: Some("ZMDESNR".into()),
        pre_export_back: Some(true),
        ..Default::default()
    };
    cfg.apply_overrides_with(&overrides);

    let entry = cfg.tcode.as_ref().unwrap().get("ZMDESNR").unwrap();
    assert_eq!(
        entry
            .additional_params
            .get("pre_export_back")
            .map(String::as_str),
        Some("true")
    );
}

#[test]
fn pre_export_back_ignored_when_tcode_not_zmdesnr() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("missing.toml");
    let mut cfg = SapConfig::load_from_path(path.to_str().unwrap()).unwrap();

    // main.rs rejects this combo at the CLI layer, but apply_overrides_with
    // is the underlying merge — guard it independently so the merge layer
    // stays safe even if validation is moved later.
    let overrides = CliOverrides {
        tcode: Some("VT11".into()),
        pre_export_back: Some(true),
        ..Default::default()
    };
    cfg.apply_overrides_with(&overrides);

    let entry = cfg.tcode.as_ref().unwrap().get("VT11").unwrap();
    assert!(!entry.additional_params.contains_key("pre_export_back"));
}

#[test]
fn tab_number_lands_only_on_zmdesnr() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("missing.toml");
    let mut cfg = SapConfig::load_from_path(path.to_str().unwrap()).unwrap();

    let overrides = CliOverrides {
        tcode: Some("ZMDESNR".into()),
        tab_number: Some(7),
        ..Default::default()
    };
    cfg.apply_overrides_with(&overrides);

    let entry = cfg.tcode.as_ref().unwrap().get("ZMDESNR").unwrap();
    assert_eq!(entry.tab_number.as_deref(), Some("7"));
}

#[test]
fn tab_number_ignored_when_tcode_not_zmdesnr() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("missing.toml");
    let mut cfg = SapConfig::load_from_path(path.to_str().unwrap()).unwrap();

    let overrides = CliOverrides {
        tcode: Some("VT11".into()),
        tab_number: Some(7),
        ..Default::default()
    };
    cfg.apply_overrides_with(&overrides);

    let entry = cfg.tcode.as_ref().unwrap().get("VT11").unwrap();
    assert!(entry.tab_number.is_none());
}

// ---------- delivery / shipment file pass-through ----------

#[test]
fn delivery_file_and_col_land_in_additional_params() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("missing.toml");
    let mut cfg = SapConfig::load_from_path(path.to_str().unwrap()).unwrap();

    let overrides = CliOverrides {
        tcode: Some("VT11".into()),
        delivery_file: Some("vt11".into()),
        delivery_col: Some("Delivery".into()),
        ..Default::default()
    };
    cfg.apply_overrides_with(&overrides);

    let entry = cfg.tcode.as_ref().unwrap().get("VT11").unwrap();
    assert_eq!(
        entry
            .additional_params
            .get("cli_delivery_file")
            .map(String::as_str),
        Some("vt11")
    );
    assert_eq!(
        entry
            .additional_params
            .get("cli_delivery_col")
            .map(String::as_str),
        Some("Delivery")
    );
}

// ---------- Case-insensitive tcode key: CLI must update on-disk entry ----------

#[test]
fn cli_overrides_lowercase_149_section_case_insensitively() {
    let (_tmp, path) = write_temp_config(
        r#"
[tcode.y_dn3_47000149]
variant = "FILE_VARIANT"
layout = "FILE_LAYOUT"
export_type = 1
plants = ["FROM_FILE"]
"#,
    );

    let mut cfg = load(&path);
    // CLI stores tcode upper-cased (as Cli::to_overrides does).
    let overrides = CliOverrides {
        tcode: Some("Y_DN3_47000149".into()),
        variant: Some("CLI_VARIANT".into()),
        layout: Some("CLI_LAYOUT".into()),
        export_type: Some(0),
        plants: Some(vec!["CLI_PLT".into()]),
        ..Default::default()
    };
    cfg.apply_overrides_with(&overrides);

    // Must update the existing lowercase key — not create a parallel uppercase entry.
    let keys: Vec<_> = cfg.tcode.as_ref().unwrap().keys().cloned().collect();
    assert_eq!(keys, vec!["y_dn3_47000149".to_string()]);

    let merged = cfg
        .get_tcode_config("y_dn3_47000149", Some(true))
        .expect("149 config present");
    assert_eq!(
        merged.get("variant").map(String::as_str),
        Some("CLI_VARIANT")
    );
    assert_eq!(merged.get("layout").map(String::as_str), Some("CLI_LAYOUT"));
    assert_eq!(merged.get("export_type").map(String::as_str), Some("0"));
    assert_eq!(merged.get("plants").map(String::as_str), Some("CLI_PLT"));

    // Lookup with either casing must see CLI values.
    let upper = cfg.get_tcode_config("Y_DN3_47000149", Some(true)).unwrap();
    assert_eq!(
        upper.get("variant").map(String::as_str),
        Some("CLI_VARIANT")
    );
}

#[test]
fn plants_cli_overrides_file_plants() {
    let (_tmp, path) = write_temp_config(
        r#"
[tcode.y_dn3_47000149]
plants = ["FILE_A", "FILE_B"]
"#,
    );

    let mut cfg = load(&path);
    let overrides = CliOverrides {
        tcode: Some("y_dn3_47000149".into()),
        plants: Some(vec!["CLI_ONLY".into()]),
        ..Default::default()
    };
    cfg.apply_overrides_with(&overrides);

    let entry = cfg
        .tcode
        .as_ref()
        .unwrap()
        .get("y_dn3_47000149")
        .expect("lowercase key preserved");
    assert_eq!(entry.plants, Some(vec!["CLI_ONLY".into()]));
}

#[test]
fn plants_loaded_from_file_when_no_cli() {
    let (_tmp, path) = write_temp_config(
        r#"
[tcode.y_dn3_47000149]
plants = ["A", "B"]
"#,
    );

    let cfg = load(&path);
    let entry = cfg.tcode.as_ref().unwrap().get("y_dn3_47000149").unwrap();
    assert_eq!(entry.plants, Some(vec!["A".into(), "B".into()]));
}

// ---------- Idempotency ----------

#[test]
fn empty_overrides_are_a_noop() {
    let (_tmp, path) = write_temp_config(
        r#"
[tcode.VT11]
variant = "FILE_VARIANT"
by_delivery = "true"
"#,
    );

    let mut cfg = load(&path);
    cfg.apply_overrides_with(&CliOverrides::default());

    // The file values should be untouched.
    let entry = cfg.tcode.as_ref().unwrap().get("VT11").unwrap();
    assert_eq!(entry.variant.as_deref(), Some("FILE_VARIANT"));
    assert_eq!(entry.by_delivery.as_deref(), Some("true"));
}

#[test]
fn applying_same_overrides_twice_is_idempotent() {
    let (_tmp, path) = write_temp_config(
        r#"
[tcode.VT11]
variant = "FILE"
"#,
    );

    let mut cfg = load(&path);
    let overrides = CliOverrides {
        tcode: Some("VT11".into()),
        variant: Some("CLI".into()),
        by_delivery: Some(false),
        ..Default::default()
    };
    cfg.apply_overrides_with(&overrides);
    cfg.apply_overrides_with(&overrides);

    let entry = cfg.tcode.as_ref().unwrap().get("VT11").unwrap();
    assert_eq!(entry.variant.as_deref(), Some("CLI"));
    assert_eq!(entry.by_delivery.as_deref(), Some("false"));
}
