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
    /// process-wide override slot.
    pub fn to_overrides(&self) -> CliOverrides {
        CliOverrides {
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
        }
    }
}
