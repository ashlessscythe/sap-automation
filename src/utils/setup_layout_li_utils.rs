//! LI layout setup (VBA port); entry points are not all called from the binary.
//!
//! `setup_layout_li` is the port of the VBA function `SetupLayout_li` (list-style editor). The
//! old version downcast the two `GuiTableControl`s to `GuiGridView` and always returned
//! `Ok(false)`; the column picking now lives in [`crate::utils::layout_editors`] (table controls)
//! and the per-tcode save step in [`crate::utils::layout_create`]. New code should use
//! [`crate::utils::layout_create::create_layout`] / [`crate::utils::layout_create::apply_columns`].
#![allow(dead_code)]

use sap_scripting::*;

use crate::utils::layout_create::{
    confirm_editor, profile_for, save_list, status_text, to_windows_error, SaveFlow,
};
use crate::utils::layout_editors::{pick_columns_list, RealListScreen};

/// Struct to hold layout parameters
#[derive(Debug, Clone, Default)]
pub struct LayoutParams {
    pub run_check: bool,
    pub err: String,
    pub name: String,
    pub type_name: String,
}

/// Setup a layout with list items
///
/// This function is a port of the VBA function SetupLayout_li. Columns that cannot be found are
/// skipped (printed) and an existing layout of that name is overwritten (legacy behaviour).
/// `Ok(false)` = the list editor is not on screen or the tcode has no list-style save flow.
#[allow(clippy::too_many_arguments)]
pub fn setup_layout_li(
    session: &GuiSession,
    tcode: &str,
    n_wnd: i32,
    base_obj_name: &str,
    layout_name: &str,
    list: &[String],
    _limit: i32,
    no_save: bool,
) -> windows::core::Result<bool> {
    println!("Setting up layout with list items: {layout_name}");

    let mut screen = RealListScreen::new(session, n_wnd, base_obj_name);
    if !screen.is_open() {
        return Ok(false);
    }
    let profile = match profile_for(tcode) {
        Ok(p) => p,
        Err(e) => {
            println!("{e}");
            return Ok(false);
        }
    };
    let SaveFlow::List { save, dialog } = profile.save else {
        println!("Tcode {tcode} has no list-style layout editor");
        return Ok(false);
    };

    let report = pick_columns_list(&mut screen, list).map_err(|e| to_windows_error(&e))?;
    for item in &report.missing {
        println!("Item ({item}) not found, skipping");
    }
    println!("Added {} items to Layout", report.moved.len());

    if no_save {
        confirm_editor(session, profile).map_err(|e| to_windows_error(&e))?;
    } else {
        save_list(
            session,
            profile,
            &save,
            dialog,
            layout_name,
            layout_name,
            true,
        )
        .map_err(|e| to_windows_error(&e))?;
    }

    let status_msg = status_text(session);
    println!("Layout ({layout_name}) Successfully Setup: {status_msg}");
    Ok(true)
}
