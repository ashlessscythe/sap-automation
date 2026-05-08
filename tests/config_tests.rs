//! Smoke tests for the workspace `config.toml` itself. These are the only
//! tests that read the on-disk config; they are gated so they pass cleanly
//! when the file is absent (e.g. on a fresh clone) and they assert against
//! the modern `[global]` / `[tcode.X]` schema.

use std::fs;
use std::path::Path;

#[test]
fn config_file_is_present_and_uses_new_schema() {
    if !Path::new("config.toml").exists() {
        eprintln!("config.toml missing — skipping");
        return;
    }
    let content = fs::read_to_string("config.toml").expect("read config.toml");
    assert!(
        content.contains("[global]") || content.contains("[tcode."),
        "expected [global] or [tcode.X] section in config.toml; got:\n{content}"
    );
}

#[test]
fn config_file_has_reports_dir_when_global_present() {
    if !Path::new("config.toml").exists() {
        eprintln!("config.toml missing — skipping");
        return;
    }
    let content = fs::read_to_string("config.toml").expect("read config.toml");
    if content.contains("[global]") {
        assert!(
            content.contains("reports_dir"),
            "[global] section must declare reports_dir"
        );
    }
}
