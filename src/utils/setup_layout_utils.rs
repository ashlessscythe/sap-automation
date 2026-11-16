//! Layout setup helpers (VBA port); kept for parity with legacy flows.
//!
//! `setup_layout` is the port of the VBA function `SetupLayout` (grid-style editor). Its column
//! picking and save dialogs now live in [`crate::utils::layout_editors`] /
//! [`crate::utils::layout_create`]; this wrapper keeps the old signature and behaviour: columns
//! that cannot be found are skipped (printed), and an existing layout of that name is
//! overwritten. New code should use [`crate::utils::layout_create::create_layout`], which reports
//! missing columns and never overwrites without being told to.
#![allow(dead_code)]

use sap_scripting::*;

use crate::utils::layout_create::{press, save_grid, status_text, to_windows_error};
use crate::utils::layout_editors::{pick_columns_grid, RealGridScreen};

/// Struct to hold layout parameters
#[derive(Debug, Clone, Default)]
pub struct LayoutParams {
    pub run_check: bool,
    pub err: String,
    pub name: String,
    pub type_name: String,
}

/// Setup a layout with the given parameters
///
/// This function is a port of the VBA function SetupLayout. `Ok(false)` = the grid editor is not
/// on screen.
pub fn setup_layout(
    session: &GuiSession,
    n_wnd: i32,
    base_obj_name: &str,
    layout_name: &str,
    list: &[String],
    limit: i32,
    no_save: bool,
) -> windows::core::Result<bool> {
    println!("Setting up layout: {layout_name}");

    let mut screen = RealGridScreen::new(session, n_wnd, base_obj_name);
    if !screen.is_open() {
        return Ok(false);
    }

    let report = pick_columns_grid(&mut screen, list, limit).map_err(|e| to_windows_error(&e))?;
    for item in &report.missing {
        println!("Item ({item}) not found, skipping");
    }
    println!("Added {} items to Layout", report.moved.len());

    if no_save {
        // Green Checkmark wnd1
        let _ = press(session, &format!("wnd[{n_wnd}]/tbar[0]/btn[0]"));
    } else {
        // user-specific, not default, name + description = layout name; legacy: overwrite = Yes
        save_grid(session, layout_name, layout_name, true).map_err(|e| to_windows_error(&e))?;
    }

    let status_msg = status_text(session);
    println!("Layout ({layout_name}) Successfully Setup: {status_msg}");
    Ok(true)
}
