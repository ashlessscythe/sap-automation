//! ZMDETPC (pallet count), summarised flow only, spec 3.6.
//!
//! Config-agnostic: everything arrives through [`ZMDETPCParams`]; no `SapConfig`, no CLI
//! overrides, no clock. The export target is built by the caller. Output columns of the
//! summarised list: `Delivery`, `Pallet`, `Child` (spec 3.6 / 9.6).
//!
//! NOT run against SAP by the author. Every control id below comes from the VBA spec only
//! (nothing in this repo confirms any of them); see "Live verification checklist (xdock)" in
//! `docs/abc_headless_plan.md`. UNVERIFIED live:
//! * selection screen controls `wnd[0]/usr/ctxtS_DATE-LOW`, `ctxtS_DATE-HIGH`,
//!   `btn%_S_VBELN_%_APP_%-VALU_PUSH`, `radR_SUMM`, `radR_COMPL`,
//! * the value cells of the delivery multi-selection (`ctxt` or `txt`; both are tried) and the
//!   `wnd[2]` "Yes" popup after pasting (`wnd[2]/usr/btnBUTTON_1`),
//! * the variant dialog (shared `variant_select`, button `btn[17]`) and the variant name
//!   (`EPDC_TONY` comes from the toyota row of the spec, 5.2 row 28),
//! * the refresh sequence `radR_COMPL` then `radR_SUMM` before execute (spec 3.6),
//! * that the selection screen keeps `radR_SUMM` when nothing is found, and the popup text in
//!   `wnd[1]/usr/txtMESSTXT1` (read by the shared screen classifier),
//! * the local file export menu `menu[0]/menu[3]/menu[2]` (`menu[1]` is the spreadsheet export
//!   and is not used; a dialog that is not titled "save list in file" is rejected).
//!
//! Not implemented (v2): the detail flow (`radR_DETA`, `.txt` export), `S_ADATE` (pallet pick
//! date) and the layout button. The spec's "delete a non-numeric first row" is not needed
//! because only data values are pasted (no header row, spec P3).
#![allow(dead_code)]

use crate::utils::export_outcome::{
    classify_after_execute, classify_screen, ClassifyOptions, ExportOutcome,
};
use chrono::NaiveDate;
use sap_scripting::*;
use windows::core::Result;

use crate::utils::sap_ctrl_utils::paste_values_with_scroll;
use crate::utils::sap_export_utils::{export_to_target, ExportTarget};
use crate::utils::sap_tcode_utils::{assert_tcode, variant_select};
use crate::utils::sap_wnd_utils::answer_popup_yes;

const MULTI_TABLE_ID: &str =
    "tabsTAB_STRIP/tabpSIVA/ssubSCREEN_HEADER:SAPLALDB:3010/tblSAPLALDBSINGLE";
// `menu[3]/menu[1]` is the spreadsheet export ("Select Spreadsheet"), `menu[3]/menu[2]` is
// "Local file" (owner's recording, 2026-11-10); only the local file entry is tried.
const LOCAL_FILE_MENUS: &[&str] = &["wnd[0]/mbar/menu[0]/menu[3]/menu[2]"];

/// ZMDETPC (summarised) parameters.
#[derive(Debug, Clone)]
pub struct ZMDETPCParams {
    /// Variant of the selection screen (`EPDC_TONY` in the macro for xdock/toyota/k2xx).
    pub variant: Option<String>,
    /// Delivery date range (`S_DATE-LOW` / `-HIGH`); equal for a single date.
    pub date_start: NaiveDate,
    pub date_end: NaiveDate,
    /// Config-style date format typed into SAP (`mm/dd/yyyy`, `yyyy-mm-dd`, ...); `None` =
    /// `mm/dd/yyyy`.
    pub date_format: Option<String>,
    /// Distinct, blank-free deliveries (the caller prepares the list).
    pub deliveries: Vec<String>,
    /// Headless target; required (this step has no legacy flow).
    pub export_target: Option<ExportTarget>,
    pub ignore_display_error: bool,
    pub t_code: String,
}

impl Default for ZMDETPCParams {
    fn default() -> Self {
        let today = NaiveDate::from_ymd_opt(1970, 1, 1).expect("valid date");
        Self {
            variant: None,
            date_start: today,
            date_end: today,
            date_format: None,
            deliveries: Vec::new(),
            export_target: None,
            ignore_display_error: false,
            t_code: "ZMDETPC".to_string(),
        }
    }
}

/// The two date strings typed into `S_DATE-LOW` / `-HIGH` (pure, unit tested). `-HIGH` is
/// never blank (spec: "ensure `S_DATE-HIGH` filled"), a single date repeats the low value.
pub fn date_fields(params: &ZMDETPCParams) -> (String, String) {
    let fmt = params.date_format.as_deref().unwrap_or("mm/dd/yyyy");
    let low = crate::utils::config_ops::format_date_with(fmt, params.date_start);
    let end = if params.date_end < params.date_start {
        params.date_start
    } else {
        params.date_end
    };
    let high = crate::utils::config_ops::format_date_with(fmt, end);
    (low, high)
}

fn fail(msg: &str) -> Result<ExportOutcome> {
    Ok(ExportOutcome::Error(format!("ZMDETPC: {msg}")))
}

pub(crate) fn press(session: &GuiSession, id: &str) -> Result<bool> {
    match session
        .find_by_id(id.to_string())
        .ok()
        .and_then(|c| c.downcast::<GuiButton>())
    {
        Some(b) => {
            b.press()?;
            Ok(true)
        }
        None => Ok(false),
    }
}

pub(crate) fn set_ctext(session: &GuiSession, id: &str, text: &str) -> Result<bool> {
    match session
        .find_by_id(id.to_string())
        .ok()
        .and_then(|c| c.downcast::<GuiCTextField>())
    {
        Some(f) => {
            f.set_text(text.to_string())?;
            Ok(true)
        }
        None => Ok(false),
    }
}

pub(crate) fn select_radio(session: &GuiSession, id: &str) -> Result<bool> {
    match session
        .find_by_id(id.to_string())
        .ok()
        .and_then(|c| c.downcast::<GuiRadioButton>())
    {
        Some(r) => {
            r.select()?;
            Ok(true)
        }
        None => Ok(false),
    }
}

pub(crate) fn close_wnd1(session: &GuiSession) {
    if let Ok(w) = session.find_by_id("wnd[1]".to_string()) {
        if let Some(f) = w.downcast::<GuiFrameWindow>() {
            let _ = f.close();
        }
    }
}

pub(crate) fn row0_filled(session: &GuiSession) -> bool {
    ["ctxt", "txt"].iter().any(|prefix| {
        session
            .find_by_id(format!(
                "wnd[1]/usr/{MULTI_TABLE_ID}/{prefix}RSCSEL_255-SLOW_I[1,0]"
            ))
            .ok()
            .map(|c| {
                c.downcast::<GuiCTextField>()
                    .and_then(|t| t.text().ok())
                    .or_else(|| c.downcast::<GuiTextField>().and_then(|t| t.text().ok()))
                    .is_some_and(|t| !t.trim().is_empty())
            })
            .unwrap_or(false)
    })
}

/// Paste the deliveries into `S_VBELN`; Ok(false) when it could not be done.
fn paste_deliveries(session: &GuiSession, deliveries: &[String]) -> Result<bool> {
    paste_multi(
        session,
        "ZMDETPC",
        "wnd[0]/usr/btn%_S_VBELN_%_APP_%-VALU_PUSH",
        deliveries,
    )
}

/// Open the multi-selection window behind `button_id`, clear it, paste `values` with scrolling,
/// answer the warnings and close it (shared with the ZVT11 Closed step). Ok(false) = failed.
pub(crate) fn paste_multi(
    session: &GuiSession,
    tcode: &str,
    button_id: &str,
    deliveries: &[String],
) -> Result<bool> {
    if !press(session, button_id)? {
        println!("{tcode}: multi-selection button not found ({button_id})");
        return Ok(false);
    }
    if let Ok(w) = session.find_by_id("wnd[1]".to_string()) {
        if let Some(m) = w.downcast::<GuiModalWindow>() {
            m.send_v_key(16)?; // clear previous entries
        }
    }
    let mut pasted = false;
    for prefix in ["ctxt", "txt"] {
        if paste_values_with_scroll(session, 1, MULTI_TABLE_ID, deliveries, 7, prefix)? {
            pasted = true;
            break;
        }
    }
    if !pasted {
        println!("{tcode}: could not paste the values");
        return Ok(false);
    }
    let _ = answer_popup_yes(session, 2, 5)?;
    if !row0_filled(session) {
        println!("{tcode}: paste not visible in the multi-selection window");
        close_wnd1(session);
        return Ok(false);
    }
    if let Ok(w) = session.find_by_id("wnd[1]".to_string()) {
        if let Some(m) = w.downcast::<GuiModalWindow>() {
            m.send_v_key(8)?; // close multi-window
        }
    }
    let _ = answer_popup_yes(session, 2, 5)?;
    Ok(true)
}

/// Run ZMDETPC summarised for `params.deliveries` (spec 3.6) and export to the target.
pub fn run_export_outcome(session: &GuiSession, params: &ZMDETPCParams) -> Result<ExportOutcome> {
    println!("Running ZMDETPC export...");
    if params.deliveries.is_empty() {
        return fail("no deliveries given");
    }
    let Some(target) = &params.export_target else {
        return fail("no export target (this step only runs headless)");
    };
    if !assert_tcode(session, "ZMDETPC", Some(0))? {
        return fail("could not start transaction ZMDETPC");
    }

    // variant first: it may carry date/radio defaults that the steps below overwrite
    if let Some(v) = params.variant.as_deref().filter(|v| !v.trim().is_empty()) {
        if !variant_select(session, &params.t_code, v)? {
            return fail(&format!("variant '{v}' could not be selected"));
        }
    }

    // dates
    let (low, high) = date_fields(params);
    if !set_ctext(session, "wnd[0]/usr/ctxtS_DATE-LOW", &low)? {
        return fail("date field ctxtS_DATE-LOW not found");
    }
    if !set_ctext(session, "wnd[0]/usr/ctxtS_DATE-HIGH", &high)? {
        return fail("date field ctxtS_DATE-HIGH not found");
    }

    // deliveries
    if !paste_deliveries(session, &params.deliveries)? {
        return fail("could not paste the deliveries");
    }

    // summarised, with the macro's refresh sequence (complete, then summarised again)
    if !select_radio(session, "wnd[0]/usr/radR_SUMM")? {
        return fail("radio button radR_SUMM not found");
    }
    if !set_ctext(session, "wnd[0]/usr/ctxtS_DATE-HIGH", &high)? {
        return fail("date field ctxtS_DATE-HIGH not found");
    }
    if !select_radio(session, "wnd[0]/usr/radR_COMPL")? {
        return fail("radio button radR_COMPL not found");
    }
    if !select_radio(session, "wnd[0]/usr/radR_SUMM")? {
        return fail("radio button radR_SUMM not found (second time)");
    }

    // execute
    if !press(session, "wnd[0]/tbar[1]/btn[8]")? {
        return fail("execute button btn[8] not found");
    }
    let opts = ClassifyOptions {
        ignore_display_error: params.ignore_display_error,
    };
    if let Some(o) = classify_after_execute(session, "ZMDETPC", "/usr/radR_SUMM", opts)? {
        println!("ZMDETPC outcome after execute: {o:?}");
        return Ok(o);
    }
    if let Some(o) = classify_screen(session, opts)? {
        return Ok(o);
    }

    export_to_target(session, "ZMDETPC", LOCAL_FILE_MENUS, target)
}

/// Boolean wrapper like the other tcodes; see [`run_export_outcome`].
pub fn run_export(session: &GuiSession, params: &ZMDETPCParams) -> Result<bool> {
    Ok(run_export_outcome(session, params)?.into_bool())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, m, day).unwrap()
    }

    #[test]
    fn defaults() {
        let p = ZMDETPCParams::default();
        assert_eq!(p.t_code, "ZMDETPC");
        assert!(p.export_target.is_none() && p.deliveries.is_empty() && p.variant.is_none());
    }

    #[test]
    fn date_fields_single_and_range() {
        let p = ZMDETPCParams {
            date_start: d(11, 5),
            date_end: d(11, 5),
            ..ZMDETPCParams::default()
        };
        assert_eq!(date_fields(&p), ("11/05/2026".into(), "11/05/2026".into()));
        let r = ZMDETPCParams {
            date_start: d(11, 1),
            date_end: d(11, 7),
            date_format: Some("yyyy-mm-dd".into()),
            ..ZMDETPCParams::default()
        };
        assert_eq!(date_fields(&r), ("2026-11-01".into(), "2026-11-07".into()));
        // an inverted range never leaves HIGH before LOW
        let bad = ZMDETPCParams {
            date_start: d(11, 7),
            date_end: d(11, 1),
            ..ZMDETPCParams::default()
        };
        assert_eq!(date_fields(&bad).0, date_fields(&bad).1);
    }
}
