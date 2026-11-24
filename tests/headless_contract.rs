//! Headless contract for a calling program: piped stdio, `--json`, `SAP_NO_SHELL` /
//! `SAP_NO_STATUS`. The shell is never entered, no escape sequences are written, nothing waits on
//! stdin, exit codes tell success from failure and `--json` ends stdout with one JSON object.
//! No SAP needed: only paths that finish before any connection are driven.

use sap_automation::logging::status_panel_wanted;
use sap_automation::shell::prompt::prompts_possible;
use sap_automation::shell::{shell_enabled, shell_wanted};
use std::process::{Command, Output, Stdio};

fn run(args: &[&str], envs: &[(&str, &str)]) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_sap_automation"));
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (k, v) in envs {
        cmd.env(k, v);
    }
    cmd.output().expect("run sap_automation")
}

fn assert_no_escapes(out: &Output) {
    for (name, bytes) in [("stdout", &out.stdout), ("stderr", &out.stderr)] {
        assert!(
            !bytes.contains(&0x1b),
            "{name} contains an escape byte: {:?}",
            String::from_utf8_lossy(bytes)
        );
    }
}

#[test]
fn shell_decision_matrix() {
    // exactly one combination turns the shell on
    for bits in 0u8..32 {
        let b = |n: u8| bits & (1 << n) != 0;
        let expect = b(0) && b(1) && !b(2) && !b(3) && !b(4);
        assert_eq!(
            shell_enabled(b(0), b(1), b(2), b(3), b(4)),
            expect,
            "{bits:05b}"
        );
    }
    // the test harness has no terminal on stdin/stdout, so the process-wide answer is "no"
    // unless the developer runs the tests interactively
    use std::io::IsTerminal;
    if !(std::io::stdout().is_terminal() && std::io::stdin().is_terminal()) {
        assert!(!shell_wanted());
    }
}

#[test]
fn status_panel_and_prompts_follow_the_same_rules() {
    assert!(status_panel_wanted(true, false, false));
    assert!(!status_panel_wanted(false, false, false), "piped");
    assert!(!status_panel_wanted(true, true, false), "SAP_NO_STATUS");
    assert!(!status_panel_wanted(true, false, true), "--json");
    assert!(prompts_possible(true, true, false));
    assert!(!prompts_possible(false, true, false), "stdin piped");
    assert!(!prompts_possible(true, false, false), "stdout piped");
    assert!(!prompts_possible(true, true, true), "--json");
}

#[test]
fn help_succeeds_without_escape_sequences() {
    let out = run(&["--help"], &[("SAP_NO_SHELL", "1")]);
    assert!(out.status.success());
    assert!(!out.stdout.is_empty());
    assert_no_escapes(&out);
}

#[test]
fn interactive_mode_without_terminal_fails_instead_of_blocking() {
    // no arguments = the interactive menu; with piped stdio it must refuse, not wait or paint
    for envs in [
        &[][..],
        &[("SAP_NO_SHELL", "1"), ("SAP_NO_STATUS", "1")][..],
    ] {
        let out = run(&[], envs);
        assert!(!out.status.success(), "must exit non-zero");
        assert_no_escapes(&out);
        let err = String::from_utf8_lossy(&out.stderr);
        assert!(err.contains("needs a terminal"), "stderr: {err}");
    }
}

#[test]
fn validation_error_is_nonzero_and_clean() {
    let out = run(
        &["--tcode=vt11", "--by-shipment=true"],
        &[("SAP_NO_SHELL", "1")],
    );
    assert!(!out.status.success());
    assert_no_escapes(&out);
    assert!(String::from_utf8_lossy(&out.stderr).contains("only supported for VL06O"));
}

#[test]
fn json_validation_error_ends_stdout_with_one_json_object() {
    let out = run(
        &[
            "--tcode=lt03",
            "--json",
            "--by-shipment=true",
            "--shipments=1",
        ],
        &[],
    );
    assert!(!out.status.success());
    assert_no_escapes(&out);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let last = stdout.lines().last().expect("a last stdout line");
    let v: serde_json::Value = serde_json::from_str(last).expect("last line is JSON");
    assert_eq!(v["ok"], false);
    assert!(v["error"].as_str().unwrap().contains("VL06O"), "{v}");
}
