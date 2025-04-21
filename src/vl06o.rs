use sap_scripting::*;
use windows::core::Result;

use crate::utils::{choose_layout, sap_file_utils::*};
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

/// Struct to hold VL06O date update parameters
#[derive(Debug)]
pub struct VL06ODateUpdateParams {
    pub delivery_numbers: Vec<String>,
    pub target_date: NaiveDate,
    pub sap_variant_name: Option<String>,
    pub t_code: String,
}

impl Default for VL06ODateUpdateParams {
    fn default() -> Self {
        Self {
            delivery_numbers: Vec::new(),
            target_date: chrono::Local::now().date_naive().succ(), // Default to tomorrow
            sap_variant_name: Some("blank_".to_string()),
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
            println!(
                "Failed to select variant '{}' for tCode '{}'",
                variant_name, params.t_code
            );
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
    let sbar = hit_ctrl(session, 0, "/sbar", "Text", "Get", "");
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
            if let Ok(menu) = session.find_by_id("wnd[0]/mbar/menu[3]/menu[2]/menu[1]".to_string())
            {
                if let Some(menu_item) = menu.downcast::<GuiMenu>() {
                    menu_item.select()?;
                }
            }
            // Use the existing layout selection utility
            let layout_result = choose_layout(session, "vl06o", layout_row);
            match layout_result {
                Ok(_) => {
                    println!("Layout selected: {}", layout_row);
                }
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

    // Save SAP file with prevent_excel_open set to true (don't open Excel)
    let run_check = save_sap_file(session, &file_path, &file_name, Some(true))?;

    Ok(run_check)
}

/// Run VL06O date update with the given parameters
///
/// This function is a port of the VBA function vl06o_date_update
pub fn run_date_update(session: &GuiSession, params: &VL06ODateUpdateParams) -> Result<(i32, Vec<(String, String)>)> {
    println!("Running VL06O date update...");
    
    // Format target date for SAP
    let target_date_str = params.target_date.format("%m/%d/%Y").to_string();
    
    // Check if tCode is active
    if !assert_tcode(session, "VL06O", Some(0))? {
        println!("Failed to activate VL06O transaction");
        return Ok((0, Vec::new()));
    }
    
    // Press "List Outbound Deliveries" button
    if let Ok(btn) = session.find_by_id("wnd[0]/usr/btnBUTTON6".to_string()) {
        if let Some(button) = btn.downcast::<GuiButton>() {
            button.press()?;
        }
    }
    
    // Apply variant if provided
    if let Some(variant_name) = &params.sap_variant_name {
        if !variant_name.is_empty() {
            // Variant select window
            if let Ok(wnd) = session.find_by_id("wnd[0]".to_string()) {
                if let Some(main_window) = wnd.downcast::<GuiMainWindow>() {
                    main_window.send_v_key(17)?; // F4 key for variant selection
                }
            }
            
            // Traditional variant select
            if let Ok(txt) = session.find_by_id("wnd[1]/usr/txtV-LOW".to_string()) {
                if let Some(text_field) = txt.downcast::<GuiTextField>() {
                    text_field.set_text(variant_name.clone())?;
                }
            }
            
            // Clear name
            if let Ok(txt) = session.find_by_id("wnd[1]/usr/txtENAME-LOW".to_string()) {
                if let Some(text_field) = txt.downcast::<GuiTextField>() {
                    text_field.set_text("".to_string())?;
                }
            }
            
            // Enter
            if let Ok(wnd) = session.find_by_id("wnd[1]".to_string()) {
                if let Some(modal_window) = wnd.downcast::<GuiModalWindow>() {
                    modal_window.send_v_key(0)?; // Enter key
                }
            }
            
            // Close
            if let Ok(wnd) = session.find_by_id("wnd[1]".to_string()) {
                if let Some(modal_window) = wnd.downcast::<GuiModalWindow>() {
                    modal_window.send_v_key(8)?; // F8 key to close
                }
            }
        }
    }
    
    // Press Multi Delivery button
    if let Ok(btn) = session.find_by_id("wnd[0]/usr/btn%_IT_VBELN_%_APP_%-VALU_PUSH".to_string()) {
        if let Some(button) = btn.downcast::<GuiButton>() {
            button.press()?;
        }
    }
    
    // Clear previous entries
    if let Ok(window) = session.find_by_id("wnd[1]".to_string()) {
        if let Some(modal_window) = window.downcast::<GuiModalWindow>() {
            modal_window.send_v_key(24)?; // Shift+F8 to clear entries
        }
    }
    
    // Enter delivery numbers
    let mut j = 0;
    for delivery_number in &params.delivery_numbers {
        let input_field_id = format!("wnd[1]/usr/tabsTAB_STRIP/tabpSIVA/ssubSCREEN_HEADER:SAPLALDB:3010/tblSAPLALDBSINGLE/ctxtRSCSEL_255-SLOW_I[1,{}]", j);
        if let Ok(txt) = session.find_by_id(input_field_id) {
            if let Some(text_field) = txt.downcast::<GuiCTextField>() {
                text_field.set_text(delivery_number.clone())?;
                j += 1;
            }
        }
    }
    
    // Close Multi-Window
    if let Ok(window) = session.find_by_id("wnd[1]".to_string()) {
        if let Some(modal_window) = window.downcast::<GuiModalWindow>() {
            modal_window.send_v_key(8)?; // F8 key to close
        }
    }
    
    // Execute
    if let Ok(wnd) = session.find_by_id("wnd[0]".to_string()) {
        if let Some(main_window) = wnd.downcast::<GuiMainWindow>() {
            main_window.send_v_key(8)?; // F8 key to execute
        }
    }
    
    // Press F5 (Refresh)
    if let Ok(wnd) = session.find_by_id("wnd[0]".to_string()) {
        if let Some(main_window) = wnd.downcast::<GuiMainWindow>() {
            main_window.send_v_key(5)?; // F5 key to refresh
        }
    }
    
    println!("Starting VL06O date update for {} deliveries", params.delivery_numbers.len());
    
    // Initialize counter and changes vector
    let mut counter = 0;
    let mut changes = Vec::new();
    
    // Press F13 (Shift+F1) to begin processing - this is the key step that starts the update process
    if let Ok(wnd) = session.find_by_id("wnd[0]".to_string()) {
        if let Some(main_window) = wnd.downcast::<GuiMainWindow>() {
            main_window.send_v_key(13)?; // F13 key (Shift+F1) to process
            println!("Pressed Shift+F1 to begin processing");
        }
    }
    
    // Check for popup message after starting processing
    let err_ctrl = exist_ctrl(session, 1, "", true)?;
    if err_ctrl.cband {
        let msg = get_sap_text_errors(session, 1, "/usr/txtMESSTXT1", 10, None)?;
        println!("Popup message: {}", msg);
        if msg.contains("loading") {
            if let Ok(wnd) = session.find_by_id("wnd[0]".to_string()) {
                if let Some(main_window) = wnd.downcast::<GuiMainWindow>() {
                    main_window.send_v_key(0)?; // Enter key to close
                    println!("Closed loading message popup");
                }
            }
        }
    }
    
    // Loop through deliveries
    loop {

        // Check if date field exists
        let date_field = exist_ctrl(session, 0, r"/usr/tabsTAXI_TABSTRIP_OVERVIEW/tabpT\01/ssubSUBSCREEN_BODY:SAPMV50A:1102/ctxtLIKP-WADAT", true)?;

        match date_field.cband {
            true => { /* no-op */ }
            false =>  { break }
        }

        // Get delivery number
        let delivery_number = if let Ok(txt) = session.find_by_id("wnd[0]/usr/subSUBSCREEN_HEADER:SAPMV50A:1502/ctxtLIKP-VBELN".to_string()) {
            if let Some(text_field) = txt.downcast::<GuiCTextField>() {
                text_field.text()?
            } else {
                "Unknown".to_string()
            }
        } else {
            "Unknown".to_string()
        };
        
        println!("Working with delivery ({})", delivery_number);

        // Select item overview tab (1st)
        if let Ok(tab) = session.find_by_id(r"wnd[0]/usr/tabsTAXI_TABSTRIP_OVERVIEW/tabpT\01".to_string()) {
            if let Some(tab_strip) = tab.downcast::<GuiTab>() {
                tab_strip.select()?;
                println!("Selected item overview tab");
            }
        }
        
        // Check if date is changeable
        let date_changeable = if let Ok(txt) = session.find_by_id(r"wnd[0]/usr/tabsTAXI_TABSTRIP_OVERVIEW/tabpT\01/ssubSUBSCREEN_BODY:SAPMV50A:1102/ctxtLIKP-WADAT".to_string()) {
            if let Some(text_field) = txt.downcast::<GuiCTextField>() {
                text_field.changeable()?
            } else {
                false
            }
        } else {
            false
        };
        
        if !date_changeable {
            println!("Delivery date not changeable for delivery {}", delivery_number);
            
            // F3 back
            if let Ok(window) = session.find_by_id("wnd[0]".to_string()) {
                if let Some(wnd) = window.downcast::<GuiMainWindow>() {
                    wnd.send_v_key(3)?;
                    println!("Pressed back button to skip non-changeable delivery");
                }
            }
        } else {
            // Get original date
            let original_date = if let Ok(txt) = session.find_by_id(r"wnd[0]/usr/tabsTAXI_TABSTRIP_OVERVIEW/tabpT\01/ssubSUBSCREEN_BODY:SAPMV50A:1102/ctxtLIKP-WADAT".to_string()) {
                if let Some(text_field) = txt.downcast::<GuiCTextField>() {
                    text_field.text()?
                } else {
                    "Unknown".to_string()
                }
            } else {
                "Unknown".to_string()
            };
            
            // Change date
            if let Ok(txt) = session.find_by_id(r"wnd[0]/usr/tabsTAXI_TABSTRIP_OVERVIEW/tabpT\01/ssubSUBSCREEN_BODY:SAPMV50A:1102/ctxtLIKP-WADAT".to_string()) {
                if let Some(text_field) = txt.downcast::<GuiCTextField>() {
                    text_field.set_text(target_date_str.clone())?;
                }
            }
            
            println!("Changing date from ({}) to ({})", original_date, target_date_str);
            
            // Enter loop to handle any messages
            loop {
                // Get status bar message
                let status_msg = hit_ctrl(session, 0, "/sbar", "Text", "Get", "")?;
                if !status_msg.is_empty() {
                    println!("Status bar: {}", status_msg);
                }
                
                // Press Enter
                if let Ok(wnd) = session.find_by_id("wnd[0]".to_string()) {
                    if let Some(main_window) = wnd.downcast::<GuiMainWindow>() {
                        main_window.send_v_key(0)?; // Enter key
                    }
                }
                
                // Check if status bar is empty or has short message
                let new_status = hit_ctrl(session, 0, "/sbar", "Text", "Get", "")?;
                if new_status.len() <= 1 {
                    break;
                }
            }
            
            // Record change if date was actually changed
            if original_date != target_date_str {
                changes.push((delivery_number.clone(), original_date));
            }
            
            // Save
            if let Ok(wnd) = session.find_by_id("wnd[0]".to_string()) {
                if let Some(main_window) = wnd.downcast::<GuiMainWindow>() {
                    main_window.send_v_key(11)?; // Ctrl+S to save
                    println!("Saved changes for delivery {}", delivery_number);
                }
            }
            
                
            // Handle confirmation popup - "Continue with next delivery?" - Always click Yes
            let popup_ctrl = exist_ctrl(session, 1, "/usr/btnSPOP-OPTION1", true)?;
            if popup_ctrl.cband {
                if let Ok(btn) = session.find_by_id("wnd[1]/usr/btnSPOP-OPTION1".to_string()) {
                    if let Some(button) = btn.downcast::<GuiButton>() {
                        button.press()?;
                        println!("Clicked 'Yes' on popup to continue with next delivery");
                    }
                }
            }
                
            // Handle any other popups (like loading messages)
            let err_popup = exist_ctrl(session, 1, "", true)?;
            if err_popup.cband {
                let msg = get_sap_text_errors(session, 1, "/usr/txtMESSTXT1", 10, None)?;
                println!("Popup message: {}", msg);
                if msg.contains("loading") {
                    if let Ok(wnd) = session.find_by_id("wnd[0]".to_string()) {
                        if let Some(main_window) = wnd.downcast::<GuiMainWindow>() {
                            main_window.send_v_key(0)?; // Enter key to close
                            println!("Closed loading message popup");
                        }
                    }
                }
            }
                
            // Check for "currently being" message in status bar
            let bar_msg = hit_ctrl(session, 0, "/sbar", "Text", "Get", "")?;
            if bar_msg.contains("currently being") {
                println!("Error: ({})", bar_msg);
                    
                // F3 to exit
                if let Ok(wnd) = session.find_by_id("wnd[0]".to_string()) {
                    if let Some(main_window) = wnd.downcast::<GuiMainWindow>() {
                        main_window.send_v_key(3)?; // F3 key to exit
                        println!("Pressed F3 to exit due to error");
                    }
                }
            }
        }
        
        // Increment counter
        counter += 1;
        
        // Check for popup message for next deliv
        if let Ok(button) = session.find_by_id("wnd[1]/usr/btnSPOP-OPTION1".to_string()) {
            if let Some(btn) = button.downcast::<GuiButton>() {
                eprintln!("pressing 'yes' button on popup");
                btn.press()?
            }
        } else  {
            // no popup
            println!("No popup, guess we're done");
            break;
        }
    }
    
    // Check for any final status bar message
    let bar_msg = hit_ctrl(session, 0, "/sbar", "Text", "Get", "")?;
    if bar_msg.contains("restricted") {
        println!("Error: ({})", bar_msg);
        
        // F3 to exit
        if let Ok(wnd) = session.find_by_id("wnd[0]".to_string()) {
            if let Some(main_window) = wnd.downcast::<GuiMainWindow>() {
                main_window.send_v_key(3)?; // F3 key to exit
                println!("Pressed F3 to exit due to error");
            }
        }
    }
    
    println!("Done... with ({}) items.", counter);
    
    Ok((counter, changes))
}

/// Check if items were pasted successfully in the multi-selection window
///
/// This is a helper function for run_export
fn check_multi_paste(
    session: &GuiSession,
    tcode: &str,
    wnd_idx: i32,
    row_idx: i32,
) -> Result<bool> {
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

    println!(
        "No items found in multi-selection window for tcode: {}",
        tcode
    );
    Ok(false)
}
