use crate::utils::close_popups;
use crate::utils::sap_constants::STR_FORM;
use crate::utils::sap_ctrl_utils::{exist_ctrl, hit_ctrl};
use sap_scripting::*;
use std::thread;
use std::time::Duration;
use windows::core::Result;

pub fn assert_tcode(session: &GuiSession, tcode: &str, wnd: Option<i32>) -> Result<bool> {
    let wnd_num = wnd.unwrap_or(0);

    // Start the transaction
    session.start_transaction(tcode.to_string())?;

    // Check for errors in status bar
    let err_msg = hit_ctrl(session, wnd_num, "/sbar", "Text", "Get", "")?;

    if err_msg.contains("exist") || err_msg.contains("autho") {
        println!("Error: {err_msg}");
        return Ok(false);
    }

    if err_msg.is_empty() {
        return Ok(true);
    }

    // Log error message
    println!("{STR_FORM}{err_msg}{STR_FORM}");

    Ok(false)
}

pub fn check_tcode(
    session: &GuiSession,
    tcode: &str,
    run: Option<bool>,
    _kill_popups: Option<bool>,
) -> Result<bool> {
    let run_val = run.unwrap_or(false);
    let b_kill_popups = _kill_popups.unwrap_or(false);

    println!("Checking if tCode ({tcode}) is active");

    if b_kill_popups {
        close_popups(session, None, None)?;
    }

    // Get current transaction
    let current = session.info()?.transaction()?;

    // Check if on tCode
    if current.contains(tcode) {
        println!("tCode ({tcode}) is active");
        Ok(true)
    } else if run_val {
        // Run if requested
        println!("tCode mismatch, attempting to run tCode ({tcode})");
        let _ = assert_tcode(session, tcode, None)?;
        thread::sleep(Duration::from_millis(500)); // Time_Event equivalent

        // Recursive call to check again
        check_tcode(session, tcode, Some(false), Some(false))
    } else {
        println!("tCode mismatch. Current tCode is ({current}), need ({tcode})");
        Ok(false)
    }
}

/// Case-insensitive "contains" match used to find a variant row in the ALV popup.
pub fn variant_matches(cell_value: &str, variant_name: &str) -> bool {
    let name = variant_name.trim();
    !name.is_empty() && cell_value.to_lowercase().contains(&name.to_lowercase())
}

/// ALV variant popup path of P2: `wnd[1]/usr/cntlALV_CONTAINER_1/shellcont/shell`,
/// find the row whose `VARIANT` cell contains the name and double-click it.
fn select_variant_from_alv_popup(
    session: &GuiSession,
    tcode: &str,
    variant_name: &str,
) -> Result<bool> {
    let grid_id = "wnd[1]/usr/cntlALV_CONTAINER_1/shellcont/shell".to_string();
    let grid = match session
        .find_by_id(grid_id)
        .ok()
        .and_then(|c| c.downcast::<GuiGridView>())
    {
        Some(g) => g,
        None => {
            println!("ALV variant popup grid not found for tCode '{tcode}'");
            return Ok(false);
        }
    };

    let rows = grid.row_count()?;
    for i in 0..rows {
        let cell = grid
            .get_cell_value(i, "VARIANT".to_string())
            .unwrap_or_default();
        if variant_matches(&cell, variant_name) {
            grid.set_selected_rows(i.to_string())?;
            grid.set_current_cell_row(i)?;
            grid.double_click_current_cell()?;
            // Popup still open means the variant failed to load.
            let still = exist_ctrl(session, 1, "", true)?;
            if still.cband {
                println!("Variant popup still open after selecting '{variant_name}'");
                return Ok(false);
            }
            println!("Variant '{variant_name}' selected via ALV popup for tCode '{tcode}'");
            return Ok(true);
        }
    }
    println!("Variant '{variant_name}' not found in ALV popup for tCode '{tcode}'");
    Ok(false)
}

pub fn variant_select(session: &GuiSession, tcode: &str, variant_name: &str) -> Result<bool> {
    println!("Selecting variant '{variant_name}' for tCode '{tcode}'");

    // Choose variant
    if let Ok(btn) = session.find_by_id("wnd[0]/tbar[1]/btn[17]".to_string()) {
        if let Some(button) = btn.downcast::<GuiButton>() {
            button.press()?;
        } else {
            println!("Failed to downcast variant button to GuiButton");
            return Ok(false);
        }
    } else {
        println!("Variant button not found for tCode '{tcode}'");
        return Ok(false);
    }

    // Check if variant selection window opened
    let err_msg = hit_ctrl(session, 0, "/sbar", "Text", "Get", "")?;
    if err_msg.contains("error") || err_msg.contains("not found") {
        println!("Error opening variant selection: {err_msg}");
        return Ok(false);
    }

    // P2: some systems show an ALV list popup (title contains "abap") instead of the
    // classic variant-name dialog. Pick the matching row and double-click it.
    let popup = exist_ctrl(session, 1, "", true)?;
    if popup.cband && popup.ctext.to_lowercase().contains("abap") {
        return select_variant_from_alv_popup(session, tcode, variant_name);
    }

    // Enter variant name
    if let Ok(txt) = session.find_by_id("wnd[1]/usr/txtV-LOW".to_string()) {
        if let Some(text_field) = txt.downcast::<GuiTextField>() {
            text_field.set_text(variant_name.to_string())?;
        } else {
            println!("Failed to downcast variant name field to GuiTextField");
            return Ok(false);
        }
    } else {
        println!("Variant name field not found");
        return Ok(false);
    }

    // Blank username
    if let Ok(txt) = session.find_by_id("wnd[1]/usr/txtENAME-LOW".to_string()) {
        if let Some(text_field) = txt.downcast::<GuiTextField>() {
            text_field.set_text("".to_string())?;
        } else {
            // If we can't find or use the username field, just continue
            println!("Warning: Could not clear username field, continuing anyway");
        }
    }

    // Close variant select window
    if let Ok(btn) = session.find_by_id("wnd[1]/tbar[0]/btn[8]".to_string()) {
        if let Some(button) = btn.downcast::<GuiButton>() {
            button.press()?;
        } else {
            println!("Failed to downcast confirm button to GuiButton");
            return Ok(false);
        }
    } else {
        println!("Confirm button not found");
        return Ok(false);
    }

    // Check for errors in status bar after variant selection
    let err_msg = hit_ctrl(session, 0, "/sbar", "Text", "Get", "")?;
    if !err_msg.is_empty() && (err_msg.contains("error") || err_msg.contains("not found")) {
        println!("Error selecting variant: {err_msg}");
        return Ok(false);
    }

    println!("Variant '{variant_name}' selected successfully for tCode '{tcode}'");
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::variant_matches;

    #[test]
    fn variant_match_is_case_insensitive_contains() {
        assert!(variant_matches("VT11_XDOCK_VARIANT", "xd_all"));
        assert!(!variant_matches("OTHER", "XD_ALL"));
        assert!(!variant_matches("ANY", "  "));
    }
}
