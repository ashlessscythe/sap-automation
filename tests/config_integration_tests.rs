//! Integration tests for `SapConfig` in the modern `[global]` / `[tcode.X]`
//! format, loaded from a per-test `TempDir`.
//!
//! Replaces the previous version of this file, which overwrote the
//! workspace `config.toml` with backup/restore. Hermetic tests are safer
//! and run in parallel cleanly.

use sap_automation::utils::config_types::{SapConfig, TcodeConfig};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

fn write_temp_config(content: &str) -> (TempDir, PathBuf) {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("config.toml");
    fs::write(&path, content).unwrap();
    (dir, path)
}

fn load(path: &Path) -> SapConfig {
    SapConfig::load_from_path(path.to_str().unwrap()).expect("load")
}

#[test]
fn load_global_and_tcode_section() {
    let (_tmp, path) = write_temp_config(
        r#"
[global]
reports_dir = "C:\\Test\\Reports"
default_tcode = "VL06O"
timezone = "MST"

[tcode.VL06O]
variant = "TEST_VARIANT"
layout = "TEST_LAYOUT"
column_name = "Test Column"
date_range_start = "04/01/2025"
date_range_end = "04/15/2025"
"#,
    );

    let cfg = load(&path);

    let g = cfg.global.as_ref().expect("global section");
    assert_eq!(g.reports_dir, "C:\\\\Test\\\\Reports");
    assert_eq!(g.default_tcode.as_deref(), Some("VL06O"));
    assert_eq!(g.timezone, "MST");

    let tcodes = cfg.tcode.as_ref().expect("tcode map");
    let v = tcodes.get("VL06O").expect("VL06O entry");
    assert_eq!(v.variant.as_deref(), Some("TEST_VARIANT"));
    assert_eq!(v.layout.as_deref(), Some("TEST_LAYOUT"));
    assert_eq!(v.column_name.as_deref(), Some("Test Column"));
    assert_eq!(v.date_range_start.as_deref(), Some("04/01/2025"));
    assert_eq!(v.date_range_end.as_deref(), Some("04/15/2025"));
}

#[test]
fn get_tcode_config_returns_full_map_for_vl06o() {
    let mut cfg = SapConfig::new();
    if cfg.tcode.is_none() {
        cfg.tcode = Some(HashMap::new());
    }

    let vl = TcodeConfig {
        variant: Some("V".into()),
        layout: Some("L".into()),
        column_name: Some("Col".into()),
        date_range_start: Some("04/01/2025".into()),
        date_range_end: Some("04/15/2025".into()),
        by_date: Some("true".into()),
        ..Default::default()
    };
    cfg.tcode.as_mut().unwrap().insert("VL06O".into(), vl);

    let mut vt = TcodeConfig::default();
    vt.additional_params
        .insert("custom_param".into(), "custom_value".into());
    cfg.tcode.as_mut().unwrap().insert("VT11".into(), vt);

    let vl_map = cfg.get_tcode_config("VL06O", None).unwrap();
    assert_eq!(vl_map.get("variant").map(String::as_str), Some("V"));
    assert_eq!(vl_map.get("layout").map(String::as_str), Some("L"));
    assert_eq!(vl_map.get("column_name").map(String::as_str), Some("Col"));
    assert_eq!(
        vl_map.get("date_range_start").map(String::as_str),
        Some("04/01/2025")
    );
    assert_eq!(
        vl_map.get("date_range_end").map(String::as_str),
        Some("04/15/2025")
    );
    assert_eq!(vl_map.get("by_date").map(String::as_str), Some("true"));

    let vt_map = cfg.get_tcode_config("VT11", None).unwrap();
    assert_eq!(
        vt_map.get("custom_param").map(String::as_str),
        Some("custom_value")
    );
    assert!(!vt_map.contains_key("by_date"));
}

#[test]
fn unknown_tcode_returns_none() {
    let cfg = SapConfig::new();
    assert!(cfg.get_tcode_config("DOES_NOT_EXIST", None).is_none());
}

#[test]
fn loop_section_round_trips() {
    // Note: the loader only parses `[loop]` when at least one of `[global]`
    // or `[tcode.X]` is also present (the "new format" signal). A config
    // with only `[loop]` is treated as legacy and silently ignored. See
    // `is_new_format` in `SapConfig::load_from_path`.
    let (_tmp, path) = write_temp_config(
        r#"
[global]
reports_dir = "C:\\Reports"

[loop]
tcode = "VL06O"
iterations = "5"
delay_seconds = "10"
"#,
    );

    let cfg = load(&path);
    let lc = cfg.loop_config.as_ref().expect("loop config");
    assert_eq!(lc.tcode, "VL06O");
    assert_eq!(lc.iterations, "5");
    assert_eq!(lc.delay_seconds, "10");
}

#[test]
fn empty_file_loads_with_default_global() {
    let (_tmp, path) = write_temp_config("");
    let cfg = load(&path);

    // Default `SapConfig` always has a `global` (Default impl); empty TOML
    // doesn't override anything but the load still succeeds.
    assert!(cfg.global.is_some());
}

#[test]
fn malformed_toml_returns_err() {
    let (_tmp, path) = write_temp_config("this is not [valid toml");
    let result = SapConfig::load_from_path(path.to_str().unwrap());
    assert!(result.is_err());
}
