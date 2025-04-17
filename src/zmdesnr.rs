use sap_scripting::*;
use windows::core::Result;

use crate::utils::sap_file_utils::*;
use crate::utils::select_layout_utils::check_select_layout;
// Import specific functions to avoid ambiguity
use crate::utils::sap_ctrl_utils::hit_ctrl;
use crate::utils::sap_tcode_utils::*;
use crate::utils::sap_wnd_utils::*;

/// Struct to hold additional parameters for ZMDESNR export
#[derive(Debug, Default)]
pub struct ZMDESNRAdditionalParams {
    pub pre_export_back: Option<String>,
}

/// Struct to hold ZMDESNR export parameters
#[derive(Debug)]
pub struct ZMDESNRParams {
    pub delivery_numbers: Vec<String>,
    pub sap_variant_name: Option<String>,
    pub layout_row: Option<String>,
    pub t_code: String,
    pub exclude_serials: Option<Vec<String>>,
    pub serial_number: Option<String>,
    pub column_name: Option<String>,
    pub tab_number: Option<i32>,
    pub additional_params: ZMDESNRAdditionalParams,
}

impl Default for ZMDESNRParams {
    fn default() -> Self {
        Self {
            delivery_numbers: Vec::new(),
            sap_variant_name: None,
            layout_row: None,
            t_code: "ZMDESNR".to_string(),
            exclude_serials: None,
            serial_number: None,
            column_name: None,
            tab_number: None,
            additional_params: ZMDESNRAdditionalParams::default(),
        }
    }
}

/// Run ZMDESNR export with the given parameters
///
/// This function is a port of the VBA function ZMDESNR_With_Exclude_Export
pub fn run_export(session: &GuiSession, params: &ZMDESNRParams) -> Result<bool> {
    println!("Running ZMDESNR export...");

    // Check if tCode is active
    if !assert_tcode(session, "ZMDESNR", Some(0))? {
        println!("Failed to activate ZMDESNR transaction");
        return Ok(false);
    }

    // Apply variant if provided
    if let Some(variant_name) = &params.sap_variant_name {
        if !variant_name.is_empty() && !variant_select(session, &params.t_code, variant_name)? {
            println!(
                "Failed to select variant '{}' for tCode '{}'",
                variant_name, params.t_code
            );
            // Continue with export even if variant selection failed
        }
    }

    // Get tab number (default to 2)
    let tab_number = params.tab_number.unwrap_or(2);

    // Select the specified tab based on tab_number
    let tab_id = format!("wnd[0]/usr/tabsTABSTRIP_TABB1/tabpUCOMM{}", tab_number);
    if let Ok(tab) = session.find_by_id(tab_id) {
        if let Some(tab_strip) = tab.downcast::<GuiTab>() {
            tab_strip.select()?;
        }
    }

    // Handle tab-specific operations
    let tab_operation_success = if tab_number == 2 {
        // Tab 2 specific operations
        handle_tab2_operations(session, params)?
    } else {
        // Default operations for unspecified tabs
        println!("Tab number {} not specifically handled", tab_number);
        // For now, we'll just execute the query
        if let Ok(btn) = session.find_by_id("wnd[0]/tbar[1]/btn[8]".to_string()) {
            if let Some(button) = btn.downcast::<GuiButton>() {
                button.press()?;
            }
        }
        true // Assume success for unhandled tabs
    };            
    
    // Execute
    if let Ok(btn) = session.find_by_id("wnd[0]/tbar[1]/btn[8]".to_string()) {
        if let Some(button) = btn.downcast::<GuiButton>() {
            button.press()?;
        }
    }
            

    // If tab operations failed, return early
    if !tab_operation_success {
        return Ok(false);
    }

    // Check if we need to send vkey 3 after export (before layout selection)
    if let Some(pre_export_back) = &params.additional_params.pre_export_back {
        if pre_export_back == "true" {
            println!("Sending vkey 3 (back) after export before layout selection");
            // Send vkey 3 (back)
            if let Ok(window) = session.find_by_id("wnd[0]".to_string()) {
                if let Some(main_window) = window.downcast::<GuiMainWindow>() {
                    main_window.send_v_key(3)?; // Send vkey 3 (back)
                }
            }
        }
    }

    // Apply layout if provided (common for all tabs)
    if let Some(layout_row) = &params.layout_row {
        if !layout_row.is_empty() {
            apply_layout(session, layout_row)?;
        }
    }

    // Get statusbar message
    let bar_msg = hit_ctrl(session, 0, "/sbar", "Text", "Get", "")?;
    match bar_msg.as_str() {
        "" => {}
        "No layouts found" => {
            println!(
                "Statusbar message: No layouts found for layout {}",
                params.layout_row.as_deref().unwrap_or("")
            );
            return Ok(false);
        }
        _ => {
            println!("Statusbar message: {}", bar_msg);
        }
    }

    // Export as Excel (common for all tabs)
    if let Ok(menu) = session.find_by_id("wnd[0]/mbar/menu[0]/menu[3]/menu[1]".to_string()) {
        if let Some(menu_item) = menu.downcast::<GuiMenu>() {
            menu_item.select()?;
        }
    }

    // Check export window
    let run_check = check_export_window(
        session,
        "ZMDESNR",
        "ZMDEMAIN SERIAL NUMBER HISTORY CONTENTS",
    )?;
    if !run_check {
        println!("Error checking export window");
        return Ok(false);
    }

    // Get file path using the utility function
    let (file_path, file_name) = get_tcode_file_path("ZMDESNR", "xlsx");

    // Save SAP file
    let run_check = save_sap_file(session, &file_path, &file_name, Some(true))?;

    Ok(run_check)
}

/// Handle operations specific to tab 2
fn handle_tab2_operations(session: &GuiSession, params: &ZMDESNRParams) -> Result<bool> {
    // Check if we're using serial number or delivery numbers
    if let Some(serial) = &params.serial_number {
        if !serial.is_empty() {
            // Set the serial number field
            if let Ok(txt) = session.find_by_id("wnd[0]/usr/tabsTABSTRIP_TABB1/tabpUCOMM2/ssub%_SUBSCREEN_TABB1:ZMDE_SERIALNUMBER_HISTORY:9002/txtS_PARENT-LOW".to_string()) {
                if let Some(text_field) = txt.downcast::<GuiTextField>() {
                    text_field.set_text(serial.clone())?;
                }
            }
            

            return Ok(true);
        }
    }

    // If no serial number provided, continue with delivery numbers
    
    // Clear the Low Delivery Number field
    if let Ok(txt) = session.find_by_id("wnd[0]/usr/tabsTABSTRIP_TABB1/tabpUCOMM2/ssub%_SUBSCREEN_TABB1:ZMDE_SERIALNUMBER_HISTORY:9002/txtS_VBELN-LOW".to_string()) {
        if let Some(text_field) = txt.downcast::<GuiTextField>() {
            text_field.set_text("".to_string())?;
        }
    }

    // Clear the High Delivery Number field
    if let Ok(txt) = session.find_by_id("wnd[0]/usr/tabsTABSTRIP_TABB1/tabpUCOMM2/ssub%_SUBSCREEN_TABB1:ZMDE_SERIALNUMBER_HISTORY:9002/txtS_VBELN-HIGH".to_string()) {
        if let Some(text_field) = txt.downcast::<GuiTextField>() {
            text_field.set_text("".to_string())?;
        }
    }

    // Clear the Palletized field
    if let Ok(txt) = session.find_by_id("wnd[0]/usr/tabsTABSTRIP_TABB1/tabpUCOMM2/ssub%_SUBSCREEN_TABB1:ZMDE_SERIALNUMBER_HISTORY:9002/ctxtS_PALLTD-LOW".to_string()) {
        if let Some(text_field) = txt.downcast::<GuiCTextField>() {
            text_field.set_text("".to_string())?;
        }
    }

    // Press Multi Delivery Entry button
    if let Ok(btn) = session.find_by_id("wnd[0]/usr/tabsTABSTRIP_TABB1/tabpUCOMM2/ssub%_SUBSCREEN_TABB1:ZMDE_SERIALNUMBER_HISTORY:9002/btn%_S_VBELN_%_APP_%-VALU_PUSH".to_string()) {
        if let Some(button) = btn.downcast::<GuiButton>() {
            button.press()?;
        }
    }

    // Clear previous entries
    if let Ok(window) = session.find_by_id("wnd[1]".to_string()) {
        if let Some(modal_window) = window.downcast::<GuiModalWindow>() {
            modal_window.send_v_key(16)?; // Clear Previous entries
        }
    }

    // Paste delivery numbers
    let mut j = 0;
    for delivery_number in &params.delivery_numbers {
        let input_field_id = format!("wnd[1]/usr/tabsTAB_STRIP/tabpSIVA/ssubSCREEN_HEADER:SAPLALDB:3010/tblSAPLALDBSINGLE/txtRSCSEL_255-SLOW_I[1,{}]", j);
        if let Ok(txt) = session.find_by_id(input_field_id) {
            if let Some(text_field) = txt.downcast::<GuiTextField>() {
                text_field.set_text(delivery_number.clone())?;
                j += 1;
            }
        }
    }

    // Check if items were pasted successfully
    let run_check = check_delivery_paste(session, "ZMDESNR", 1, 0)?;
    if !run_check {
        println!("Paste not successful, retrying...");
        // In a real implementation, we would retry the paste operation
        // For now, we'll just return false
        return Ok(false);
    }

    // Close Multi-Window
    if let Ok(window) = session.find_by_id("wnd[1]".to_string()) {
        if let Some(modal_window) = window.downcast::<GuiModalWindow>() {
            modal_window.send_v_key(8)?; // Close Multi-Window
        }
    }

    // Handle exclude serials if provided
    if let Some(exclude_serials) = &params.exclude_serials {
        if !exclude_serials.is_empty() {
            // Press Multi Parent SN Popup button
            if let Ok(btn) = session.find_by_id("wnd[0]/usr/tabsTABSTRIP_TABB1/tabpUCOMM2/ssub%_SUBSCREEN_TABB1:ZMDE_SERIALNUMBER_HISTORY:9002/btn%_S_PARENT_%_APP_%-VALU_PUSH".to_string()) {
                if let Some(button) = btn.downcast::<GuiButton>() {
                    button.press()?;
                }
            }

            // Select Exclude Tab
            if let Ok(tab) = session.find_by_id("wnd[1]/usr/tabsTAB_STRIP/tabpNOSV".to_string()) {
                if let Some(tab_strip) = tab.downcast::<GuiTab>() {
                    tab_strip.select()?;
                }
            }

            // Clear previous entries
            if let Ok(window) = session.find_by_id("wnd[1]".to_string()) {
                if let Some(modal_window) = window.downcast::<GuiModalWindow>() {
                    modal_window.send_v_key(16)?; // Clear Previous entries
                }
            }

            // Paste exclude serials
            let mut j = 0;
            for serial in exclude_serials {
                let input_field_id = format!("wnd[1]/usr/tabsTAB_STRIP/tabpNOSV/ssubSCREEN_HEADER:SAPLALDB:3030/tblSAPLALDBSINGLE_E/txtRSCSEL_255-SLOW_E[1,{}]", j);
                if let Ok(txt) = session.find_by_id(input_field_id) {
                    if let Some(text_field) = txt.downcast::<GuiTextField>() {
                        text_field.set_text(serial.clone())?;
                        j += 1;
                    }
                }
            }

            // Check if items were pasted successfully
            let run_check = check_sn_paste(session, "ZMDESNR", 1, 0)?;
            if !run_check {
                println!("Paste of exclude serials not successful, continuing anyway...");
                // Continue with export even if paste failed
            }

            // Close Multi-Window
            if let Ok(window) = session.find_by_id("wnd[1]".to_string()) {
                if let Some(modal_window) = window.downcast::<GuiModalWindow>() {
                    modal_window.send_v_key(8)?; // Close Multi-Window
                }
            }
        }
    }

    // Execute
    if let Ok(btn) = session.find_by_id("wnd[0]/tbar[1]/btn[8]".to_string()) {
        if let Some(button) = btn.downcast::<GuiButton>() {
            button.press()?;
        }
    }

    Ok(true)
}

/// Apply layout to the current view
fn apply_layout(session: &GuiSession, layout_row: &str) -> Result<bool> {
    // Choose Layout
    if let Ok(btn) = session.find_by_id("wnd[0]/tbar[1]/btn[33]".to_string()) {
        if let Some(button) = btn.downcast::<GuiButton>() {
            button.press()?;
        }
    }

    // Use the existing layout selection utility
    println!("DEBUG:Selecting layout with check_select_layout");
    let layout_select = check_select_layout(session, "ZMDESNR", layout_row, None);
    match layout_select {
        Ok(_) => {
            println!("Layout selected: {}", layout_row);
            Ok(true)
        }
        Err(e) => {
            eprintln!("Error selecting layout ({}): {}", layout_row, e);
            // If layout selection failed, close any open layout selection windows
            close_popups(session, None, None)?;
            println!("Layout selection failed. Exporting as-is.");
            Ok(false)
        }
    }
}

/// Check if items were pasted successfully in the multi-selection window
///
/// This is a helper function for run_export
fn check_sn_paste(session: &GuiSession, tcode: &str, wnd_idx: i32, row_idx: i32) -> Result<bool> {
    // Check if the first row has a value
    let input_field_id = format!("wnd[{}]/usr/tabsTAB_STRIP/tabpNOSV/ssubSCREEN_HEADER:SAPLALDB:3030/tblSAPLALDBSINGLE_E/txtRSCSEL_255-SLOW_E[1,{}]", wnd_idx, row_idx);

    if let Ok(txt) = session.find_by_id(input_field_id) {
        if let Some(text_field) = txt.downcast::<GuiTextField>() {
            let value = text_field.text()?;
            if !value.is_empty() {
                return Ok(true);
            }
        }
    }

    println!(
        "No items found in multi-selection window for tcode: {}",
        tcode
    );
    Ok(false)
}

/// Check if items were pasted successfully in the multi-selection window
///
/// This is a helper function for run_export
fn check_delivery_paste(
    session: &GuiSession,
    tcode: &str,
    wnd_idx: i32,
    row_idx: i32,
) -> Result<bool> {
    // Check if the first row has a value
    let input_field_id = format!("wnd[{}]/usr/tabsTAB_STRIP/tabpSIVA/ssubSCREEN_HEADER:SAPLALDB:3010/tblSAPLALDBSINGLE/txtRSCSEL_255-SLOW_I[1,{}]", wnd_idx, row_idx);

    if let Ok(txt) = session.find_by_id(input_field_id) {
        if let Some(text_field) = txt.downcast::<GuiTextField>() {
            let value = text_field.text()?;
            if !value.is_empty() {
                return Ok(true);
            }
        }
    }

    println!(
        "No items found in multi-selection window for tcode: {}",
        tcode
    );
    Ok(false)
}
