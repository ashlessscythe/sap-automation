use anyhow::{anyhow, Result};
use chrono::NaiveDate;
use clap::{Parser, Subcommand};

use crate::utils::cli_overrides::CliOverrides;

#[derive(Parser)]
#[command(name = "sap_automation")]
#[command(about = "SAP GUI Automation utilities")]
#[command(version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,

    /// Run the loop configuration unattended (alias for the `run-loop` subcommand)
    #[arg(long = "run-loop", conflicts_with = "run_sequence")]
    pub run_loop: bool,

    /// Run the sequence configuration unattended (alias for the `run-sequence` subcommand)
    #[arg(long = "run-sequence", conflicts_with = "run_loop")]
    pub run_sequence: bool,

    /// Skip SAP connection check (for testing)
    #[arg(long, default_value = "false")]
    pub skip_sap_check: bool,

    /// Keep the system awake during execution
    #[arg(long, default_value = "false")]
    pub keep_awake: bool,

    /// Override target TCode (case-insensitive: vt11, VL06O, y_dn3_47000149, ...).
    /// On its own runs the TCode auto-flow once. With --run-loop, drives the loop.
    #[arg(long)]
    pub tcode: Option<String>,

    /// Override layout for the target TCode.
    #[arg(long)]
    pub layout: Option<String>,

    /// Override variant for the target TCode.
    #[arg(long)]
    pub variant: Option<String>,

    /// Override export type for the target TCode (0=unconverted, 1=text-tabs, 2=rich-text, 3=HTML, 4=clipboard).
    #[arg(long)]
    pub export_type: Option<u8>,

    /// Override TCode run-type (rcv | mat | tsp). Required when running 149 reports.
    #[arg(long)]
    pub tcode_run_type: Option<String>,

    /// Override iterations for --run-loop / --run-sequence (0 = infinite).
    #[arg(long)]
    pub iterations: Option<usize>,

    /// Override seconds between sequence steps (--run-sequence only).
    #[arg(long)]
    pub interval_seconds: Option<u64>,

    /// Override seconds between iterations of --run-loop / --run-sequence.
    #[arg(long)]
    pub delay_seconds: Option<u64>,

    /// Override [global].reports_dir.
    #[arg(long)]
    pub reports_dir: Option<String>,

    /// Override [global].date_format (e.g. yyyy-mm-dd, mm/dd/yyyy, dd-mm-yy).
    #[arg(long)]
    pub date_format: Option<String>,

    /// Override [global].timezone (e.g. UTC, MDT, America/Denver).
    #[arg(long)]
    pub timezone: Option<String>,

    // ---- per-tcode delivery / shipment / date filter overrides ----
    /// Filter the report by date range (true|false).
    #[arg(long, value_name = "BOOL")]
    pub by_date: Option<bool>,

    /// Filter the report by delivery numbers (VT11 | ZVT11 | VL06O) (true|false).
    #[arg(long, value_name = "BOOL")]
    pub by_delivery: Option<bool>,

    /// Filter the report by shipment numbers — VL06O ONLY (true|false).
    #[arg(long, value_name = "BOOL")]
    pub by_shipment: Option<bool>,

    /// VT11/ZVT11 limiter type. Currently only `date_range` is implemented;
    /// other values are accepted but no-ops.
    #[arg(long)]
    pub limiter: Option<String>,

    /// Inclusive date-range start in ISO format (YYYY-MM-DD).
    #[arg(long, value_name = "YYYY-MM-DD")]
    pub date_start: Option<String>,

    /// Inclusive date-range end in ISO format (YYYY-MM-DD).
    #[arg(long, value_name = "YYYY-MM-DD")]
    pub date_end: Option<String>,

    /// Where to read delivery numbers from. A bare slug like `vt11` is
    /// resolved to `<reports_dir>\<slug-lowercased>\` (newest file). A path
    /// like `C:\out\dn.csv` or `./out.xlsx` is used literally.
    #[arg(long)]
    pub delivery_file: Option<String>,

    /// Header column to read for delivery numbers. Default `Delivery`.
    /// Only used when --delivery-file points at a header CSV/XLSX.
    #[arg(long)]
    pub delivery_col: Option<String>,

    /// Where to read shipment numbers from (VL06O only). Same resolution as
    /// --delivery-file.
    #[arg(long)]
    pub shipment_file: Option<String>,

    /// Header column to read for shipment numbers. Default `Shipment Number`.
    #[arg(long)]
    pub shipment_col: Option<String>,

    /// ZMDESNR ONLY: send vkey 3 (back) after export, before layout selection
    /// (true|false). Overrides `[tcode.ZMDESNR].pre_export_back`.
    #[arg(long, value_name = "BOOL")]
    pub pre_export_back: Option<bool>,

    /// ZMDESNR ONLY: which results tab to select (e.g. 2, 7). Overrides
    /// `[tcode.ZMDESNR].tab_number`. When neither this flag nor config sets a
    /// value, the in-code default is 2. The tab is selected only AFTER the
    /// variant is applied because SAP GUI requires plant (WERKS) to be valid
    /// before tab switching works.
    #[arg(long, value_name = "N")]
    pub tab_number: Option<i32>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Run the loop configuration unattended
    #[command(name = "run-loop")]
    RunLoop {
        /// Skip SAP connection check (for testing)
        #[arg(long, default_value = "false")]
        skip_sap_check: bool,
        /// Keep the system awake during execution
        #[arg(long, default_value = "false")]
        keep_awake: bool,
    },

    /// Run the sequence configuration unattended
    #[command(name = "run-sequence")]
    RunSequence {
        /// Skip SAP connection check (for testing)
        #[arg(long, default_value = "false")]
        skip_sap_check: bool,
        /// Keep the system awake during execution
        #[arg(long, default_value = "false")]
        keep_awake: bool,
    },
}

impl Cli {
    pub fn parse() -> Self {
        <Cli as clap::Parser>::parse()
    }

    /// Project the parsed CLI into a [`CliOverrides`] suitable for the
    /// process-wide override slot. Surfaces parse errors for ISO dates so the
    /// user gets a clear message instead of silent fallback.
    pub fn to_overrides(&self) -> Result<CliOverrides> {
        let date_start = parse_iso_date_opt("--date-start", self.date_start.as_deref())?;
        let date_end = parse_iso_date_opt("--date-end", self.date_end.as_deref())?;

        Ok(CliOverrides {
            tcode: self.tcode.as_ref().map(|s| s.to_uppercase()),
            layout: self.layout.clone(),
            variant: self.variant.clone(),
            export_type: self.export_type,
            iterations: self.iterations,
            interval_seconds: self.interval_seconds,
            delay_seconds: self.delay_seconds,
            tcode_run_type: self.tcode_run_type.clone(),
            date_format: self.date_format.clone(),
            timezone: self.timezone.clone(),
            reports_dir: self.reports_dir.clone(),
            by_date: self.by_date,
            by_delivery: self.by_delivery,
            by_shipment: self.by_shipment,
            limiter: self.limiter.clone(),
            date_start,
            date_end,
            delivery_file: self.delivery_file.clone(),
            delivery_col: self.delivery_col.clone(),
            shipment_file: self.shipment_file.clone(),
            shipment_col: self.shipment_col.clone(),
            pre_export_back: self.pre_export_back,
            tab_number: self.tab_number,
        })
    }
}

fn parse_iso_date_opt(flag: &str, raw: Option<&str>) -> Result<Option<NaiveDate>> {
    match raw {
        Some(s) => NaiveDate::parse_from_str(s, "%Y-%m-%d")
            .map(Some)
            .map_err(|e| {
                anyhow!(
                    "Invalid {} value '{}': expected ISO YYYY-MM-DD ({})",
                    flag,
                    s,
                    e
                )
            }),
        None => Ok(None),
    }
}
