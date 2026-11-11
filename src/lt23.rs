//! LT23 (transfer orders: list of resident documents), spec 3.5.
//!
//! Config-agnostic: everything arrives through [`LT23Params`]; no `SapConfig`, no CLI overrides
//! and no timestamp/config lookups happen here (the export target is built by the caller).
//!
//! UNVERIFIED live (no source in this repo confirms these, they come from the VBA spec):
//! * control ids `wnd[0]/usr/ctxtT1_LGNUM`, `wnd[0]/usr/radT1_QUITA`,
//!   `wnd[0]/usr/btn%_T1_TANUM_%_APP_%-VALU_PUSH` (the `/usr/` prefix is assumed, the selection
//!   screen could wrap them in a subscreen),
//! * the value cells of the TO multi-selection (`ctxt` or `txt`; both are tried),
//! * the `wnd[2]` "Yes" popup after pasting (`wnd[2]/usr/btnBUTTON_1`),
//! * that the selection screen has no default date/status limits that hide TOs,
//! * the layout chooser grid `wnd[1]/usr/cntlGRID/shellcont/shell` with a `VARIANT` column,
//! * layout creation (`btn[32]` + `setup_layout_li` on [`LI_BASE_OBJECT`]),
//! * the local file export menu `menu[0]/menu[1]/menu[2]` (sibling of the Excel export
//!   `menu[0]/menu[1]/menu[1]`),
//! * that the selection screen (`ctxtT1_LGNUM`) stays on screen when nothing is found.
#![allow(dead_code)]

use crate::utils::export_outcome::{classify_after_execute, ClassifyOptions, ExportOutcome};
use sap_scripting::*;
use windows::core::Result;

use crate::utils::sap_ctrl_utils::{exist_ctrl, paste_values_with_scroll};
use crate::utils::sap_export_utils::{export_to_target, ExportTarget};
use crate::utils::sap_file_utils::{get_tcode_file_path, save_sap_file};
use crate::utils::sap_tcode_utils::assert_tcode;
use crate::utils::sap_wnd_utils::{answer_popup_yes, check_export_window};
use crate::utils::setup_layout_li_utils::setup_layout_li;

/// Layout `to_mg_lt23_layoutrows` of the macro (spec 3.5), used when the layout must be created.
pub const TO_MG_COLUMNS: &[&str] = &[
    "Delivery",
    "Transfer Order Number",
    "Creation Date",
    "Creation time",
    "User",
    "Plant",
    "Material",
    "Source Storage Type",
    "Source Storage Bin",
    "Source target qty",
    "Confirmation status",
];

/// Base object of the LI-style layout editor (spec P4, VT11 row; assumed identical for LT23).
pub const LI_BASE_OBJECT: &str = "/usr/tabsTS_LINES/tabpLI01/ssubSUB810:SAPLSKBH:0810";

const LAYOUT_GRID: &str = "wnd[1]/usr/cntlGRID/shellcont/shell";
const MULTI_TABLE_ID: &str =
    "tabsTAB_STRIP/tabpSIVA/ssubSCREEN_HEADER:SAPLALDB:3010/tblSAPLALDBSINGLE";
const LOCAL_FILE_MENUS: &[&str] = &["wnd[0]/mbar/menu[0]/menu[1]/menu[2]"];

/// LT23 export parameters.
#[derive(Debug, Clone)]
pub struct LT23Params {
    pub warehouse: String,
    /// Distinct, blank-free transfer order numbers (the caller prepares the list).
    pub transfer_orders: Vec<String>,
    /// `radT1_QUITA`: confirmed transfer orders only.
    pub confirmed_only: bool,
    /// Layout to select (e.g. `to_mg`).
    pub layout_row: Option<String>,
    /// Column titles used to create the layout when it does not exist; `None` = fail instead.
    pub create_layout_columns: Option<Vec<String>>,
    /// Save a created layout as a variant (otherwise it is only applied on the fly).
    pub save_layout: bool,
    /// Headless target. `None` = legacy Excel export like LX03 (uses config for the file path).
    pub export_target: Option<ExportTarget>,
    pub ignore_display_error: bool,
    pub t_code: String,
}

impl Default for LT23Params {
    fn default() -> Self {
        Self {
            warehouse: "WH01".to_string(),
            transfer_orders: Vec::new(),
            confirmed_only: true,
            layout_row: None,
            create_layout_columns: None,
            save_layout: false,
            export_target: None,
            ignore_display_error: false,
            t_code: "LT23".to_string(),
        }
    }
}

/// Index of the layout called `wanted` (trimmed, case-insensitive) in the chooser's names.
pub fn find_layout_row(names: &[String], wanted: &str) -> Option<usize> {
    let w = wanted.trim();
    if w.is_empty() {
        return None;
    }
    names.iter().position(|n| n.trim().eq_ignore_ascii_case(w))
}

fn fail(msg: &str) -> Result<ExportOutcome> {
    Ok(ExportOutcome::Error(format!("LT23: {msg}")))
}

fn press(session: &GuiSession, id: &str) -> Result<bool> {
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

fn close_wnd1(session: &GuiSession) {
    if let Ok(w) = session.find_by_id("wnd[1]".to_string()) {
        if let Some(f) = w.downcast::<GuiFrameWindow>() {
            let _ = f.close();
        }
    }
}

fn row0_filled(session: &GuiSession) -> bool {
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

/// Paste the TOs into `T1_TANUM`; Ok(false) when it could not be done.
fn paste_transfer_orders(session: &GuiSession, tos: &[String]) -> Result<bool> {
    if !press(session, "wnd[0]/usr/btn%_T1_TANUM_%_APP_%-VALU_PUSH")? {
        println!("LT23: TO multi-selection button (T1_TANUM) not found");
        return Ok(false);
    }
    if let Ok(w) = session.find_by_id("wnd[1]".to_string()) {
        if let Some(m) = w.downcast::<GuiModalWindow>() {
            m.send_v_key(16)?; // clear previous entries
        }
    }
    let mut pasted = false;
    for prefix in ["ctxt", "txt"] {
        if paste_values_with_scroll(session, 1, MULTI_TABLE_ID, tos, 7, prefix)? {
            pasted = true;
            break;
        }
    }
    if !pasted {
        println!("LT23: could not paste transfer orders");
        return Ok(false);
    }
    let _ = answer_popup_yes(session, 2, 5)?;
    if !row0_filled(session) {
        println!("LT23: transfer order paste not visible in the multi-selection window");
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

/// Select (or create) the layout. Ok(false) when it could not be applied.
fn select_layout(session: &GuiSession, params: &LT23Params, layout: &str) -> Result<bool> {
    if !press(session, "wnd[0]/tbar[1]/btn[33]")? {
        println!("LT23: layout button btn[33] not found");
        return Ok(false);
    }
    let grid = session
        .find_by_id(LAYOUT_GRID.to_string())
        .ok()
        .and_then(|c| c.downcast::<GuiGridView>());
    let Some(grid) = grid else {
        println!("LT23: layout chooser grid {LAYOUT_GRID} not found");
        close_wnd1(session);
        return Ok(false);
    };
    let rows = grid.row_count()?;
    let names: Vec<String> = (0..rows)
        .map(|i| {
            grid.get_cell_value(i, "VARIANT".to_string())
                .unwrap_or_default()
        })
        .collect();
    if let Some(i) = find_layout_row(&names, layout) {
        grid.set_selected_rows(i.to_string())?;
        grid.set_current_cell_row(i as i32)?;
        grid.double_click_current_cell()?;
        let still_open = exist_ctrl(session, 1, "", true)?.cband;
        if still_open {
            println!("LT23: layout chooser still open after selecting '{layout}'");
            close_wnd1(session);
            return Ok(false);
        }
        println!("LT23: layout '{layout}' selected");
        return Ok(true);
    }
    println!("LT23: layout '{layout}' not found among {rows} layouts");
    close_wnd1(session);
    let Some(columns) = &params.create_layout_columns else {
        return Ok(false);
    };
    // Create it: change layout (btn[32]) + LI column editor
    if !press(session, "wnd[0]/tbar[1]/btn[32]")? {
        println!("LT23: change-layout button btn[32] not found");
        return Ok(false);
    }
    let ok = setup_layout_li(
        session,
        "LT23",
        1,
        LI_BASE_OBJECT,
        layout,
        columns,
        200,
        !params.save_layout,
    )?;
    if !ok {
        println!("LT23: layout creation reported failure");
        close_wnd1(session);
        return Ok(false);
    }
    if !params.save_layout {
        // not saved: confirm the column selection with Enter
        if let Ok(w) = session.find_by_id("wnd[1]".to_string()) {
            if let Some(f) = w.downcast::<GuiFrameWindow>() {
                f.send_v_key(0)?;
            }
        }
    }
    if exist_ctrl(session, 1, "", true)?.cband {
        println!("LT23: a dialog is still open after layout creation");
        close_wnd1(session);
        return Ok(false);
    }
    Ok(true)
}

/// Run LT23 for `params.transfer_orders` (spec 3.5).
pub fn run_export_outcome(session: &GuiSession, params: &LT23Params) -> Result<ExportOutcome> {
    println!("Running LT23 export...");
    if params.transfer_orders.is_empty() {
        return fail("no transfer orders given");
    }
    if !assert_tcode(session, "LT23", Some(0))? {
        return fail("could not start transaction LT23");
    }

    // warehouse
    match session
        .find_by_id("wnd[0]/usr/ctxtT1_LGNUM".to_string())
        .ok()
        .and_then(|c| c.downcast::<GuiCTextField>())
    {
        Some(f) => f.set_text(params.warehouse.clone())?,
        None => return fail("warehouse field ctxtT1_LGNUM not found"),
    }

    // transfer orders
    if !paste_transfer_orders(session, &params.transfer_orders)? {
        return fail("could not paste the transfer orders");
    }

    // confirmed only
    if params.confirmed_only {
        match session
            .find_by_id("wnd[0]/usr/radT1_QUITA".to_string())
            .ok()
            .and_then(|c| c.downcast::<GuiRadioButton>())
        {
            Some(r) => r.select()?,
            None => return fail("radio button radT1_QUITA not found"),
        }
    }

    // execute
    if !press(session, "wnd[0]/tbar[1]/btn[8]")? {
        return fail("execute button btn[8] not found");
    }
    let opts = ClassifyOptions {
        ignore_display_error: params.ignore_display_error,
    };
    if let Some(o) = classify_after_execute(session, "LT23", "/usr/ctxtT1_LGNUM", opts)? {
        println!("LT23 outcome after execute: {o:?}");
        return Ok(o);
    }

    // layout
    if let Some(layout) = params
        .layout_row
        .as_deref()
        .filter(|l| !l.trim().is_empty())
    {
        if !select_layout(session, params, layout)? {
            return fail(&format!("layout '{layout}' could not be selected"));
        }
        if let Some(o) = crate::utils::export_outcome::classify_screen(session, opts)? {
            return Ok(o);
        }
    }

    // export
    if let Some(target) = &params.export_target {
        return export_to_target(session, "LT23", LOCAL_FILE_MENUS, target);
    }
    // Legacy Excel export (spec P5): menu[0]/menu[1]/menu[1]
    match session
        .find_by_id("wnd[0]/mbar/menu[0]/menu[1]/menu[1]".to_string())
        .ok()
        .and_then(|c| c.downcast::<GuiMenu>())
    {
        Some(m) => m.select()?,
        None => return fail("Excel export menu not found"),
    }
    if !check_export_window(
        session,
        "LT23",
        "Transfer Orders: List of Resident Documents",
    )? {
        return fail("export window check failed");
    }
    let (file_path, file_name) = get_tcode_file_path("LT23", "xlsx");
    let saved = save_sap_file(session, &file_path, &file_name, Some(true))?;
    Ok(ExportOutcome::from_saved(saved, &file_path, &file_name))
}

/// Boolean wrapper like the other tcodes; see [`run_export_outcome`].
pub fn run_export(session: &GuiSession, params: &LT23Params) -> Result<bool> {
    Ok(run_export_outcome(session, params)?.into_bool())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults() {
        let p = LT23Params::default();
        assert_eq!(p.warehouse, "WH01");
        assert!(p.confirmed_only && p.export_target.is_none());
        assert_eq!(p.t_code, "LT23");
    }

    #[test]
    fn layout_row_lookup() {
        let names: Vec<String> = ["A", " LT23_MUSTGO_LAYOUT ", "x"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(find_layout_row(&names, "LT23_MUSTGO_LAYOUT"), Some(1));
        assert_eq!(find_layout_row(&names, "nope"), None);
        assert_eq!(find_layout_row(&names, "  "), None);
        assert_eq!(find_layout_row(&[], "a"), None);
    }

    #[test]
    fn to_mg_columns_match_spec() {
        assert_eq!(TO_MG_COLUMNS.len(), 11);
        assert_eq!(TO_MG_COLUMNS[1], "Transfer Order Number");
        assert_eq!(TO_MG_COLUMNS[10], "Confirmation status");
    }
}
