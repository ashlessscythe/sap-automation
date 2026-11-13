//! LT03 per-delivery flow, written against the [`Lt03Screen`] seam so it runs (and is tested)
//! without SAP. The real implementation over the SAP GUI is `lt03::RealLt03Screen`.
//!
//! Port of `lt03_deliv` of the `manual_to` macro: open the delivery, walk its materials, plan the
//! picks from the stock rows SAP shows ([`crate::lt03_plan`]), enter them, save the transfer order.
//! Never prompts: a missing GR date follows `proceed_without_gr` (headless: `true`, sequential
//! fallback logged yellow). In a dry run nothing is entered and nothing is saved; the planned picks
//! are returned in [`DeliveryResult`].
#![allow(dead_code)]

use crate::lt03_db::{set_has, StockDb};
use crate::lt03_plan::{
    plan_material, prepare_snapshot, BlacklistPolicy, Pick, PlanEvent, PlanOpts, SnapRow, Strategy,
};
use anyhow::{bail, Result};
use chrono::{NaiveDate, NaiveDateTime};
use std::collections::HashSet;

/// Header info of the material currently shown by LT03.
#[derive(Debug, Clone, PartialEq)]
pub struct MaterialInfo {
    pub material: String,
    /// `RL03T-AUSME`: quantity still to cover.
    pub need: f64,
    /// `RL03T-OFFME`: open quantity.
    pub open: f64,
    /// `RL03T-VERME`: quantity available.
    pub avail: f64,
}

/// One row of the stock table as read from the screen.
#[derive(Debug, Clone, PartialEq)]
pub struct RawRow {
    pub abs_row: usize,
    pub bin: String,
    pub avail: f64,
    pub sel: f64,
    /// Goods receipt date exactly as SAP shows it.
    pub gr_text: String,
}

/// What opening a delivery gave.
#[derive(Debug, Clone, PartialEq)]
pub enum OpenOutcome {
    /// Stock view of the first material is showing.
    Ready,
    /// Status bar: no items to be picked.
    NoItems(String),
    /// Status bar: delivery is being processed by someone else.
    InProcess(String),
}

/// What the SAP side must provide for one delivery. All methods act on the current screen.
pub trait Lt03Screen {
    /// Start LT03, enter warehouse + delivery, Enter, check the status bar and (when ready) switch
    /// to the stock view (Shift+F6).
    fn open_delivery(&mut self, warehouse: &str, delivery: &str) -> Result<OpenOutcome>;
    /// Material header of the current screen; `None` when there is no (more) material.
    fn current_material(&mut self) -> Result<Option<MaterialInfo>>;
    /// All rows of the stock table (paged to the end, back on the first page afterwards). Rows
    /// without a bin are left out.
    fn read_stock(&mut self) -> Result<Vec<RawRow>>;
    /// Type `qty` into the selected-quantity cell of the row `abs_row` (bin `bin` is verified)
    /// and press Enter. `false` when the row/bin could not be found.
    fn enter_pick(&mut self, abs_row: usize, bin: &str, qty: f64) -> Result<bool>;
    /// Open quantity of the current material (re-read after picks).
    fn open_qty(&mut self) -> Result<f64>;
    /// Happy path: next material (the entered picks stay).
    fn next_material(&mut self) -> Result<()>;
    /// Sad path / dry run: cancel the edits of this material, then next material.
    fn skip_material(&mut self) -> Result<()>;
    /// True once the delivery overview (after the last material) is showing.
    fn at_overview(&mut self) -> Result<bool>;
    /// `save = true`: save the TO and return the status bar text. `false`: leave the delivery
    /// without creating anything.
    fn finish(&mut self, save: bool) -> Result<String>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogColor {
    None,
    Yellow,
    Green,
    Red,
}

impl LogColor {
    pub fn name(self) -> &'static str {
        match self {
            Self::None => "",
            Self::Yellow => "yellow",
            Self::Green => "green",
            Self::Red => "red",
        }
    }
}

/// One line of the run log (the five columns of the macro's log sheet + qty/bin/color).
#[derive(Debug, Clone, PartialEq)]
pub struct LogLine {
    pub main: String,
    pub item: String,
    pub action: String,
    pub details: String,
    pub at: NaiveDateTime,
    pub qty: Option<f64>,
    pub bin: String,
    pub color: LogColor,
}

#[derive(Debug, Clone, Default)]
pub struct RunLog {
    pub lines: Vec<LogLine>,
}

impl RunLog {
    /// A plain informational line (not tied to a delivery), e.g. the run settings.
    pub fn info(&mut self, action: &str, details: String) {
        self.lines.push(LogLine {
            main: "Run".to_string(),
            item: String::new(),
            action: action.to_string(),
            details,
            at: chrono::Local::now().naive_local(),
            qty: None,
            bin: String::new(),
            color: LogColor::None,
        });
    }

    #[allow(clippy::too_many_arguments)]
    fn add(
        &mut self,
        delivery: &str,
        material: &str,
        qty: Option<f64>,
        bin: &str,
        action: &str,
        details: String,
        color: LogColor,
    ) {
        self.lines.push(LogLine {
            main: format!("Delivery {delivery}"),
            item: format!("Material: {material}"),
            action: action.to_string(),
            details,
            at: chrono::Local::now().naive_local(),
            qty,
            bin: bin.to_string(),
            color,
        });
    }

    /// TSV with a header; tabs/newlines in cells become spaces.
    pub fn to_tsv(&self) -> String {
        let clean = |s: &str| s.replace(['\t', '\r', '\n'], " ");
        let mut out =
            String::from("Main Object\tItem\tAction\tDetails\tTimestamp\tQty\tBin\tColor\n");
        for l in &self.lines {
            out.push_str(&format!(
                "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
                clean(&l.main),
                clean(&l.item),
                clean(&l.action),
                clean(&l.details),
                l.at.format("%Y-%m-%d %H:%M:%S"),
                l.qty.map(|q| q.to_string()).unwrap_or_default(),
                clean(&l.bin),
                l.color.name()
            ));
        }
        out
    }
}

/// Settings of a run (config and CLI resolved by the caller).
#[derive(Debug, Clone)]
pub struct Lt03Config {
    pub warehouse: String,
    pub strategy: Strategy,
    pub age_cutoff_days: u32,
    pub allow_relabel: bool,
    pub skip_blacklist: bool,
    pub blacklist_policy: BlacklistPolicy,
    /// Missing GR dates with fifo/lifo: continue sequentially (`true`) or skip the material.
    pub proceed_without_gr: bool,
    /// SAP date format of the GR column, VBA style (`mm/dd/yyyy`, `yyyy-mm-dd`, ...).
    pub sap_date_format: String,
    /// Plan only: nothing is entered, nothing is saved.
    pub dry_run: bool,
    pub today: NaiveDate,
}

impl Lt03Config {
    pub fn new(warehouse: &str, today: NaiveDate) -> Self {
        Self {
            warehouse: warehouse.to_string(),
            strategy: Strategy::Sequential,
            age_cutoff_days: 0,
            allow_relabel: false,
            skip_blacklist: false,
            blacklist_policy: BlacklistPolicy::SkipIfAny,
            proceed_without_gr: true,
            sap_date_format: "yyyy-mm-dd".to_string(),
            dry_run: false,
            today,
        }
    }
}

/// Outcome of one material.
#[derive(Debug, Clone, PartialEq)]
pub enum MaterialOutcome {
    /// Picks entered (real run) or planned (dry run).
    Picked(Vec<Pick>),
    Skipped(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct MaterialResult {
    pub material: String,
    pub open_qty: f64,
    pub outcome: MaterialOutcome,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DeliveryStatus {
    /// TO saved; the string is the status bar text.
    Created(String),
    /// Dry run finished, nothing created.
    Planned,
    NoItems(String),
    InProcess(String),
    /// Nothing could be picked for any material: no TO is created.
    NothingPicked,
    Failed(String),
}

impl DeliveryStatus {
    /// Green in the result file (TO created or a clean dry run).
    pub fn is_ok(&self) -> bool {
        matches!(self, Self::Created(_) | Self::Planned)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct DeliveryResult {
    pub delivery: String,
    /// Shipment the delivery belongs to (blank when unknown).
    pub shipment: Option<String>,
    pub ship_to: Option<String>,
    pub status: DeliveryStatus,
    pub materials: Vec<MaterialResult>,
}

impl DeliveryResult {
    pub fn picked_materials(&self) -> usize {
        self.materials
            .iter()
            .filter(|m| matches!(m.outcome, MaterialOutcome::Picked(_)))
            .count()
    }
}

// ---------------------------------------------------------------------------------------------
// SAP date handling

/// VBA-style date pattern (`yyyy`, `yy`, `mm`, `dd`) -> chrono format.
pub fn chrono_format(vba: &str) -> String {
    vba.to_ascii_lowercase()
        .replace("yyyy", "%Y")
        .replace("yy", "%y")
        .replace("mm", "%m")
        .replace("dd", "%d")
}

/// Parse a date SAP shows, using the configured pattern for the field order and the common
/// delimiters as fallback (`/`, `-`, `.`). Two-digit years: >= 70 is 19xx, else 20xx.
pub fn parse_sap_date(s: &str, vba_format: &str) -> Option<NaiveDate> {
    let t = s.trim();
    if t.is_empty() {
        return None;
    }
    let fmt = vba_format.to_ascii_lowercase();
    let delim = fmt.chars().find(|c| !c.is_ascii_alphabetic());
    let fmt_parts: Vec<&str> = match delim {
        Some(d) => fmt.split(d).collect(),
        None => Vec::new(),
    };
    let split = |d: char| -> Vec<&str> { t.split(d).collect() };
    let mut parts = delim.map(split).unwrap_or_default();
    if parts.len() != 3 {
        parts = ['/', '-', '.']
            .iter()
            .map(|&d| split(d))
            .find(|p| p.len() == 3)
            .unwrap_or_default();
    }
    if fmt_parts.len() != 3 || parts.len() != 3 {
        return None;
    }
    let (mut y, mut m, mut d) = (None, None, None);
    for (f, v) in fmt_parts.iter().zip(&parts) {
        let n: i32 = v.trim().parse().ok()?;
        if f.contains('y') {
            y = Some(match n {
                n if n >= 100 => n,
                n if n >= 70 => 1900 + n,
                n => 2000 + n,
            });
        } else if f.contains('d') {
            d = Some(n);
        } else if f.contains('m') {
            m = Some(n);
        }
    }
    NaiveDate::from_ymd_opt(y?, u32::try_from(m?).ok()?, u32::try_from(d?).ok()?)
}

// ---------------------------------------------------------------------------------------------
// the flow

fn bin_status(has_whitelist: bool, is_wl: bool) -> &'static str {
    match (has_whitelist, is_wl) {
        (true, true) => " (Whitelist)",
        (true, false) => " (Non-Whitelist - Relabel)",
        _ => " (No Whitelist)",
    }
}

struct Run<'a> {
    cfg: &'a Lt03Config,
    db: &'a StockDb,
    log: &'a mut RunLog,
    delivery: String,
    ship_to: Option<String>,
}

impl Run<'_> {
    #[allow(clippy::too_many_arguments)]
    fn log(
        &mut self,
        material: &str,
        qty: Option<f64>,
        bin: &str,
        action: &str,
        details: String,
        color: LogColor,
    ) {
        let d = self.delivery.clone();
        self.log.add(&d, material, qty, bin, action, details, color);
    }

    fn error(&mut self, material: &str, qty: f64, action: &str, details: String) {
        self.log(material, Some(qty), "", action, details, LogColor::Red);
    }

    fn mode_info(&self, gr_fallback: bool) -> String {
        let mut s = format!(" [pick={}", self.cfg.strategy.name());
        if gr_fallback {
            s.push_str(" (gr-fallback)");
        }
        if self.cfg.age_cutoff_days > 0 {
            s.push_str(&format!("; age_cutoff={}", self.cfg.age_cutoff_days));
        }
        s.push_str(&format!(
            "; blacklist_policy={}]",
            self.cfg.blacklist_policy.name()
        ));
        s
    }

    /// One material. Returns its outcome; the caller moves on (next / skip).
    fn material<S: Lt03Screen>(
        &mut self,
        screen: &mut S,
        info: &MaterialInfo,
    ) -> Result<(MaterialOutcome, f64)> {
        let mat = info.material.clone();
        let mut open_qty = info.open;
        self.log(
            &mat,
            Some(open_qty),
            "",
            "Material Processing",
            format!(
                "Starting processing for material {mat} (Need: {}, Open: {}, Available: {})",
                info.need, info.open, info.avail
            ),
            LogColor::None,
        );
        let raw = screen.read_stock()?;
        let skip = |me: &mut Self, qty: f64, why: String, action: &str| {
            me.error(&mat, qty, action, why.clone());
            Ok::<_, anyhow::Error>((MaterialOutcome::Skipped(why), qty))
        };

        // blocked stock anywhere in the material's bins
        if self.cfg.skip_blacklist
            && self.cfg.blacklist_policy == BlacklistPolicy::SkipIfAny
            && raw.iter().any(|r| self.db.is_blacklisted(&r.bin))
        {
            return skip(
                self,
                open_qty,
                format!("Material {mat} has blocked stock (blacklist) - skipping material"),
                "Material Skipped",
            );
        }
        if open_qty <= 0.0 {
            open_qty = info.need;
        }
        if open_qty <= 0.0 {
            return skip(
                self,
                0.0,
                format!("No open quantity for material {mat} (Need: {})", info.need),
                "Material Skipped",
            );
        }

        let ship = self.ship_to.clone().unwrap_or_default();
        let whitelist = self.db.whitelist(&mat, &ship);
        let has_wl = whitelist.is_some();
        let all_bins = self.db.all_bins(&mat);
        if !has_wl && !self.cfg.allow_relabel {
            return skip(
                self,
                open_qty,
                format!("No whitelist available and relabel not allowed for material {mat}"),
                "Material Skipped",
            );
        }

        // snapshot: blocked bins are always dropped, relabel limits bins to the DB universe
        let mut snap: Vec<SnapRow> = Vec::new();
        let mut blocked_logged: HashSet<String> = HashSet::new();
        // quantity SAP shows that the rules leave out, for the "Insufficient Quantity" message
        let (mut blocked_qty, mut outside_db_qty) = (0.0_f64, 0.0_f64);
        for r in &raw {
            if r.bin.contains("___") {
                continue;
            }
            if self.db.is_blacklisted(&r.bin) {
                blocked_qty += r.avail.max(0.0);
                if blocked_logged.insert(r.bin.to_ascii_uppercase()) {
                    self.log(
                        &mat,
                        Some(0.0),
                        &r.bin,
                        "Bin Skipped",
                        format!("Bin {} skipped (blocked serial)", r.bin),
                        LogColor::Red,
                    );
                }
                continue;
            }
            if self.cfg.allow_relabel && !set_has(all_bins, &r.bin) {
                outside_db_qty += r.avail.max(0.0);
                continue;
            }
            if r.avail <= 0.0 {
                continue;
            }
            snap.push(SnapRow {
                abs_row: r.abs_row,
                bin: r.bin.clone(),
                avail: r.avail,
                sel: r.sel,
                is_wl: has_wl && set_has(whitelist, &r.bin),
                gr_date: parse_sap_date(&r.gr_text, &self.cfg.sap_date_format),
            });
        }

        let opts = PlanOpts {
            strategy: self.cfg.strategy,
            age_cutoff_days: self.cfg.age_cutoff_days,
            allow_relabel: self.cfg.allow_relabel,
            delivery: self.delivery.clone(),
            material: mat.clone(),
            today: self.cfg.today,
        };
        let prepared = match prepare_snapshot(&snap, &opts, self.cfg.proceed_without_gr) {
            Ok(p) => p,
            Err(e) => {
                return skip(
                    self,
                    open_qty,
                    format!("Material {mat}: {e}; continuing without GR ordering was refused"),
                    "Material Skipped",
                );
            }
        };
        for ev in &prepared.events {
            match ev {
                PlanEvent::AgeCutoffSkip { bin, reason } => self.log(
                    &mat,
                    Some(0.0),
                    bin,
                    "Bin Skipped",
                    format!("Bin {bin} skipped (GR age cutoff: {reason})"),
                    LogColor::Yellow,
                ),
                PlanEvent::GrFallback { missing } => self.log(
                    &mat,
                    Some(0.0),
                    "",
                    "GR Date Pick Fallback",
                    format!(
                        "{missing} bin(s) missing GR date - continuing with sequential picks for material {mat}"
                    ),
                    LogColor::Yellow,
                ),
            }
        }

        let plan = match plan_material(&prepared, open_qty, self.db.full_pallet(&mat), &opts) {
            Ok(p) => p,
            Err(total) => {
                // where the rest of what SAP shows went (SAP's own "Available" counts everything)
                let snap_qty: f64 = snap.iter().map(|r| r.avail).sum();
                let prep_qty: f64 = prepared.rows.iter().map(|r| r.avail).sum();
                let mut why = Vec::new();
                for (label, q) in [
                    ("blocked bins", blocked_qty),
                    ("outside the stock list", outside_db_qty),
                    ("GR age cutoff", snap_qty - prep_qty),
                ] {
                    if q > 0.0 {
                        why.push(format!("{label} {q}"));
                    }
                }
                let detail = if why.is_empty() {
                    String::new()
                } else {
                    format!(
                        " (SAP shows {} available; left out: {})",
                        info.avail,
                        why.join(", ")
                    )
                };
                return skip(
                    self,
                    open_qty,
                    format!(
                        "Need {open_qty} but only have {total} available{detail}. Skipping material."
                    ),
                    "Insufficient Quantity",
                );
            }
        };

        let mode = self.mode_info(prepared.gr_fallback);
        let fmt = chrono_format(&self.cfg.sap_date_format);
        let dry = self.cfg.dry_run;
        let mut done: Vec<Pick> = Vec::new();
        for p in &plan.picks {
            if !dry && !screen.enter_pick(p.abs_row, &p.bin, p.qty)? {
                continue; // row not found: left out, the open-quantity check below catches it
            }
            let gr = match (prepared.gr_active, p.gr_date) {
                (true, Some(d)) => format!(" GR={}", d.format(&fmt)),
                _ => String::new(),
            };
            let color = if p.relabel() || p.repack {
                LogColor::Yellow
            } else {
                LogColor::None
            };
            let mut tags = String::new();
            if p.relabel() {
                tags.push_str(" [RELABEL]");
            }
            if p.repack {
                tags.push_str(" [REPACK]");
            }
            self.log(
                &mat,
                Some(p.qty),
                &p.bin,
                "Bin Selection",
                format!(
                    "{}Selected {} from bin {}{}{gr}{tags} - Planned pick (snapshot){mode}",
                    if dry { "[DRY RUN] " } else { "" },
                    p.qty,
                    p.bin,
                    bin_status(has_wl, p.is_wl),
                ),
                color,
            );
            done.push(p.clone());
        }
        if dry {
            return Ok((MaterialOutcome::Picked(done), open_qty));
        }
        let left = screen.open_qty()?;
        if left > 0.0 {
            return skip(
                self,
                open_qty,
                format!(
                    "Need {open_qty} but only have {} available. Skipping material.",
                    open_qty - left
                ),
                "Insufficient Quantity",
            );
        }
        Ok((MaterialOutcome::Picked(done), open_qty))
    }
}

/// Run one delivery. `ship_to` is the resolved ship-to (whitelist key).
pub fn run_delivery<S: Lt03Screen>(
    screen: &mut S,
    cfg: &Lt03Config,
    db: &StockDb,
    delivery: &str,
    ship_to: Option<&str>,
    log: &mut RunLog,
) -> DeliveryResult {
    let mut res = DeliveryResult {
        delivery: delivery.to_string(),
        shipment: None,
        ship_to: ship_to.map(str::to_string),
        status: DeliveryStatus::Failed(String::new()),
        materials: Vec::new(),
    };
    let mut run = Run {
        cfg,
        db,
        log,
        delivery: delivery.to_string(),
        ship_to: ship_to.map(str::to_string),
    };
    res.status = match run_inner(screen, &mut run, &mut res.materials) {
        Ok(s) => s,
        Err(e) => {
            run.error("", 0.0, "Error", format!("{e:#}"));
            DeliveryStatus::Failed(format!("{e:#}"))
        }
    };
    res
}

fn run_inner<S: Lt03Screen>(
    screen: &mut S,
    run: &mut Run,
    materials: &mut Vec<MaterialResult>,
) -> Result<DeliveryStatus> {
    let delivery = run.delivery.clone();
    match screen.open_delivery(&run.cfg.warehouse, &delivery)? {
        OpenOutcome::Ready => {}
        OpenOutcome::NoItems(msg) => {
            run.error(
                "",
                0.0,
                "No Items to Pick",
                format!("{msg} for delivery: {delivery}"),
            );
            return Ok(DeliveryStatus::NoItems(msg));
        }
        OpenOutcome::InProcess(msg) => {
            run.error(
                "",
                0.0,
                "Delivery currently being processed",
                format!("{msg} for delivery: {delivery}"),
            );
            return Ok(DeliveryStatus::InProcess(msg));
        }
    }
    run.log(
        "",
        Some(0.0),
        "",
        "TO Creation Started",
        "Processing delivery for transfer order creation".to_string(),
        LogColor::None,
    );

    let mut guard = 0;
    while let Some(info) = screen.current_material()? {
        guard += 1;
        if guard > 1000 {
            bail!("more than 1000 materials in one delivery: aborting");
        }
        let (outcome, open_qty) = run.material(screen, &info)?;
        let picked = matches!(&outcome, MaterialOutcome::Picked(p) if !p.is_empty());
        // real run + picks entered: keep them; everything else cancels the material
        if picked && !run.cfg.dry_run {
            screen.next_material()?;
        } else {
            screen.skip_material()?;
        }
        materials.push(MaterialResult {
            material: info.material,
            open_qty,
            outcome,
        });
        if screen.at_overview()? {
            break;
        }
    }

    let any = materials
        .iter()
        .any(|m| matches!(&m.outcome, MaterialOutcome::Picked(p) if !p.is_empty()));
    if run.cfg.dry_run {
        screen.finish(false)?;
        return Ok(DeliveryStatus::Planned);
    }
    if !any {
        screen.finish(false)?;
        run.error(
            "",
            0.0,
            "Nothing Picked",
            "No material could be picked: no transfer order created".to_string(),
        );
        return Ok(DeliveryStatus::NothingPicked);
    }
    let status = screen.finish(true)?;
    run.log(
        "",
        Some(0.0),
        "",
        "TO Completed",
        format!("Transfer order saved successfully. Status: {status}"),
        LogColor::Green,
    );
    Ok(DeliveryStatus::Created(status))
}

#[cfg(test)]
pub(crate) mod fakes {
    use super::*;

    pub struct Mat {
        pub info: MaterialInfo,
        pub rows: Vec<RawRow>,
    }

    pub struct Fake {
        pub mats: Vec<Mat>,
        pub idx: usize,
        pub open: OpenOutcome,
        pub entered: Vec<(usize, String, f64)>,
        pub saved: Option<bool>,
        pub calls: Vec<&'static str>,
    }

    pub fn mat(m: &str, open: f64, rows: &[(usize, &str, f64, &str)]) -> Mat {
        Mat {
            info: MaterialInfo {
                material: m.into(),
                need: open,
                open,
                avail: 0.0,
            },
            rows: rows
                .iter()
                .map(|(a, b, v, g)| RawRow {
                    abs_row: *a,
                    bin: (*b).into(),
                    avail: *v,
                    sel: 0.0,
                    gr_text: (*g).into(),
                })
                .collect(),
        }
    }

    impl Fake {
        pub fn new(mats: Vec<Mat>) -> Self {
            Self {
                mats,
                idx: 0,
                open: OpenOutcome::Ready,
                entered: Vec::new(),
                saved: None,
                calls: Vec::new(),
            }
        }
    }

    impl Lt03Screen for Fake {
        fn open_delivery(&mut self, _w: &str, _d: &str) -> Result<OpenOutcome> {
            Ok(self.open.clone())
        }
        fn current_material(&mut self) -> Result<Option<MaterialInfo>> {
            Ok(self.mats.get(self.idx).map(|m| m.info.clone()))
        }
        fn read_stock(&mut self) -> Result<Vec<RawRow>> {
            Ok(self.mats[self.idx].rows.clone())
        }
        fn enter_pick(&mut self, abs_row: usize, bin: &str, qty: f64) -> Result<bool> {
            self.entered.push((abs_row, bin.into(), qty));
            self.mats[self.idx].info.open -= qty;
            Ok(true)
        }
        fn open_qty(&mut self) -> Result<f64> {
            Ok(self.mats[self.idx].info.open)
        }
        fn next_material(&mut self) -> Result<()> {
            self.calls.push("next");
            self.idx += 1;
            Ok(())
        }
        fn skip_material(&mut self) -> Result<()> {
            self.calls.push("skip");
            self.idx += 1;
            Ok(())
        }
        fn at_overview(&mut self) -> Result<bool> {
            Ok(self.idx >= self.mats.len())
        }
        fn finish(&mut self, save: bool) -> Result<String> {
            self.saved = Some(save);
            Ok("Transfer order 0001 created".into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fakes::*;
    use super::*;
    use crate::lt03_db::DbColumns;

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, 13).unwrap()
    }

    fn grid(rows: &[&[&str]]) -> Vec<Vec<String>> {
        rows.iter()
            .map(|r| r.iter().map(|c| c.to_string()).collect())
            .collect()
    }

    fn db() -> StockDb {
        StockDb::from_grid(
            &grid(&[
                &[
                    "material",
                    "storage_bin",
                    "quantity",
                    "shipto_party",
                    "blocked",
                ],
                &["M1", "B1", "100", "S1", ""],
                &["M1", "B2", "100", "S1", ""],
                &["M2", "C1", "50", "S1", ""],
                &["M3", "D1", "10", "S9", "X"],
            ]),
            &DbColumns::default(),
        )
        .unwrap()
    }

    fn cfg() -> Lt03Config {
        let mut c = Lt03Config::new("W1", today());
        c.skip_blacklist = true;
        c
    }

    #[test]
    fn date_parsing_follows_the_configured_order() {
        let d = |y, m, dd| NaiveDate::from_ymd_opt(y, m, dd);
        assert_eq!(parse_sap_date("09/16/2026", "mm/dd/yyyy"), d(2026, 9, 16));
        assert_eq!(parse_sap_date("16.09.2026", "dd.mm.yyyy"), d(2026, 9, 16));
        assert_eq!(parse_sap_date("2026-09-16", "yyyy-mm-dd"), d(2026, 9, 16));
        assert_eq!(parse_sap_date("09-16-26", "mm/dd/yy"), d(2026, 9, 16)); // delimiter fallback
        assert_eq!(parse_sap_date("09/16/99", "mm/dd/yy"), d(1999, 9, 16));
        assert_eq!(parse_sap_date("", "mm/dd/yyyy"), None);
        assert_eq!(parse_sap_date("13/45/2026", "mm/dd/yyyy"), None);
        assert_eq!(chrono_format("mm/dd/yyyy"), "%m/%d/%Y");
    }

    #[test]
    fn happy_path_enters_picks_and_saves() {
        let mut s = Fake::new(vec![
            mat("M1", 100.0, &[(0, "B1", 100.0, ""), (1, "B2", 100.0, "")]),
            mat("M2", 50.0, &[(0, "C1", 50.0, "")]),
        ]);
        let mut log = RunLog::default();
        let r = run_delivery(&mut s, &cfg(), &db(), "D1", Some("S1"), &mut log);
        assert_eq!(
            r.status,
            DeliveryStatus::Created("Transfer order 0001 created".into())
        );
        assert_eq!(s.entered.len(), 2);
        assert_eq!(s.calls, ["next", "next"]);
        assert_eq!(s.saved, Some(true));
        assert!(log.lines.iter().any(|l| l.action == "TO Completed"));
        assert!(log
            .lines
            .iter()
            .any(|l| l.action == "Bin Selection" && l.details.contains("(Whitelist)")));
    }

    #[test]
    fn dry_run_enters_nothing_and_saves_nothing() {
        let mut s = Fake::new(vec![mat("M1", 100.0, &[(0, "B1", 100.0, "")])]);
        let mut c = cfg();
        c.dry_run = true;
        let mut log = RunLog::default();
        let r = run_delivery(&mut s, &c, &db(), "D1", Some("S1"), &mut log);
        assert_eq!(r.status, DeliveryStatus::Planned);
        assert!(s.entered.is_empty());
        assert_eq!(s.saved, Some(false));
        assert_eq!(s.calls, ["skip"]);
        match &r.materials[0].outcome {
            MaterialOutcome::Picked(p) => assert_eq!((p[0].bin.as_str(), p[0].qty), ("B1", 100.0)),
            o => panic!("{o:?}"),
        }
        assert!(log.lines.iter().any(|l| l.details.starts_with("[DRY RUN]")));
    }

    #[test]
    fn insufficient_material_is_skipped_and_the_rest_still_saved() {
        let mut s = Fake::new(vec![
            mat("M1", 500.0, &[(0, "B1", 100.0, "")]),
            mat("M2", 50.0, &[(0, "C1", 50.0, "")]),
        ]);
        let mut log = RunLog::default();
        let r = run_delivery(&mut s, &cfg(), &db(), "D1", Some("S1"), &mut log);
        assert!(r.status.is_ok());
        assert_eq!(s.calls, ["skip", "next"]);
        assert!(matches!(
            r.materials[0].outcome,
            MaterialOutcome::Skipped(_)
        ));
        assert!(log
            .lines
            .iter()
            .any(|l| l.action == "Insufficient Quantity" && l.color == LogColor::Red));
    }

    #[test]
    fn nothing_picked_creates_no_to() {
        let mut s = Fake::new(vec![mat("M1", 500.0, &[(0, "B1", 100.0, "")])]);
        let mut log = RunLog::default();
        let r = run_delivery(&mut s, &cfg(), &db(), "D1", Some("S1"), &mut log);
        assert_eq!(r.status, DeliveryStatus::NothingPicked);
        assert_eq!(s.saved, Some(false));
    }

    #[test]
    fn no_items_and_in_process_are_reported_without_walking() {
        let mut s = Fake::new(vec![]);
        s.open = OpenOutcome::NoItems("No items that are to be picked".into());
        let mut log = RunLog::default();
        let r = run_delivery(&mut s, &cfg(), &db(), "D1", Some("S1"), &mut log);
        assert!(matches!(r.status, DeliveryStatus::NoItems(_)));
        assert_eq!(log.lines[0].action, "No Items to Pick");
        s.open = OpenOutcome::InProcess("currently being processed".into());
        let r = run_delivery(
            &mut s,
            &cfg(),
            &db(),
            "D1",
            Some("S1"),
            &mut RunLog::default(),
        );
        assert!(matches!(r.status, DeliveryStatus::InProcess(_)));
    }

    #[test]
    fn no_whitelist_without_relabel_skips_the_material() {
        let mut s = Fake::new(vec![mat("M1", 100.0, &[(0, "B1", 100.0, "")])]);
        let mut log = RunLog::default();
        // ship-to S7 has no bins for M1
        let r = run_delivery(&mut s, &cfg(), &db(), "D1", Some("S7"), &mut log);
        assert!(matches!(
            r.materials[0].outcome,
            MaterialOutcome::Skipped(_)
        ));
        assert_eq!(r.status, DeliveryStatus::NothingPicked);
        // with relabel the DB universe is used
        let mut s = Fake::new(vec![mat("M1", 100.0, &[(0, "B1", 100.0, "")])]);
        let mut c = cfg();
        c.allow_relabel = true;
        let r = run_delivery(&mut s, &c, &db(), "D1", Some("S7"), &mut RunLog::default());
        assert!(r.status.is_ok());
        assert!(s.entered.iter().all(|(_, b, _)| b == "B1"));
    }

    #[test]
    fn blacklist_policies() {
        // M3 has a blocked bin (D1); policy skip_if_any skips the material
        let rows = [(0, "D1", 10.0, ""), (1, "D2", 10.0, "")];
        let mut c = cfg();
        c.allow_relabel = true;
        let mut s = Fake::new(vec![mat("M3", 10.0, &rows)]);
        let r = run_delivery(&mut s, &c, &db(), "D1", Some("S9"), &mut RunLog::default());
        assert!(
            matches!(&r.materials[0].outcome, MaterialOutcome::Skipped(w) if w.contains("blocked"))
        );
        // allow_material_skip_bins: blocked bin dropped, rest usable if in the DB universe
        let db2 = StockDb::from_grid(
            &grid(&[
                &[
                    "material",
                    "storage_bin",
                    "quantity",
                    "shipto_party",
                    "blocked",
                ],
                &["M3", "D1", "10", "S9", "X"],
                &["M3", "D2", "10", "S9", ""],
            ]),
            &DbColumns::default(),
        )
        .unwrap();
        c.blacklist_policy = BlacklistPolicy::AllowMaterialSkipBins;
        let mut s = Fake::new(vec![mat("M3", 10.0, &rows)]);
        let mut log = RunLog::default();
        let r = run_delivery(&mut s, &c, &db2, "D1", Some("S9"), &mut log);
        assert!(r.status.is_ok());
        assert_eq!(s.entered, vec![(1, "D2".to_string(), 10.0)]);
        assert!(log
            .lines
            .iter()
            .any(|l| l.action == "Bin Skipped" && l.details.contains("blocked serial")));
    }

    #[test]
    fn fifo_with_age_cutoff_logs_skipped_bins() {
        let mut c = cfg();
        c.strategy = Strategy::Fifo;
        c.age_cutoff_days = 30;
        c.sap_date_format = "mm/dd/yyyy".into();
        let mut s = Fake::new(vec![mat(
            "M1",
            100.0,
            &[
                (0, "B1", 100.0, "08/01/2026"),
                (1, "B2", 100.0, "11/01/2026"),
            ],
        )]);
        let mut log = RunLog::default();
        let r = run_delivery(&mut s, &c, &db(), "D1", Some("S1"), &mut log);
        assert!(r.status.is_ok());
        assert_eq!(s.entered, vec![(1, "B2".to_string(), 100.0)]);
        let sel = log
            .lines
            .iter()
            .find(|l| l.action == "Bin Selection")
            .unwrap();
        assert!(sel.details.contains("GR=11/01/2026") && sel.details.contains("pick=fifo"));
        assert!(log
            .lines
            .iter()
            .any(|l| l.details.contains("GR age cutoff")));
    }

    #[test]
    fn missing_gr_falls_back_or_skips_by_setting() {
        let mk = || Fake::new(vec![mat("M1", 100.0, &[(0, "B1", 100.0, "")])]);
        let mut c = cfg();
        c.strategy = Strategy::Fifo;
        let mut log = RunLog::default();
        let r = run_delivery(&mut mk(), &c, &db(), "D1", Some("S1"), &mut log);
        assert!(r.status.is_ok());
        assert!(log
            .lines
            .iter()
            .any(|l| l.action == "GR Date Pick Fallback"));
        c.proceed_without_gr = false;
        let r = run_delivery(
            &mut mk(),
            &c,
            &db(),
            "D1",
            Some("S1"),
            &mut RunLog::default(),
        );
        assert_eq!(r.status, DeliveryStatus::NothingPicked);
    }

    #[test]
    fn tsv_has_header_and_rows() {
        let mut s = Fake::new(vec![mat("M1", 100.0, &[(0, "B1", 100.0, "")])]);
        let mut log = RunLog::default();
        run_delivery(&mut s, &cfg(), &db(), "D1", Some("S1"), &mut log);
        let t = log.to_tsv();
        assert!(t.starts_with("Main Object\tItem\tAction"));
        assert_eq!(t.lines().count(), log.lines.len() + 1);
    }
}
