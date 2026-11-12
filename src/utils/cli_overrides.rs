//! Process-wide store for CLI flag overrides.
//!
//! `main` builds a [`CliOverrides`] from the parsed [`crate::cli::Cli`] and installs
//! it via [`set_cli_overrides`] before any config is loaded. The various
//! `*Config::load` paths then call [`cli_overrides`] and merge in any values that
//! were set on the command line (CLI wins over `config.toml`).
//!
//! Some helpers are only used from tests or future CLI plumbing; the library build
//! linked from the binary does not see every `pub` use (policy 4.A).
#![allow(dead_code)]

use crate::abc::run::RunDate;
use chrono::NaiveDate;
use std::sync::OnceLock;

/// `--output-format`: extra output next to the canonical TSV files of a headless report run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, clap::ValueEnum)]
pub enum OutputFormat {
    /// TSV files only (default, legacy behaviour).
    #[default]
    Tsv,
    /// TSV files plus one `<DATE>_<RNAME>.xlsx` workbook with every output table as a sheet.
    Xlsx,
}

static UNATTENDED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Mark the process as an unattended run (`--tcode`, loop, sequence, `--rname`): no prompts.
pub fn set_unattended(on: bool) {
    UNATTENDED.store(on, std::sync::atomic::Ordering::Relaxed);
}

pub fn is_unattended() -> bool {
    UNATTENDED.load(std::sync::atomic::Ordering::Relaxed)
}

/// May the program ask the user something? Not in unattended mode, and only with a terminal.
pub fn can_prompt() -> bool {
    use std::io::IsTerminal;
    !is_unattended() && std::io::stdin().is_terminal() && std::io::stdout().is_terminal()
}

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

    /// 149-report run type (rcv | mat | tsp | none). Omit / none = regular.
    pub tcode_run_type: Option<String>,

    /// Regular 149 plant list (from `--plants=a,b`). Wins over config `plants`.
    pub plants: Option<Vec<String>>,

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
    /// MM60-only: LX03 export the material list comes from (path or slug). Default: newest
    /// file of the LX03 export dir.
    pub lx03_file: Option<String>,

    /// VT11-only: ignore "no display authorization" status-bar errors.
    /// Maps to `[tcode.VT11].ignore_display_error` in `config.toml`.
    pub ignore_display_error: Option<bool>,

    /// VT11-only: actual shipment end ("closed") date range (`K_DATEN`), inclusive.
    pub vt11_closed_date: Option<(NaiveDate, NaiveDate)>,
    /// VT11-only: creation date range (`K_ERDAT`), inclusive.
    pub vt11_created_date: Option<(NaiveDate, NaiveDate)>,

    /// Export subdirectory (`--rname-dir`). Relative = under the reports dir; files land
    /// in `<reports_dir>\<rname_dir>\<tcode>`. Absolute paths are used as is.
    pub rname_dir: Option<String>,

    /// Headless report name (`--rname`), canonical registry name (e.g. `mustgo`).
    pub rname: Option<String>,

    /// Run date of a headless report (`--run-date`): single date, range or week.
    pub run_date: Option<RunDate>,

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

    /// ZMDESNR-only: storage bin filter (e.g. `64EP*`). Maps to
    /// `[tcode.ZMDESNR].bin_filter` in `config.toml`. When set, the export
    /// fills LGPLA-LOW and skips delivery multi-paste.
    pub bin_filter: Option<String>,

    /// `--date-range-buffer`: days added on each side of the run date range where a headless
    /// report sets a buffered date range (ZMDETPC today). `None` = config, else 3.
    pub date_range_buffer: Option<u32>,

    /// `--use-clipboard`: fill multi-selection popups (deliveries, shipments, TOs, ...) from the
    /// Windows clipboard (Shift+F12 "upload from clipboard") instead of typing row by row.
    /// `None` = off (legacy behaviour).
    pub use_clipboard: Option<bool>,

    /// `--output-format`: `tsv` (default) or `xlsx` (TSV plus an xlsx workbook per report run).
    pub output_format: Option<OutputFormat>,
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
        if self.plants.is_some() {
            v.push("--plants");
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
        if self.lx03_file.is_some() {
            v.push("--lx03-file");
        }
        if self.shipment_col.is_some() {
            v.push("--shipment-col");
        }
        if self.ignore_display_error.is_some() {
            v.push("--ignore-display-error");
        }
        if self.vt11_closed_date.is_some() {
            v.push("--vt11-closed-date");
        }
        if self.vt11_created_date.is_some() {
            v.push("--vt11-created-date");
        }
        if self.pre_export_back.is_some() {
            v.push("--pre-export-back");
        }
        if self.tab_number.is_some() {
            v.push("--tab-number");
        }
        if self.bin_filter.is_some() {
            v.push("--bin-filter");
        }
        v
    }

    /// Pretty one-line summary of which flags are overriding config. Returns
    /// `None` when no overrides are set.
    pub fn summary_line(&self) -> Option<String> {
        let mut parts: Vec<String> = Vec::new();
        if let Some(v) = &self.tcode {
            parts.push(format!("--tcode={v}"));
        }
        if let Some(v) = &self.layout {
            parts.push(format!("--layout={v}"));
        }
        if let Some(v) = &self.variant {
            parts.push(format!("--variant={v}"));
        }
        if let Some(v) = self.export_type {
            parts.push(format!("--export-type={v}"));
        }
        if let Some(v) = &self.tcode_run_type {
            parts.push(format!("--tcode-run-type={v}"));
        }
        if let Some(v) = &self.plants {
            parts.push(format!("--plants={}", v.join(",")));
        }
        if let Some(v) = self.iterations {
            parts.push(format!("--iterations={v}"));
        }
        if let Some(v) = self.interval_seconds {
            parts.push(format!("--interval-seconds={v}"));
        }
        if let Some(v) = self.delay_seconds {
            parts.push(format!("--delay-seconds={v}"));
        }
        if let Some(v) = &self.reports_dir {
            parts.push(format!("--reports-dir={v}"));
        }
        if let Some(v) = &self.date_format {
            parts.push(format!("--date-format={v}"));
        }
        if let Some(v) = &self.timezone {
            parts.push(format!("--timezone={v}"));
        }
        if let Some(v) = self.by_date {
            parts.push(format!("--by-date={v}"));
        }
        if let Some(v) = self.by_delivery {
            parts.push(format!("--by-delivery={v}"));
        }
        if let Some(v) = self.by_shipment {
            parts.push(format!("--by-shipment={v}"));
        }
        if let Some(v) = &self.limiter {
            parts.push(format!("--limiter={v}"));
        }
        if let Some(v) = self.date_start {
            parts.push(format!("--date-start={}", v.format("%Y-%m-%d")));
        }
        if let Some(v) = self.date_end {
            parts.push(format!("--date-end={}", v.format("%Y-%m-%d")));
        }
        if let Some(v) = &self.delivery_file {
            parts.push(format!("--delivery-file={v}"));
        }
        if let Some(v) = &self.delivery_col {
            parts.push(format!("--delivery-col={v}"));
        }
        if let Some(v) = &self.shipment_file {
            parts.push(format!("--shipment-file={v}"));
        }
        if let Some(v) = &self.lx03_file {
            parts.push(format!("--lx03-file={v}"));
        }
        if let Some(v) = &self.shipment_col {
            parts.push(format!("--shipment-col={v}"));
        }
        if let Some(v) = self.ignore_display_error {
            parts.push(format!("--ignore-display-error={v}"));
        }
        for (flag, range) in [
            ("--vt11-closed-date", self.vt11_closed_date),
            ("--vt11-created-date", self.vt11_created_date),
        ] {
            if let Some((s, e)) = range {
                parts.push(format!("{flag}={s}..{e}"));
            }
        }
        if let Some(v) = self.date_range_buffer {
            parts.push(format!("--date-range-buffer={v}"));
        }
        if let Some(v) = &self.rname_dir {
            parts.push(format!("--rname-dir={v}"));
        }
        if let Some(v) = &self.rname {
            parts.push(format!("--rname={v}"));
        }
        if let Some(v) = &self.run_date {
            parts.push(format!(
                "--run-date={}",
                crate::abc::runner::export_date_label(v)
            ));
        }
        if let Some(v) = self.pre_export_back {
            parts.push(format!("--pre-export-back={v}"));
        }
        if let Some(v) = self.tab_number {
            parts.push(format!("--tab-number={v}"));
        }
        if let Some(v) = &self.bin_filter {
            parts.push(format!("--bin-filter={v}"));
        }
        if let Some(v) = self.use_clipboard {
            parts.push(format!("--use-clipboard={v}"));
        }
        if let Some(OutputFormat::Xlsx) = self.output_format {
            parts.push("--output-format=xlsx".to_string());
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

/// Is `--use-clipboard` active? Off unless the flag was passed (true / 1 / yes / on).
pub fn use_clipboard() -> bool {
    cli_overrides().use_clipboard.unwrap_or(false)
}

/// Active `--output-format` (tsv unless the flag was passed).
pub fn output_format() -> OutputFormat {
    cli_overrides().output_format.unwrap_or_default()
}

/// Returns the active overrides, or an empty default when `main` hasn't
/// installed any (e.g. unit tests, library consumers).
pub fn cli_overrides() -> &'static CliOverrides {
    CLI_OVERRIDES.get_or_init(CliOverrides::default)
}
