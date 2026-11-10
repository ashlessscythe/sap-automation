//! Real SAP adapter for the `mustgo` runner: [`RealSapSteps`] implements
//! [`super::runner::SapSteps`] on top of the tcode steps (`vt11`, `vl06o`, `zmdesnr`, `lt23`).
//!
//! Nothing here reads config or CLI overrides: the caller builds a [`MustgoSapConfig`]
//! (defaults below) and passes the session. Every step forces "text with tabs"
//! (export type 1) because [`super::raw`] only parses that format, writes the file to
//! `<export_dir>/<TCODE>/<yyyymmddhhmmss>_<TCODE>.txt`, loads it, and deletes it afterwards when
//! `delete_exports` is set (a file that failed to load is kept for diagnosis).
//!
//! NOT run against SAP by the author. See "Live verification checklist (mustgo)" in
//! `docs/abc_headless_plan.md` for every control id / behaviour that is unverified.
#![allow(dead_code)]

use super::raw::RawOptions;
use super::run::RunDate;
use super::runner::{outcome_to_output, SapSteps, StepOutput};
use crate::lt23::LT23Params;
use crate::utils::export_outcome::ExportOutcome;
use crate::utils::sap_export_utils::{export_file_name, ExportTarget};
use crate::vl06o::VL06OParams;
use crate::vt11::VT11Params;
use crate::zmdesnr::ZMDESNRParams;
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
}

impl Default for MustgoSapConfig {
    fn default() -> Self {
        Self {
            vt11_variant: "XD_ALL_EPDC_".to_string(),
            zmdesnr_variant: "mg_k2xx".to_string(),
            zmdesnr_layout: "mg_view".to_string(),
            lt23_layout: "to_mg".to_string(),
            lt23_create_layout_columns: None,
            warehouse: "E01".to_string(),
            export_dir: "reports".to_string(),
            date_format: None,
            ignore_display_error: true,
            delete_exports: true,
            raw: RawOptions::default(),
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

/// `(start, end)` the VT11 closed-date range for a run date (single date: both equal).
pub fn vt11_range(date: &RunDate) -> (NaiveDate, NaiveDate) {
    date.as_range()
}

/// Step 9: VT11 with the variant, the closed date (`K_DATEN`) range, no layout (export as-is).
pub fn vt11_params(cfg: &MustgoSapConfig, date: &RunDate, target: ExportTarget) -> VT11Params {
    let (start, end) = vt11_range(date);
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
        assert_eq!(p.sap_variant_name.as_deref(), Some("XD_ALL_EPDC_"));
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
        assert_eq!(p.sap_variant_name.as_deref(), Some("mg_k2xx"));
        assert_eq!(p.layout_row.as_deref(), Some("mg_view"));
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
        assert_eq!(p.warehouse, "E01");
        assert!(p.confirmed_only && !p.save_layout);
        assert_eq!(p.layout_row.as_deref(), Some("to_mg"));
        assert!(p.create_layout_columns.is_none());
    }

    fn temp_file(tag: &str, content: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("mustgo_adapters_{tag}_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("20261110102002_LT23.txt");
        std::fs::write(&p, content).unwrap();
        p
    }

    const LT23_SAMPLE: &str = "Warehouse No.\t\t\tE01\tEl Paso\r\n\r\n\r\n\tDelivery\tTO Number\t\t\tCreated On\tTime\tUser\tPlnt\tMaterial\tTyp\tSource Bin\tSource target qty\tCS\r\n\r\n\t85650366\t0209193416\t\t\t2026-11-05\t06:26:01\tHZTDX4\tFV33\t36067872\tSKD\t75EP-2-010\t         144.000\r\n";

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
            "209193416"
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
