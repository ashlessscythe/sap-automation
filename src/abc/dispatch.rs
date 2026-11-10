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
//!
//! [abc.k2xx]
//! vt11_variant = "VT11_K2XX_VARIANT"
//! vt11_layout = "scorecard"
//! monday_include_sunday = true    # a Monday run date makes VT11 start on the Sunday
//! vl06o_layout = "pallets"
//! zvt11_variant = "k2xx_"
//! zvt11_layout = "add_del"
//! zvt11_days = 100                # ZVT11 Closed dates = run date -/+ this many days
//! zmdetpc_variant = "ZMDETPC_VARIANT"
//!
//! [abc.misc]                         # vt11_variant = "VT11_MISC_VARIANT", vt11_layout, vl06o_layout = "inv"
//!
//! [abc.toyota]                       # same keys as [abc.xdock]; only the defaults differ
//! vt11_variant = "VT11_TOYOTA_VARIANT"        # (vt11_layout, vl06o_layout, zmdetpc_variant: as xdock)
//!
//! [abc.xdock]
//! vt11_variant = "VT11_XDOCK_VARIANT"
//! vt11_layout = "scorecard"
//! vl06o_layout = "xdock"
//! zmdetpc_variant = "ZMDETPC_VARIANT"   # from the toyota row of the spec; check the variant exists
//! delete_exports = true
//! ignore_display_error = true
//! date_range_buffer = 3           # any [abc.<report>]: days added each side of the run date range of
//!                                 # every tcode; unset = 3 for ZMDETPC on a week (WK<n>), else 0
//! ```
//! `reports_dir` and `date_format` come from `--reports-dir`/`--date-format`, else `[global]`.
#![allow(dead_code)]

use super::adapters::{MustgoSapConfig, RealSapSteps};
use super::inv_runner::{run_misc, run_toyota, run_xdock, InvSelection, InvSummary};
use super::k2xx_runner::{run_k2xx, K2xxSelection, K2xxSummary};
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
    /// `--date-range-buffer` (days).
    pub cli_date_range_buffer: Option<u32>,
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

fn int(section: &Option<toml::Value>, key: &str) -> Option<i64> {
    section.as_ref()?.get(key)?.as_integer()
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
        // CLI > `[abc.<report>] date_range_buffer` > none (the per-tcode defaults apply:
        // 3 days for ZMDETPC on a week run, otherwise 0)
        date_range_buffer: input
            .cli_date_range_buffer
            .map(i64::from)
            .or_else(|| int(&input.section, "date_range_buffer")),
    };
    ResolvedMustgo {
        cfg,
        reports_dir: PathBuf::from(reports_dir),
    }
}

/// Effective settings of an INV run (`xdock`, `toyota`, `misc`).
#[derive(Debug, Clone)]
pub struct ResolvedInv {
    /// Export/session settings shared with `mustgo` (its VT11/ZMDESNR/LT23 names are unused).
    pub cfg: MustgoSapConfig,
    pub sel: InvSelection,
    /// Root of the outputs (`<reports_dir>\INV\<today>\xdock\...`).
    pub reports_dir: PathBuf,
}

/// CLI > `[abc.xdock]` > `[global]` > defaults.
pub fn resolve_xdock(input: &ResolveInput) -> ResolvedInv {
    resolve_inv(input, InvSelection::xdock())
}

/// CLI > `[abc.toyota]` > `[global]` > defaults.
pub fn resolve_toyota(input: &ResolveInput) -> ResolvedInv {
    resolve_inv(input, InvSelection::toyota())
}

/// CLI > `[abc.misc]` > `[global]` > defaults.
pub fn resolve_misc(input: &ResolveInput) -> ResolvedInv {
    resolve_inv(input, InvSelection::misc())
}

fn resolve_inv(input: &ResolveInput, d: InvSelection) -> ResolvedInv {
    let base = resolve_mustgo(input);
    let sel = InvSelection {
        vt11_variant: s(&input.section, "vt11_variant").unwrap_or(d.vt11_variant),
        vt11_layout: s(&input.section, "vt11_layout").unwrap_or(d.vt11_layout),
        vl06o_layout: s(&input.section, "vl06o_layout").unwrap_or(d.vl06o_layout),
        zmdetpc_variant: s(&input.section, "zmdetpc_variant").unwrap_or(d.zmdetpc_variant),
    };
    ResolvedInv {
        cfg: base.cfg,
        sel,
        reports_dir: base.reports_dir,
    }
}

/// Effective settings of a `k2xx` run.
#[derive(Debug, Clone)]
pub struct ResolvedK2xx {
    /// Export/session settings shared with `mustgo` (its VT11/ZMDESNR/LT23 names are unused).
    pub cfg: MustgoSapConfig,
    pub sel: K2xxSelection,
    /// Root of the outputs (`<reports_dir>\K2XX\<today>\k2xx\...`).
    pub reports_dir: PathBuf,
}

/// CLI > `[abc.k2xx]` > `[global]` > defaults (spec 5.5). A negative `zvt11_days` is ignored.
pub fn resolve_k2xx(input: &ResolveInput) -> ResolvedK2xx {
    let base = resolve_mustgo(input);
    let d = K2xxSelection::default();
    let sel = K2xxSelection {
        vt11_variant: s(&input.section, "vt11_variant").unwrap_or(d.vt11_variant),
        vt11_layout: s(&input.section, "vt11_layout").unwrap_or(d.vt11_layout),
        monday_include_sunday: b(&input.section, "monday_include_sunday")
            .unwrap_or(d.monday_include_sunday),
        vl06o_layout: s(&input.section, "vl06o_layout").unwrap_or(d.vl06o_layout),
        zvt11_variant: s(&input.section, "zvt11_variant").unwrap_or(d.zvt11_variant),
        zvt11_layout: s(&input.section, "zvt11_layout").unwrap_or(d.zvt11_layout),
        zvt11_days: int(&input.section, "zvt11_days")
            .filter(|n| *n >= 0)
            .unwrap_or(d.zvt11_days),
        zmdetpc_variant: s(&input.section, "zmdetpc_variant").unwrap_or(d.zmdetpc_variant),
    };
    ResolvedK2xx {
        cfg: base.cfg,
        sel,
        reports_dir: base.reports_dir,
    }
}

/// Resolve `k2xx` from the CLI overrides and `config.toml` (section `[abc.k2xx]`).
pub fn resolve_k2xx_from_environment() -> ResolvedK2xx {
    resolve_k2xx(&environment_input("k2xx"))
}

/// Printable summary lines of a finished `k2xx` run.
pub fn k2xx_summary_lines(name: &str, s: &K2xxSummary) -> Vec<String> {
    let mut v = vec![
        format!("{name} finished (date {})", s.export_date),
        format!(
            "  shipments {}, deliveries {}, zmdetpc rows {} ({} deliveries without one)",
            s.shipments, s.deliveries, s.zmdetpc_rows, s.without_zmdetpc
        ),
        format!(
            "  rows: zvt11 {}, vl06o {}, pivot {} (total {})",
            s.zvt11_rows, s.vl06o_rows, s.pivot_rows, s.total
        ),
    ];
    v.extend(s.outputs.iter().map(|p| format!("  wrote {}", p.display())));
    v
}

/// Resolve from the CLI overrides and `config.toml` (caller layer: this reads the config).
pub fn resolve_from_environment() -> ResolvedMustgo {
    resolve_mustgo(&environment_input("mustgo"))
}

/// Same for `xdock` (section `[abc.xdock]`).
pub fn resolve_xdock_from_environment() -> ResolvedInv {
    resolve_xdock(&environment_input("xdock"))
}

/// Same for `toyota` (section `[abc.toyota]`).
pub fn resolve_toyota_from_environment() -> ResolvedInv {
    resolve_toyota(&environment_input("toyota"))
}

/// Same for `misc` (section `[abc.misc]`).
pub fn resolve_misc_from_environment() -> ResolvedInv {
    resolve_misc(&environment_input("misc"))
}

/// The resolver input for report `name`: CLI overrides, `[global]` and `[abc.<name>]`.
pub fn environment_input(name: &str) -> ResolveInput {
    let cli = crate::utils::cli_overrides::cli_overrides();
    let config = crate::utils::config_types::SapConfig::load().ok();
    let section = config
        .as_ref()
        .and_then(|c| c.raw_config.as_ref())
        .and_then(|r| r.get("abc"))
        .and_then(|a| a.get(name))
        .cloned();
    ResolveInput {
        cli_reports_dir: cli.reports_dir.clone(),
        cli_date_format: cli.date_format.clone(),
        cli_ignore_display_error: cli.ignore_display_error,
        cli_date_range_buffer: cli.date_range_buffer,
        global_reports_dir: config
            .as_ref()
            .and_then(|c| c.global.as_ref())
            .map(|g| g.reports_dir.clone()),
        global_date_format: config
            .as_ref()
            .and_then(|c| c.global.as_ref())
            .map(|g| g.date_format.clone()),
        section,
    }
}

/// Names of the reports that have a runner.
pub fn runnable_reports() -> &'static [&'static str] {
    &["mustgo", "xdock", "toyota", "misc", "k2xx"]
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

/// Printable summary lines of a finished `xdock` run.
pub fn inv_summary_lines(name: &str, s: &InvSummary) -> Vec<String> {
    let mut v = vec![
        format!("{name} finished (date {})", s.export_date),
        format!(
            "  shipments {}, deliveries {}, zmdetpc rows {} ({} deliveries without one)",
            s.shipments, s.deliveries, s.zmdetpc_rows, s.without_zmdetpc
        ),
        format!(
            "  rows: vl06o {}, pivot {} (total {})",
            s.vl06o_rows, s.pivot_rows, s.total
        ),
    ];
    if !s.trailers.is_empty() {
        let t: Vec<String> = s.trailers.iter().map(|(p, n)| format!("{p} {n}")).collect();
        v.push(format!("  trailers (total / 14.5): {}", t.join(", ")));
    }
    v.extend(s.outputs.iter().map(|p| format!("  wrote {}", p.display())));
    v
}

/// Printable summary lines of a finished `misc` run.
pub fn misc_summary_lines(name: &str, s: &InvSummary) -> Vec<String> {
    let mut v = vec![
        format!("{name} finished (date {})", s.export_date),
        format!(
            "  shipments {}, deliveries {}, PLACEHOLDER_SP shipping points replaced {}",
            s.shipments, s.deliveries, s.regex_replaced
        ),
        format!(
            "  rows: vl06o {}, pivot {} (total packages {})",
            s.vl06o_rows, s.pivot_rows, s.total
        ),
    ];
    v.extend(s.outputs.iter().map(|p| format!("  wrote {}", p.display())));
    v
}

/// Run report `name` for `date` on a live session and print the summary; returns the files
/// written. Thin: attach/login checks are the caller's; everything else is tested through the
/// pieces above.
pub fn run_report(session: &GuiSession, name: &str, date: &RunDate) -> Result<Vec<PathBuf>> {
    let name = check_report(name)?;
    let today = chrono::Local::now().date_naive();
    let result: Result<(Vec<String>, Vec<PathBuf>)> = match name {
        "mustgo" => {
            let resolved = resolve_from_environment();
            let out = OutputSpec {
                reports_dir: resolved.reports_dir.clone(),
                today,
            };
            announce(name, date, &resolved.cfg.export_dir, &out);
            let mut steps = RealSapSteps::new(session, resolved.cfg);
            run_mustgo(&mut steps, date, &out).map(|s| (summary_lines(name, &s), s.outputs))
        }
        "xdock" => {
            let resolved = resolve_xdock_from_environment();
            let out = OutputSpec {
                reports_dir: resolved.reports_dir.clone(),
                today,
            };
            announce(name, date, &resolved.cfg.export_dir, &out);
            let mut steps = RealSapSteps::new(session, resolved.cfg);
            run_xdock(&mut steps, date, &resolved.sel, &out)
                .map(|s| (inv_summary_lines(name, &s), s.outputs))
        }
        "toyota" => {
            let resolved = resolve_toyota_from_environment();
            let out = OutputSpec {
                reports_dir: resolved.reports_dir.clone(),
                today,
            };
            announce(name, date, &resolved.cfg.export_dir, &out);
            let mut steps = RealSapSteps::new(session, resolved.cfg);
            run_toyota(&mut steps, date, &resolved.sel, &out)
                .map(|s| (inv_summary_lines(name, &s), s.outputs))
        }
        "misc" => {
            let resolved = resolve_misc_from_environment();
            let out = OutputSpec {
                reports_dir: resolved.reports_dir.clone(),
                today,
            };
            announce(name, date, &resolved.cfg.export_dir, &out);
            let mut steps = RealSapSteps::new(session, resolved.cfg);
            run_misc(&mut steps, date, &resolved.sel, &out)
                .map(|s| (misc_summary_lines(name, &s), s.outputs))
        }
        "k2xx" => {
            let resolved = resolve_k2xx_from_environment();
            let out = OutputSpec {
                reports_dir: resolved.reports_dir.clone(),
                today,
            };
            announce(name, date, &resolved.cfg.export_dir, &out);
            let mut steps = RealSapSteps::new(session, resolved.cfg);
            run_k2xx(&mut steps, date, &resolved.sel, &out)
                .map(|s| (k2xx_summary_lines(name, &s), s.outputs))
        }
        other => Err(anyhow!("no runner for '{other}'")),
    };
    match result {
        Ok((lines, outputs)) => {
            for l in lines {
                println!("{l}");
            }
            Ok(outputs)
        }
        Err(e) => {
            eprintln!("{}", describe_error(&e));
            Err(e)
        }
    }
}

fn announce(name: &str, date: &RunDate, export_dir: &str, out: &OutputSpec) {
    println!(
        "Running report '{name}' for {} (exports under {export_dir}, outputs under {})",
        super::runner::export_date_label(date),
        out.reports_dir.display()
    );
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
            cli_date_range_buffer: None,
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
    fn xdock_defaults_and_precedence() {
        let r = resolve_xdock(&ResolveInput::default());
        assert_eq!(r.sel, InvSelection::xdock());
        assert_eq!(r.sel.vt11_variant, "VT11_XDOCK_VARIANT");
        assert_eq!(r.sel.vt11_layout, "scorecard");
        assert_eq!(r.sel.vl06o_layout, "xdock");
        assert_eq!(r.sel.zmdetpc_variant, "ZMDETPC_VARIANT");
        assert_eq!(r.reports_dir, PathBuf::from("reports"));
        assert!(r.cfg.delete_exports && r.cfg.ignore_display_error);

        let r = resolve_xdock(&ResolveInput {
            cli_reports_dir: Some("C:/cli".into()),
            global_reports_dir: Some("C:/global".into()),
            cli_date_format: Some("mm/dd/yyyy".into()),
            cli_ignore_display_error: Some(false),
            section: section(
                "zmdetpc_variant = \"MYVAR\"\nvl06o_layout = \"xd2\"\nvt11_layout = \"\"\ndelete_exports = false\nignore_display_error = true\n",
            ),
            ..ResolveInput::default()
        });
        assert_eq!(r.reports_dir, PathBuf::from("C:/cli"));
        assert_eq!(r.cfg.date_format.as_deref(), Some("mm/dd/yyyy"));
        assert!(!r.cfg.ignore_display_error, "CLI beats the section");
        assert!(!r.cfg.delete_exports);
        assert_eq!(r.sel.zmdetpc_variant, "MYVAR");
        assert_eq!(r.sel.vl06o_layout, "xd2");
        assert_eq!(r.sel.vt11_layout, "scorecard", "blank value falls through");
        assert_eq!(r.sel.vt11_variant, "VT11_XDOCK_VARIANT");
    }

    #[test]
    fn date_range_buffer_default_and_override() {
        // nothing set: no explicit value (the per-tcode defaults apply later)
        assert_eq!(
            resolve_xdock(&ResolveInput::default())
                .cfg
                .date_range_buffer,
            None
        );
        let r = resolve_toyota(&ResolveInput {
            section: section("date_range_buffer = 5\n"),
            ..ResolveInput::default()
        });
        assert_eq!(r.cfg.date_range_buffer, Some(5));
        // the CLI flag beats the section, and 0 is a valid value (no buffer)
        let r = resolve_toyota(&ResolveInput {
            cli_date_range_buffer: Some(0),
            section: section("date_range_buffer = 5\n"),
            ..ResolveInput::default()
        });
        assert_eq!(r.cfg.date_range_buffer, Some(0));
        let r = resolve_xdock(&ResolveInput {
            cli_date_range_buffer: Some(7),
            ..ResolveInput::default()
        });
        assert_eq!(r.cfg.date_range_buffer, Some(7));
        // a wrong type is ignored
        let r = resolve_xdock(&ResolveInput {
            section: section("date_range_buffer = \"x\"\n"),
            ..ResolveInput::default()
        });
        assert_eq!(r.cfg.date_range_buffer, None);
    }

    #[test]
    fn toyota_defaults_and_precedence() {
        let r = resolve_toyota(&ResolveInput::default());
        assert_eq!(r.sel, InvSelection::toyota());
        assert_eq!(r.sel.vt11_variant, "VT11_TOYOTA_VARIANT");
        assert_eq!(r.sel.vt11_layout, "scorecard");
        assert_eq!(r.sel.vl06o_layout, "xdock");
        assert_eq!(r.sel.zmdetpc_variant, "ZMDETPC_VARIANT");
        let r = resolve_toyota(&ResolveInput {
            section: section("vt11_variant = \"MYTOYO\"\n"),
            ..ResolveInput::default()
        });
        assert_eq!(r.sel.vt11_variant, "MYTOYO");
        // xdock keeps its own default
        assert_eq!(
            resolve_xdock(&ResolveInput::default()).sel.vt11_variant,
            "VT11_XDOCK_VARIANT"
        );
    }

    #[test]
    fn misc_defaults_and_summary() {
        let r = resolve_misc(&ResolveInput::default());
        assert_eq!(r.sel, InvSelection::misc());
        assert_eq!(r.sel.vt11_variant, "VT11_MISC_VARIANT");
        assert_eq!(r.sel.vt11_layout, "scorecard");
        assert_eq!(r.sel.vl06o_layout, "inv");
        let r = resolve_misc(&ResolveInput {
            section: section("vl06o_layout = \"inv2\"\n"),
            ..ResolveInput::default()
        });
        assert_eq!(r.sel.vl06o_layout, "inv2");
        assert_eq!(check_report("MISC").unwrap(), "misc");
        let s = InvSummary {
            outputs: vec![PathBuf::from("m.tsv")],
            export_date: "2026-11-05".into(),
            shipments: 3,
            deliveries: 3,
            vl06o_rows: 3,
            pivot_rows: 2,
            total: "2".into(),
            regex_replaced: 1,
            ..InvSummary::default()
        };
        let l = misc_summary_lines("misc", &s);
        assert!(l[1].contains("replaced 1") && l[2].contains("pivot 2"));
        assert!(!l.iter().any(|x| x.contains("zmdetpc")));
        assert!(l.last().unwrap().contains("m.tsv"));
    }

    #[test]
    fn k2xx_defaults_precedence_and_summary() {
        let r = resolve_k2xx(&ResolveInput::default());
        assert_eq!(r.sel, K2xxSelection::default());
        assert_eq!(r.sel.vt11_variant, "VT11_K2XX_VARIANT");
        assert_eq!(r.sel.vt11_layout, "scorecard");
        assert!(r.sel.monday_include_sunday);
        assert_eq!(r.sel.vl06o_layout, "pallets");
        assert_eq!(r.sel.zvt11_variant, "k2xx_");
        assert_eq!(r.sel.zvt11_layout, "add_del");
        assert_eq!(r.sel.zvt11_days, 100);
        assert_eq!(r.sel.zmdetpc_variant, "ZMDETPC_VARIANT");
        assert_eq!(r.reports_dir, PathBuf::from("reports"));

        let r = resolve_k2xx(&ResolveInput {
            cli_reports_dir: Some("C:/cli".into()),
            cli_ignore_display_error: Some(false),
            section: section(
                "vt11_variant = \"V\"\nmonday_include_sunday = false\nzvt11_variant = \"Z\"\nzvt11_layout = \"\"\nzvt11_days = 30\nvl06o_layout = \"p2\"\ndelete_exports = false\ndate_range_buffer = 2\n",
            ),
            ..ResolveInput::default()
        });
        assert_eq!(r.reports_dir, PathBuf::from("C:/cli"));
        assert!(!r.cfg.ignore_display_error && !r.cfg.delete_exports);
        assert_eq!(r.cfg.date_range_buffer, Some(2));
        assert_eq!(r.sel.vt11_variant, "V");
        assert!(!r.sel.monday_include_sunday);
        assert_eq!(r.sel.zvt11_variant, "Z");
        assert_eq!(r.sel.zvt11_layout, "add_del", "blank value falls through");
        assert_eq!(r.sel.zvt11_days, 30);
        assert_eq!(r.sel.vl06o_layout, "p2");
        // a negative or wrongly typed day count is ignored
        for bad in ["zvt11_days = -1\n", "zvt11_days = \"x\"\n"] {
            let r = resolve_k2xx(&ResolveInput {
                section: section(bad),
                ..ResolveInput::default()
            });
            assert_eq!(r.sel.zvt11_days, 100);
        }

        assert_eq!(check_report(" K2XX ").unwrap(), "k2xx");
        let s = K2xxSummary {
            outputs: vec![PathBuf::from("k.tsv")],
            export_date: "2026-11-05".into(),
            shipments: 54,
            deliveries: 107,
            zvt11_rows: 409,
            zmdetpc_rows: 106,
            without_zmdetpc: 1,
            vl06o_rows: 107,
            pivot_rows: 12,
            total: "900".into(),
        };
        let l = k2xx_summary_lines("k2xx", &s);
        assert!(l[0].contains("k2xx") && l[0].contains("2026-11-05"));
        assert!(l[1].contains("54") && l[1].contains("107") && l[1].contains("106"));
        assert!(l[2].contains("409") && l[2].contains("pivot 12") && l[2].contains("900"));
        assert!(l.last().unwrap().contains("k.tsv"));
    }

    #[test]
    fn inv_summary_text() {
        let s = InvSummary {
            outputs: vec![PathBuf::from("b.tsv")],
            export_date: "2026-11-05".into(),
            shipments: 91,
            deliveries: 148,
            zmdetpc_rows: 140,
            without_zmdetpc: 8,
            vl06o_rows: 148,
            pivot_rows: 13,
            total: "1493".into(),
            trailers: vec![("PL55".into(), "0".into())],
            regex_replaced: 0,
        };
        let l = inv_summary_lines("xdock", &s);
        assert!(l[0].contains("xdock") && l[0].contains("2026-11-05"));
        assert!(l[1].contains("91") && l[1].contains("148") && l[1].contains('8'));
        assert!(l[2].contains("1493"));
        assert!(l
            .iter()
            .any(|x| x.contains("trailers") && x.contains("PL55 0")));
        assert!(l.last().unwrap().contains("b.tsv"));
    }

    #[test]
    fn report_checks() {
        assert_eq!(check_report(" XDock ").unwrap(), "xdock");
        assert_eq!(check_report("TOYOTA").unwrap(), "toyota");
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
