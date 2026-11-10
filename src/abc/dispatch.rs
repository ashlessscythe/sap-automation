//! Caller layer of the headless runner: resolves the effective settings (CLI > config > default)
//! and runs a report by name against a live SAP session. The resolver and the message helpers
//! are pure and unit tested; [`run_report`] is deliberately thin (it cannot be tested offline).
//!
//! Optional config section (all keys optional), read from the raw `config.toml`:
//! ```toml
//! [abc.mustgo]
//! vt11_variant = "VT11_XDOCK_VARIANT"
//! zmdesnr_variant = "mg_k2xx"
//! zmdesnr_layout = "mg_view"
//! lt23_layout = "to_mg"
//! lt23_create_layout = false      # true: create the layout from the spec column list when missing
//! warehouse = "E01"
//! delete_exports = true
//! ignore_display_error = true
//! ```
//! `reports_dir` and `date_format` come from `--reports-dir`/`--date-format`, else `[global]`.
#![allow(dead_code)]

use super::adapters::{MustgoSapConfig, RealSapSteps};
use super::raw::RawOptions;
use super::registry;
use super::run::RunDate;
use super::runner::{run_mustgo, NoDataError, OutputSpec, RunSummary};
use anyhow::{anyhow, Result};
use sap_scripting::GuiSession;
use std::path::PathBuf;

/// Inputs of the resolver (already extracted from CLI / config by the caller).
#[derive(Debug, Clone, Default)]
pub struct ResolveInput {
    pub cli_reports_dir: Option<String>,
    pub cli_date_format: Option<String>,
    pub cli_ignore_display_error: Option<bool>,
    pub global_reports_dir: Option<String>,
    pub global_date_format: Option<String>,
    /// The `[abc.mustgo]` table, if present.
    pub section: Option<toml::Value>,
}

/// Effective settings of a `mustgo` run.
#[derive(Debug, Clone)]
pub struct ResolvedMustgo {
    pub cfg: MustgoSapConfig,
    /// Root of the outputs (`<reports_dir>\K2XX\<today>\mustgo\...`); exports go to
    /// `<reports_dir>\<TCODE>\...`.
    pub reports_dir: PathBuf,
}

/// Chrono pattern for a config-style date format name (`mm/dd/yyyy`, ...). Unknown names give
/// ISO, like `config_ops::format_date_with`.
pub fn chrono_date_format(name: &str) -> &'static str {
    match name {
        "mm/dd/yyyy" => "%m/%d/%Y",
        "dd-mm-yy" => "%d-%m-%y",
        "dd-mm-yyyy" => "%d-%m-%Y",
        _ => "%Y-%m-%d",
    }
}

fn s(section: &Option<toml::Value>, key: &str) -> Option<String> {
    section
        .as_ref()?
        .get(key)?
        .as_str()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_string)
}

fn b(section: &Option<toml::Value>, key: &str) -> Option<bool> {
    section.as_ref()?.get(key)?.as_bool()
}

/// CLI > `[abc.mustgo]` > `[global]` > defaults.
pub fn resolve_mustgo(input: &ResolveInput) -> ResolvedMustgo {
    let d = MustgoSapConfig::default();
    let reports_dir = input
        .cli_reports_dir
        .clone()
        .or_else(|| input.global_reports_dir.clone())
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| d.export_dir.clone());
    let date_format = input
        .cli_date_format
        .clone()
        .or_else(|| input.global_date_format.clone())
        .filter(|v| !v.trim().is_empty());
    let raw = RawOptions {
        date_format: chrono_date_format(date_format.as_deref().unwrap_or("yyyy-mm-dd")).to_string(),
        ..RawOptions::default()
    };
    let create = b(&input.section, "lt23_create_layout").unwrap_or(false);
    let cfg = MustgoSapConfig {
        vt11_variant: s(&input.section, "vt11_variant").unwrap_or(d.vt11_variant),
        zmdesnr_variant: s(&input.section, "zmdesnr_variant").unwrap_or(d.zmdesnr_variant),
        zmdesnr_layout: s(&input.section, "zmdesnr_layout").unwrap_or(d.zmdesnr_layout),
        lt23_layout: s(&input.section, "lt23_layout").unwrap_or(d.lt23_layout),
        lt23_create_layout_columns: create.then(|| {
            crate::lt23::TO_MG_COLUMNS
                .iter()
                .map(|c| c.to_string())
                .collect()
        }),
        warehouse: s(&input.section, "warehouse").unwrap_or(d.warehouse),
        export_dir: reports_dir.clone(),
        date_format,
        ignore_display_error: input
            .cli_ignore_display_error
            .or_else(|| b(&input.section, "ignore_display_error"))
            .unwrap_or(d.ignore_display_error),
        delete_exports: b(&input.section, "delete_exports").unwrap_or(d.delete_exports),
        raw,
    };
    ResolvedMustgo {
        cfg,
        reports_dir: PathBuf::from(reports_dir),
    }
}

/// Resolve from the CLI overrides and `config.toml` (caller layer: this reads the config).
pub fn resolve_from_environment() -> ResolvedMustgo {
    let cli = crate::utils::cli_overrides::cli_overrides();
    let config = crate::utils::config_types::SapConfig::load().ok();
    let section = config
        .as_ref()
        .and_then(|c| c.raw_config.as_ref())
        .and_then(|r| r.get("abc"))
        .and_then(|a| a.get("mustgo"))
        .cloned();
    resolve_mustgo(&ResolveInput {
        cli_reports_dir: cli.reports_dir.clone(),
        cli_date_format: cli.date_format.clone(),
        cli_ignore_display_error: cli.ignore_display_error,
        global_reports_dir: config
            .as_ref()
            .and_then(|c| c.global.as_ref())
            .map(|g| g.reports_dir.clone()),
        global_date_format: config
            .as_ref()
            .and_then(|c| c.global.as_ref())
            .map(|g| g.date_format.clone()),
        section,
    })
}

/// Names of the reports that have a runner.
pub fn runnable_reports() -> &'static [&'static str] {
    &["mustgo"]
}

/// Check that `name` is a registered report with a runner; returns the canonical name.
pub fn check_report(name: &str) -> Result<&'static str> {
    let def = registry::find_report(name).ok_or_else(|| {
        anyhow!(
            "unknown report '{name}'. Known reports: {}",
            registry::report_names().join(", ")
        )
    })?;
    if !runnable_reports().contains(&def.name) {
        return Err(anyhow!(
            "report '{}' is registered but has no headless runner yet (runnable: {})",
            def.name,
            runnable_reports().join(", ")
        ));
    }
    Ok(def.name)
}

/// One-line description of a failed run; "no data" is told apart from other errors.
pub fn describe_error(e: &anyhow::Error) -> String {
    match e.downcast_ref::<NoDataError>() {
        Some(nd) => format!("NO DATA at step '{}': {}", nd.step, nd.message),
        None => format!("ERROR: {e:#}"),
    }
}

/// Printable summary lines of a finished run.
pub fn summary_lines(name: &str, s: &RunSummary) -> Vec<String> {
    let mut v = vec![
        format!("{name} finished (export_date {})", s.export_date),
        format!(
            "  shipments {}, deliveries {}, transfer orders {}",
            s.shipments, s.deliveries, s.transfer_orders
        ),
        format!(
            "  rows: lt23 {}, snr_copy {}, pivot {}",
            s.lt23_rows, s.snr_copy_rows, s.pivot_rows
        ),
    ];
    v.extend(s.outputs.iter().map(|p| format!("  wrote {}", p.display())));
    v
}

/// Run report `name` for `date` on a live session and print the summary. Thin: attach/login
/// checks are the caller's; everything else is tested through the pieces above.
pub fn run_report(session: &GuiSession, name: &str, date: &RunDate) -> Result<RunSummary> {
    let name = check_report(name)?;
    let resolved = resolve_from_environment();
    let out = OutputSpec {
        reports_dir: resolved.reports_dir.clone(),
        today: chrono::Local::now().date_naive(),
    };
    println!(
        "Running report '{name}' for {} (exports under {}, outputs under {})",
        super::runner::export_date_label(date),
        resolved.cfg.export_dir,
        out.reports_dir.display()
    );
    let mut steps = RealSapSteps::new(session, resolved.cfg);
    let summary = match name {
        "mustgo" => run_mustgo(&mut steps, date, &out),
        other => Err(anyhow!("no runner for '{other}'")),
    };
    match summary {
        Ok(s) => {
            for l in summary_lines(name, &s) {
                println!("{l}");
            }
            Ok(s)
        }
        Err(e) => {
            eprintln!("{}", describe_error(&e));
            Err(e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn section(t: &str) -> Option<toml::Value> {
        Some(toml::from_str::<toml::Value>(t).unwrap())
    }

    #[test]
    fn defaults() {
        let r = resolve_mustgo(&ResolveInput::default());
        let d = MustgoSapConfig::default();
        assert_eq!(r.cfg.vt11_variant, d.vt11_variant);
        assert_eq!(r.cfg.zmdesnr_layout, "mg_view");
        assert_eq!(r.cfg.lt23_layout, "to_mg");
        assert!(r.cfg.lt23_create_layout_columns.is_none());
        assert!(r.cfg.delete_exports && r.cfg.ignore_display_error);
        assert_eq!(r.reports_dir, PathBuf::from("reports"));
        assert_eq!(r.cfg.export_dir, "reports");
        assert_eq!(r.cfg.raw.date_format, "%Y-%m-%d");
    }

    #[test]
    fn precedence_cli_over_section_over_global() {
        let input = ResolveInput {
            cli_reports_dir: Some("C:/cli".into()),
            global_reports_dir: Some("C:/global".into()),
            cli_date_format: Some("mm/dd/yyyy".into()),
            global_date_format: Some("yyyy-mm-dd".into()),
            cli_ignore_display_error: Some(false),
            section: section(
                "vt11_variant = \"V1\"\nwarehouse = \"E02\"\ndelete_exports = false\nignore_display_error = true\nlt23_create_layout = true\n",
            ),
        };
        let r = resolve_mustgo(&input);
        assert_eq!(r.reports_dir, PathBuf::from("C:/cli"));
        assert_eq!(r.cfg.export_dir, "C:/cli");
        assert_eq!(r.cfg.date_format.as_deref(), Some("mm/dd/yyyy"));
        assert_eq!(r.cfg.raw.date_format, "%m/%d/%Y");
        assert!(!r.cfg.ignore_display_error, "CLI beats the section");
        assert_eq!(r.cfg.vt11_variant, "V1");
        assert_eq!(r.cfg.warehouse, "E02");
        assert!(!r.cfg.delete_exports);
        assert_eq!(r.cfg.lt23_create_layout_columns.as_ref().unwrap().len(), 11);

        let g = resolve_mustgo(&ResolveInput {
            global_reports_dir: Some("C:/global".into()),
            global_date_format: Some("dd-mm-yyyy".into()),
            ..ResolveInput::default()
        });
        assert_eq!(g.reports_dir, PathBuf::from("C:/global"));
        assert_eq!(g.cfg.raw.date_format, "%d-%m-%Y");
    }

    #[test]
    fn blank_values_fall_through() {
        let r = resolve_mustgo(&ResolveInput {
            cli_reports_dir: Some("  ".into()),
            section: section("vt11_variant = \"\"\nwarehouse = 5\n"),
            ..ResolveInput::default()
        });
        assert_eq!(r.reports_dir, PathBuf::from("reports"));
        assert_eq!(r.cfg.vt11_variant, "VT11_XDOCK_VARIANT");
        assert_eq!(r.cfg.warehouse, "E01", "wrong type is ignored");
    }

    #[test]
    fn report_checks() {
        assert_eq!(check_report(" MustGo ").unwrap(), "mustgo");
        let e = check_report("nope").unwrap_err().to_string();
        assert!(e.contains("unknown report") && e.contains("mustgo"), "{e}");
    }

    #[test]
    fn error_descriptions() {
        let nd: anyhow::Error = NoDataError {
            step: "vt11".into(),
            message: "none".into(),
        }
        .into();
        assert_eq!(describe_error(&nd), "NO DATA at step 'vt11': none");
        let other = anyhow!("boom").context("mustgo step 'lt23'");
        let m = describe_error(&other);
        assert!(m.starts_with("ERROR:") && m.contains("boom"), "{m}");
    }

    #[test]
    fn summary_text() {
        let s = RunSummary {
            outputs: vec![PathBuf::from("a.tsv")],
            export_date: "2026-11-05".into(),
            shipments: 54,
            deliveries: 107,
            transfer_orders: 87,
            lt23_rows: 15,
            snr_copy_rows: 92,
            pivot_rows: 6,
        };
        let l = summary_lines("mustgo", &s);
        assert!(l[0].contains("2026-11-05"));
        assert!(l[1].contains("54") && l[1].contains("107") && l[1].contains("87"));
        assert!(l.last().unwrap().contains("a.tsv"));
    }
}
