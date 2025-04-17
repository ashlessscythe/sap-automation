use chrono::NaiveDate;
use sap_scripting::*;
use windows::core::Result;

use crate::utils::{choose_layout, sap_file_utils::*};
// Import specific functions to avoid ambiguity
use crate::utils::sap_ctrl_utils::exist_ctrl;
use crate::utils::sap_tcode_utils::*;
use crate::utils::sap_wnd_utils::*;

/// Struct to hold VT11 export parameters
#[derive(Debug)]
pub struct VT11Params {
    pub start_date: NaiveDate,
    pub end_date: NaiveDate,
    pub sap_variant_name: Option<String>,
    pub layout_row: Option<String>,
    pub by_date: bool,
    pub limiter: Option<String>,
    pub t_code: String,
}

impl Default for VT11Params {
    fn default() -> Self {
        Self {
            start_date: chrono::Local::now().date_naive(),
            end_date: chrono::Local::now().date_naive(),
            sap_variant_name: None,
            layout_row: None,
            by_date: true,
            limiter: None,
            t_code: "VT11".to_string(),
        }
    }
}

/// Run VT11 export with the given parameters
///
/// This function is a port of the VBA function VT11_Run_Export
pub fn run_export(session: &GuiSession, params: &VT11Params) -> Result<bool> {
    println!("Running VT11 export...");

    // Check if tCode is active
    if !assert_tcode(session, "VT11", Some(0))? {
        println!("Failed to activate VT11 transaction");
        return Ok(false);
    }

    // Format dates for SAP
    let start_date_str = params.start_date.format("%m/%d/%Y").to_string();
    let end_date_str = params.end_date.format("%m/%d/%Y").to_string();

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

    // Set date fields based on by_date parameter
    if params.by_date {
        // Set start date
        if let Ok(txt) = session.find_by_id("wnd[0]/usr/ctxtK_DATEN-LOW".to_string()) {
            if let Some(text_field) = txt.downcast::<GuiTextField>() {
                text_field.set_text(start_date_str.clone())?;
            }
        }

        // Set end date (leave blank if same as start date)
        if let Ok(txt) = session.find_by_id("wnd[0]/usr/ctxtK_DATEN-HIGH".to_string()) {
            if let Some(text_field) = txt.downcast::<GuiTextField>() {
                if params.start_date == params.end_date {
                    text_field.set_text("".to_string())?;
                } else {
                    text_field.set_text(end_date_str.clone())?;
                }
            }
        }
    }

    // Handle limiter if provided
    if let Some(limiter) = &params.limiter {
        if !limiter.is_empty() {
            match limiter.to_lowercase().as_str() {
                "delivery" => {
                    // This would require clipboard functionality which is more complex in Rust
                    // For now, we'll just log that this functionality is not yet implemented
                    println!("Delivery limiter functionality not yet implemented");

                    // In a full implementation, we would:
                    // 1. Get the delivery numbers from Excel
                    // 2. Press the multi outbound delivery button
                    // 3. Paste the delivery numbers
                    // 4. Close the popup
                }
                "date_range" => {
                    // Blank 2nd description to prevent issues
                    if let Ok(txt) = session.find_by_id("wnd[0]/usr/txtK_TPBEZ-HIGH".to_string()) {
                        if let Some(text_field) = txt.downcast::<GuiTextField>() {
                            text_field.set_text("".to_string())?;
                        }
                    }

                    // This would also require clipboard functionality
                    println!("Date range limiter functionality not yet implemented");
                }
                _ => {
                    println!("Unknown limiter type: {}", limiter);
                }
            }
        }
    }

    // Execute the transaction
    if let Ok(btn) = session.find_by_id("wnd[0]/tbar[1]/btn[8]".to_string()) {
        if let Some(button) = btn.downcast::<GuiButton>() {
            button.press()?;
        }
    }

    // Check for error (No Shipments Found)
    let err_ctl = exist_ctrl(session, 1, "/usr/txtMESSTXT1", false)?;
    if err_ctl.cband {
        if let Ok(txt) = session.find_by_id("wnd[1]/usr/txtMESSTXT1".to_string()) {
            if let Some(text_field) = txt.downcast::<GuiTextField>() {
                let error_text = text_field.text()?;
                if error_text.contains("No shipments were found for the selection criteria") {
                    println!(
                        "No shipments found from dates ({} to {})",
                        start_date_str, end_date_str
                    );

                    // Close window
                    if let Ok(window) = session.find_by_id("wnd[1]".to_string()) {
                        if let Some(modal_window) = window.downcast::<GuiFrameWindow>() {
                            modal_window.close()?;
                        }
                    }

                    return Ok(false);
                }
            }
        }
    }

    // Check if layout exists and select it
    if let Some(layout_row) = &params.layout_row {
        if !layout_row.is_empty() {
            // Choose Layout - only open layout selection if a layout is provided
            if let Ok(menu) = session.find_by_id("wnd[0]/mbar/menu[3]/menu[0]/menu[1]".to_string())
            {
                if let Some(menu_item) = menu.downcast::<GuiMenu>() {
                    menu_item.select()?;
                }
            }

            // Check if window exists
            let err_ctl = exist_ctrl(session, 1, "", true)?;

            if err_ctl.cband {
                // String layout name
                let msg = choose_layout(session, &params.t_code, layout_row);
                match msg {
                    Ok(message) if message.is_empty() => {} // no-op
                    Ok(message) => {
                        eprintln!("Message after choosing layout {}: {}", layout_row, message);
                    }
                    Err(e) => {
                        eprintln!("Error after choosing layout {}: {:?}", layout_row, e);
                    }
                }

                // If we get here and the layout window is still open, the layout wasn't found
                let err_ctl = exist_ctrl(session, 1, "", true)?;
                if err_ctl.cband {
                    if let Ok(window) = session.find_by_id("wnd[1]".to_string()) {
                        if let Some(modal_window) = window.downcast::<GuiFrameWindow>() {
                            modal_window.close()?;
                        }
                    }

                    println!("Layout ({}) not found. Setting up layout...", layout_row);
                    // Setup layout functionality would be implemented here
                }
            }
        } else {
            // If layout is empty or zero-length, close popup window and export as-is
            let err_ctl = exist_ctrl(session, 1, "", true)?;
            if err_ctl.cband {
                if let Ok(window) = session.find_by_id("wnd[1]".to_string()) {
                    if let Some(modal_window) = window.downcast::<GuiFrameWindow>() {
                        modal_window.close()?;
                    }
                }
            }

            println!("Layout is empty or zero-length. Exporting as-is.");
        }
    }

    // Export to Excel
    if let Ok(menu) = session.find_by_id("wnd[0]/mbar/menu[0]/menu[10]/menu[0]".to_string()) {
        if let Some(menu_item) = menu.downcast::<GuiMenu>() {
            menu_item.select()?;
        }
    }

    // debug
    eprintln!("DEBUG: Exporting to Excel");
    // Check export window
    let run_check = check_export_window(session, "VT11", "SHIPMENT LIST: PLANNING")?;
    match run_check {
        true => {
            println!("Export window opened successfully.");
        }
        false => {
            eprintln!("Error checking export window.");
        }
    }

    // Get file path using the utility function
    let (file_path, file_name) = get_tcode_file_path("VT11", "xlsx");

    // save sap file with prevent_excel_open set to true
    let run_check = save_sap_file(session, &file_path, &file_name, Some(true))?;

    Ok(run_check)
}
