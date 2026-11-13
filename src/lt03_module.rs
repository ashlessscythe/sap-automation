//! `--tcode lt03`: the `manual_to` flow, headless.
//!
//! Per shipment (or per group of shipments on a single session): VL06O -> deliveries, ZMDESNR by
//! material -> the stock DB, then LT03 per delivery ([`crate::lt03_runner`]). Settings come from
//! `[abc.manual_to]` / `[global]` and the CLI (CLI wins). Never prompts; the outcome is written to
//! files (log, result, summary json) and the return value tells the caller whether everything
//! worked (non-zero exit otherwise). `--dry-run` is the exception: it prints a table of the picks
//! that would be made.
#![allow(dead_code)]

use crate::abc::adapters::{export_target, finish_step, vl06o_params, MustgoSapConfig};
use crate::abc::dispatch::{b as pb, int as pi, s as ps, ResolveInput};
use crate::abc::runner::StepOutput;
use crate::abc::table::Table;
use crate::lt03::RealLt03Screen;
use crate::lt03_db::{
    deliveries_from_grid, parse_shipments, read_shipments_file, DbColumns, Delivery,
    ShipToStrategy, StockDb,
};
use crate::lt03_plan::{BlacklistPolicy, Strategy};
use crate::lt03_runner::{
    run_delivery, DeliveryResult, DeliveryStatus, Lt03Config, Lt03Screen, MaterialInfo,
    MaterialOutcome, OpenOutcome, RawRow, RunLog,
};
use crate::utils::cli_overrides::Lt03Cli;
use anyhow::{anyhow, bail, Context, Result};
use chrono::NaiveDate;
use sap_scripting::*;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Everything a run needs, resolved from CLI > `[abc.manual_to]` > `[global]` > defaults.
#[derive(Debug, Clone)]
pub struct ManualToSettings {
    /// Export dir, raw parsing options and warehouse (shared with the ABC steps).
    pub sap: MustgoSapConfig,
    pub reports_dir: PathBuf,
    pub vl06o_variant: Option<String>,
    pub vl06o_layout: Option<String>,
    pub zmdesnr_variant: Option<String>,
    pub zmdesnr_layout: Option<String>,
    pub lt: Lt03Config,
    pub shipto_strategy: ShipToStrategy,
    pub db_cols: DbColumns,
    pub resume: bool,
    /// One VL06O per shipment (every delivery then knows its shipment, but SAP sees one shipment
    /// at a time) instead of one VL06O with all shipments. The default is one run with all of
    /// them; the shipment then comes from a shipment column of the VL06O layout, when it has one.
    pub vl06o_per_shipment: bool,
}

/// Neutral placeholders (site names live in the git-ignored `config.toml`).
pub const DEFAULT_VL06O_VARIANT: &str = "MANUAL_TO_VL06O_VARIANT";
pub const DEFAULT_VL06O_LAYOUT: &str = "MANUAL_TO_VL06O_LAYOUT";
pub const DEFAULT_ZMDESNR_VARIANT: &str = "MANUAL_TO_ZMDESNR_VARIANT";
pub const DEFAULT_ZMDESNR_LAYOUT: &str = "MANUAL_TO_ZMDESNR_LAYOUT";

/// Defaults when neither the CLI nor `[abc.manual_to]` says otherwise (the owner's macro settings):
/// fifo, bins up to 30 days old, relabel allowed (other ship-tos only when the ship-to's bins are
/// short), blocked bins skipped (the material itself is kept), never any popup.
pub const DEFAULT_BIN_PICK_STRATEGY: &str = "fifo";
pub const DEFAULT_AGE_CUTOFF_DAYS: u32 = 30;

/// One line with the effective settings, shown at the start of every run (console and log).
pub fn describe_settings(st: &ManualToSettings) -> String {
    format!(
        "bin_pick_strategy={}, age_cutoff={}, allow_relabel={}, skip_blacklist={}, \
         blacklist_material_policy={}, shipto_strategy={}, proceed_without_gr={}, warehouse={}, \
         sap_date_format={}, dry_run={}, resume={}",
        st.lt.strategy.name(),
        st.lt.age_cutoff_days,
        st.lt.allow_relabel,
        st.lt.skip_blacklist,
        st.lt.blacklist_policy.name(),
        match st.shipto_strategy {
            ShipToStrategy::First => "first",
            ShipToStrategy::Majority => "majority",
        },
        st.lt.proceed_without_gr,
        st.lt.warehouse,
        st.lt.sap_date_format,
        st.lt.dry_run,
        st.resume
    )
}

/// Print the settings and make them the first line of the run log.
pub fn announce_settings(st: &ManualToSettings, log: &mut RunLog) {
    let d = describe_settings(st);
    println!("Settings: {d}");
    log.info("Run Settings", d);
}

pub fn resolve_manual_to(
    input: &ResolveInput,
    cli: &Lt03Cli,
    today: NaiveDate,
) -> Result<ManualToSettings> {
    let base = crate::abc::dispatch::resolve_mustgo(input);
    let sec = &input.section;
    let mut lt = Lt03Config::new(&base.cfg.warehouse, today);

    let strategy = cli
        .bin_pick_strategy
        .clone()
        .or_else(|| ps(sec, "bin_pick_strategy"))
        .unwrap_or_else(|| DEFAULT_BIN_PICK_STRATEGY.to_string());
    lt.strategy = Strategy::parse(&strategy)?;
    lt.age_cutoff_days = cli
        .age_cutoff
        .or_else(|| pi(sec, "age_cutoff").and_then(|v| u32::try_from(v.max(0)).ok()))
        .unwrap_or(DEFAULT_AGE_CUTOFF_DAYS);
    lt.allow_relabel = cli
        .allow_relabel
        .or_else(|| pb(sec, "allow_relabel"))
        .unwrap_or(true);
    lt.skip_blacklist = cli
        .skip_blacklist
        .or_else(|| pb(sec, "skip_blacklist"))
        .unwrap_or(true);
    lt.blacklist_policy = BlacklistPolicy::parse(
        &cli.blacklist_material_policy
            .clone()
            .or_else(|| ps(sec, "blacklist_material_policy"))
            .unwrap_or_else(|| BlacklistPolicy::AllowMaterialSkipBins.name().to_string()),
    );
    lt.proceed_without_gr = pb(sec, "proceed_without_gr").unwrap_or(true);
    lt.sap_date_format = ps(sec, "sap_date_format")
        .or_else(|| input.global_date_format.clone())
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| "yyyy-mm-dd".to_string());
    lt.dry_run = cli.dry_run;

    let mut cols = DbColumns::default();
    for (key, slot) in [
        ("material_col", &mut cols.material),
        ("bin_col", &mut cols.bin),
        ("qty_col", &mut cols.qty),
        ("shipto_col", &mut cols.shipto),
        ("blocked_col", &mut cols.blocked),
    ] {
        if let Some(v) = ps(sec, key) {
            *slot = crate::abc::table::sanitize_header(&v);
        }
    }

    Ok(ManualToSettings {
        reports_dir: base.reports_dir,
        sap: base.cfg,
        vl06o_variant: Some(
            ps(sec, "vl06o_variant").unwrap_or_else(|| DEFAULT_VL06O_VARIANT.to_string()),
        ),
        vl06o_layout: Some(
            ps(sec, "vl06o_layout").unwrap_or_else(|| DEFAULT_VL06O_LAYOUT.to_string()),
        ),
        zmdesnr_variant: Some(
            ps(sec, "zmdesnr_variant").unwrap_or_else(|| DEFAULT_ZMDESNR_VARIANT.to_string()),
        ),
        zmdesnr_layout: Some(
            ps(sec, "zmdesnr_layout").unwrap_or_else(|| DEFAULT_ZMDESNR_LAYOUT.to_string()),
        ),
        lt,
        shipto_strategy: ShipToStrategy::parse(&ps(sec, "shipto_strategy").unwrap_or_default()),
        db_cols: cols,
        resume: cli.resume,
        vl06o_per_shipment: pb(sec, "vl06o_per_shipment").unwrap_or(false),
    })
}

// ---------------------------------------------------------------------------------------------
// shipment input

/// Where the shipment numbers come from. `stdin` / `clipboard` are injected so tests (and callers)
/// control them.
pub struct ShipmentSources<'a> {
    pub stdin: &'a dyn Fn() -> Result<String>,
    pub clipboard: &'a dyn Fn() -> Result<String>,
    /// Newest VT11 export directory (fallback source).
    pub vt11_dir: PathBuf,
}

/// Shipments of `--shipments`, `--shipments-file`, `--shipments-clipboard` (all that are given, in
/// that order, duplicates dropped); with none of them the `shipment_number` column of the newest
/// VT11 export.
pub fn collect_shipments(cli: &Lt03Cli, src: &ShipmentSources) -> Result<Vec<String>> {
    let mut all: Vec<String> = Vec::new();
    let explicit =
        !cli.shipments.is_empty() || cli.shipments_file.is_some() || cli.shipments_clipboard;
    if !cli.shipments.is_empty() {
        all.extend(parse_shipments(&cli.shipments.join(",")));
    }
    if let Some(f) = &cli.shipments_file {
        if f.trim() == "-" {
            all.extend(parse_shipments(&(src.stdin)()?));
        } else {
            all.extend(read_shipments_file(Path::new(f.trim()), None)?);
        }
    }
    if cli.shipments_clipboard {
        all.extend(parse_shipments(&(src.clipboard)()?));
    }
    if !explicit {
        let file = crate::mm60::newest_lx03_file(&src.vt11_dir).ok_or_else(|| {
            anyhow!(
                "no shipments given (--shipments / --shipments-file / --shipments-clipboard) and \
                 no VT11 export found in {}",
                src.vt11_dir.display()
            )
        })?;
        println!("Shipments from the newest VT11 export: {}", file.display());
        all.extend(read_shipments_file(&file, None)?);
    }
    let mut seen = HashSet::new();
    all.retain(|s| seen.insert(s.clone()));
    if all.is_empty() {
        bail!("no shipment numbers found: nothing to do");
    }
    Ok(all)
}

/// One piece of work for a session.
#[derive(Debug, Clone, PartialEq)]
pub struct Unit {
    pub tag: String,
    pub shipments: Vec<String>,
}

/// One unit with all shipments (one session: one VL06O and one ZMDESNR for everything, like the
/// macro), or one unit per shipment (parallel sessions take them from a queue).
pub fn plan_units(shipments: &[String], per_shipment: bool) -> Vec<Unit> {
    if per_shipment {
        shipments
            .iter()
            .map(|s| Unit {
                tag: s.clone(),
                shipments: vec![s.clone()],
            })
            .collect()
    } else {
        vec![Unit {
            tag: "all".to_string(),
            shipments: shipments.to_vec(),
        }]
    }
}

// ---------------------------------------------------------------------------------------------
// the SAP seam of one unit

/// What a unit needs from SAP: the LT03 screen plus the two exports in front of it.
pub trait ManualToSap: Lt03Screen {
    fn vl06o(&mut self, shipments: &[String]) -> Result<StepOutput>;
    fn zmdesnr_by_materials(&mut self, materials: &[String]) -> Result<StepOutput>;
}

/// The real thing over one SAP session.
pub struct RealManualTo<'a> {
    screen: RealLt03Screen<'a>,
    session: &'a GuiSession,
    settings: &'a ManualToSettings,
}

impl<'a> RealManualTo<'a> {
    pub fn new(session: &'a GuiSession, settings: &'a ManualToSettings) -> Self {
        Self {
            screen: RealLt03Screen::new(session),
            session,
            settings,
        }
    }
}

macro_rules! delegate_screen {
    () => {
        fn open_delivery(&mut self, w: &str, d: &str) -> Result<OpenOutcome> {
            self.screen.open_delivery(w, d)
        }
        fn current_material(&mut self) -> Result<Option<MaterialInfo>> {
            self.screen.current_material()
        }
        fn read_stock(&mut self) -> Result<Vec<RawRow>> {
            self.screen.read_stock()
        }
        fn enter_pick(&mut self, abs_row: usize, bin: &str, qty: f64) -> Result<bool> {
            self.screen.enter_pick(abs_row, bin, qty)
        }
        fn open_qty(&mut self) -> Result<f64> {
            self.screen.open_qty()
        }
        fn next_material(&mut self) -> Result<()> {
            self.screen.next_material()
        }
        fn skip_material(&mut self) -> Result<()> {
            self.screen.skip_material()
        }
        fn at_overview(&mut self) -> Result<bool> {
            self.screen.at_overview()
        }
        fn finish(&mut self, save: bool) -> Result<String> {
            self.screen.finish(save)
        }
    };
}

impl Lt03Screen for RealManualTo<'_> {
    delegate_screen!();
}

impl ManualToSap for RealManualTo<'_> {
    fn vl06o(&mut self, shipments: &[String]) -> Result<StepOutput> {
        let st = self.settings;
        let now = chrono::Local::now().naive_local();
        let target = export_target(&st.sap, "VL06O", now);
        let mut params = vl06o_params(&st.sap, shipments, target, now.date());
        params.sap_variant_name = st.vl06o_variant.clone();
        params.layout_row = st.vl06o_layout.clone();
        let outcome = crate::vl06o::run_export_outcome(self.session, &params)
            .map_err(|e| anyhow!("vl06o: SAP step failed: {e}"))?;
        finish_step(&st.sap, "vl06o", outcome)
    }

    fn zmdesnr_by_materials(&mut self, materials: &[String]) -> Result<StepOutput> {
        let st = self.settings;
        let now = chrono::Local::now().naive_local();
        let params = crate::zmdesnr::ZMDESNRParams {
            materials: materials.to_vec(),
            sap_variant_name: st.zmdesnr_variant.clone(),
            layout_row: st.zmdesnr_layout.clone(),
            tab_number: Some(2),
            export_target: Some(export_target(&st.sap, "ZMDESNR", now)),
            ignore_display_error: st.sap.ignore_display_error,
            ..crate::zmdesnr::ZMDESNRParams::default()
        };
        let outcome = crate::zmdesnr::run_export_outcome(self.session, &params)
            .map_err(|e| anyhow!("zmdesnr (by material): SAP step failed: {e}"))?;
        finish_step(&st.sap, "zmdesnr", outcome)
    }
}

// ---------------------------------------------------------------------------------------------
// running a unit

#[derive(Debug, Clone, PartialEq)]
pub enum UnitStatus {
    Done,
    /// VL06O / ZMDESNR returned nothing: the unit is skipped, not an error.
    NoData(String),
    /// A step failed: the run is a failure and no new units start.
    Failed(String),
}

#[derive(Debug, Clone)]
pub struct UnitReport {
    pub tag: String,
    pub shipments: Vec<String>,
    pub status: UnitStatus,
    pub deliveries: Vec<DeliveryResult>,
}

/// Sort key that puts `10` after `3` (numbers by value, anything else after them).
fn numeric_key(s: &str) -> (u8, u128) {
    match s.trim().parse::<u128>() {
        Ok(n) => (0, n),
        Err(_) => (1, 0),
    }
}

fn table_grid(t: &Table) -> Vec<Vec<String>> {
    let mut g = vec![t.headers.clone()];
    g.extend(t.rows.iter().cloned());
    g
}

/// Run one unit: exports, stock DB, then every delivery. `skip` = deliveries to leave out
/// (resume). Errors become [`UnitStatus::Failed`]; nothing here panics or prompts.
pub fn run_unit<S: ManualToSap>(
    sap: &mut S,
    st: &ManualToSettings,
    unit: &Unit,
    skip: &HashSet<String>,
    log: &mut RunLog,
) -> UnitReport {
    let mut rep = UnitReport {
        tag: unit.tag.clone(),
        shipments: unit.shipments.clone(),
        status: UnitStatus::Done,
        deliveries: Vec::new(),
    };
    if let Err(e) = run_unit_inner(sap, st, unit, skip, log, &mut rep) {
        rep.status = match e.downcast::<NoData>() {
            Ok(nd) => UnitStatus::NoData(nd.0),
            Err(e) => UnitStatus::Failed(format!("{e:#}")),
        };
    }
    rep
}

#[derive(Debug)]
struct NoData(String);

impl std::fmt::Display for NoData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for NoData {}

/// Keep what SAP returned next to the results (the export files themselves are deleted after they
/// loaded): `<dir>\<prefix><unit>_<name>.tsv`. Best effort, never fails the run.
fn save_table(st: &ManualToSettings, unit_tag: &str, name: &str, t: &Table) {
    let dir = output_dir(&st.reports_dir, st.lt.today);
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let prefix = file_prefix(st.lt.dry_run);
    let _ = std::fs::write(
        dir.join(format!("{prefix}{unit_tag}_{name}.tsv")),
        t.to_tsv(),
    );
}

fn run_unit_inner<S: ManualToSap>(
    sap: &mut S,
    st: &ManualToSettings,
    unit: &Unit,
    skip: &HashSet<String>,
    log: &mut RunLog,
    rep: &mut UnitReport,
) -> Result<()> {
    // one VL06O with all shipments (default), or one per shipment (`vl06o_per_shipment`)
    let groups: Vec<Vec<String>> = if unit.shipments.len() > 1 && st.vl06o_per_shipment {
        unit.shipments.iter().map(|s| vec![s.clone()]).collect()
    } else {
        vec![unit.shipments.clone()]
    };
    let mut deliveries: Vec<Delivery> = Vec::new();
    let mut no_data: Vec<String> = Vec::new();
    for g in &groups {
        let vl = match sap.vl06o(g)? {
            StepOutput::Table(t) => t,
            StepOutput::NoData(m) => {
                no_data.push(format!("shipment {}: {m}", g.join(",")));
                continue;
            }
        };
        let name = if groups.len() > 1 {
            format!("vl06o_{}", g[0])
        } else {
            "vl06o".to_string()
        };
        save_table(st, &unit.tag, &name, &vl);
        let found =
            deliveries_from_grid(&table_grid(&vl), "delivery", &st.db_cols.shipto, "material")
                .with_context(|| {
                    format!("VL06O export (columns found: {})", vl.headers.join(", "))
                })?;
        for mut d in found {
            // a shipment column of the layout wins; else the shipment of a one-shipment run
            if d.shipment.is_none() && g.len() == 1 {
                d.shipment = g.first().cloned();
            }
            if !deliveries
                .iter()
                .any(|x| x.number.eq_ignore_ascii_case(&d.number))
            {
                deliveries.push(d);
            }
        }
    }
    if deliveries.is_empty() {
        let why = if no_data.is_empty() {
            "VL06O returned no deliveries".to_string()
        } else {
            format!("VL06O: {}", no_data.join("; "))
        };
        return Err(NoData(why).into());
    }
    if unit.shipments.len() > 1 && deliveries.iter().any(|d| d.shipment.is_none()) {
        println!(
            "[{}] note: the VL06O layout has no shipment column, so the deliveries cannot be tied to              their shipment (the shipment column stays blank). Add 'Shipment Number' to the layout              {:?} or set vl06o_per_shipment = true in [abc.manual_to].",
            unit.tag,
            st.vl06o_layout.as_deref().unwrap_or("")
        );
    }
    // shipments in the order given, deliveries in numerical order within a shipment
    let ship_pos = |d: &Delivery| {
        d.shipment
            .as_ref()
            .and_then(|s| unit.shipments.iter().position(|x| x == s))
            .unwrap_or(usize::MAX)
    };
    deliveries.sort_by(|a, b| {
        (ship_pos(a), numeric_key(&a.number), &a.number).cmp(&(
            ship_pos(b),
            numeric_key(&b.number),
            &b.number,
        ))
    });
    let mut materials: Vec<String> = Vec::new();
    for d in &deliveries {
        for m in &d.materials {
            if !materials.iter().any(|x| x.eq_ignore_ascii_case(m)) {
                materials.push(m.clone());
            }
        }
    }
    if materials.is_empty() {
        bail!("VL06O export has no material column or no materials");
    }
    let sn = match sap.zmdesnr_by_materials(&materials)? {
        StepOutput::Table(t) => t,
        StepOutput::NoData(m) => return Err(NoData(format!("ZMDESNR: {m}")).into()),
    };
    save_table(st, &unit.tag, "to_sn", &sn);
    let db = StockDb::from_grid(&table_grid(&sn), &st.db_cols)
        .with_context(|| format!("ZMDESNR export (columns found: {})", sn.headers.join(", ")))?;
    println!(
        "[{}] {} delivery(ies), {} material(s), stock DB {} row(s), {} blocked bin(s)",
        unit.tag,
        deliveries.len(),
        materials.len(),
        db.rows,
        db.blacklist_len()
    );

    for d in &deliveries {
        if skip.contains(&d.number) {
            println!(
                "[{}] delivery {} already done, skipped (resume)",
                unit.tag, d.number
            );
            continue;
        }
        let (ship_to, note) = d.ship_to(st.shipto_strategy);
        if let Some(n) = note {
            println!("[{}] delivery {}: {n}", unit.tag, d.number);
        }
        let mut r = run_delivery(sap, &st.lt, &db, &d.number, ship_to.as_deref(), log);
        r.shipment = d.shipment.clone();
        println!(
            "[{}] delivery {}: {}",
            unit.tag,
            r.delivery,
            status_text(&r.status)
        );
        rep.deliveries.push(r);
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// results

pub fn status_key(s: &DeliveryStatus) -> &'static str {
    match s {
        DeliveryStatus::Created(_) => "created",
        DeliveryStatus::Planned => "planned",
        DeliveryStatus::NoItems(_) => "no_items",
        DeliveryStatus::InProcess(_) => "in_process",
        DeliveryStatus::NothingPicked => "nothing_picked",
        DeliveryStatus::Failed(_) => "failed",
    }
}

pub fn status_text(s: &DeliveryStatus) -> String {
    match s {
        DeliveryStatus::Created(t) => format!("created ({t})"),
        DeliveryStatus::Planned => "planned (dry run)".to_string(),
        DeliveryStatus::NoItems(t) | DeliveryStatus::InProcess(t) | DeliveryStatus::Failed(t) => {
            format!("{}: {t}", status_key(s))
        }
        DeliveryStatus::NothingPicked => "nothing picked".to_string(),
    }
}

fn status_detail(s: &DeliveryStatus) -> String {
    match s {
        DeliveryStatus::Created(t)
        | DeliveryStatus::NoItems(t)
        | DeliveryStatus::InProcess(t)
        | DeliveryStatus::Failed(t) => t.clone(),
        _ => String::new(),
    }
}

/// A delivery that needs attention (exit code): anything but created / planned / no items.
fn is_failure(s: &DeliveryStatus) -> bool {
    !s.is_ok() && !matches!(s, DeliveryStatus::NoItems(_))
}

#[derive(Debug, Clone)]
pub struct RunReport {
    pub units: Vec<UnitReport>,
    pub dry_run: bool,
    pub today: NaiveDate,
}

impl RunReport {
    pub fn deliveries(&self) -> impl Iterator<Item = (&UnitReport, &DeliveryResult)> {
        self.units
            .iter()
            .flat_map(|u| u.deliveries.iter().map(move |d| (u, d)))
    }

    /// Units that failed plus deliveries that need attention.
    pub fn failures(&self) -> usize {
        self.units
            .iter()
            .filter(|u| matches!(u.status, UnitStatus::Failed(_)))
            .count()
            + self
                .deliveries()
                .filter(|(_, d)| is_failure(&d.status))
                .count()
    }

    pub fn ok(&self) -> bool {
        self.failures() == 0
    }

    pub fn result_tsv(&self) -> String {
        let mut s = String::from(
            "unit\tshipment\tdelivery\tship_to\tstatus\tdetail\tmaterials_picked\tmaterials_skipped\n",
        );
        for (u, d) in self.deliveries() {
            s.push_str(&format!(
                "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
                u.tag,
                d.shipment.clone().unwrap_or_default(),
                d.delivery,
                d.ship_to.clone().unwrap_or_default(),
                status_key(&d.status),
                status_detail(&d.status).replace(['\t', '\r', '\n'], " "),
                d.picked_materials(),
                d.materials.len() - d.picked_materials()
            ));
        }
        s
    }

    /// Rows of the dry-run table: one per planned pick.
    pub fn pick_rows(&self) -> Vec<Vec<String>> {
        let mut rows = Vec::new();
        for (_, d) in self.deliveries() {
            for m in &d.materials {
                match &m.outcome {
                    MaterialOutcome::Picked(picks) => {
                        for p in picks {
                            rows.push(vec![
                                d.shipment.clone().unwrap_or_default(),
                                d.delivery.clone(),
                                m.material.clone(),
                                fmt_qty(m.open_qty),
                                p.bin.clone(),
                                fmt_qty(p.qty),
                                if p.relabel() { "yes" } else { "" }.to_string(),
                                if p.repack { "yes" } else { "" }.to_string(),
                                p.gr_date.map(|g| g.to_string()).unwrap_or_default(),
                                p.reason.clone(),
                            ]);
                        }
                    }
                    MaterialOutcome::Skipped(why) => rows.push(vec![
                        d.shipment.clone().unwrap_or_default(),
                        d.delivery.clone(),
                        m.material.clone(),
                        fmt_qty(m.open_qty),
                        "-".into(),
                        "-".into(),
                        String::new(),
                        String::new(),
                        String::new(),
                        format!("SKIPPED: {why}"),
                    ]),
                }
            }
        }
        rows
    }

    pub const PICK_HEADERS: [&'static str; 10] = [
        "shipment", "delivery", "material", "open", "bin", "qty", "relabel", "repack", "gr_date",
        "reason",
    ];

    pub fn picks_tsv(&self) -> String {
        let mut s = Self::PICK_HEADERS.join("\t");
        s.push('\n');
        for r in self.pick_rows() {
            s.push_str(&r.join("\t"));
            s.push('\n');
        }
        s
    }

    /// Fixed-width table for the console.
    pub fn picks_table(&self) -> String {
        let (headers, rows) = Self::display_table(&self.pick_rows());
        Self::format_table(&headers, &rows)
    }

    /// Every shipment of the run, in the order given (no duplicates).
    pub fn shipments(&self) -> Vec<String> {
        let mut seen = HashSet::new();
        self.units
            .iter()
            .flat_map(|u| u.shipments.iter().cloned())
            .filter(|s| seen.insert(s.clone()))
            .collect()
    }

    /// The table as shown on screen: without the shipment column (the files keep it).
    pub fn display_table(rows: &[Vec<String>]) -> (Vec<&'static str>, Vec<Vec<String>>) {
        (
            Self::PICK_HEADERS[1..].to_vec(),
            rows.iter().map(|r| r[1..].to_vec()).collect(),
        )
    }

    /// For display: the shipment number is shown on the first row of its group only.
    pub fn blank_repeats(rows: &[Vec<String>]) -> Vec<Vec<String>> {
        let mut prev = String::new();
        rows.iter()
            .map(|r| {
                let mut r = r.clone();
                if !r.is_empty() {
                    if r[0] == prev {
                        r[0] = String::new();
                    } else {
                        prev = r[0].clone();
                    }
                }
                r
            })
            .collect()
    }

    /// Fixed-width text table for any headers + rows.
    pub fn format_table(headers: &[&str], rows: &[Vec<String>]) -> String {
        let mut w: Vec<usize> = headers.iter().map(|h| h.len()).collect();
        for r in rows {
            for (i, c) in r.iter().enumerate() {
                w[i] = w[i].max(c.chars().count());
            }
        }
        let line = |cells: Vec<&str>| {
            cells
                .iter()
                .enumerate()
                .map(|(i, c)| format!("{c:<width$}", width = w[i]))
                .collect::<Vec<_>>()
                .join("  ")
                .trim_end()
                .to_string()
        };
        let mut out = vec![line(headers.to_vec())];
        out.push(
            w.iter()
                .map(|n| "-".repeat(*n))
                .collect::<Vec<_>>()
                .join("  "),
        );
        for r in rows {
            out.push(line(r.iter().map(String::as_str).collect()));
        }
        out.join("\n")
    }

    pub fn summary_json(&self) -> serde_json::Value {
        use serde_json::json;
        json!({
            "tcode": "LT03",
            "date": self.today.to_string(),
            "dry_run": self.dry_run,
            "ok": self.ok(),
            "failures": self.failures(),
            "units": self.units.iter().map(|u| json!({
                "unit": u.tag,
                "shipments": u.shipments,
                "status": match &u.status {
                    UnitStatus::Done => "done",
                    UnitStatus::NoData(_) => "no_data",
                    UnitStatus::Failed(_) => "failed",
                },
                "message": match &u.status {
                    UnitStatus::Done => String::new(),
                    UnitStatus::NoData(m) | UnitStatus::Failed(m) => m.clone(),
                },
                "deliveries": u.deliveries.iter().map(|d| json!({
                    "delivery": d.delivery,
                    "shipment": d.shipment,
                    "ship_to": d.ship_to,
                    "status": status_key(&d.status),
                    "detail": status_detail(&d.status),
                    "materials": d.materials.iter().map(|m| json!({
                        "material": m.material,
                        "open_qty": m.open_qty,
                        "skipped": match &m.outcome {
                            MaterialOutcome::Skipped(w) => Some(w.clone()),
                            _ => None,
                        },
                        "picks": match &m.outcome {
                            MaterialOutcome::Picked(p) => p.iter().map(|p| json!({
                                "bin": p.bin, "qty": p.qty, "reason": p.reason,
                                "relabel": p.relabel(), "repack": p.repack,
                            })).collect::<Vec<_>>(),
                            _ => Vec::new(),
                        },
                    })).collect::<Vec<_>>(),
                })).collect::<Vec<_>>(),
            })).collect::<Vec<_>>(),
        })
    }
}

fn fmt_qty(q: f64) -> String {
    if q.fract() == 0.0 {
        format!("{}", q as i64)
    } else {
        q.to_string()
    }
}

/// Folder of a run's outputs: `<reports_dir>\MANUAL_TO\<yyyy-mm-dd>`.
pub fn output_dir(reports_dir: &Path, today: NaiveDate) -> PathBuf {
    reports_dir
        .join("MANUAL_TO")
        .join(today.format("%Y-%m-%d").to_string())
}

fn file_prefix(dry_run: bool) -> &'static str {
    if dry_run {
        "dryrun_"
    } else {
        ""
    }
}

/// Deliveries already created today (resume).
pub fn read_done_deliveries(dir: &Path) -> HashSet<String> {
    let text = std::fs::read_to_string(dir.join("result.tsv")).unwrap_or_default();
    text.lines()
        .skip(1)
        .filter_map(|l| {
            let c: Vec<&str> = l.split('\t').collect();
            (c.get(4) == Some(&"created")).then(|| c[2].to_string())
        })
        .collect()
}

/// Write log, result, picks (dry run) and summary json; returns the files.
pub fn write_outputs(dir: &Path, rep: &RunReport, log: &RunLog) -> Result<Vec<PathBuf>> {
    std::fs::create_dir_all(dir).with_context(|| format!("cannot create {}", dir.display()))?;
    let pre = file_prefix(rep.dry_run);
    let mut files = Vec::new();
    let mut put = |name: String, body: String| -> Result<()> {
        let p = dir.join(name);
        std::fs::write(&p, body).with_context(|| format!("cannot write {}", p.display()))?;
        files.push(p);
        Ok(())
    };
    put(format!("{pre}log.tsv"), log.to_tsv())?;
    put(format!("{pre}result.tsv"), rep.result_tsv())?;
    if rep.dry_run {
        put("dryrun_picks.tsv".to_string(), rep.picks_tsv())?;
    }
    put(
        format!("{pre}summary.json"),
        serde_json::to_string_pretty(&rep.summary_json())?,
    )?;
    Ok(files)
}

/// Interactive viewer (sticky header, scrolling rows, fixed status line) for the dry-run table;
/// only with a terminal on both ends. The plain table is printed afterwards either way, so it
/// stays in the scrollback.
fn show_dry_run_table(rep: &RunReport) {
    view_rows(
        &rep.pick_rows(),
        rep.deliveries().count(),
        &rep.shipments(),
        "LT03 dry run - planned picks",
    );
}

/// `Shipments (3): a, b, c` for the top of the table (empty without shipments).
pub fn shipments_line(shipments: &[String]) -> String {
    if shipments.is_empty() {
        String::new()
    } else {
        format!(
            "Shipments in this run ({}): {}",
            shipments.len(),
            shipments.join(", ")
        )
    }
}

/// The viewer over rows in the [`RunReport::PICK_HEADERS`] layout.
fn view_rows(rows: &[Vec<String>], deliveries: usize, shipments: &[String], title: &str) {
    use ratatui::style::{Color, Style};
    use std::io::IsTerminal;
    if !(std::io::stdin().is_terminal() && std::io::stdout().is_terminal()) {
        return;
    }
    let count = |col: usize| rows.iter().filter(|r| r[col] == "yes").count();
    let skipped = rows.iter().filter(|r| r[9].starts_with("SKIPPED")).count();
    let summary = format!(
        "{deliveries} delivery(ies), {} pick(s): {} relabel, {} repack, {} skipped material(s)",
        rows.len() - skipped,
        count(6),
        count(7),
        skipped
    );
    // the screen table has no shipment column: relabel / repack / reason are one column earlier
    let style = |r: &[String]| -> Style {
        if r[8].starts_with("SKIPPED") {
            Style::default().fg(Color::Red)
        } else if r[5] == "yes" {
            Style::default().fg(Color::Yellow)
        } else if r[6] == "yes" {
            Style::default().fg(Color::Magenta)
        } else {
            Style::default()
        }
    };
    let mut title = format!("{title} (yellow = relabel, magenta = repack, red = skipped)");
    let ships = shipments_line(shipments);
    if !ships.is_empty() {
        title.push('\n');
        title.push_str(&ships);
    }
    let (headers, shown) = RunReport::display_table(rows);
    if let Err(e) = crate::tui::show_table_view(&title, &headers, &shown, &summary, &style) {
        println!("(table viewer unavailable: {e})");
    }
}

/// The newest `dryrun_picks.tsv` under `<reports_dir>\MANUAL_TO\<date>\` (latest date folder).
pub fn find_last_picks(reports_dir: &Path) -> Option<PathBuf> {
    let root = reports_dir.join("MANUAL_TO");
    let mut days: Vec<PathBuf> = std::fs::read_dir(&root)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.join("dryrun_picks.tsv").is_file())
        .collect();
    days.sort();
    days.pop().map(|d| d.join("dryrun_picks.tsv"))
}

/// Rows of a `dryrun_picks.tsv` (header skipped, short rows padded).
pub fn parse_picks_tsv(text: &str) -> Vec<Vec<String>> {
    let n = RunReport::PICK_HEADERS.len();
    text.lines()
        .skip(1)
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let mut c: Vec<String> = l.split('\t').map(str::to_string).collect();
            c.resize(n, String::new());
            c
        })
        .collect()
}

/// Shipments of a run from its `summary.json` (empty when it is missing or unreadable).
pub fn shipments_of_summary(path: &Path) -> Vec<String> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else {
        return Vec::new();
    };
    let mut seen = HashSet::new();
    v["units"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|u| u["shipments"].as_array().into_iter().flatten())
        .filter_map(|s| s.as_str().map(str::to_string))
        .filter(|s| seen.insert(s.clone()))
        .collect()
}

/// `--tcode lt03 --show-last`: open the table of the last dry run again, no SAP.
pub fn show_last_table() -> Result<()> {
    let st = settings_from_environment()?;
    let file = find_last_picks(&st.reports_dir).ok_or_else(|| {
        anyhow!(
            "no dry run found under {} (run LT03 with --dry-run first)",
            st.reports_dir.join("MANUAL_TO").display()
        )
    })?;
    let text = std::fs::read_to_string(&file)
        .with_context(|| format!("cannot read {}", file.display()))?;
    let rows = parse_picks_tsv(&text);
    let deliveries = rows
        .iter()
        .map(|r| (r[0].clone(), r[1].clone()))
        .collect::<HashSet<_>>()
        .len();
    println!("Last dry run: {}", file.display());
    let shipments = shipments_of_summary(&file.with_file_name("dryrun_summary.json"));
    view_rows(&rows, deliveries, &shipments, "LT03 dry run (last)");
    let (headers, shown) = RunReport::display_table(&rows);
    let ships = shipments_line(&shipments);
    if !ships.is_empty() {
        println!("\n{ships}\n");
    }
    println!("{}", RunReport::format_table(&headers, &shown));
    Ok(())
}

/// Print the summary, write the files, and turn failures into an error (non-zero exit).
pub fn finish_run(st: &ManualToSettings, rep: &RunReport, log: &RunLog) -> Result<()> {
    if rep.dry_run {
        show_dry_run_table(rep);
        println!("\nDRY RUN - nothing was entered or created. Planned picks:\n");
        let ships = shipments_line(&rep.shipments());
        if !ships.is_empty() {
            println!("{ships}\n");
        }
        println!("{}", rep.picks_table());
    }
    let dir = output_dir(&st.reports_dir, rep.today);
    for f in write_outputs(&dir, rep, log)? {
        println!("wrote {}", f.display());
    }
    for u in &rep.units {
        match &u.status {
            UnitStatus::NoData(m) => println!("[{}] skipped, no data: {m}", u.tag),
            UnitStatus::Failed(m) => println!("[{}] FAILED: {m}", u.tag),
            UnitStatus::Done => {}
        }
    }
    let n = rep.deliveries().count();
    let ok = rep.deliveries().filter(|(_, d)| d.status.is_ok()).count();
    println!(
        "LT03 {}: {ok} of {n} delivery(ies) ok, {} failure(s)",
        if rep.dry_run { "dry run" } else { "run" },
        rep.failures()
    );
    if rep.ok() {
        Ok(())
    } else {
        Err(anyhow!("LT03: {} failure(s), see the log", rep.failures()))
    }
}

/// Settings from the environment (CLI overrides, `config.toml`).
pub fn settings_from_environment() -> Result<ManualToSettings> {
    resolve_manual_to(
        &crate::abc::dispatch::environment_input("manual_to"),
        crate::utils::cli_overrides::lt03(),
        crate::utils::cli_overrides::effective_today(),
    )
}

/// Shipments from the environment: CLI flags, stdin, clipboard, newest VT11 export.
pub fn shipments_from_environment(st: &ManualToSettings) -> Result<Vec<String>> {
    let stdin = || -> Result<String> {
        use std::io::Read;
        let mut s = String::new();
        std::io::stdin().read_to_string(&mut s)?;
        Ok(s)
    };
    let clip = || -> Result<String> {
        clipboard_win::get_clipboard_string().map_err(|e| anyhow!("cannot read the clipboard: {e}"))
    };
    collect_shipments(
        crate::utils::cli_overrides::lt03(),
        &ShipmentSources {
            stdin: &stdin,
            clipboard: &clip,
            vt11_dir: PathBuf::from(crate::abc::adapters::export_dir_for(&st.sap, "VT11")),
        },
    )
}

/// `--tcode lt03` on the session in hand (one connection).
pub fn run_lt03_auto(session: &GuiSession) -> Result<()> {
    println!("LT03 (manual_to) - unattended");
    println!("=============================");
    let st = settings_from_environment()?;
    let shipments = shipments_from_environment(&st)?;
    println!("{} shipment(s): {}", shipments.len(), shipments.join(", "));
    let dir = output_dir(&st.reports_dir, st.lt.today);
    let skip = if st.resume && !st.lt.dry_run {
        read_done_deliveries(&dir)
    } else {
        HashSet::new()
    };
    let mut log = RunLog::default();
    announce_settings(&st, &mut log);
    let mut sap = RealManualTo::new(session, &st);
    let units = plan_units(&shipments, false);
    let reports: Vec<UnitReport> = units
        .iter()
        .map(|u| run_unit(&mut sap, &st, u, &skip, &mut log))
        .collect();
    let rep = RunReport {
        units: reports,
        dry_run: st.lt.dry_run,
        today: st.lt.today,
    };
    finish_run(&st, &rep, &log)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lt03_runner::fakes::{mat, Fake};

    struct FakeSap {
        screen: Fake,
        vl: std::collections::VecDeque<StepOutput>,
        sn: Option<StepOutput>,
        vl_calls: Vec<Vec<String>>,
    }

    impl Lt03Screen for FakeSap {
        fn open_delivery(&mut self, w: &str, d: &str) -> Result<OpenOutcome> {
            self.screen.open_delivery(w, d)
        }
        fn current_material(&mut self) -> Result<Option<MaterialInfo>> {
            self.screen.current_material()
        }
        fn read_stock(&mut self) -> Result<Vec<RawRow>> {
            self.screen.read_stock()
        }
        fn enter_pick(&mut self, a: usize, b: &str, q: f64) -> Result<bool> {
            self.screen.enter_pick(a, b, q)
        }
        fn open_qty(&mut self) -> Result<f64> {
            self.screen.open_qty()
        }
        fn next_material(&mut self) -> Result<()> {
            self.screen.next_material()
        }
        fn skip_material(&mut self) -> Result<()> {
            self.screen.skip_material()
        }
        fn at_overview(&mut self) -> Result<bool> {
            self.screen.at_overview()
        }
        fn finish(&mut self, save: bool) -> Result<String> {
            self.screen.finish(save)
        }
    }

    impl ManualToSap for FakeSap {
        fn vl06o(&mut self, s: &[String]) -> Result<StepOutput> {
            self.vl_calls.push(s.to_vec());
            Ok(self.vl.pop_front().unwrap())
        }
        fn zmdesnr_by_materials(&mut self, _m: &[String]) -> Result<StepOutput> {
            Ok(self.sn.take().unwrap())
        }
    }

    fn table(name: &str, head: &[&str], rows: &[&[&str]]) -> Table {
        Table::new(
            name,
            head.iter().map(|s| s.to_string()).collect(),
            rows.iter()
                .map(|r| r.iter().map(|c| c.to_string()).collect())
                .collect(),
        )
    }

    fn settings(dry: bool) -> ManualToSettings {
        let cli = Lt03Cli {
            dry_run: dry,
            // the fake stock rows carry no GR date: no age cutoff
            age_cutoff: Some(0),
            skip_blacklist: Some(true),
            ..Lt03Cli::default()
        };
        resolve_manual_to(
            &ResolveInput::default(),
            &cli,
            NaiveDate::from_ymd_opt(2026, 11, 13).unwrap(),
        )
        .unwrap()
    }

    fn fake(vl: StepOutput, sn: StepOutput) -> FakeSap {
        FakeSap {
            screen: Fake::new(vec![mat("M1", 100.0, &[(0, "B1", 100.0, "")])]),
            vl: [vl].into(),
            sn: Some(sn),
            vl_calls: Vec::new(),
        }
    }

    fn vl() -> StepOutput {
        StepOutput::Table(table(
            "vl06o",
            &["delivery", "shipto_party", "material"],
            &[&["D1", "S1", "M1"]],
        ))
    }

    fn sn() -> StepOutput {
        StepOutput::Table(table(
            "zmdesnr",
            &[
                "material",
                "storage_bin",
                "quantity",
                "shipto_party",
                "blocked",
            ],
            &[&["M1", "B1", "100", "S1", ""]],
        ))
    }

    #[test]
    fn unit_runs_the_chain_and_reports() {
        let st = settings(false);
        let mut sap = fake(vl(), sn());
        let mut log = RunLog::default();
        let unit = Unit {
            tag: "s1".into(),
            shipments: vec!["111".into()],
        };
        let rep = run_unit(&mut sap, &st, &unit, &HashSet::new(), &mut log);
        assert_eq!(rep.status, UnitStatus::Done);
        assert_eq!(rep.deliveries.len(), 1);
        assert!(rep.deliveries[0].status.is_ok());
        assert_eq!(sap.vl_calls, vec![vec!["111".to_string()]]);
        assert_eq!(sap.screen.entered.len(), 1);
        let run = RunReport {
            units: vec![rep],
            dry_run: false,
            today: st.lt.today,
        };
        assert!(run.ok());
        assert!(run.result_tsv().contains("D1\tS1\tcreated"));
        assert_eq!(
            run.summary_json()["units"][0]["deliveries"][0]["status"],
            "created"
        );
    }

    #[test]
    fn no_data_is_a_skip_and_errors_are_failures() {
        let st = settings(false);
        let unit = Unit {
            tag: "s1".into(),
            shipments: vec!["1".into()],
        };
        let mut sap = fake(StepOutput::NoData("none".into()), sn());
        let rep = run_unit(
            &mut sap,
            &st,
            &unit,
            &HashSet::new(),
            &mut RunLog::default(),
        );
        assert!(matches!(rep.status, UnitStatus::NoData(_)));
        let run = RunReport {
            units: vec![rep],
            dry_run: false,
            today: st.lt.today,
        };
        assert!(run.ok(), "no data is not a failure");
        // ZMDESNR without the stock columns
        let bad = StepOutput::Table(table("z", &["foo"], &[&["1"]]));
        let mut sap = fake(vl(), bad);
        let rep = run_unit(
            &mut sap,
            &st,
            &unit,
            &HashSet::new(),
            &mut RunLog::default(),
        );
        assert!(matches!(rep.status, UnitStatus::Failed(_)));
        let run = RunReport {
            units: vec![rep],
            dry_run: false,
            today: st.lt.today,
        };
        assert!(!run.ok());
    }

    #[test]
    fn resume_skips_deliveries() {
        let st = settings(false);
        let mut sap = fake(vl(), sn());
        let unit = Unit {
            tag: "s1".into(),
            shipments: vec!["1".into()],
        };
        let skip: HashSet<String> = ["D1".to_string()].into();
        let rep = run_unit(&mut sap, &st, &unit, &skip, &mut RunLog::default());
        assert!(rep.deliveries.is_empty());
        assert!(sap.screen.entered.is_empty());
    }

    #[test]
    fn dry_run_table_lists_the_planned_picks() {
        let st = settings(true);
        let mut sap = fake(vl(), sn());
        let unit = Unit {
            tag: "s1".into(),
            shipments: vec!["1".into()],
        };
        let rep = run_unit(
            &mut sap,
            &st,
            &unit,
            &HashSet::new(),
            &mut RunLog::default(),
        );
        let run = RunReport {
            units: vec![rep],
            dry_run: true,
            today: st.lt.today,
        };
        let t = run.picks_table();
        assert!(t.lines().next().unwrap().starts_with("delivery"));
        assert!(t.contains("D1") && t.contains("B1") && t.contains("100"));
        assert!(sap.screen.entered.is_empty());
        assert_eq!(run.picks_tsv().lines().count(), 2);
    }

    #[test]
    fn settings_precedence_cli_over_config_over_default() {
        let d = NaiveDate::from_ymd_opt(2026, 11, 13).unwrap();
        let toml: toml::Value = toml::from_str(
            "bin_pick_strategy = \"lifo\"\nage_cutoff = 10\nallow_relabel = true\nwarehouse = \"W9\"\nshipto_col = \"Ship To\"",
        )
        .unwrap();
        let input = ResolveInput {
            section: Some(toml),
            ..ResolveInput::default()
        };
        let s = resolve_manual_to(&input, &Lt03Cli::default(), d).unwrap();
        assert_eq!(s.lt.strategy, Strategy::Lifo);
        assert_eq!(s.lt.age_cutoff_days, 10);
        assert!(s.lt.allow_relabel && s.lt.skip_blacklist && s.lt.proceed_without_gr);
        assert_eq!(s.lt.warehouse, "W9");
        assert_eq!(s.db_cols.shipto, "ship_to");
        let cli = Lt03Cli {
            bin_pick_strategy: Some("fifo".into()),
            age_cutoff: Some(0),
            allow_relabel: Some(false),
            ..Lt03Cli::default()
        };
        let s = resolve_manual_to(&input, &cli, d).unwrap();
        assert_eq!(s.lt.strategy, Strategy::Fifo);
        assert_eq!(s.lt.age_cutoff_days, 0);
        assert!(!s.lt.allow_relabel);
        let bad = Lt03Cli {
            bin_pick_strategy: Some("random".into()),
            ..Lt03Cli::default()
        };
        assert!(resolve_manual_to(&input, &bad, d).is_err());
    }

    #[test]
    fn defaults_are_fifo_30_days_relabel_skip_blocked_bins() {
        let d = NaiveDate::from_ymd_opt(2026, 11, 13).unwrap();
        let s = resolve_manual_to(&ResolveInput::default(), &Lt03Cli::default(), d).unwrap();
        assert_eq!(s.lt.strategy, Strategy::Fifo);
        assert_eq!(s.lt.age_cutoff_days, 30);
        assert!(s.lt.allow_relabel && s.lt.skip_blacklist);
        assert_eq!(
            s.lt.blacklist_policy,
            BlacklistPolicy::AllowMaterialSkipBins
        );
        let line = describe_settings(&s);
        assert!(line.contains("bin_pick_strategy=fifo") && line.contains("age_cutoff=30"));
        let mut log = RunLog::default();
        announce_settings(&s, &mut log);
        assert_eq!(log.lines[0].action, "Run Settings");
        assert!(log.lines[0].details.contains("allow_relabel=true"));
        // explicit off still works
        let cli = Lt03Cli {
            age_cutoff: Some(0),
            skip_blacklist: Some(false),
            ..Lt03Cli::default()
        };
        let s = resolve_manual_to(&ResolveInput::default(), &cli, d).unwrap();
        assert_eq!(s.lt.age_cutoff_days, 0);
        assert!(!s.lt.skip_blacklist);
    }

    #[test]
    fn shipments_from_flags_stdin_clipboard_and_vt11() {
        let stdin = || Ok("333\n444".to_string());
        let clip = || Ok("555, 111".to_string());
        let src = ShipmentSources {
            stdin: &stdin,
            clipboard: &clip,
            vt11_dir: PathBuf::from("Z:\\does\\not\\exist"),
        };
        let cli = Lt03Cli {
            shipments: vec!["111".into(), "222".into()],
            shipments_file: Some("-".into()),
            shipments_clipboard: true,
            ..Lt03Cli::default()
        };
        assert_eq!(
            collect_shipments(&cli, &src).unwrap(),
            ["111", "222", "333", "444", "555"]
        );
        // nothing given and no VT11 export
        assert!(collect_shipments(&Lt03Cli::default(), &src).is_err());
    }

    #[test]
    fn shipments_fall_back_to_the_newest_vt11_export() {
        let dir = std::env::temp_dir().join(format!("lt03_vt11_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.txt"), "shipment_number\n24483092\n24483094\n").unwrap();
        let none = || Ok(String::new());
        let src = ShipmentSources {
            stdin: &none,
            clipboard: &none,
            vt11_dir: dir.clone(),
        };
        assert_eq!(
            collect_shipments(&Lt03Cli::default(), &src).unwrap(),
            ["24483092", "24483094"]
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn last_dry_run_table_is_found_and_parsed() {
        let st = settings(true);
        let mut sap = fake(vl(), sn());
        let unit = Unit {
            tag: "s1".into(),
            shipments: vec!["1".into()],
        };
        let mut log = RunLog::default();
        let rep = run_unit(&mut sap, &st, &unit, &HashSet::new(), &mut log);
        let run = RunReport {
            units: vec![rep],
            dry_run: true,
            today: st.lt.today,
        };
        let root = std::env::temp_dir().join(format!("lt03_last_{}", std::process::id()));
        for day in ["2026-11-12", "2026-11-13"] {
            write_outputs(
                &output_dir(&root, NaiveDate::parse_from_str(day, "%Y-%m-%d").unwrap()),
                &run,
                &log,
            )
            .unwrap();
        }
        // a real (non dry-run) folder is ignored
        std::fs::create_dir_all(root.join("MANUAL_TO").join("2026-11-14")).unwrap();
        let f = find_last_picks(&root).unwrap();
        assert!(f.to_string_lossy().contains("2026-11-13"), "{f:?}");
        let rows = parse_picks_tsv(&std::fs::read_to_string(&f).unwrap());
        assert_eq!(rows, run.pick_rows());
        assert!(find_last_picks(&root.join("nothing")).is_none());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn deliveries_are_in_numerical_order_within_each_shipment_and_blank_repeats() {
        let mut st = settings(true);
        st.vl06o_per_shipment = true;
        let t = |rows: &[&[&str]]| {
            StepOutput::Table(table(
                "vl06o",
                &["delivery", "shipto_party", "material"],
                rows,
            ))
        };
        let mut sap = fake(t(&[&["10", "S1", "M1"], &["3", "S1", "M1"]]), sn());
        sap.vl.push_back(t(&[&["2", "S1", "M1"]]));
        let unit = Unit {
            tag: "all".into(),
            shipments: vec!["900".into(), "100".into()],
        };
        let rep = run_unit(
            &mut sap,
            &st,
            &unit,
            &HashSet::new(),
            &mut RunLog::default(),
        );
        assert_eq!(rep.status, UnitStatus::Done);
        // one VL06O per shipment, shipments in the order given
        assert_eq!(
            sap.vl_calls,
            vec![vec!["900".to_string()], vec!["100".to_string()]]
        );
        let order: Vec<(String, String)> = rep
            .deliveries
            .iter()
            .map(|d| (d.shipment.clone().unwrap(), d.delivery.clone()))
            .collect();
        assert_eq!(
            order,
            [("900", "3"), ("900", "10"), ("100", "2")]
                .map(|(a, b)| (a.to_string(), b.to_string()))
        );
        // the shipment is shown once per group
        let rows = vec![
            vec!["900".to_string(), "3".to_string()],
            vec!["900".to_string(), "3".to_string()],
            vec!["900".to_string(), "10".to_string()],
            vec!["100".to_string(), "2".to_string()],
        ];
        let shown = RunReport::blank_repeats(&rows);
        let col0: Vec<&str> = shown.iter().map(|r| r[0].as_str()).collect();
        assert_eq!(col0, ["900", "", "", "100"]);
    }

    #[test]
    fn one_vl06o_with_all_shipments_by_default() {
        let st = settings(true);
        assert!(!st.vl06o_per_shipment);
        let t = StepOutput::Table(table(
            "vl06o",
            &["delivery", "shipto_party", "material", "shipment_number"],
            &[&["10", "S1", "M1", "900"], &["2", "S1", "M1", "100"]],
        ));
        let mut sap = fake(t, sn());
        let unit = Unit {
            tag: "all".into(),
            shipments: vec!["900".into(), "100".into()],
        };
        let rep = run_unit(
            &mut sap,
            &st,
            &unit,
            &HashSet::new(),
            &mut RunLog::default(),
        );
        assert_eq!(rep.status, UnitStatus::Done);
        // SAP got both shipments in one go; the layout's column gives every delivery its shipment
        assert_eq!(
            sap.vl_calls,
            vec![vec!["900".to_string(), "100".to_string()]]
        );
        let got: Vec<(String, String)> = rep
            .deliveries
            .iter()
            .map(|d| (d.shipment.clone().unwrap(), d.delivery.clone()))
            .collect();
        assert_eq!(
            got,
            [("900", "10"), ("100", "2")].map(|(a, b)| (a.to_string(), b.to_string()))
        );
    }

    #[test]
    fn shipments_line_and_summary_round_trip() {
        assert_eq!(shipments_line(&[]), "");
        let l = shipments_line(&["1".to_string(), "22".to_string()]);
        assert_eq!(l, "Shipments in this run (2): 1, 22");
        let st = settings(true);
        let mut sap = fake(vl(), sn());
        let unit = Unit {
            tag: "all".into(),
            shipments: vec!["111".into(), "222".into(), "111".into()],
        };
        let mut log = RunLog::default();
        let rep = run_unit(&mut sap, &st, &unit, &HashSet::new(), &mut log);
        let run = RunReport {
            units: vec![rep],
            dry_run: true,
            today: st.lt.today,
        };
        assert_eq!(run.shipments(), ["111", "222"]);
        let dir = std::env::temp_dir().join(format!("lt03_ship_{}", std::process::id()));
        write_outputs(&dir, &run, &log).unwrap();
        assert_eq!(
            shipments_of_summary(&dir.join("dryrun_summary.json")),
            ["111", "222"]
        );
        assert!(shipments_of_summary(&dir.join("nothing.json")).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn units_one_or_per_shipment() {
        let s: Vec<String> = ["1", "2", "3"].iter().map(|x| x.to_string()).collect();
        assert_eq!(plan_units(&s, false).len(), 1);
        let u = plan_units(&s, true);
        assert_eq!(u.len(), 3);
        assert_eq!(u[2].shipments, ["3"]);
    }

    #[test]
    fn outputs_written_and_resume_reads_created() {
        let st = settings(false);
        let mut sap = fake(vl(), sn());
        let unit = Unit {
            tag: "s1".into(),
            shipments: vec!["1".into()],
        };
        let mut log = RunLog::default();
        let rep = run_unit(&mut sap, &st, &unit, &HashSet::new(), &mut log);
        let run = RunReport {
            units: vec![rep],
            dry_run: false,
            today: st.lt.today,
        };
        let dir = std::env::temp_dir().join(format!("lt03_out_{}", std::process::id()));
        let files = write_outputs(&dir, &run, &log).unwrap();
        assert_eq!(files.len(), 3);
        assert_eq!(read_done_deliveries(&dir), ["D1".to_string()].into());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
