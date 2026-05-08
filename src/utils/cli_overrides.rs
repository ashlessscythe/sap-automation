//! Process-wide store for CLI flag overrides.
//!
//! `main` builds a [`CliOverrides`] from the parsed [`crate::cli::Cli`] and installs
//! it via [`set_cli_overrides`] before any config is loaded. The various
//! `*Config::load` paths then call [`cli_overrides`] and merge in any values that
//! were set on the command line (CLI wins over `config.toml`).

use chrono::NaiveDate;
use std::sync::OnceLock;

#[derive(Debug, Default, Clone)]
pub struct CliOverrides {
    /// Target TCode (case-insensitive on input; stored upper-cased).
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

    // ------ delivery / shipment / date filter overrides (per-tcode) ------
    /// Filter the SAP report by date range.
    pub by_date: Option<bool>,
    /// Filter the SAP report by delivery numbers (VT11/ZVT11/VL06O).
    pub by_delivery: Option<bool>,
    /// Filter the SAP report by shipment numbers (VL06O only).
    pub by_shipment: Option<bool>,
    /// VT11/ZVT11 limiter type (e.g. `date_range`). Other values are no-ops today.
    pub limiter: Option<String>,
    /// Inclusive date-range start as ISO `YYYY-MM-DD`.
    pub date_start: Option<NaiveDate>,
    /// Inclusive date-range end as ISO `YYYY-MM-DD`.
    pub date_end: Option<NaiveDate>,
    /// Source for delivery numbers: a subdir slug under `reports_dir` (lower-cased
    /// at lookup time), or a literal path to a CSV/XLSX/etc.
    pub delivery_file: Option<String>,
    /// Header column to read for delivery numbers. Default `"Delivery"`.
    pub delivery_col: Option<String>,
    /// Source for shipment numbers (VL06O only). Same resolution rules as
    /// `delivery_file`.
    pub shipment_file: Option<String>,
    /// Header column to read for shipment numbers. Default `"Shipment Number"`.
    pub shipment_col: Option<String>,

    /// ZMDESNR-only: send vkey 3 (back) after export but before layout selection.
    /// Maps to `[tcode.ZMDESNR].pre_export_back` in `config.toml`.
    pub pre_export_back: Option<bool>,

    /// ZMDESNR-only: which results tab to select. Maps to
    /// `[tcode.ZMDESNR].tab_number` in `config.toml`. Resolution order:
    /// CLI override → config → in-code default of `2`.
    ///
    /// NOTE: SAP GUI requires a valid plant (`WERKS`) value to be entered
    /// before tab switching works, so the report flow always selects the
    /// variant first (which fills `WERKS`) and only then switches to this tab.
    pub tab_number: Option<i32>,
}

impl CliOverrides {
    /// Heuristic: does `value` look like a literal path rather than a subdir slug?
    /// `--delivery-file=vt11` is a slug; `--delivery-file=C:\foo\bar.csv` or
    /// `--delivery-file=./out.xlsx` is a literal path.
    pub fn looks_like_path(value: &str) -> bool {
        value.contains('\\') || value.contains('/') || value.contains('.')
    }

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
        if self.by_date.is_some() {
            v.push("--by-date");
        }
        if self.by_delivery.is_some() {
            v.push("--by-delivery");
        }
        if self.by_shipment.is_some() {
            v.push("--by-shipment");
        }
        if self.limiter.is_some() {
            v.push("--limiter");
        }
        if self.date_start.is_some() {
            v.push("--date-start");
        }
        if self.date_end.is_some() {
            v.push("--date-end");
        }
        if self.delivery_file.is_some() {
            v.push("--delivery-file");
        }
        if self.delivery_col.is_some() {
            v.push("--delivery-col");
        }
        if self.shipment_file.is_some() {
            v.push("--shipment-file");
        }
        if self.shipment_col.is_some() {
            v.push("--shipment-col");
        }
        if self.pre_export_back.is_some() {
            v.push("--pre-export-back");
        }
        if self.tab_number.is_some() {
            v.push("--tab-number");
        }
        v
    }

    /// Pretty one-line summary of which flags are overriding config. Returns
    /// `None` when no overrides are set.
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
        if let Some(v) = self.by_date {
            parts.push(format!("--by-date={}", v));
        }
        if let Some(v) = self.by_delivery {
            parts.push(format!("--by-delivery={}", v));
        }
        if let Some(v) = self.by_shipment {
            parts.push(format!("--by-shipment={}", v));
        }
        if let Some(v) = &self.limiter {
            parts.push(format!("--limiter={}", v));
        }
        if let Some(v) = self.date_start {
            parts.push(format!("--date-start={}", v.format("%Y-%m-%d")));
        }
        if let Some(v) = self.date_end {
            parts.push(format!("--date-end={}", v.format("%Y-%m-%d")));
        }
        if let Some(v) = &self.delivery_file {
            parts.push(format!("--delivery-file={}", v));
        }
        if let Some(v) = &self.delivery_col {
            parts.push(format!("--delivery-col={}", v));
        }
        if let Some(v) = &self.shipment_file {
            parts.push(format!("--shipment-file={}", v));
        }
        if let Some(v) = &self.shipment_col {
            parts.push(format!("--shipment-col={}", v));
        }
        if let Some(v) = self.pre_export_back {
            parts.push(format!("--pre-export-back={}", v));
        }
        if let Some(v) = self.tab_number {
            parts.push(format!("--tab-number={}", v));
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
