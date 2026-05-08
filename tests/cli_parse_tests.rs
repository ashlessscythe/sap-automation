//! Pure parse + projection tests for the CLI surface.
//!
//! These tests do NOT touch the filesystem, the SAP COM layer, or the
//! `OnceLock` override singleton. They exercise:
//!
//! - `Cli::try_parse_from(...)` — does clap accept the argv and which fields
//!   does it populate?
//! - `Cli::to_overrides()` — does the projection map every CLI flag to the
//!   right `CliOverrides` field, including ISO date parsing and uppercasing?
//! - `CliOverrides::looks_like_path`, `per_tcode_flag_names`, `summary_line` —
//!   small pure helpers used by validation and pretty-printing.
//!
//! Anything that depends on `SapConfig`, the override singleton, or
//! subprocess behavior lives in the other test files.

use chrono::NaiveDate;
use clap::Parser;
use sap_automation::cli::Cli;
use sap_automation::utils::cli_overrides::CliOverrides;

/// Convenience wrapper. Always prepends a fake program name.
fn parse(argv: &[&str]) -> Result<Cli, clap::Error> {
    let mut all = vec!["sap_automation"];
    all.extend_from_slice(argv);
    Cli::try_parse_from(all)
}

// ---------- Cli::to_overrides ----------

#[test]
fn tcode_uppercases() {
    let cli = parse(&["--tcode", "vt11"]).unwrap();
    let o = cli.to_overrides().unwrap();
    assert_eq!(o.tcode, Some("VT11".to_string()));
}

#[test]
fn omitted_flags_yield_all_none() {
    let cli = parse(&[]).unwrap();
    let o = cli.to_overrides().unwrap();
    assert!(o.tcode.is_none());
    assert!(o.layout.is_none());
    assert!(o.variant.is_none());
    assert!(o.export_type.is_none());
    assert!(o.iterations.is_none());
    assert!(o.interval_seconds.is_none());
    assert!(o.delay_seconds.is_none());
    assert!(o.tcode_run_type.is_none());
    assert!(o.date_format.is_none());
    assert!(o.timezone.is_none());
    assert!(o.reports_dir.is_none());
    assert!(o.by_date.is_none());
    assert!(o.by_delivery.is_none());
    assert!(o.by_shipment.is_none());
    assert!(o.limiter.is_none());
    assert!(o.date_start.is_none());
    assert!(o.date_end.is_none());
    assert!(o.delivery_file.is_none());
    assert!(o.delivery_col.is_none());
    assert!(o.shipment_file.is_none());
    assert!(o.shipment_col.is_none());
    assert!(o.pre_export_back.is_none());
    assert!(o.tab_number.is_none());
}

#[test]
fn boolean_filter_flags_round_trip() {
    let cli = parse(&[
        "--by-date=true",
        "--by-delivery=false",
        "--by-shipment=true",
        "--pre-export-back=false",
    ])
    .unwrap();
    let o = cli.to_overrides().unwrap();
    assert_eq!(o.by_date, Some(true));
    assert_eq!(o.by_delivery, Some(false));
    assert_eq!(o.by_shipment, Some(true));
    assert_eq!(o.pre_export_back, Some(false));
}

#[test]
fn numeric_flags_parse() {
    let cli = parse(&[
        "--export-type=2",
        "--iterations=10",
        "--interval-seconds=30",
        "--delay-seconds=5",
        "--tab-number=7",
    ])
    .unwrap();
    let o = cli.to_overrides().unwrap();
    assert_eq!(o.export_type, Some(2));
    assert_eq!(o.iterations, Some(10));
    assert_eq!(o.interval_seconds, Some(30));
    assert_eq!(o.delay_seconds, Some(5));
    assert_eq!(o.tab_number, Some(7));
}

#[test]
fn date_start_iso_ok() {
    let cli = parse(&["--date-start=2026-05-08"]).unwrap();
    let o = cli.to_overrides().unwrap();
    assert_eq!(o.date_start, NaiveDate::from_ymd_opt(2026, 5, 8));
}

#[test]
fn date_end_iso_ok() {
    let cli = parse(&["--date-end=2026-12-31"]).unwrap();
    let o = cli.to_overrides().unwrap();
    assert_eq!(o.date_end, NaiveDate::from_ymd_opt(2026, 12, 31));
}

#[test]
fn date_start_garbage_errors_with_iso_hint() {
    let cli = parse(&["--date-start=not-a-date"]).unwrap();
    let err = cli
        .to_overrides()
        .expect_err("garbage date should be rejected");
    let msg = format!("{err}");
    assert!(
        msg.contains("YYYY-MM-DD"),
        "expected ISO hint in error, got: {msg}"
    );
    assert!(
        msg.contains("--date-start"),
        "expected flag name in error, got: {msg}"
    );
}

#[test]
fn date_end_garbage_errors_with_iso_hint() {
    let cli = parse(&["--date-end=2026-13-99"]).unwrap();
    let err = cli.to_overrides().expect_err("invalid date should reject");
    let msg = format!("{err}");
    assert!(msg.contains("YYYY-MM-DD"), "got: {msg}");
    assert!(msg.contains("--date-end"), "got: {msg}");
}

#[test]
fn string_flags_round_trip() {
    let cli = parse(&[
        "--tcode=ZMDESNR",
        "--layout=ob_6",
        "--variant=window_tk",
        "--limiter=date_range",
        "--reports-dir=C:\\Reports",
        "--date-format=yyyy-mm-dd",
        "--timezone=America/Denver",
        "--tcode-run-type=rcv",
        "--delivery-file=vt11",
        "--delivery-col=Delivery",
        "--shipment-file=vl06o",
        "--shipment-col=Shipment Number",
    ])
    .unwrap();
    let o = cli.to_overrides().unwrap();
    assert_eq!(o.tcode.as_deref(), Some("ZMDESNR"));
    assert_eq!(o.layout.as_deref(), Some("ob_6"));
    assert_eq!(o.variant.as_deref(), Some("window_tk"));
    assert_eq!(o.limiter.as_deref(), Some("date_range"));
    assert_eq!(o.reports_dir.as_deref(), Some("C:\\Reports"));
    assert_eq!(o.date_format.as_deref(), Some("yyyy-mm-dd"));
    assert_eq!(o.timezone.as_deref(), Some("America/Denver"));
    assert_eq!(o.tcode_run_type.as_deref(), Some("rcv"));
    assert_eq!(o.delivery_file.as_deref(), Some("vt11"));
    assert_eq!(o.delivery_col.as_deref(), Some("Delivery"));
    assert_eq!(o.shipment_file.as_deref(), Some("vl06o"));
    assert_eq!(o.shipment_col.as_deref(), Some("Shipment Number"));
}

// ---------- top-level mode flags ----------

#[test]
fn run_loop_and_run_sequence_are_mutually_exclusive() {
    // `Cli` doesn't impl Debug, so we can't use `expect_err`. Match instead.
    let err = match parse(&["--run-loop", "--run-sequence"]) {
        Err(e) => e,
        Ok(_) => panic!("clap should reject mutually-exclusive flags"),
    };
    let msg = format!("{err}");
    assert!(
        msg.contains("--run-sequence") && msg.contains("--run-loop"),
        "got: {msg}"
    );
}

#[test]
fn run_loop_alone_parses() {
    let cli = parse(&["--run-loop"]).unwrap();
    assert!(cli.run_loop);
    assert!(!cli.run_sequence);
}

#[test]
fn run_sequence_alone_parses() {
    let cli = parse(&["--run-sequence"]).unwrap();
    assert!(!cli.run_loop);
    assert!(cli.run_sequence);
}

#[test]
fn skip_sap_check_and_keep_awake_default_false() {
    let cli = parse(&[]).unwrap();
    assert!(!cli.skip_sap_check);
    assert!(!cli.keep_awake);
}

// ---------- CliOverrides::looks_like_path ----------

#[test]
fn looks_like_path_recognizes_paths() {
    assert!(CliOverrides::looks_like_path("./out.csv"));
    assert!(CliOverrides::looks_like_path("C:\\foo\\bar.xlsx"));
    assert!(CliOverrides::looks_like_path("/home/user/data.csv"));
    assert!(CliOverrides::looks_like_path("data.tsv"));
}

#[test]
fn looks_like_path_recognizes_slugs() {
    assert!(!CliOverrides::looks_like_path("vt11"));
    assert!(!CliOverrides::looks_like_path("vl06o"));
    assert!(!CliOverrides::looks_like_path("ZMDESNR"));
}

// ---------- CliOverrides::per_tcode_flag_names ----------

#[test]
fn per_tcode_flag_names_empty_when_unset() {
    let o = CliOverrides::default();
    assert!(o.per_tcode_flag_names().is_empty());
}

#[test]
fn per_tcode_flag_names_lists_only_set_fields() {
    let cli = parse(&[
        "--tcode=vt11",
        "--by-date=true",
        "--limiter=date_range",
        "--tab-number=7",
    ])
    .unwrap();
    let o = cli.to_overrides().unwrap();
    let names = o.per_tcode_flag_names();
    assert!(names.contains(&"--tcode"));
    assert!(names.contains(&"--by-date"));
    assert!(names.contains(&"--limiter"));
    assert!(names.contains(&"--tab-number"));
    assert!(!names.contains(&"--by-delivery"));
    assert!(!names.contains(&"--variant"));
}

// ---------- CliOverrides::summary_line ----------

#[test]
fn summary_line_none_when_empty() {
    assert!(CliOverrides::default().summary_line().is_none());
}

#[test]
fn summary_line_includes_set_flags() {
    let cli = parse(&[
        "--tcode=vt11",
        "--variant=v1",
        "--by-delivery=false",
        "--iterations=3",
        "--date-start=2026-01-01",
    ])
    .unwrap();
    let o = cli.to_overrides().unwrap();
    let line = o.summary_line().expect("non-empty CliOverrides has summary");
    assert!(line.contains("--tcode=VT11"));
    assert!(line.contains("--variant=v1"));
    assert!(line.contains("--by-delivery=false"));
    assert!(line.contains("--iterations=3"));
    assert!(line.contains("--date-start=2026-01-01"));
}
