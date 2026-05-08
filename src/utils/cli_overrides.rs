//! Process-wide store for CLI flag overrides.
//!
//! `main` builds a [`CliOverrides`] from the parsed [`crate::cli::Cli`] and installs
//! it via [`set_cli_overrides`] before any config is loaded. The various
//! `*Config::load` paths then call [`cli_overrides`] and merge in any values that
//! were set on the command line (CLI wins over `config.toml`).

use std::sync::OnceLock;

#[derive(Debug, Default, Clone)]
pub struct CliOverrides {
    /// Target TCode (case-insensitive on input; stored as-is, callers should
    /// uppercase before lookup).
    pub tcode: Option<String>,

    /// Per-tcode layout name.
    pub layout: Option<String>,

    /// Per-tcode variant name.
    pub variant: Option<String>,

    /// Per-tcode export type (0..=4).
    pub export_type: Option<u8>,

    /// Loop / sequence iteration count (0 = infinite).
    pub iterations: Option<usize>,

    /// Sequence-only: seconds between sequence steps.
    pub interval_seconds: Option<u64>,

    /// Loop / sequence: seconds between iterations.
    pub delay_seconds: Option<u64>,

    /// 149-report run type (rcv | mat | tsp).
    pub tcode_run_type: Option<String>,

    /// Global date format string.
    pub date_format: Option<String>,

    /// Global timezone string.
    pub timezone: Option<String>,

    /// Global reports directory.
    pub reports_dir: Option<String>,
}

impl CliOverrides {
    /// Names of per-tcode flags currently set. Used to produce a precise error
    /// when these are mixed with `--run-sequence`.
    pub fn per_tcode_flag_names(&self) -> Vec<&'static str> {
        let mut v = Vec::new();
        if self.tcode.is_some() {
            v.push("--tcode");
        }
        if self.layout.is_some() {
            v.push("--layout");
        }
        if self.variant.is_some() {
            v.push("--variant");
        }
        if self.export_type.is_some() {
            v.push("--export-type");
        }
        if self.tcode_run_type.is_some() {
            v.push("--tcode-run-type");
        }
        v
    }

    /// Pretty one-line summary of which flags are overriding config, e.g.
    /// `--tcode=VT11 --layout=ob_6 --iterations=3`. Returns `None` when no
    /// overrides are set.
    pub fn summary_line(&self) -> Option<String> {
        let mut parts: Vec<String> = Vec::new();
        if let Some(v) = &self.tcode {
            parts.push(format!("--tcode={}", v));
        }
        if let Some(v) = &self.layout {
            parts.push(format!("--layout={}", v));
        }
        if let Some(v) = &self.variant {
            parts.push(format!("--variant={}", v));
        }
        if let Some(v) = self.export_type {
            parts.push(format!("--export-type={}", v));
        }
        if let Some(v) = &self.tcode_run_type {
            parts.push(format!("--tcode-run-type={}", v));
        }
        if let Some(v) = self.iterations {
            parts.push(format!("--iterations={}", v));
        }
        if let Some(v) = self.interval_seconds {
            parts.push(format!("--interval-seconds={}", v));
        }
        if let Some(v) = self.delay_seconds {
            parts.push(format!("--delay-seconds={}", v));
        }
        if let Some(v) = &self.reports_dir {
            parts.push(format!("--reports-dir={}", v));
        }
        if let Some(v) = &self.date_format {
            parts.push(format!("--date-format={}", v));
        }
        if let Some(v) = &self.timezone {
            parts.push(format!("--timezone={}", v));
        }
        if parts.is_empty() {
            None
        } else {
            Some(parts.join(" "))
        }
    }
}

static CLI_OVERRIDES: OnceLock<CliOverrides> = OnceLock::new();

/// Install CLI overrides. May be called once per process; subsequent calls are
/// silently ignored.
pub fn set_cli_overrides(overrides: CliOverrides) {
    let _ = CLI_OVERRIDES.set(overrides);
}

/// Returns the active overrides, or an empty default when `main` hasn't
/// installed any (e.g. unit tests, library consumers).
pub fn cli_overrides() -> &'static CliOverrides {
    CLI_OVERRIDES.get_or_init(CliOverrides::default)
}
