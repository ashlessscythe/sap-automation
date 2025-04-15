use sap_scripting::*;
use windows::core::Result;

use crate::utils::{sap_file_utils::*, choose_layout};
// Import specific functions to avoid ambiguity
use crate::utils::sap_ctrl_utils::*;
use crate::utils::sap_tcode_utils::*;
use crate::utils::sap_wnd_utils::*;

use chrono::NaiveDate;

/// Struct to hold VL06O export parameters
#[derive(Debug)]
pub struct VL06OParams {
    pub sap_variant_name: Option<String>,
    pub layout_row: Option<String>,
    pub shipment_numbers: Vec<String>,
    pub start_date: NaiveDate,
    pub end_date: NaiveDate,
    pub by_date: bool,
    pub column_name: Option<String>,
    pub t_code: String,
}

impl Default for VL06OParams {
    fn default() -> Self {
        Self {
            sap_variant_name: None,
            layout_row: None,
            shipment_numbers: Vec::new(),
            start_date: chrono::Local::now().date_naive(),
            end_date: chrono::Local::now().date_naive(),
            by_date: false,
            column_name: None,
            t_code: "VL06O".to_string(),
        }
    }
}

/// Run VL06O export with the given parameters
/// 
/// This function is a port of the VBA function VL06O_DeliveryList_Run_Export
pub fn run_export(session: &GuiSession, params: &VL06OParams) -> Result<bool> {
    println!("Running VL06O export...");
    
    // Check if tCode is active
    if !assert_tcode(session, "VL06O", Some(0))? {
        println!("Failed to activate VL06O transaction");
        return Ok(false);
    }

    
    // Press "List Outbound Deliveries" button
    if let Ok(btn) = session.find_by_id("wnd[0]/usr/btnBUTTON6".to_string()) {
        if let Some(button) = btn.downcast::<GuiButton>() {
            button.press()?;
        }
    }
    
    // Apply variant if provided
    if let Some(variant_name) = &params.sap_variant_name {
        if !variant_name.is_empty() && !variant_select(session, &params.t_code, variant_name)? {
            println!("Failed to select variant '{}' for tCode '{}'", variant_name, params.t_code);
            // Continue with export even if variant selection failed
        }
    }

    // Clear date fields
    if let Ok(txt) = session.find_by_id("wnd[0]/usr/ctxtIT_WADAT-LOW".to_string()) {
        if let Some(text_field) = txt.downcast::<GuiCTextField>() {
            text_field.set_text("".to_string())?;
        }
    }
    
    if let Ok(txt) = session.find_by_id("wnd[0]/usr/ctxtIT_WADAT-HIGH".to_string()) {
        if let Some(text_field) = txt.downcast::<GuiCTextField>() {
            text_field.set_text("".to_string())?;
        }
    }
    
    // Press Multi Shipment Number button
    if let Ok(btn) = session.find_by_id("wnd[0]/usr/btn%_IT_TKNUM_%_APP_%-VALU_PUSH".to_string()) {
        if let Some(button) = btn.downcast::<GuiButton>() {
            button.press()?;
        }
    }
    
    // Clear previous entries
    println!("DEBUG:Clearing Entries");
    if let Ok(window) = session.find_by_id("wnd[1]".to_string()) {
        if let Some(modal_window) = window.downcast::<GuiModalWindow>() {
            modal_window.send_v_key(16)?; // Clear Previous entries
        }
    }
    
    // Paste shipment numbers
    // In a real implementation, we would use the clipboard to paste the shipment numbers
    // For now, we'll manually enter each shipment number
    let mut j = 0;
    println!("DEBUG:Pasting");
    for shipment_number in &params.shipment_numbers {
        let input_field_id = format!("wnd[1]/usr/tabsTAB_STRIP/tabpSIVA/ssubSCREEN_HEADER:SAPLALDB:3010/tblSAPLALDBSINGLE/ctxtRSCSEL_255-SLOW_I[1,{}]", j);
        if let Ok(txt) = session.find_by_id(input_field_id) {
            if let Some(text_field) = txt.downcast::<GuiCTextField>() {
                text_field.set_text(shipment_number.clone())?;
                j += 1;
            }
        }
    }
    
    // Check if items were pasted successfully
    let run_check = check_multi_paste(session, "VL06O", 1, 0)?;
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
    
    // Execute
    if let Ok(wnd) = session.find_by_id("wnd[0]".to_string()) {
       if let Some(gui) = wnd.downcast::<GuiMainWindow>() {
            gui.send_v_key(8)?;
       } 
    }

    // check for popup
    let sbar = hit_ctrl(session, 0, "/sbar", "Get", "Text", "");
    match sbar {
        Ok(s) => {
            if !s.is_empty() {
                eprintln!("status bar message: {}", s);
            }
        }
        Err(e) => {
            eprintln!("ERror getting sbar message: {}", e);
        }
    }
    
    // Press Item View Button
    if let Ok(btn) = session.find_by_id("wnd[0]/tbar[1]/btn[18]".to_string()) {
        if let Some(button) = btn.downcast::<GuiButton>() {
            button.press()?;
        }
    }
    
    
    // Check if layout provided
    if let Some(layout_row) = &params.layout_row {
        if !layout_row.is_empty() {
            // Choose Layout
            if let Ok(menu) = session.find_by_id("wnd[0]/mbar/menu[3]/menu[2]/menu[1]".to_string()) {
                if let Some(menu_item) = menu.downcast::<GuiMenu>() {
                    menu_item.select()?;
                }
            }
            // Use the existing layout selection utility
            let layout_result = choose_layout(session, "vl06o", layout_row);
            match layout_result {
                Ok(_) => {
                    println!("Layout selected: {}", layout_row);
                },
                Err(e) => {
                    eprintln!("Error selecting layout ({}): {}", layout_row, e);
                    // If no layout specified, close any open layout selection windows
                    close_popups(session, None, None)?;
                    println!("No layout specified. Exporting as-is.");
                }
            }
        }
    }
    
    // Get statusbar message
    let err_msg = hit_ctrl(session, 0, "/sbar", "Text", "Get", "")?;
    println!("Statusbar message: ({})", err_msg);
    
    // Export as Excel
    if let Ok(menu) = session.find_by_id("wnd[0]/mbar/menu[0]/menu[5]/menu[1]".to_string()) {
        if let Some(menu_item) = menu.downcast::<GuiMenu>() {
            menu_item.select()?;
        }
    }
    
    // Check export window
    let run_check = check_export_window(session, "VL06O", "LIST OF OUTBOUND DELIVERIES")?;
    if !run_check {
        println!("Error checking export window");
        return Ok(false);
    }
    
    // Get file path using the utility function
    let (file_path, file_name) = get_tcode_file_path("VL06O", "xlsx");
    
    // Save SAP file
    let run_check = save_sap_file(session, &file_path, &file_name)?;
    
    Ok(run_check)
}

/// Check if items were pasted successfully in the multi-selection window
/// 
/// This is a helper function for run_export
fn check_multi_paste(session: &GuiSession, tcode: &str, wnd_idx: i32, row_idx: i32) -> Result<bool> {
    // Check if the first row has a value
    let input_field_id = format!("wnd[{}]/usr/tabsTAB_STRIP/tabpSIVA/ssubSCREEN_HEADER:SAPLALDB:3010/tblSAPLALDBSINGLE/ctxtRSCSEL_255-SLOW_I[1,{}]", wnd_idx, row_idx);
    
    if let Ok(txt) = session.find_by_id(input_field_id) {
        if let Some(text_field) = txt.downcast::<GuiCTextField>() {
            let value = text_field.text()?;
            if !value.is_empty() {
                return Ok(true);
            }
        }
    }
    
    println!("No items found in multi-selection window for tcode: {}", tcode);
    Ok(false)
}
