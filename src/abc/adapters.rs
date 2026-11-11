//! Real SAP adapter for the headless runners: [`RealSapSteps`] implements
//! [`super::runner::SapSteps`] (`mustgo`), [`super::inv_runner::InvSteps`] (`xdock`, `toyota`),
//! [`super::inv_runner::MiscSteps`], [`super::k2xx_runner::K2xxSteps`] and
//! [`super::relabel_runner::RelabelSteps`] on top of the tcode steps (`vt11`, `vl06o`,
//! `zmdesnr`, `lt23`, `zmdetpc`, `zvt11` Closed).
//!
//! Nothing here reads config or CLI overrides: the caller builds a [`MustgoSapConfig`]
//! (defaults below) and passes the session. Every step forces "text with tabs"
//! (export type 1) because [`super::raw`] only parses that format, writes the file to
//! `<export_dir>/<TCODE>/<yyyymmddhhmmss>_<TCODE>.txt`, loads it, and deletes it afterwards when
//! `delete_exports` is set (a file that failed to load is kept for diagnosis).
//!
//! NOT run against SAP by the author. See "Live verification checklist (mustgo)", "(xdock)"
//! "(k2xx)" and "(relabel)" in `docs/private/abc_headless_plan.md` for every control id / behaviour that is
//! unverified.
#![allow(dead_code)]

use super::inv_runner::{InvSteps, MiscSteps};
use super::k2xx_runner::K2xxSteps;
use super::raw::RawOptions;
use super::relabel_runner::RelabelSteps;
use super::run::RunDate;
use super::runner::{outcome_to_output, SapSteps, StepOutput};
use crate::lt23::LT23Params;
use crate::utils::export_outcome::ExportOutcome;
use crate::utils::sap_export_utils::{export_file_name, ExportTarget};
use crate::vl06o::VL06OParams;
use crate::vt11::VT11Params;
use crate::zmdesnr::ZMDESNRParams;
use crate::zmdetpc::ZMDETPCParams;
use crate::zvt11::ZVT11ClosedParams;
use anyhow::{anyhow, Context, Result};
use chrono::{Local, NaiveDate, NaiveDateTime};
use sap_scripting::GuiSession;
use std::path::Path;

/// Text with tabs; the only format [`super::raw`] reads.
pub const EXPORT_TYPE_TEXT_TABS: u8 = 1;

/// Everything the adapter needs, resolved by the caller.
#[derive(Debug, Clone)]
pub struct MustgoSapConfig {
    /// VT11 variant (spec step 9).
    pub vt11_variant: String,
    /// ZMDESNR variant (carries the date selection) and layout.
    pub zmdesnr_variant: String,
    pub zmdesnr_layout: String,
    /// LT23 layout and, when set, the columns used to create it if it does not exist.
    pub lt23_layout: String,
    pub lt23_create_layout_columns: Option<Vec<String>>,
    pub warehouse: String,
    /// Base directory of the temporary export files (`<export_dir>/<TCODE>/...`).
    pub export_dir: String,
    /// Config-style SAP date format typed into VT11 (`mm/dd/yyyy`, `yyyy-mm-dd`, ...).
    pub date_format: Option<String>,
    pub ignore_display_error: bool,
    /// Delete each export file after it was loaded successfully.
    pub delete_exports: bool,
    /// How the exports are parsed (date format of the SAP user, raw text columns).
    pub raw: RawOptions,
    /// Explicit buffer (`--date-range-buffer` / `[abc.<report>] date_range_buffer`): days added
    /// before the start and after the end of the run date range of every tcode that sets one.
    /// `None` = the defaults of [`buffer_days`].
    pub date_range_buffer: Option<i64>,
}

/// Default buffer for ZMDETPC when the run date is a week (`WK<n>`): 3 days on either side
/// (owner decision 2026-11-10). Every other case defaults to 0.
pub const DEFAULT_WEEK_BUFFER_DAYS: i64 = 3;

/// Days added on each side of the run date range of `tcode`: the explicit setting (negative
/// counts as 0) when given, else [`DEFAULT_WEEK_BUFFER_DAYS`] for ZMDETPC on a week run, else 0.
pub fn buffer_days(cfg: &MustgoSapConfig, date: &RunDate, tcode: &str) -> i64 {
    match cfg.date_range_buffer {
        Some(n) => n.max(0),
        None if tcode.eq_ignore_ascii_case("ZMDETPC") && date.is_week() => DEFAULT_WEEK_BUFFER_DAYS,
        None => 0,
    }
}

/// The run date range of `tcode` widened by [`buffer_days`] on both sides.
pub fn buffered_range(
    cfg: &MustgoSapConfig,
    date: &RunDate,
    tcode: &str,
) -> (NaiveDate, NaiveDate) {
    let (start, end) = date.as_range();
    let b = chrono::Duration::days(buffer_days(cfg, date, tcode));
    (start - b, end + b)
}

impl Default for MustgoSapConfig {
    fn default() -> Self {
        Self {
            vt11_variant: "VT11_XDOCK_VARIANT".to_string(),
            zmdesnr_variant: "ZMDESNR_MUSTGO_VARIANT".to_string(),
            zmdesnr_layout: "ZMDESNR_MUSTGO_LAYOUT".to_string(),
            lt23_layout: "LT23_MUSTGO_LAYOUT".to_string(),
            lt23_create_layout_columns: None,
            warehouse: "WH01".to_string(),
            export_dir: "reports".to_string(),
            date_format: None,
            ignore_display_error: true,
            delete_exports: true,
            raw: RawOptions::default(),
            date_range_buffer: None,
        }
    }
}

// ---------------------------------------------------------------- pure parameter builders

/// Directory of the exports of `tcode`: `<export_dir>/<TCODE>`.
pub fn export_dir_for(cfg: &MustgoSapConfig, tcode: &str) -> String {
    Path::new(&cfg.export_dir)
        .join(tcode)
        .to_string_lossy()
        .into_owned()
}

/// Export target for `tcode` at time `now` (text with tabs).
pub fn export_target(cfg: &MustgoSapConfig, tcode: &str, now: NaiveDateTime) -> ExportTarget {
    ExportTarget {
        export_type: EXPORT_TYPE_TEXT_TABS,
        dir: export_dir_for(cfg, tcode),
        file_name: export_file_name(tcode, EXPORT_TYPE_TEXT_TABS, now),
    }
}

/// `(start, end)` the VT11 closed-date range for a run date (single date: both equal); widened
/// only by an explicit buffer (VT11 has no default buffer).
pub fn vt11_range(cfg: &MustgoSapConfig, date: &RunDate) -> (NaiveDate, NaiveDate) {
    buffered_range(cfg, date, "VT11")
}

/// Step 9: VT11 with the variant, the closed date (`K_DATEN`) range, no layout (export as-is).
pub fn vt11_params(cfg: &MustgoSapConfig, date: &RunDate, target: ExportTarget) -> VT11Params {
    let (start, end) = vt11_range(cfg, date);
    VT11Params {
        start_date: start,
        end_date: end,
        sap_variant_name: Some(cfg.vt11_variant.clone()),
        layout_row: None,
        by_date: false,
        closed_range: Some((start, end)),
        monday_include_sunday: false,
        ignore_display_error: cfg.ignore_display_error,
        date_format: cfg.date_format.clone(),
        export_target: Some(target),
        ..VT11Params::default()
    }
}

/// Step 11: VL06O for the shipments, item view, no variant/layout. Not `VL06OParams::default()`
/// (that one reads the config).
pub fn vl06o_params(
    cfg: &MustgoSapConfig,
    shipments: &[String],
    target: ExportTarget,
    today: NaiveDate,
) -> VL06OParams {
    VL06OParams {
        sap_variant_name: None,
        layout_row: None,
        shipment_numbers: shipments.to_vec(),
        start_date: today,
        end_date: today,
        by_date: false,
        column_name: None,
        t_code: "VL06O".to_string(),
        export_target: Some(target),
        ignore_display_error: cfg.ignore_display_error,
    }
}

/// Step 12: ZMDESNR by deliveries (variant carries the date; layout `mg_view`).
pub fn zmdesnr_by_deliveries_params(
    cfg: &MustgoSapConfig,
    deliveries: &[String],
    target: ExportTarget,
) -> ZMDESNRParams {
    ZMDESNRParams {
        delivery_numbers: deliveries.to_vec(),
        sap_variant_name: Some(cfg.zmdesnr_variant.clone()),
        layout_row: Some(cfg.zmdesnr_layout.clone()),
        tab_number: Some(2),
        scroll_paste: true,
        export_target: Some(target),
        ignore_display_error: cfg.ignore_display_error,
        ..ZMDESNRParams::default()
    }
}

/// Step 19: ZMDESNR by deliveries + transfer orders.
pub fn zmdesnr_by_deliveries_and_tos_params(
    cfg: &MustgoSapConfig,
    deliveries: &[String],
    transfer_orders: &[String],
    target: ExportTarget,
) -> ZMDESNRParams {
    ZMDESNRParams {
        transfer_numbers: transfer_orders.to_vec(),
        ..zmdesnr_by_deliveries_params(cfg, deliveries, target)
    }
}

/// `relabel`: ZMDESNR by deliveries with the given variant and layout (tab 2, scroll paste).
pub fn relabel_by_deliveries_params(
    cfg: &MustgoSapConfig,
    deliveries: &[String],
    variant: &str,
    layout: &str,
    target: ExportTarget,
) -> ZMDESNRParams {
    ZMDESNRParams {
        sap_variant_name: Some(variant.to_string()).filter(|v| !v.trim().is_empty()),
        layout_row: Some(layout.to_string()).filter(|l| !l.trim().is_empty()),
        ..zmdesnr_by_deliveries_params(cfg, deliveries, target)
    }
}

/// `relabel`: ZMDESNR by serial / History (tab 1) for the serials with the given variant and
/// layout.
pub fn relabel_history_params(
    cfg: &MustgoSapConfig,
    serials: &[String],
    variant: &str,
    layout: &str,
    target: ExportTarget,
) -> ZMDESNRParams {
    ZMDESNRParams {
        history_serials: serials.to_vec(),
        sap_variant_name: Some(variant.to_string()).filter(|v| !v.trim().is_empty()),
        layout_row: Some(layout.to_string()).filter(|l| !l.trim().is_empty()),
        tab_number: Some(1),
        export_target: Some(target),
        ignore_display_error: cfg.ignore_display_error,
        ..ZMDESNRParams::default()
    }
}

/// Step 16: LT23 for the transfer orders, confirmed only, layout `to_mg`.
pub fn lt23_params(
    cfg: &MustgoSapConfig,
    transfer_orders: &[String],
    target: ExportTarget,
) -> LT23Params {
    LT23Params {
        warehouse: cfg.warehouse.clone(),
        transfer_orders: transfer_orders.to_vec(),
        confirmed_only: true,
        layout_row: Some(cfg.lt23_layout.clone()),
        create_layout_columns: cfg.lt23_create_layout_columns.clone(),
        save_layout: false,
        export_target: Some(target),
        ignore_display_error: cfg.ignore_display_error,
        t_code: "LT23".to_string(),
    }
}

/// `xdock` VT11: like [`vt11_params`] with the report's variant and a layout (`scorecard`).
pub fn inv_vt11_params(
    cfg: &MustgoSapConfig,
    date: &RunDate,
    variant: &str,
    layout: &str,
    target: ExportTarget,
) -> VT11Params {
    VT11Params {
        sap_variant_name: Some(variant.to_string()),
        layout_row: Some(layout.to_string()).filter(|l| !l.trim().is_empty()),
        ..vt11_params(cfg, date, target)
    }
}

/// `xdock` VL06O: like [`vl06o_params`] with a layout (`xdock`).
pub fn inv_vl06o_params(
    cfg: &MustgoSapConfig,
    shipments: &[String],
    layout: &str,
    target: ExportTarget,
    today: NaiveDate,
) -> VL06OParams {
    VL06OParams {
        layout_row: Some(layout.to_string()).filter(|l| !l.trim().is_empty()),
        ..vl06o_params(cfg, shipments, target, today)
    }
}

/// `(start, end)` typed into ZMDETPC `S_DATE-LOW` / `-HIGH`: the run date range widened by
/// [`buffer_days`] (3 each side for a week run unless set explicitly, else none).
pub fn zmdetpc_range(cfg: &MustgoSapConfig, date: &RunDate) -> (NaiveDate, NaiveDate) {
    buffered_range(cfg, date, "ZMDETPC")
}

/// ZMDETPC summarised for the deliveries; the dates are [`zmdetpc_range`].
pub fn zmdetpc_params(
    cfg: &MustgoSapConfig,
    deliveries: &[String],
    date: &RunDate,
    variant: &str,
    target: ExportTarget,
) -> ZMDETPCParams {
    let (start, end) = zmdetpc_range(cfg, date);
    ZMDETPCParams {
        variant: Some(variant.to_string()).filter(|v| !v.trim().is_empty()),
        date_start: start,
        date_end: end,
        date_format: cfg.date_format.clone(),
        deliveries: deliveries.to_vec(),
        export_target: Some(target),
        ignore_display_error: cfg.ignore_display_error,
        t_code: "ZMDETPC".to_string(),
    }
}

/// `k2xx` VT11: like [`inv_vt11_params`] plus the `monday_include_sunday` rule (spec 5.5 row
/// 52; the tcode step moves a Monday start to the previous Sunday).
pub fn k2xx_vt11_params(
    cfg: &MustgoSapConfig,
    date: &RunDate,
    variant: &str,
    layout: &str,
    monday_include_sunday: bool,
    target: ExportTarget,
) -> VT11Params {
    VT11Params {
        monday_include_sunday,
        ..inv_vt11_params(cfg, date, variant, layout, target)
    }
}

/// ZVT11 Closed for the deliveries; `from`..`to` is the already widened window (the
/// `date_range_buffer` is not applied on top of it).
pub fn zvt11_closed_params(
    cfg: &MustgoSapConfig,
    deliveries: &[String],
    from: NaiveDate,
    to: NaiveDate,
    variant: &str,
    layout: &str,
    target: ExportTarget,
) -> ZVT11ClosedParams {
    ZVT11ClosedParams {
        variant: Some(variant.to_string()).filter(|v| !v.trim().is_empty()),
        layout_row: Some(layout.to_string()).filter(|l| !l.trim().is_empty()),
        date_start: from,
        date_end: to,
        date_format: cfg.date_format.clone(),
        deliveries: deliveries.to_vec(),
        ship_point: None,
        export_target: Some(target),
        ignore_display_error: cfg.ignore_display_error,
        t_code: "ZVT11".to_string(),
    }
}

/// Map an export outcome to a step output (via [`outcome_to_output`]) and, when configured,
/// delete the export file after it loaded successfully. A file that failed to load stays.
pub fn finish_step(
    cfg: &MustgoSapConfig,
    tcode: &str,
    outcome: ExportOutcome,
) -> Result<StepOutput> {
    let file = outcome.path().cloned();
    let out = outcome_to_output(tcode, outcome, &cfg.raw)?;
    if cfg.delete_exports {
        if let Some(p) = file.filter(|p| !p.as_os_str().is_empty()) {
            if let Err(e) = std::fs::remove_file(&p) {
                println!("[mustgo] could not delete export {}: {e}", p.display());
            }
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------- the adapter

/// [`SapSteps`] over a live SAP GUI session.
pub struct RealSapSteps<'a> {
    pub session: &'a GuiSession,
    pub cfg: MustgoSapConfig,
    /// Clock used for file names and the VL06O placeholder dates (replaceable).
    pub now: fn() -> NaiveDateTime,
}

impl<'a> RealSapSteps<'a> {
    pub fn new(session: &'a GuiSession, cfg: MustgoSapConfig) -> Self {
        Self {
            session,
            cfg,
            now: || Local::now().naive_local(),
        }
    }

    fn target(&self, tcode: &str) -> (ExportTarget, NaiveDate) {
        let now = (self.now)();
        (export_target(&self.cfg, tcode, now), now.date())
    }
}

impl SapSteps for RealSapSteps<'_> {
    fn vt11(&mut self, date: &RunDate) -> Result<StepOutput> {
        let (target, _) = self.target("VT11");
        let params = vt11_params(&self.cfg, date, target);
        let outcome = crate::vt11::run_export_outcome(self.session, &params)
            .map_err(|e| anyhow!("vt11: SAP step failed: {e}"))?;
        finish_step(&self.cfg, "vt11", outcome)
    }

    fn vl06o(&mut self, shipments: &[String]) -> Result<StepOutput> {
        let (target, today) = self.target("VL06O");
        let params = vl06o_params(&self.cfg, shipments, target, today);
        let outcome = crate::vl06o::run_export_outcome(self.session, &params)
            .map_err(|e| anyhow!("vl06o: SAP step failed: {e}"))?;
        finish_step(&self.cfg, "vl06o", outcome)
    }

    fn zmdesnr_by_deliveries(&mut self, deliveries: &[String]) -> Result<StepOutput> {
        let (target, _) = self.target("ZMDESNR");
        let params = zmdesnr_by_deliveries_params(&self.cfg, deliveries, target);
        let outcome = crate::zmdesnr::run_export_outcome(self.session, &params)
            .map_err(|e| anyhow!("zmdesnr (by deliveries): SAP step failed: {e}"))?;
        finish_step(&self.cfg, "zmdesnr", outcome)
    }

    fn lt23(&mut self, transfer_orders: &[String]) -> Result<StepOutput> {
        let (target, _) = self.target("LT23");
        let params = lt23_params(&self.cfg, transfer_orders, target);
        let outcome = crate::lt23::run_export_outcome(self.session, &params)
            .map_err(|e| anyhow!("lt23: SAP step failed: {e}"))?;
        finish_step(&self.cfg, "lt23", outcome)
    }

    fn zmdesnr_by_deliveries_and_tos(
        &mut self,
        deliveries: &[String],
        transfer_orders: &[String],
    ) -> Result<StepOutput> {
        let (target, _) = self.target("ZMDESNR");
        let params =
            zmdesnr_by_deliveries_and_tos_params(&self.cfg, deliveries, transfer_orders, target);
        let outcome = crate::zmdesnr::run_export_outcome(self.session, &params)
            .map_err(|e| anyhow!("zmdesnr (by deliveries + TOs): SAP step failed: {e}"))?;
        finish_step(&self.cfg, "zmdesnr", outcome).context("zmdesnr (by deliveries + TOs)")
    }
}

impl MiscSteps for RealSapSteps<'_> {
    fn vt11(&mut self, date: &RunDate, variant: &str, layout: &str) -> Result<StepOutput> {
        InvSteps::vt11(self, date, variant, layout)
    }

    fn vl06o_by_shipments(&mut self, shipments: &[String], layout: &str) -> Result<StepOutput> {
        InvSteps::vl06o_by_shipments(self, shipments, layout)
    }
}

impl InvSteps for RealSapSteps<'_> {
    fn vt11(&mut self, date: &RunDate, variant: &str, layout: &str) -> Result<StepOutput> {
        let (target, _) = self.target("VT11");
        let params = inv_vt11_params(&self.cfg, date, variant, layout, target);
        let outcome = crate::vt11::run_export_outcome(self.session, &params)
            .map_err(|e| anyhow!("vt11: SAP step failed: {e}"))?;
        // the INV list layout (`scorecard`) has its own header aliases
        finish_step(&self.cfg, "vt11_inv", outcome)
    }

    fn vl06o_by_shipments(&mut self, shipments: &[String], layout: &str) -> Result<StepOutput> {
        let (target, today) = self.target("VL06O");
        let params = inv_vl06o_params(&self.cfg, shipments, layout, target, today);
        let outcome = crate::vl06o::run_export_outcome(self.session, &params)
            .map_err(|e| anyhow!("vl06o: SAP step failed: {e}"))?;
        finish_step(&self.cfg, "vl06o", outcome)
    }

    fn zmdetpc_summarised(
        &mut self,
        deliveries: &[String],
        date: &RunDate,
        variant: &str,
    ) -> Result<StepOutput> {
        let (target, _) = self.target("ZMDETPC");
        let params = zmdetpc_params(&self.cfg, deliveries, date, variant, target);
        let outcome = crate::zmdetpc::run_export_outcome(self.session, &params)
            .map_err(|e| anyhow!("zmdetpc: SAP step failed: {e}"))?;
        finish_step(&self.cfg, "zmdetpc", outcome)
    }
}

impl K2xxSteps for RealSapSteps<'_> {
    fn vt11(
        &mut self,
        date: &RunDate,
        variant: &str,
        layout: &str,
        monday_include_sunday: bool,
    ) -> Result<StepOutput> {
        let (target, _) = self.target("VT11");
        let params = k2xx_vt11_params(
            &self.cfg,
            date,
            variant,
            layout,
            monday_include_sunday,
            target,
        );
        let outcome = crate::vt11::run_export_outcome(self.session, &params)
            .map_err(|e| anyhow!("vt11: SAP step failed: {e}"))?;
        // same list layout (`scorecard`) as the INV reports
        finish_step(&self.cfg, "vt11_inv", outcome)
    }

    fn vl06o_by_shipments(
        &mut self,
        shipments: &[String],
        variant: &str,
        layout: &str,
    ) -> Result<StepOutput> {
        let (target, today) = self.target("VL06O");
        let params = VL06OParams {
            sap_variant_name: Some(variant.to_string()).filter(|v| !v.trim().is_empty()),
            ..inv_vl06o_params(&self.cfg, shipments, layout, target, today)
        };
        let outcome = crate::vl06o::run_export_outcome(self.session, &params)
            .map_err(|e| anyhow!("vl06o: SAP step failed: {e}"))?;
        finish_step(&self.cfg, "vl06o", outcome)
    }

    fn zvt11_closed(
        &mut self,
        deliveries: &[String],
        from: NaiveDate,
        to: NaiveDate,
        variant: &str,
        layout: &str,
    ) -> Result<StepOutput> {
        let (target, _) = self.target("ZVT11");
        let params = zvt11_closed_params(&self.cfg, deliveries, from, to, variant, layout, target);
        let outcome = crate::zvt11::run_closed_export_outcome(self.session, &params)
            .map_err(|e| anyhow!("zvt11 (closed): SAP step failed: {e}"))?;
        finish_step(&self.cfg, "zvt11", outcome)
    }

    fn zmdetpc_summarised(
        &mut self,
        deliveries: &[String],
        date: &RunDate,
        variant: &str,
    ) -> Result<StepOutput> {
        InvSteps::zmdetpc_summarised(self, deliveries, date, variant)
    }
}

impl RelabelSteps for RealSapSteps<'_> {
    fn zmdesnr_by_deliveries(
        &mut self,
        deliveries: &[String],
        variant: &str,
        layout: &str,
    ) -> Result<StepOutput> {
        let (target, _) = self.target("ZMDESNR");
        let params = relabel_by_deliveries_params(&self.cfg, deliveries, variant, layout, target);
        let outcome = crate::zmdesnr::run_export_outcome(self.session, &params)
            .map_err(|e| anyhow!("zmdesnr (by deliveries): SAP step failed: {e}"))?;
        finish_step(&self.cfg, "zmdesnr", outcome)
    }

    fn zmdesnr_history(
        &mut self,
        serials: &[String],
        variant: &str,
        layout: &str,
    ) -> Result<StepOutput> {
        let (target, _) = self.target("ZMDESNR");
        let params = relabel_history_params(&self.cfg, serials, variant, layout, target);
        let outcome = crate::zmdesnr::run_export_outcome(self.session, &params)
            .map_err(|e| anyhow!("zmdesnr (serial / history): SAP step failed: {e}"))?;
        finish_step(&self.cfg, "zmdesnr", outcome)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn d(m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, m, day).unwrap()
    }

    fn now() -> NaiveDateTime {
        d(11, 10).and_hms_opt(10, 15, 50).unwrap()
    }

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn targets_and_dirs() {
        let cfg = MustgoSapConfig {
            export_dir: "out".into(),
            ..MustgoSapConfig::default()
        };
        let t = export_target(&cfg, "VT11", now());
        assert_eq!(t.export_type, 1);
        assert_eq!(t.file_name, "20261110101550_VT11.txt");
        assert_eq!(
            PathBuf::from(&t.dir),
            Path::new("out").join("VT11"),
            "dir is <export_dir>/<TCODE>"
        );
    }

    #[test]
    fn vt11_maps_run_date_to_closed_range() {
        let cfg = MustgoSapConfig {
            date_format: Some("yyyy-mm-dd".into()),
            ..MustgoSapConfig::default()
        };
        let t = export_target(&cfg, "VT11", now());
        let single = RunDate {
            date1: d(11, 5),
            date2: None,
        };
        let p = vt11_params(&cfg, &single, t.clone());
        assert_eq!(p.closed_range, Some((d(11, 5), d(11, 5))));
        assert_eq!(p.sap_variant_name.as_deref(), Some("VT11_XDOCK_VARIANT"));
        assert!(p.layout_row.is_none() && !p.by_date && !p.by_created_date);
        assert!(p.created_range.is_none() && p.shipment_numbers.is_empty());
        assert!(p.ignore_display_error && !p.monday_include_sunday);
        assert_eq!(p.date_format.as_deref(), Some("yyyy-mm-dd"));
        assert_eq!(p.export_target, Some(t.clone()));
        let week = RunDate {
            date1: d(11, 1),
            date2: Some(d(11, 7)),
        };
        assert_eq!(
            vt11_params(&cfg, &week, t).closed_range,
            Some((d(11, 1), d(11, 7)))
        );
    }

    #[test]
    fn vl06o_params_have_no_variant_or_layout() {
        let cfg = MustgoSapConfig::default();
        let t = export_target(&cfg, "VL06O", now());
        let p = vl06o_params(&cfg, &s(&["1", "2"]), t.clone(), d(11, 10));
        assert_eq!(p.shipment_numbers, s(&["1", "2"]));
        assert!(p.sap_variant_name.is_none() && p.layout_row.is_none());
        assert_eq!(p.t_code, "VL06O");
        assert_eq!(p.export_target, Some(t));
    }

    #[test]
    fn zmdesnr_params() {
        let cfg = MustgoSapConfig::default();
        let t = export_target(&cfg, "ZMDESNR", now());
        let p = zmdesnr_by_deliveries_params(&cfg, &s(&["8"]), t.clone());
        assert_eq!(
            p.sap_variant_name.as_deref(),
            Some("ZMDESNR_MUSTGO_VARIANT")
        );
        assert_eq!(p.layout_row.as_deref(), Some("ZMDESNR_MUSTGO_LAYOUT"));
        assert_eq!(p.tab_number, Some(2));
        assert!(p.scroll_paste && p.transfer_numbers.is_empty());
        assert!(p.serial_number.is_none() && p.bin_filter.is_none());
        assert!(p.additional_params.pre_export_back.is_none());
        let q = zmdesnr_by_deliveries_and_tos_params(&cfg, &s(&["8"]), &s(&["T1", "T2"]), t);
        assert_eq!(q.delivery_numbers, s(&["8"]));
        assert_eq!(q.transfer_numbers, s(&["T1", "T2"]));
        assert_eq!(q.layout_row, p.layout_row);
    }

    #[test]
    fn lt23_params_mapping() {
        let cfg = MustgoSapConfig::default();
        let t = export_target(&cfg, "LT23", now());
        let p = lt23_params(&cfg, &s(&["5"]), t);
        assert_eq!(p.warehouse, "WH01");
        assert!(p.confirmed_only && !p.save_layout);
        assert_eq!(p.layout_row.as_deref(), Some("LT23_MUSTGO_LAYOUT"));
        assert!(p.create_layout_columns.is_none());
    }

    #[test]
    fn inv_params() {
        let cfg = MustgoSapConfig {
            date_format: Some("yyyy-mm-dd".into()),
            ..MustgoSapConfig::default()
        };
        let single = RunDate {
            date1: d(11, 5),
            date2: None,
        };
        let t = export_target(&cfg, "VT11", now());
        let p = inv_vt11_params(
            &cfg,
            &single,
            "VT11_XDOCK_VARIANT",
            "VT11_LAYOUT",
            t.clone(),
        );
        assert_eq!(p.sap_variant_name.as_deref(), Some("VT11_XDOCK_VARIANT"));
        assert_eq!(p.layout_row.as_deref(), Some("VT11_LAYOUT"));
        assert_eq!(p.closed_range, Some((d(11, 5), d(11, 5))));
        assert_eq!(p.export_target, Some(t.clone()));
        assert!(inv_vt11_params(&cfg, &single, "V", " ", t.clone())
            .layout_row
            .is_none());

        let v = inv_vl06o_params(
            &cfg,
            &s(&["1", "2"]),
            "VL06O_XDOCK_LAYOUT",
            t.clone(),
            d(11, 10),
        );
        assert_eq!(v.layout_row.as_deref(), Some("VL06O_XDOCK_LAYOUT"));
        assert!(v.sap_variant_name.is_none());
        assert_eq!(v.shipment_numbers, s(&["1", "2"]));

        let week = RunDate {
            date1: d(11, 1),
            date2: Some(d(11, 7)),
        };
        let z = zmdetpc_params(&cfg, &s(&["8", "9"]), &week, "ZMDETPC_VARIANT", t.clone());
        assert_eq!(z.variant.as_deref(), Some("ZMDETPC_VARIANT"));
        // week 11-01..11-07 buffered by 3 days on each side
        assert_eq!((z.date_start, z.date_end), (d(10, 29), d(11, 10)));
        assert_eq!(z.date_format.as_deref(), Some("yyyy-mm-dd"));
        assert_eq!(z.deliveries, s(&["8", "9"]));
        assert_eq!(z.export_target, Some(t));
        let z1 = zmdetpc_params(
            &cfg,
            &s(&["8"]),
            &single,
            "",
            export_target(&cfg, "ZMDETPC", now()),
        );
        assert!(z1.variant.is_none());
        // a single date gets no default buffer
        assert_eq!((z1.date_start, z1.date_end), (d(11, 5), d(11, 5)));
    }

    #[test]
    fn k2xx_params() {
        let cfg = MustgoSapConfig {
            date_format: Some("yyyy-mm-dd".into()),
            date_range_buffer: Some(5),
            ..MustgoSapConfig::default()
        };
        let monday = RunDate {
            date1: d(11, 2),
            date2: None,
        };
        let t = export_target(&cfg, "VT11", now());
        let p = k2xx_vt11_params(
            &cfg,
            &monday,
            "VT11_K2XX_VARIANT",
            "VT11_LAYOUT",
            true,
            t.clone(),
        );
        assert!(p.monday_include_sunday);
        assert_eq!(p.sap_variant_name.as_deref(), Some("VT11_K2XX_VARIANT"));
        assert_eq!(p.layout_row.as_deref(), Some("VT11_LAYOUT"));
        // the Monday becomes Sunday..Monday inside the VT11 step
        assert_eq!(
            crate::vt11::effective_start_date(d(11, 2), p.monday_include_sunday),
            d(11, 1)
        );
        assert!(!k2xx_vt11_params(&cfg, &monday, "V", "L", false, t.clone()).monday_include_sunday);

        // the window is passed through as given: no date_range_buffer on top
        let z = zvt11_closed_params(
            &cfg,
            &s(&["8", "9"]),
            d(7, 28),
            NaiveDate::from_ymd_opt(2027, 2, 13).unwrap(),
            "ZVT11_K2XX_VARIANT",
            "ZVT11_K2XX_LAYOUT",
            t.clone(),
        );
        assert_eq!(z.variant.as_deref(), Some("ZVT11_K2XX_VARIANT"));
        assert_eq!(z.layout_row.as_deref(), Some("ZVT11_K2XX_LAYOUT"));
        assert_eq!(z.date_start, d(7, 28));
        assert_eq!(z.date_end, NaiveDate::from_ymd_opt(2027, 2, 13).unwrap());
        assert_eq!(z.date_format.as_deref(), Some("yyyy-mm-dd"));
        assert_eq!(z.deliveries, s(&["8", "9"]));
        assert!(z.ship_point.is_none());
        assert_eq!(z.export_target, Some(t.clone()));
        let blank = zvt11_closed_params(&cfg, &s(&["8"]), d(1, 1), d(1, 2), " ", "", t);
        assert!(blank.variant.is_none() && blank.layout_row.is_none());
    }

    #[test]
    fn relabel_params() {
        let cfg = MustgoSapConfig::default();
        let t = export_target(&cfg, "ZMDESNR", now());
        let p = relabel_by_deliveries_params(
            &cfg,
            &s(&["8", "9"]),
            "ZMDESNR_RELABEL_VARIANT",
            "ZMDESNR_RELABEL_LAYOUT",
            t.clone(),
        );
        assert_eq!(
            p.sap_variant_name.as_deref(),
            Some("ZMDESNR_RELABEL_VARIANT")
        );
        assert_eq!(p.layout_row.as_deref(), Some("ZMDESNR_RELABEL_LAYOUT"));
        assert_eq!(p.tab_number, Some(2));
        assert!(p.scroll_paste && p.history_serials.is_empty());
        assert_eq!(p.delivery_numbers, s(&["8", "9"]));
        assert_eq!(p.export_target, Some(t.clone()));
        let h = relabel_history_params(
            &cfg,
            &s(&["5", "6"]),
            "ZMDESNR_RELABEL_VARIANT",
            "ZMDESNR_RELABEL_LAYOUT",
            t,
        );
        assert_eq!(h.tab_number, Some(1));
        assert_eq!(h.history_serials, s(&["5", "6"]));
        assert!(h.delivery_numbers.is_empty() && h.export_target.is_some());
        assert_eq!(h.layout_row.as_deref(), Some("ZMDESNR_RELABEL_LAYOUT"));
    }

    #[test]
    fn default_buffer_is_3_for_zmdetpc_weeks_only() {
        let cfg = MustgoSapConfig::default();
        assert_eq!(cfg.date_range_buffer, None);
        let single = RunDate {
            date1: d(11, 5),
            date2: None,
        };
        let week = RunDate {
            date1: d(11, 1),
            date2: Some(d(11, 7)),
        };
        let other_range = RunDate {
            date1: d(11, 2),
            date2: Some(d(11, 12)),
        };
        // ZMDETPC: week -> 3 days each side; single date and non-week range -> none
        assert_eq!(zmdetpc_range(&cfg, &week), (d(10, 29), d(11, 10)));
        assert_eq!(zmdetpc_range(&cfg, &single), (d(11, 5), d(11, 5)));
        assert_eq!(zmdetpc_range(&cfg, &other_range), (d(11, 2), d(11, 12)));
        // every other tcode: no default buffer, week or not
        assert_eq!(vt11_range(&cfg, &week), (d(11, 1), d(11, 7)));
        assert_eq!(vt11_range(&cfg, &single), (d(11, 5), d(11, 5)));
        assert_eq!(buffer_days(&cfg, &week, "VL06O"), 0);
        assert_eq!(buffer_days(&cfg, &week, "zmdetpc"), 3);
        // a week crossing the year boundary
        let ny = RunDate {
            date1: NaiveDate::from_ymd_opt(2026, 12, 27).unwrap(),
            date2: Some(NaiveDate::from_ymd_opt(2027, 1, 2).unwrap()),
        };
        assert_eq!(
            zmdetpc_range(&cfg, &ny),
            (
                NaiveDate::from_ymd_opt(2026, 12, 24).unwrap(),
                NaiveDate::from_ymd_opt(2027, 1, 5).unwrap()
            )
        );
    }

    #[test]
    fn explicit_buffer_applies_to_every_tcode_range() {
        let single = RunDate {
            date1: d(11, 5),
            date2: None,
        };
        let week = RunDate {
            date1: d(11, 1),
            date2: Some(d(11, 7)),
        };
        let mut cfg = MustgoSapConfig {
            date_range_buffer: Some(5),
            ..MustgoSapConfig::default()
        };
        // explicit value: also single dates, also VT11
        assert_eq!(zmdetpc_range(&cfg, &single), (d(10, 31), d(11, 10)));
        assert_eq!(vt11_range(&cfg, &single), (d(10, 31), d(11, 10)));
        assert_eq!(vt11_range(&cfg, &week), (d(10, 27), d(11, 12)));
        let t = export_target(&cfg, "VT11", now());
        assert_eq!(
            vt11_params(&cfg, &single, t).closed_range,
            Some((d(10, 31), d(11, 10)))
        );
        // explicit 0 switches the ZMDETPC week default off; a negative value counts as 0
        cfg.date_range_buffer = Some(0);
        assert_eq!(zmdetpc_range(&cfg, &week), (d(11, 1), d(11, 7)));
        cfg.date_range_buffer = Some(-2);
        assert_eq!(zmdetpc_range(&cfg, &week), (d(11, 1), d(11, 7)));
    }

    fn temp_file(tag: &str, content: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("mustgo_adapters_{tag}_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("20261110102002_LT23.txt");
        std::fs::write(&p, content).unwrap();
        p
    }

    const LT23_SAMPLE: &str = "Warehouse No.\t\t\tWH01\tExample\r\n\r\n\r\n\tDelivery\tTO Number\t\t\tCreated On\tTime\tUser\tPlnt\tMaterial\tTyp\tSource Bin\tSource target qty\tCS\r\n\r\n\t13000917\t0130000089\t\t\t2026-11-05\t06:26:01\tNAAAA4\tPL33\t13000723\tSKD\t75GM-2-010\t         144.000\r\n";

    #[test]
    fn finish_step_loads_and_deletes() {
        let p = temp_file("del", LT23_SAMPLE);
        let cfg = MustgoSapConfig::default();
        let out = finish_step(&cfg, "lt23", ExportOutcome::File(p.clone())).unwrap();
        let StepOutput::Table(t) = out else {
            panic!("table expected")
        };
        assert_eq!(t.nrows(), 1);
        assert_eq!(
            t.rows[0][t.col("transfer_order_number").unwrap()],
            "130000089"
        );
        assert_eq!(t.rows[0][t.col("source_target_qty").unwrap()], "144");
        assert!(!p.exists(), "export deleted after a successful load");
        let _ = std::fs::remove_dir_all(p.parent().unwrap());
    }

    #[test]
    fn finish_step_keeps_file_when_configured_or_when_load_fails() {
        let p = temp_file("keep", LT23_SAMPLE);
        let cfg = MustgoSapConfig {
            delete_exports: false,
            ..MustgoSapConfig::default()
        };
        finish_step(&cfg, "lt23", ExportOutcome::File(p.clone())).unwrap();
        assert!(p.exists());
        let _ = std::fs::remove_dir_all(p.parent().unwrap());

        let bad = temp_file("bad", "x");
        std::fs::write(&bad, [0xFFu8, 0xFE, 0x00]).unwrap();
        let del = MustgoSapConfig::default();
        assert!(finish_step(&del, "lt23", ExportOutcome::File(bad.clone())).is_err());
        assert!(
            bad.exists(),
            "a file that failed to load stays for diagnosis"
        );
        let _ = std::fs::remove_dir_all(bad.parent().unwrap());
    }

    #[test]
    fn finish_step_passes_no_data_and_errors() {
        let cfg = MustgoSapConfig::default();
        assert!(matches!(
            finish_step(&cfg, "vt11", ExportOutcome::NoData("none".into())).unwrap(),
            StepOutput::NoData(m) if m == "none"
        ));
        assert!(finish_step(&cfg, "vt11", ExportOutcome::Error("boom".into())).is_err());
    }
}
