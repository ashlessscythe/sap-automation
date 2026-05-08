//! "Auto-run" config consumption tests, rewritten to use the modern
//! `[global]` / `[tcode.X]` format and `SapConfig::load_from_path` against a
//! per-test `TempDir`.
//!
//! The original version of this file overwrote the workspace `config.toml`
//! with backup/restore. That meant tests couldn't run in parallel and a
//! panicking test could destroy the user's real config. This version is
//! hermetic: each test owns its config file via `TempDir`.
//!
//! These mock helpers mirror the field extraction that the real `*_module.rs`
//! files do — they don't touch SAP — and assert the same `SapConfig` shape
//! the report modules consume.

use chrono::NaiveDate;
use sap_automation::utils::config_types::SapConfig;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

// ----- helpers -----

fn write_temp_config(content: &str) -> (TempDir, PathBuf) {
    let dir = TempDir::new().expect("tempdir");
    let path = dir.path().join("config.toml");
    fs::write(&path, content).expect("write temp config");
    (dir, path)
}

fn load(path: &PathBuf) -> SapConfig {
    SapConfig::load_from_path(path.to_str().unwrap()).expect("load_from_path")
}

fn parse_date(s: &str) -> Result<NaiveDate, &'static str> {
    for fmt in &["%m/%d/%Y", "%m-%d-%Y", "%Y-%m-%d"] {
        if let Ok(d) = NaiveDate::parse_from_str(s, fmt) {
            return Ok(d);
        }
    }
    Err("unrecognized date format")
}

#[derive(Debug, Default, PartialEq, Clone)]
struct VL06OParams {
    start_date: NaiveDate,
    end_date: NaiveDate,
    sap_variant_name: Option<String>,
    layout_row: Option<String>,
    by_date: bool,
    column_name: Option<String>,
}

#[derive(Debug, Default, PartialEq, Clone)]
struct VT11Params {
    start_date: NaiveDate,
    end_date: NaiveDate,
    sap_variant_name: Option<String>,
    layout_row: Option<String>,
    column_name: Option<String>,
}

#[derive(Debug, Default, PartialEq, Clone)]
struct ZMDESNRParams {
    sap_variant_name: Option<String>,
    layout_row: Option<String>,
    serial_number: String,
}

fn vl06o_from_map(m: &HashMap<String, String>) -> VL06OParams {
    let mut p = VL06OParams::default();
    if let Some(v) = m.get("variant") {
        p.sap_variant_name = Some(v.clone());
    }
    if let Some(v) = m.get("layout") {
        p.layout_row = Some(v.clone());
    }
    if let Some(v) = m.get("date_range_start") {
        if let Ok(d) = parse_date(v) {
            p.start_date = d;
        }
    }
    if let Some(v) = m.get("date_range_end") {
        if let Ok(d) = parse_date(v) {
            p.end_date = d;
        }
    }
    if let Some(v) = m.get("by_date") {
        p.by_date = v.eq_ignore_ascii_case("true");
    }
    if let Some(v) = m.get("column_name") {
        p.column_name = Some(v.clone());
    }
    p
}

fn vt11_from_map(m: &HashMap<String, String>) -> VT11Params {
    let mut p = VT11Params::default();
    if let Some(v) = m.get("variant") {
        p.sap_variant_name = Some(v.clone());
    }
    if let Some(v) = m.get("layout") {
        p.layout_row = Some(v.clone());
    }
    if let Some(v) = m.get("date_range_start") {
        if let Ok(d) = parse_date(v) {
            p.start_date = d;
        }
    }
    if let Some(v) = m.get("date_range_end") {
        if let Ok(d) = parse_date(v) {
            p.end_date = d;
        }
    }
    if let Some(v) = m.get("column_name") {
        p.column_name = Some(v.clone());
    }
    p
}

fn zmdesnr_from_map(m: &HashMap<String, String>) -> ZMDESNRParams {
    let mut p = ZMDESNRParams::default();
    if let Some(v) = m.get("variant") {
        p.sap_variant_name = Some(v.clone());
    }
    if let Some(v) = m.get("layout") {
        p.layout_row = Some(v.clone());
    }
    if let Some(v) = m.get("serial_number") {
        p.serial_number = v.clone();
    }
    p
}

// ----- tests -----

#[test]
fn vl06o_full_config_round_trips() {
    let (_tmp, path) = write_temp_config(
        r#"
[global]
reports_dir = "C:\\Test\\Reports"
default_tcode = "VL06O"

[tcode.VL06O]
variant = "TEST_VARIANT"
layout = "TEST_LAYOUT"
column_name = "Test Column"
date_range_start = "04/01/2025"
date_range_end = "04/15/2025"
by_date = "true"
"#,
    );

    let cfg = load(&path);
    let map = cfg
        .get_tcode_config("VL06O", None)
        .expect("VL06O config present");
    let p = vl06o_from_map(&map);

    assert_eq!(p.sap_variant_name.as_deref(), Some("TEST_VARIANT"));
    assert_eq!(p.layout_row.as_deref(), Some("TEST_LAYOUT"));
    assert_eq!(p.column_name.as_deref(), Some("Test Column"));
    assert!(p.by_date);
    assert_eq!(
        p.start_date,
        NaiveDate::parse_from_str("04/01/2025", "%m/%d/%Y").unwrap()
    );
    assert_eq!(
        p.end_date,
        NaiveDate::parse_from_str("04/15/2025", "%m/%d/%Y").unwrap()
    );
}

#[test]
fn vt11_full_config_round_trips() {
    let (_tmp, path) = write_temp_config(
        r#"
[tcode.VT11]
variant = "VT11_VARIANT"
layout = "VT11_LAYOUT"
column_name = "VT11 Column"
date_range_start = "04/01/2025"
date_range_end = "04/15/2025"
"#,
    );

    let cfg = load(&path);
    let map = cfg.get_tcode_config("VT11", None).unwrap();
    let p = vt11_from_map(&map);

    assert_eq!(p.sap_variant_name.as_deref(), Some("VT11_VARIANT"));
    assert_eq!(p.layout_row.as_deref(), Some("VT11_LAYOUT"));
    assert_eq!(p.column_name.as_deref(), Some("VT11 Column"));
}

#[test]
fn zmdesnr_full_config_round_trips() {
    let (_tmp, path) = write_temp_config(
        r#"
[tcode.ZMDESNR]
variant = "ZMD_VARIANT"
layout = "ZMD_LAYOUT"
serial_number = "123456789"
"#,
    );

    let cfg = load(&path);
    let map = cfg.get_tcode_config("ZMDESNR", None).unwrap();
    let p = zmdesnr_from_map(&map);

    assert_eq!(p.sap_variant_name.as_deref(), Some("ZMD_VARIANT"));
    assert_eq!(p.layout_row.as_deref(), Some("ZMD_LAYOUT"));
    assert_eq!(p.serial_number, "123456789");
}

#[test]
fn missing_config_returns_none_for_unknown_tcode() {
    // Empty file → SapConfig still loads but tcode map is empty.
    let (_tmp, path) = write_temp_config("");
    let cfg = load(&path);
    assert!(cfg.get_tcode_config("VT11", None).is_none());
    assert!(cfg.get_tcode_config("ZMDESNR", None).is_none());
}

#[test]
fn nonexistent_path_returns_default_config() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("never-existed.toml");
    let cfg = SapConfig::load_from_path(path.to_str().unwrap())
        .expect("load_from_path tolerates a missing file");
    assert!(cfg.get_tcode_config("VT11", None).is_none());
}

#[test]
fn multiple_date_formats_are_accepted() {
    let (_tmp, path) = write_temp_config(
        r#"
[tcode.VL06O]
variant = "v"
date_range_start = "2025-04-01"
date_range_end = "04-15-2025"
"#,
    );
    let cfg = load(&path);
    let map = cfg.get_tcode_config("VL06O", None).unwrap();
    let p = vl06o_from_map(&map);
    assert_eq!(
        p.start_date,
        NaiveDate::parse_from_str("2025-04-01", "%Y-%m-%d").unwrap()
    );
    assert_eq!(
        p.end_date,
        NaiveDate::parse_from_str("04-15-2025", "%m-%d-%Y").unwrap()
    );
}

#[test]
fn tcode_specific_sections_are_isolated() {
    let (_tmp, path) = write_temp_config(
        r#"
[tcode.VL06O]
variant = "VL06O_VARIANT"
by_date = "true"

[tcode.VT11]
column_name = "VT11 Column"

[tcode.ZMDESNR]
serial_number = "123456789"
"#,
    );

    let cfg = load(&path);

    // VL06O sees its own values, not VT11 / ZMDESNR fields.
    let vl_map = cfg.get_tcode_config("VL06O", None).unwrap();
    assert_eq!(vl_map.get("variant").map(String::as_str), Some("VL06O_VARIANT"));
    assert_eq!(vl_map.get("by_date").map(String::as_str), Some("true"));
    assert!(vl_map.get("serial_number").is_none());

    let vt_map = cfg.get_tcode_config("VT11", None).unwrap();
    assert_eq!(vt_map.get("column_name").map(String::as_str), Some("VT11 Column"));
    assert!(vt_map.get("by_date").is_none());

    let zm_map = cfg.get_tcode_config("ZMDESNR", None).unwrap();
    assert_eq!(zm_map.get("serial_number").map(String::as_str), Some("123456789"));
    assert!(zm_map.get("by_date").is_none());
}
