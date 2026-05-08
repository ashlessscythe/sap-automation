//! Subprocess tests for the validation rules in `src/main.rs`.
//!
//! Every case here intentionally targets a code path that returns `Err(...)`
//! BEFORE `init_sap_connection()` is called, so these tests are safe to run
//! on a developer machine without SAP installed. The tested rules live in
//! `main.rs` lines 57-130 (mutually-exclusive flags, TCode-specific flag
//! gating, ISO date parsing, --run-sequence + per-tcode flag rejection)
//! plus clap's built-in `conflicts_with` enforcement.
//!
//! Tests use `assert_cmd` to spawn the project's binary directly via
//! `Command::cargo_bin("sap_automation")`.

use assert_cmd::Command;
use predicates::prelude::*;

fn bin() -> Command {
    Command::cargo_bin("sap_automation").expect("cargo_bin sap_automation")
}

// ---------- mutually exclusive booleans ----------

#[test]
fn rejects_by_delivery_and_by_shipment_both_true() {
    bin()
        .args(["--by-delivery=true", "--by-shipment=true", "--tcode=vl06o"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "--by-delivery=true and --by-shipment=true",
        ));
}

// ---------- by_shipment is VL06O-only ----------

#[test]
fn rejects_by_shipment_with_non_vl06o_tcode() {
    bin()
        .args(["--by-shipment=true", "--tcode=vt11"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("only supported for VL06O"));
}

// ---------- pre_export_back is ZMDESNR-only ----------

#[test]
fn rejects_pre_export_back_with_non_zmdesnr_tcode() {
    bin()
        .args(["--pre-export-back=true", "--tcode=vt11"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("only supported for ZMDESNR"));
}

// ---------- tab_number is ZMDESNR-only ----------

#[test]
fn rejects_tab_number_with_non_zmdesnr_tcode() {
    bin()
        .args(["--tab-number=7", "--tcode=vt11"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("only supported for ZMDESNR"));
}

// ---------- ISO date parse errors ----------

#[test]
fn rejects_garbage_date_start() {
    bin()
        .args(["--date-start=not-a-date"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("YYYY-MM-DD"));
}

#[test]
fn rejects_garbage_date_end() {
    bin()
        .args(["--date-end=2026-13-99"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("YYYY-MM-DD"));
}

// ---------- --run-sequence + per-tcode flag rejection ----------

#[test]
fn rejects_run_sequence_with_per_tcode_flags() {
    bin()
        .args(["--run-sequence", "--tcode=vt11", "--by-date=true"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--tcode"))
        .stderr(predicate::str::contains("--run-sequence"));
}

// ---------- clap built-in: --run-loop and --run-sequence are mutually exclusive ----------

#[test]
fn rejects_run_loop_and_run_sequence_together() {
    bin()
        .args(["--run-loop", "--run-sequence"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--run-loop").or(predicate::str::contains("--run-sequence")));
}

// ---------- smoke: --help and --version still work ----------

#[test]
fn help_lists_key_flags() {
    bin()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("--tcode"))
        .stdout(predicate::str::contains("--run-loop"))
        .stdout(predicate::str::contains("--run-sequence"));
}

#[test]
fn version_prints_semver() {
    bin()
        .arg("--version")
        .assert()
        .success()
        // Cargo.toml says 0.6.0 today; assert generic semver shape so the
        // test doesn't have to be edited every release.
        .stdout(predicate::str::is_match(r"\d+\.\d+\.\d+").unwrap());
}
