use anyhow::{anyhow, Result};
use chrono::{Datelike, NaiveDate};
use clap::{Parser, Subcommand};

use crate::utils::cli_overrides::{CliOverrides, OutputFormat};

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

    /// Feasibility test: open one new SAP session per tcode of `--tcodes` and start each
    /// tcode in its own session at the same time (no exports).
    #[arg(
        long = "test",
        hide = true,
        requires = "tcodes",
        conflicts_with_all = ["run_loop", "run_sequence", "rname", "tcode"]
    )]
    pub test: bool,

    /// Comma-separated tcodes for `--test`, e.g. `zvt11,vt11` (2 to 6).
    #[arg(long, hide = true, value_name = "T1,T2", requires = "test")]
    pub tcodes: Option<String>,

    /// PARALLEL SESSIONS for `--rname`: repeat `--rname` (`--rname xdock --rname k2xx`, 2 to 6)
    /// and each report runs its single-run flow for `--run-date` in its own NEW SAP session at
    /// the same time (sessions are created and closed by the tool; max 6 per connection,
    /// counting the ones already open; export files get a `-s<N>` session tag). Accepts bare /
    /// `true` / `1` / `yes`. Opt-in: several `--rname` without it is an error. The number of
    /// sessions follows the work: one per `--rname`, or with `--loop` one per day of the range
    /// (at most 6, fewer if some windows are already open); `--conn-count` sets it explicitly.
    /// Not combinable with `--no-sap`, `--skip-existing`, `--with-prereqs` or `--use-clipboard`
    /// (one shared Windows clipboard); several `--rname` can't be combined with `--loop`.
    #[arg(
        long = "multi-conn",
        value_name = "BOOL",
        num_args = 0..=1,
        default_missing_value = "true",
        value_parser = clap::builder::BoolishValueParser::new(),
        conflicts_with_all = ["no_sap", "skip_existing", "with_prereqs"]
    )]
    pub multi_conn: Option<bool>,

    /// PARALLEL DAYS for `--rname ... --loop`: run the days of the range or week on exactly N new
    /// SAP sessions at the same time (2 to 6; max 6 per connection, counting the ones already
    /// open). Implies parallel mode (`--multi-conn` with `--loop` picks N itself: one per day).
    /// The sessions stay open and take the next day when free (reused, not respawned); with
    /// `--merge` the merge runs once at the end. Same policy as the sequential loop: a day without
    /// data is skipped, any other error stops new days and nothing is merged. Opt-in; not
    /// combinable with `--no-sap` or `--use-clipboard` (shared clipboard).
    #[arg(
        long = "conn-count",
        value_name = "N",
        value_parser = clap::value_parser!(u8).range(2..=6),
        conflicts_with_all = ["no_sap"]
    )]
    pub conn_count: Option<u8>,

    /// Skip SAP connection check (for testing)
    #[arg(long, default_value = "false")]
    pub skip_sap_check: bool,

    /// Keep the system awake during execution
    #[arg(long, default_value = "false")]
    pub keep_awake: bool,

    /// Run a headless report by name (see `--run-date`), e.g. `--rname=mustgo`. Drives all
    /// SAP steps of the report unattended and writes TSV outputs under the reports dir.
    /// Cannot be combined with --run-loop / --run-sequence / per-tcode flags.
    /// Repeat it together with `--multi-conn` to run several reports in parallel sessions.
    #[arg(
        long,
        value_name = "NAME",
        action = clap::ArgAction::Append,
        requires = "run_date",
        conflicts_with_all = ["run_loop", "run_sequence"]
    )]
    pub rname: Vec<String>,

    /// Run date of --rname: `YYYY-MM-DD`, `YYYY-MM-DD..YYYY-MM-DD`, or a week `WK45` /
    /// `WK45/2026` (Sunday..Saturday, Excel WEEKNUM). A range or week is one run.
    #[arg(long, value_name = "DATE|A..B|WKnn", requires = "rname")]
    pub run_date: Option<String>,

    /// Per-day runs of a `--rname` report: every day of the `--run-date` range or week runs as
    /// its own single-day run (prerequisite reports first, e.g. repack: k2xx, relabel, repack).
    /// A day without data is skipped, any other error stops the iteration. Needs a range or
    /// week. Unrelated to `--run-loop` (the config-driven tcode loop).
    #[arg(
        long = "loop",
        requires = "rname",
        conflicts_with_all = ["run_loop", "run_sequence"]
    )]
    pub iterate_loop: bool,

    /// With `--loop`: after the per-day runs, merge the outputs into one `.xlsx` per report
    /// (all tables as sheets, a `run_date` column). Unrelated to `--run-loop` / `--iterations`.
    #[arg(long = "merge", requires = "iterate_loop")]
    pub merge: bool,

    /// STRICT OFFLINE for `--rname`: SAP is never touched (no session is created, SAP GUI may be
    /// closed). The final TSVs of the report and its prerequisites must already exist for every
    /// day (all output tables), else it is an error listing what is missing. With `--loop
    /// --merge` it re-merges an exported week; with `--loop` alone or a single date it only
    /// verifies the files. Use `--output-date` for the date folder. A single run checks only
    /// the report itself unless `--with-prereqs` is given. Not combinable with
    /// `--skip-existing` or `--output-format=xlsx`.
    #[arg(long = "no-sap", requires = "rname", conflicts_with = "skip_existing")]
    pub no_sap: bool,

    /// RESUME for `--rname`: a day+report whose output tables are ALL present is not run again
    /// ("exists, skipped"); SAP only runs for what is missing or incomplete (e.g. after a hard
    /// error stop). Skipped days still count for the summary and `--merge`. Works with `--loop`
    /// and for a single run (which considers only the report itself unless `--with-prereqs` is
    /// given). Not combinable with `--no-sap`.
    #[arg(long = "skip-existing", requires = "rname")]
    pub skip_existing: bool,

    /// SINGLE runs of `--rname`: also cover the report's prerequisite chain for that one date,
    /// in order (relabel: k2xx, relabel; repack: k2xx, relabel, repack), never anything
    /// downstream. No data in a chain member skips the rest of the chain. `--loop` already
    /// includes the prerequisites, so this flag has no extra effect there. With `--skip-existing`
    /// only the incomplete chain members run. With `--no-sap` EVERY chain member must already
    /// be on disk, else it is an error listing what is missing (nothing can be generated
    /// without SAP). Needs `--rname`; unrelated to `--run-loop` / `--run-sequence` /
    /// `--iterations`.
    #[arg(long = "with-prereqs", requires = "rname")]
    pub with_prereqs: bool,

    /// Date folder of the outputs (`<reports_dir>\<folder>\<DATE>\<rName>\...`) to read and
    /// write instead of today's, `YYYY-MM-DD`. For `--rname` runs, `--loop`, `--merge`,
    /// `--skip-existing` and `--no-sap` (relabel / repack re-read the k2xx / relabel TSVs of this
    /// folder too).
    #[arg(long, value_name = "YYYY-MM-DD", requires = "rname")]
    pub output_date: Option<String>,

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

    /// Override TCode run-type for 149 reports: `rcv` | `mat` | `tsp` | `none`.
    /// Optional; omit or `none` runs the regular plant-loop 149 flow.
    #[arg(long)]
    pub tcode_run_type: Option<String>,

    /// Comma-separated plant list for regular 149 (e.g. `PLT1,PLT2`).
    /// Overrides `[tcode.y_dn3_47000149].plants` in config.toml.
    #[arg(long, value_name = "LIST")]
    pub plants: Option<String>,

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

    /// VT11 ONLY: actual shipment end ("closed") date, field `K_DATEN`. `YYYY-MM-DD`,
    /// `YYYY-MM-DD..YYYY-MM-DD`, or a week `WK45` / `WK45/2026` (Sunday..Saturday, Excel
    /// WEEKNUM). Independent of --by-date; may be combined with --vt11-created-date.
    #[arg(long, value_name = "DATE|A..B|WKnn")]
    pub vt11_closed_date: Option<String>,

    /// VT11 ONLY: creation date, field `K_ERDAT`. Same formats as --vt11-closed-date.
    #[arg(long, value_name = "DATE|A..B|WKnn")]
    pub vt11_created_date: Option<String>,

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

    /// MM60 ONLY: LX03 export the material list is read from (`.txt` or `.xlsx`). A path is
    /// used literally (a directory = its newest file); a bare slug like `lx03` is
    /// `<reports_dir>\<slug>`. Default: newest file in the LX03 export dir (honours
    /// --rname-dir). Overrides `[tcode.MM60].lx03_file`.
    #[arg(long, value_name = "SLUG|PATH")]
    pub lx03_file: Option<String>,

    /// Where to read shipment numbers from (VL06O only). Same resolution as
    /// --delivery-file.
    #[arg(long)]
    pub shipment_file: Option<String>,

    /// Header column to read for shipment numbers. Default `Shipment Number`.
    #[arg(long)]
    pub shipment_col: Option<String>,

    /// VT11 ONLY: treat a "no display authorization" status-bar message as harmless
    /// (true|false). Overrides `[tcode.VT11].ignore_display_error`.
    #[arg(long, value_name = "BOOL", num_args = 0..=1, default_missing_value = "true")]
    pub ignore_display_error: Option<bool>,

    /// Subdirectory for exports. Relative paths (`./mustgo`, `mustgo`) are placed under
    /// the reports dir and files go to `{reports_dir}\<rname-dir>\{tcode}`. An absolute
    /// path is used as is: `<rname-dir>\{tcode}`.
    #[arg(long, value_name = "DIR")]
    pub rname_dir: Option<String>,

    /// ZMDESNR ONLY: send vkey 3 (back) after export, before layout selection
    /// (true|false). Overrides `[tcode.ZMDESNR].pre_export_back`.
    #[arg(long, value_name = "BOOL")]
    pub pre_export_back: Option<bool>,

    /// Fill multi-selection popups (delivery / shipment / TO / material numbers, ...) from the
    /// Windows clipboard (SAP "upload from clipboard", Shift+F12) instead of entering the values
    /// row by row with Page Down. Bare flag, `true`, `1`, `yes` or `on` enables it; default off.
    /// The paste is verified and falls back to the row-by-row entry if it did not take.
    #[arg(
        long,
        value_name = "BOOL",
        num_args = 0..=1,
        default_missing_value = "true",
        value_parser = clap::builder::BoolishValueParser::new()
    )]
    pub use_clipboard: Option<bool>,

    /// Output format of a headless `--rname` report run: `tsv` (default) writes the TSV
    /// files only; `xlsx` writes the TSV files (always the canonical output) AND one
    /// `<DATE>_<RNAME>.xlsx` workbook next to them with every output table as a sheet.
    #[arg(long, value_enum, value_name = "FORMAT")]
    pub output_format: Option<OutputFormat>,

    /// ZMDESNR ONLY: which results tab to select (e.g. 2, 7). Overrides
    /// `[tcode.ZMDESNR].tab_number`. When neither this flag nor config sets a
    /// value, the in-code default is 2. The tab is selected only AFTER the
    /// variant is applied because SAP GUI requires plant (WERKS) to be valid
    /// before tab switching works.
    #[arg(long, value_name = "N")]
    pub tab_number: Option<i32>,

    /// ZMDESNR ONLY: storage bin filter (e.g. `64EP*`). Sets LGPLA-LOW and
    /// skips delivery paste. Overrides `[tcode.ZMDESNR].bin_filter`. Quote the
    /// value in PowerShell so `*` is not expanded (`--bin-filter="64EP*"`).
    #[arg(long, value_name = "BIN")]
    pub bin_filter: Option<String>,

    /// LT03 ONLY (`--tcode lt03`): shipment numbers to create transfer orders for, comma separated.
    /// Per shipment: VL06O -> ZMDESNR by material -> LT03 per delivery. See also
    /// `--shipments-file` and `--shipments-clipboard`; without any of them the shipment_number
    /// column of the newest VT11 export is used.
    #[arg(long, value_name = "N1,N2", value_delimiter = ',', num_args = 1..)]
    pub shipments: Vec<String>,

    /// LT03 ONLY: file with the shipment numbers (`.txt` one per line, `.tsv`/`.xlsx` with a
    /// `shipment_number` column). `-` reads standard input, e.g.
    /// `Get-Clipboard | sap_automation --tcode lt03 --shipments-file -`.
    #[arg(long, value_name = "PATH|-")]
    pub shipments_file: Option<String>,

    /// LT03 ONLY: read the shipment numbers from the Windows clipboard once at start.
    #[arg(long)]
    pub shipments_clipboard: bool,

    /// LT03 ONLY: drive SAP up to the stock screen of every material and show what would be
    /// picked, but enter no quantity and create no transfer order.
    #[arg(long)]
    pub dry_run: bool,

    /// LT03 ONLY: bin pick strategy `fifo`, `lifo`, `load_balance` or `sequential` (default).
    /// Overrides `[tcode.LT03].bin_pick_strategy`.
    #[arg(long, value_name = "STRATEGY")]
    pub bin_pick_strategy: Option<String>,

    /// LT03 ONLY: maximum goods receipt age in days of a bin (fifo/lifo only; 0 = off).
    #[arg(long, value_name = "DAYS")]
    pub age_cutoff: Option<u32>,

    /// LT03 ONLY: allow bins of other ship-tos when the ship-to bins cannot cover the quantity.
    #[arg(
        long,
        value_name = "BOOL",
        num_args = 0..=1,
        default_missing_value = "true",
        value_parser = clap::builder::BoolishValueParser::new()
    )]
    pub allow_relabel: Option<bool>,

    /// LT03 ONLY: apply the blocked-stock (blacklist) rules from the stock DB.
    #[arg(
        long,
        value_name = "BOOL",
        num_args = 0..=1,
        default_missing_value = "true",
        value_parser = clap::builder::BoolishValueParser::new()
    )]
    pub skip_blacklist: Option<bool>,

    /// LT03 ONLY: `skip_if_any` (default: a material with blocked stock is skipped) or
    /// `allow_material_skip_bins` (only the blocked bins are skipped).
    #[arg(long, value_name = "POLICY")]
    pub blacklist_material_policy: Option<String>,

    /// LT03 ONLY: open the table of the last `--dry-run` again (scrollable viewer, then the plain
    /// table). Reads `dryrun_picks.tsv` of the newest run; does not touch SAP.
    #[arg(long = "show-last")]
    pub show_last: bool,

    /// LT03 ONLY: resume: deliveries with a green result today are not run again.
    #[arg(long = "lt03-resume")]
    pub lt03_resume: bool,

    /// Days added before the start and after the end of the run date range of every tcode that
    /// sets one (VT11, ZMDETPC). When omitted: 3 for ZMDETPC on a week run (`WK<n>`), else 0.
    /// Overrides `[abc.<report>] date_range_buffer`.
    #[arg(long, value_name = "DAYS", value_parser = clap::value_parser!(u32).range(0..=366))]
    pub date_range_buffer: Option<u32>,
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
        let by_date = self.by_date;
        let vt11_closed_date = parse_run_date_opt("--vt11-closed-date", &self.vt11_closed_date)?;
        let vt11_created_date = parse_run_date_opt("--vt11-created-date", &self.vt11_created_date)?;

        let output_date = parse_iso_date_opt("--output-date", self.output_date.as_deref())?;

        // Several --rname only with --multi-conn; the process-wide override keeps the first
        // report (main reads the whole list from `Cli::rname`).
        if self.rname.len() > 1 && self.multi_conn != Some(true) {
            return Err(anyhow!(
                "--rname given {} times: add --multi-conn to run them in parallel sessions",
                self.rname.len()
            ));
        }
        if self.rname.len() > 1 && self.iterate_loop {
            return Err(anyhow!(
                "several --rname can't be combined with --loop (parallel days run one report: \
                 use --multi-conn or --conn-count with a single --rname)"
            ));
        }
        let (rname, run_date) = parse_rname_opts(
            self.rname.first().map(String::as_str),
            self.run_date.as_deref(),
        )?;

        let lt03 = crate::utils::cli_overrides::Lt03Cli {
            shipments: self
                .shipments
                .iter()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect(),
            shipments_file: self.shipments_file.clone(),
            shipments_clipboard: self.shipments_clipboard,
            dry_run: self.dry_run,
            bin_pick_strategy: self.bin_pick_strategy.clone(),
            age_cutoff: self.age_cutoff,
            allow_relabel: self.allow_relabel,
            skip_blacklist: self.skip_blacklist,
            blacklist_material_policy: self.blacklist_material_policy.clone(),
            resume: self.lt03_resume,
            show_last: self.show_last,
        };
        let is_lt03 = self
            .tcode
            .as_deref()
            .is_some_and(|t| t.eq_ignore_ascii_case("lt03"));
        let lt03_flags = lt03.flag_names();
        if !lt03_flags.is_empty() && !is_lt03 {
            return Err(anyhow!(
                "{} only work with --tcode lt03",
                lt03_flags.join(", ")
            ));
        }
        if let Some(s) = &lt03.bin_pick_strategy {
            crate::lt03_plan::Strategy::parse(s)?;
        }
        if (self.multi_conn == Some(true) || self.conn_count.is_some())
            && self.rname.is_empty()
            && !is_lt03
        {
            return Err(anyhow!(
                "--multi-conn / --conn-count need --rname (reports) or --tcode lt03 (shipments)"
            ));
        }
        if self.conn_count.is_some() && !self.iterate_loop && !is_lt03 {
            return Err(anyhow!(
                "--conn-count needs --loop (days of a range) or --tcode lt03 (shipments)"
            ));
        }

        Ok(CliOverrides {
            lt03,
            rname,
            run_date,
            tcode: self.tcode.as_ref().map(|s| s.to_uppercase()),
            layout: self.layout.clone(),
            variant: self.variant.clone(),
            export_type: self.export_type,
            iterations: self.iterations,
            interval_seconds: self.interval_seconds,
            delay_seconds: self.delay_seconds,
            tcode_run_type: self.tcode_run_type.clone(),
            plants: parse_plants_opt(self.plants.as_deref()),
            date_format: self.date_format.clone(),
            timezone: self.timezone.clone(),
            reports_dir: self.reports_dir.clone(),
            by_date,
            by_delivery: self.by_delivery,
            by_shipment: self.by_shipment,
            limiter: self.limiter.clone(),
            date_start,
            date_end,
            delivery_file: self.delivery_file.clone(),
            delivery_col: self.delivery_col.clone(),
            shipment_file: self.shipment_file.clone(),
            shipment_col: self.shipment_col.clone(),
            lx03_file: self.lx03_file.clone(),
            ignore_display_error: self.ignore_display_error,
            vt11_closed_date,
            vt11_created_date,
            rname_dir: self.rname_dir.clone(),
            pre_export_back: self.pre_export_back,
            tab_number: self.tab_number,
            bin_filter: self.bin_filter.clone(),
            date_range_buffer: self.date_range_buffer,
            use_clipboard: self.use_clipboard,
            output_format: self.output_format,
            iterate_loop: self.iterate_loop,
            merge: self.merge,
            no_sap: self.no_sap,
            skip_existing: self.skip_existing,
            with_prereqs: self.with_prereqs,
            output_date,
        })
    }
}

impl Cli {
    /// Cross-flag rules of `--rname` that need the parsed overrides (the pairing with
    /// `--run-date` and the conflict with `--run-loop`/`--run-sequence` are enforced by clap).
    pub fn validate_rname(&self, o: &CliOverrides) -> Result<()> {
        if o.rname.is_none() {
            return Ok(());
        }
        if self.command.is_some() {
            return Err(anyhow!(
                "--rname can't be combined with the run-loop / run-sequence subcommands"
            ));
        }
        let bad = o.per_tcode_flag_names();
        if !bad.is_empty() {
            return Err(anyhow!(
                "{} can't be used with --rname; the report flow sets its own variants/layouts \
                 (override them in the [abc.<report>] section of config.toml)",
                bad.join(", ")
            ));
        }
        if o.rname_dir.is_some() {
            return Err(anyhow!(
                "--rname-dir can't be used with --rname (outputs go under the reports dir)"
            ));
        }
        if o.no_sap && o.output_format == Some(OutputFormat::Xlsx) {
            return Err(anyhow!(
                "--no-sap with --output-format=xlsx needs SAP-free regeneration of the per-day                  workbooks, which is not supported yet (--merge always writes its merged xlsx)"
            ));
        }
        if o.iterate_loop {
            if let Some(d) = &o.run_date {
                if d.days().len() < 2 {
                    return Err(anyhow!(
                        "--loop needs a range or week (--run-date=A..B or WKnn), got a single date"
                    ));
                }
            }
        }
        Ok(())
    }
}

/// Validate `--rname` against the registry and parse `--run-date` (default year = this year).
fn parse_rname_opts(
    rname: Option<&str>,
    run_date: Option<&str>,
) -> Result<(Option<String>, Option<crate::abc::run::RunDate>)> {
    match (rname, run_date) {
        (None, None) => Ok((None, None)),
        (Some(_), None) => Err(anyhow!("--rname requires --run-date")),
        (None, Some(_)) => Err(anyhow!("--run-date requires --rname")),
        (Some(n), Some(d)) => {
            let name = crate::abc::dispatch::check_report(n)
                .map_err(|e| anyhow!("Invalid --rname value: {e}"))?;
            let rd = crate::abc::run::parse_run_date(d, chrono::Local::now().year())
                .map_err(|e| anyhow!("Invalid --run-date value '{d}': {e}"))?;
            Ok((Some(name.to_string()), Some(rd)))
        }
    }
}

/// Parse `DATE | A..B | WKnn[/year]` into an inclusive `(start, end)` range.
fn parse_run_date_opt(flag: &str, raw: &Option<String>) -> Result<Option<(NaiveDate, NaiveDate)>> {
    match raw {
        Some(s) => {
            let year = chrono::Local::now().year();
            crate::abc::run::parse_run_date(s, year)
                .map(|rd| Some(rd.as_range()))
                .map_err(|e| anyhow!("Invalid {flag} value: {e}"))
        }
        None => Ok(None),
    }
}

fn parse_iso_date_opt(flag: &str, raw: Option<&str>) -> Result<Option<NaiveDate>> {
    match raw {
        Some(s) => NaiveDate::parse_from_str(s, "%Y-%m-%d")
            .map(Some)
            .map_err(|e| anyhow!("Invalid {flag} value '{s}': expected ISO YYYY-MM-DD ({e})")),
        None => Ok(None),
    }
}

/// Parse `--plants=a,b,c` into a trimmed non-empty list. `None` when the flag
/// was not passed; `Some(vec![])` when passed but empty after trimming.
fn parse_plants_opt(raw: Option<&str>) -> Option<Vec<String>> {
    raw.map(|s| {
        s.split(',')
            .map(|p| p.trim().to_string())
            .filter(|p| !p.is_empty())
            .collect()
    })
}

#[cfg(test)]
mod rname_tests {
    use super::*;

    fn parse(args: &[&str]) -> std::result::Result<Cli, clap::Error> {
        let mut v = vec!["sap_automation"];
        v.extend_from_slice(args);
        <Cli as Parser>::try_parse_from(v)
    }

    fn ov(args: &[&str]) -> Result<CliOverrides> {
        let cli = parse(args).map_err(|e| anyhow!("{e}"))?;
        let o = cli.to_overrides()?;
        cli.validate_rname(&o)?;
        Ok(o)
    }

    #[test]
    fn multi_conn_is_opt_in() {
        // nothing changes without the new flags
        let cli = parse(&["--rname=mustgo", "--run-date=2026-11-05"]).unwrap();
        assert!(cli.multi_conn.is_none() && !cli.test && cli.tcodes.is_none());
        assert_eq!(cli.rname, vec!["mustgo"]);
        // several --rname need --multi-conn
        let args = [
            "--rname",
            "xdock",
            "--rname",
            "k2xx",
            "--run-date=2026-11-05",
        ];
        assert!(ov(&args).is_err());
        let mut with = args.to_vec();
        with.push("--multi-conn");
        let cli = parse(&with).unwrap();
        assert_eq!(cli.multi_conn, Some(true));
        let o = cli.to_overrides().unwrap();
        assert_eq!(o.rname.as_deref(), Some("xdock"));
        for v in ["true", "1", "yes"] {
            let flag = format!("--multi-conn={v}");
            let mut a = args.to_vec();
            a.push(&flag);
            assert_eq!(parse(&a).unwrap().multi_conn, Some(true));
        }
        // conflicts
        for bad in ["--no-sap", "--skip-existing", "--with-prereqs"] {
            let mut a = with.clone();
            a.push(bad);
            assert!(parse(&a).is_err(), "{bad}");
        }
        // --multi-conn alone is rejected
        assert!(ov(&["--multi-conn"]).is_err());
        // with --loop it parallelises the days of ONE report, several --rname + --loop is an error
        let one = ["--rname=k2xx", "--run-date=WK45", "--loop", "--multi-conn"];
        assert!(ov(&one).is_ok());
        let mut a = one.to_vec();
        a.extend(["--conn-count=3"]);
        assert_eq!(parse(&a).unwrap().conn_count, Some(3));
        let two = [
            "--rname",
            "xdock",
            "--rname",
            "k2xx",
            "--run-date=WK45",
            "--loop",
            "--multi-conn",
        ];
        assert!(ov(&two).is_err());
    }

    #[test]
    fn conn_count_is_opt_in_and_validated() {
        let base = ["--rname=k2xx", "--run-date=WK45", "--loop"];
        let mut a = base.to_vec();
        a.push("--conn-count=3");
        assert_eq!(parse(&a).unwrap().conn_count, Some(3));
        assert!(parse(&base).unwrap().conn_count.is_none());
        for bad in ["--conn-count=1", "--conn-count=7", "--conn-count=x"] {
            let mut a = base.to_vec();
            a.push(bad);
            assert!(parse(&a).is_err(), "{bad}");
        }
        // needs --loop, conflicts with --no-sap
        assert!(ov(&["--rname=k2xx", "--run-date=WK45", "--conn-count=3"]).is_err());
        let mut a = base.to_vec();
        a.extend(["--conn-count=3", "--no-sap"]);
        assert!(parse(&a).is_err());
    }

    #[test]
    fn lt03_flags_are_opt_in_and_tied_to_the_tcode() {
        let o = ov(&[
            "--tcode=lt03",
            "--shipments=111,222",
            "--shipments",
            "333",
            "--dry-run",
            "--bin-pick-strategy=FIFO",
            "--age-cutoff=30",
            "--allow-relabel",
            "--skip-blacklist=false",
            "--blacklist-material-policy=allow_material_skip_bins",
            "--lt03-resume",
        ])
        .unwrap();
        assert_eq!(o.lt03.shipments, ["111", "222", "333"]);
        assert!(o.lt03.dry_run && o.lt03.resume);
        assert_eq!(o.lt03.age_cutoff, Some(30));
        assert_eq!(o.lt03.allow_relabel, Some(true));
        assert_eq!(o.lt03.skip_blacklist, Some(false));
        // without the flags nothing is set
        assert!(ov(&["--tcode=lt03"]).unwrap().lt03.flag_names().is_empty());
        // the flags are LT03 only
        assert!(ov(&["--tcode=vt11", "--dry-run"]).is_err());
        assert!(ov(&["--shipments=1"]).is_err());
        // unknown strategy
        assert!(ov(&["--tcode=lt03", "--bin-pick-strategy=random"]).is_err());
    }

    #[test]
    fn lt03_parallel_sessions() {
        let o = ov(&["--tcode=lt03", "--shipments=1,2,3", "--conn-count=2"]).unwrap();
        assert_eq!(o.lt03.shipments.len(), 3);
        assert!(ov(&["--tcode=lt03", "--shipments=1,2", "--multi-conn"]).is_ok());
        assert!(parse(&["--tcode=lt03", "--conn-count=7"]).is_err());
        // other tcodes still can't run in parallel
        assert!(ov(&["--tcode=vt11", "--conn-count=2"]).is_err());
        assert!(ov(&["--tcode=vt11", "--multi-conn"]).is_err());
    }

    #[test]
    fn valid_combinations() {
        let o = ov(&["--rname=mustgo", "--run-date=2026-11-05"]).unwrap();
        assert_eq!(o.rname.as_deref(), Some("mustgo"));
        assert_eq!(o.run_date.unwrap().as_range().0.to_string(), "2026-11-05");
        let o = ov(&["--rname=MustGo", "--run-date=WK45/2026"]).unwrap();
        assert_eq!(o.rname.as_deref(), Some("mustgo"));
        assert_eq!(
            o.run_date.unwrap().as_range(),
            (
                NaiveDate::from_ymd_opt(2026, 11, 1).unwrap(),
                NaiveDate::from_ymd_opt(2026, 11, 7).unwrap()
            )
        );
        assert!(ov(&["--rname=mustgo", "--run-date=2026-11-01..2026-11-07"]).is_ok());
        // global flags are allowed
        assert!(ov(&[
            "--rname=mustgo",
            "--run-date=2026-11-05",
            "--reports-dir=C:/r",
            "--date-format=yyyy-mm-dd",
            "--keep-awake",
            "--skip-sap-check"
        ])
        .is_ok());
        // WK without year uses the current year
        assert!(ov(&["--rname=mustgo", "--run-date=WK45"]).is_ok());
    }

    #[test]
    fn pairing_is_required() {
        assert!(parse(&["--rname=mustgo"]).is_err());
        assert!(parse(&["--run-date=2026-11-05"]).is_err());
    }

    #[test]
    fn invalid_values() {
        let e = ov(&["--rname=nope", "--run-date=2026-11-05"])
            .unwrap_err()
            .to_string();
        assert!(e.contains("unknown report") && e.contains("mustgo"), "{e}");
        let e = ov(&["--rname=mustgo", "--run-date=banana"])
            .unwrap_err()
            .to_string();
        assert!(e.contains("--run-date"), "{e}");
        assert!(ov(&["--rname=mustgo", "--run-date=2026-11-07..2026-11-01"]).is_err());
        assert!(ov(&["--rname=mustgo", "--run-date=WK99/2026"]).is_err());
    }

    #[test]
    fn conflicts() {
        assert!(parse(&["--rname=mustgo", "--run-date=2026-11-05", "--run-loop"]).is_err());
        assert!(parse(&["--rname=mustgo", "--run-date=2026-11-05", "--run-sequence"]).is_err());
        let e = ov(&["--rname=mustgo", "--run-date=2026-11-05", "--tcode=vt11"])
            .unwrap_err()
            .to_string();
        assert!(e.contains("--tcode") && e.contains("--rname"), "{e}");
        assert!(ov(&["--rname=mustgo", "--run-date=2026-11-05", "--layout=x"]).is_err());
        assert!(ov(&["--rname=mustgo", "--run-date=2026-11-05", "--rname-dir=x"]).is_err());
        let cli = parse(&["--rname=mustgo", "--run-date=2026-11-05"]).unwrap();
        let o = cli.to_overrides().unwrap();
        assert!(cli.validate_rname(&o).is_ok());
    }

    #[test]
    fn loop_and_merge_flags() {
        let o = ov(&["--rname=xdock", "--run-date=WK45/2026", "--loop"]).unwrap();
        assert!(o.iterate_loop && !o.merge);
        let o = ov(&["--rname=xdock", "--run-date=WK45/2026", "--loop", "--merge"]).unwrap();
        assert!(o.iterate_loop && o.merge);
        let o = ov(&[
            "--rname=repack",
            "--run-date=2026-11-01..2026-11-03",
            "--loop",
        ])
        .unwrap();
        assert!(o.iterate_loop);
        // without the flags nothing is set
        let o = ov(&["--rname=xdock", "--run-date=WK45/2026"]).unwrap();
        assert!(!o.iterate_loop && !o.merge);
    }

    #[test]
    fn loop_and_merge_rules() {
        // --merge needs --loop; --loop needs --rname; no collision with --run-loop
        assert!(parse(&["--rname=xdock", "--run-date=WK45", "--merge"]).is_err());
        assert!(parse(&["--merge"]).is_err());
        assert!(parse(&["--loop"]).is_err());
        assert!(parse(&["--rname=xdock", "--run-date=WK45", "--loop", "--run-loop"]).is_err());
        assert!(parse(&["--loop", "--run-loop"]).is_err());
        assert!(parse(&["--run-loop"]).unwrap().run_loop);
        let cli = parse(&["--run-loop"]).unwrap();
        assert!(!cli.iterate_loop && !cli.merge);
        // a single date cannot be iterated
        let e = ov(&["--rname=xdock", "--run-date=2026-11-05", "--loop"])
            .unwrap_err()
            .to_string();
        assert!(e.contains("--loop needs a range or week"), "{e}");
        // a one-day range is still a single date
        assert!(ov(&[
            "--rname=xdock",
            "--run-date=2026-11-05..2026-11-05",
            "--loop"
        ])
        .is_err());
    }

    #[test]
    fn offline_resume_and_output_date_flags() {
        let o = ov(&[
            "--rname=xdock",
            "--run-date=WK45/2026",
            "--loop",
            "--merge",
            "--no-sap",
            "--output-date=2026-11-12",
        ])
        .unwrap();
        assert!(o.no_sap && !o.skip_existing && o.merge);
        assert_eq!(
            o.output_date,
            Some(NaiveDate::from_ymd_opt(2026, 11, 12).unwrap())
        );
        let line = o.summary_line().unwrap();
        assert!(line.contains("--no-sap") && line.contains("--output-date=2026-11-12"));
        let o = ov(&["--rname=xdock", "--run-date=WK45/2026", "--skip-existing"]).unwrap();
        assert!(o.skip_existing && !o.no_sap && o.output_date.is_none());
        // single run
        assert!(ov(&["--rname=xdock", "--run-date=2026-11-05", "--no-sap"]).is_ok());
    }

    #[test]
    fn offline_resume_and_output_date_rules() {
        fn with<'a>(extra: &[&'a str]) -> Vec<&'a str> {
            let mut v = vec!["--rname=xdock", "--run-date=WK45"];
            v.extend_from_slice(extra);
            v
        }
        // conflict
        assert!(parse(&with(&["--no-sap", "--skip-existing"])).is_err());
        // bad date formats
        for bad in [
            "--output-date=12.11.2026",
            "--output-date=2026-13-01",
            "--output-date=",
        ] {
            assert!(ov(&with(&[bad])).is_err(), "{bad}");
        }
        let e = ov(&with(&["--output-date=nope"])).unwrap_err().to_string();
        assert!(e.contains("--output-date"), "{e}");
        // all three need --rname
        assert!(parse(&["--no-sap"]).is_err());
        assert!(parse(&["--skip-existing"]).is_err());
        assert!(parse(&["--output-date=2026-11-12"]).is_err());
        assert!(parse(&["--tcode=vt11", "--no-sap"]).is_err());
        // xlsx cannot be regenerated offline
        let e = ov(&with(&["--no-sap", "--output-format=xlsx"]))
            .unwrap_err()
            .to_string();
        assert!(e.contains("--no-sap") && e.contains("not supported"), "{e}");
        // existing flags unaffected
        let o = ov(&with(&["--loop", "--merge", "--output-format=xlsx"])).unwrap();
        assert!(!o.no_sap && !o.skip_existing && o.output_date.is_none());
        assert!(!parse(&["--run-loop"]).unwrap().no_sap);
    }

    #[test]
    fn with_prereqs_rules() {
        fn with<'a>(extra: &[&'a str]) -> Vec<&'a str> {
            let mut v = vec!["--rname=repack", "--run-date=2026-11-05"];
            v.extend_from_slice(extra);
            v
        }
        // needs --rname
        assert!(parse(&["--with-prereqs"]).is_err());
        assert!(parse(&["--tcode=vt11", "--with-prereqs"]).is_err());
        // conflicts with the config-driven loops (through --rname)
        assert!(parse(&["--run-loop", "--with-prereqs"]).is_err());
        assert!(parse(&["--run-sequence", "--with-prereqs"]).is_err());
        // combines with everything of the family
        let o = ov(&with(&["--with-prereqs"])).unwrap();
        assert!(o.with_prereqs);
        assert!(o.summary_line().unwrap().contains("--with-prereqs"));
        assert!(ov(&with(&["--with-prereqs", "--no-sap"])).is_ok());
        assert!(ov(&with(&["--with-prereqs", "--skip-existing"])).is_ok());
        assert!(ov(&with(&["--with-prereqs", "--output-date=2026-11-12"])).is_ok());
        assert!(ov(&with(&["--with-prereqs", "--output-format=xlsx"])).is_ok());
        let o = ov(&[
            "--rname=repack",
            "--run-date=WK45",
            "--loop",
            "--merge",
            "--with-prereqs",
        ])
        .unwrap();
        assert!(o.with_prereqs && o.iterate_loop && o.merge);
        // default off
        assert!(!ov(&with(&[])).unwrap().with_prereqs);
        // the clap error names the flag
        let e = parse(&["--run-loop", "--with-prereqs"])
            .err()
            .unwrap()
            .to_string();
        assert!(e.contains("--with-prereqs"), "{e}");
    }

    #[test]
    fn help_text_documents_with_prereqs() {
        use clap::CommandFactory;
        let help = Cli::command().render_long_help().to_string();
        // collapse the wrapping so phrases can span lines
        let flat = help.split_whitespace().collect::<Vec<_>>().join(" ");
        for phrase in [
            "--with-prereqs",
            "prerequisite chain",
            "never anything downstream",
            "`--loop` already includes the prerequisites",
            "has no extra effect there",
            "only the incomplete chain members run",
            "EVERY chain member must already be on disk",
            "nothing can be generated without SAP",
            "Needs `--rname`",
            "unrelated to `--run-loop` / `--run-sequence` / `--iterations`",
        ] {
            assert!(flat.contains(phrase), "help lacks: {phrase}");
        }
        // cross references in the help of --no-sap and --skip-existing
        assert!(
            flat.matches("unless `--with-prereqs` is given").count() >= 2,
            "--no-sap and --skip-existing must point to --with-prereqs"
        );
    }

    #[test]
    fn without_rname_nothing_changes() {
        let o = ov(&["--tcode=vt11"]).unwrap();
        assert!(o.rname.is_none() && o.run_date.is_none());
        let line = ov(&["--rname=mustgo", "--run-date=WK45/2026"])
            .unwrap()
            .summary_line()
            .unwrap();
        assert!(
            line.contains("--rname=mustgo") && line.contains("--run-date=WK_45"),
            "{line}"
        );
    }
}
