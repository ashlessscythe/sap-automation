use sap_scripting::*;
use windows::core::Result;

use crate::utils::choose_layout_utils::*;
use crate::utils::config_types::SapConfig;
use crate::utils::sap_export_utils::export_local_file;
use crate::utils::sap_tcode_utils::*;

/// Struct to hold 149 export parameters
#[derive(Debug)]
pub struct Report149Params {
    pub variant: String,
    pub plants: Vec<String>,
    pub export_type: u8,
}

impl Default for Report149Params {
    fn default() -> Self {
        Self {
            variant: "149_unload".to_string(),
            plants: Vec::new(),
            export_type: 1, // Default to text with tabs
        }
    }
}

/// Run 149 report export with the given parameters
///
/// This function is a port of the VBA code from docs/149.md
pub fn run_export(session: &GuiSession, params: &Report149Params) -> Result<bool> {
    println!("Running 149 report export...");

    // Check if tCode is active
    if !assert_tcode(session, "y_dn3_47000149", Some(0))? {
        println!("Failed to activate y_dn3_47000149 transaction");
        return Ok(false);
    }

    // Use the standard variant_select utility
    if !variant_select(session, "y_dn3_47000149", &params.variant)? {
        println!(
            "Failed to select variant '{}' for tCode 'y_dn3_47000149'",
            &params.variant
        );
        return Ok(false);
    }
    println!("[DEBUG] Variant selection complete");

    // Skip plant filtering if no plants specified, otherwise process each plant
    if params.plants.is_empty() {
        println!("No plants specified, skipping plant filtering step");

        // Press Enter to proceed without plant filtering
        if let Ok(wnd) = session.find_by_id("wnd[0]".to_string()) {
            if let Some(window) = wnd.downcast::<GuiMainWindow>() {
                window.send_v_key(0)?; // Enter
                println!("[DEBUG] Sent Enter key to skip plant filtering");
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(400));

        // Execute (F8)
        println!("[DEBUG] Pressing F8 to execute...");
        if let Ok(wnd) = session.find_by_id("wnd[0]".to_string()) {
            if let Some(window) = wnd.downcast::<GuiMainWindow>() {
                window.send_v_key(8)?; // F8
                println!("[DEBUG] Sent F8 key");
            }
        }

        // choose layout if configured
        if let Ok(cfg) = SapConfig::load() {
            if let Some(tcode_cfg) = cfg.get_tcode_config("y_dn3_47000149", Some(true)) {
                if let Some(layout) = tcode_cfg.get("layout") {
                    if let Err(e) = choose_layout_149(session, layout) {
                        println!("Error choosing layout: {}", e);
                        return Ok(false);
                    }
                }
            }
        }

        // Export to file
        if let Err(e) = export_to_file(session, "ALL", params.export_type) {
            println!("Error exporting data: {}", e);
            return Ok(false);
        }
        println!("Successfully exported data for all plants");
    } else {
        // Process each plant individually
        for plant in &params.plants {
            println!("Processing plant: {}", plant);

            // Start tcode for each plant (ensures clean state)
            if !assert_tcode(session, "y_dn3_47000149", Some(0))? {
                println!("Failed to activate y_dn3_47000149 transaction");
                continue;
            }

            // Use the standard variant_select utility
            if !variant_select(session, "y_dn3_47000149", &params.variant)? {
                println!(
                    "Failed to select variant '{}' for tCode 'y_dn3_47000149'",
                    &params.variant
                );
                continue;
            }
            println!("[DEBUG] Variant selection complete, attempting to find plant field...");

            // Try to find and set the plant field
            let mut plant_set = false;
            for attempt in 0..3 {
                println!("[DEBUG] Attempt {} to find plant field...", attempt + 1);
                if let Ok(txt) = session.find_by_id("wnd[0]/usr/ctxtS_WERKS-LOW".to_string()) {
                    if let Some(text_field) = txt.downcast::<GuiCTextField>() {
                        println!("[DEBUG] Found plant field, setting value to: {}", plant);
                        text_field.set_text(plant.clone())?;
                        plant_set = true;
                        break;
                    } else {
                        println!(
                            "[DEBUG] Plant field found but could not downcast to GuiTextField"
                        );
                    }
                } else {
                    println!("[DEBUG] Plant field not found on attempt {}", attempt + 1);
                }
                std::thread::sleep(std::time::Duration::from_millis(500));
            }
            if !plant_set {
                println!(
                    "[DEBUG] Could not find or set plant field for plant: {}",
                    plant
                );
                continue;
            }
            println!("[DEBUG] Plant field set, pressing Enter...");
            // Press Enter
            if let Ok(wnd) = session.find_by_id("wnd[0]".to_string()) {
                if let Some(window) = wnd.downcast::<GuiMainWindow>() {
                    window.send_v_key(0)?; // Enter
                    println!("[DEBUG] Sent Enter key");
                }
            } else {
                println!("[DEBUG] Could not find main window to send Enter");
            }
            std::thread::sleep(std::time::Duration::from_millis(400));
            println!("[DEBUG] Pressing F8 to execute...");
            // Execute (F8)
            if let Ok(wnd) = session.find_by_id("wnd[0]".to_string()) {
                if let Some(window) = wnd.downcast::<GuiMainWindow>() {
                    window.send_v_key(8)?; // F8
                    println!("[DEBUG] Sent F8 key");
                }
            } else {
                println!("[DEBUG] Could not find main window to send F8");
            }

            // choose layout if configured
            if let Ok(cfg) = SapConfig::load() {
                if let Some(tcode_cfg) = cfg.get_tcode_config("y_dn3_47000149", Some(true)) {
                    if let Some(layout) = tcode_cfg.get("layout") {
                        if let Err(e) = choose_layout_149(session, layout) {
                            println!("Error choosing layout: {}", e);
                            continue;
                        }
                    }
                }
            }

            // Export to file
            if let Err(e) = export_to_file(session, plant, params.export_type) {
                println!("Error exporting data: {}", e);
                continue;
            }
            println!("Successfully exported data for plant: {}", plant);

            // Return to main screen (simulate /n or /o or go back)
            if let Ok(wnd) = session.find_by_id("wnd[0]".to_string()) {
                if let Some(window) = wnd.downcast::<GuiMainWindow>() {
                    window.send_v_key(15)?; // F3 (Back)
                    println!("[DEBUG] Sent F3 to return to main screen");
                }
            }
        }
    }

    Ok(true)
}

/// Parameters for 149 export filtered by delivery numbers (inbond flow).
#[derive(Debug, Clone)]
pub struct Report149ByDeliveryParams {
    pub variant: String,
    pub layout: String,
    /// Column display names for Change Layout fallback when `layout` is missing/not found.
    pub layout_columns: Vec<String>,
    pub delivery_numbers: Vec<String>,
    pub export_type: u8,
    pub filename_suffix: Option<String>,
}

impl Default for Report149ByDeliveryParams {
    fn default() -> Self {
        Self {
            variant: "inb_ship".to_string(),
            layout: "inb_ship".to_string(),
            layout_columns: crate::utils::choose_layout_utils::inbond_default_layout_columns(),
            delivery_numbers: Vec::new(),
            export_type: 1,
            filename_suffix: Some("inbond".to_string()),
        }
    }
}

/// Run 149 report filtered by delivery numbers, apply layout, and export to a local file.
///
/// Returns the exported file path on success.
pub fn run_export_by_delivery(
    session: &GuiSession,
    params: &Report149ByDeliveryParams,
) -> Result<Option<String>> {
    println!("Running 149 report export by delivery...");

    if params.delivery_numbers.is_empty() {
        println!("No delivery numbers provided for 149 export");
        return Ok(None);
    }

    if !assert_tcode(session, "y_dn3_47000149", Some(0))? {
        println!("Failed to activate y_dn3_47000149 transaction");
        return Ok(None);
    }

    if !variant_select(session, "y_dn3_47000149", &params.variant)? {
        println!(
            "Failed to select variant '{}' for tCode 'y_dn3_47000149'",
            &params.variant
        );
        return Ok(None);
    }

    // Clear plant filter (removed 260807 variant fills in plant)
    // if let Ok(txt) = session.find_by_id("wnd[0]/usr/ctxtS_WERKS-LOW".to_string()) {
    //     if let Some(text_field) = txt.downcast::<GuiCTextField>() {
    //         text_field.set_text("".to_string())?;
    //     }
    // }

    // Open delivery multi-select
    if let Ok(btn) = session.find_by_id("wnd[0]/usr/btn%_S_DELIV_%_APP_%-VALU_PUSH".to_string()) {
        if let Some(button) = btn.downcast::<GuiButton>() {
            button.press()?;
        }
    } else {
        println!("Delivery multi-select button not found");
        return Ok(None);
    }

    // Clear previous entries
    if let Ok(window) = session.find_by_id("wnd[1]".to_string()) {
        if let Some(modal_window) = window.downcast::<GuiModalWindow>() {
            modal_window.send_v_key(16)?;
        }
    }

    let table_id = "tabsTAB_STRIP/tabpSIVA/ssubSCREEN_HEADER:SAPLALDB:3010/tblSAPLALDBSINGLE";
    let paste_ok = crate::utils::sap_ctrl_utils::paste_values_with_scroll(
        session,
        1,
        table_id,
        &params.delivery_numbers,
        7,
        "txt",
    )?;
    if !paste_ok {
        println!("Failed to paste delivery numbers into 149");
        return Ok(None);
    }

    // Close multi window (toolbar confirm, matching recording)
    if let Ok(btn) = session.find_by_id("wnd[1]/tbar[0]/btn[8]".to_string()) {
        if let Some(button) = btn.downcast::<GuiButton>() {
            button.press()?;
        }
    } else if let Ok(window) = session.find_by_id("wnd[1]".to_string()) {
        if let Some(modal_window) = window.downcast::<GuiModalWindow>() {
            modal_window.send_v_key(8)?;
        }
    }

    // Execute via toolbar F8 button (recording: tbar[1]/btn[8])
    if let Ok(btn) = session.find_by_id("wnd[0]/tbar[1]/btn[8]".to_string()) {
        if let Some(button) = btn.downcast::<GuiButton>() {
            button.press()?;
        }
    } else if let Ok(wnd) = session.find_by_id("wnd[0]".to_string()) {
        if let Some(window) = wnd.downcast::<GuiMainWindow>() {
            window.send_v_key(8)?;
        }
    }

    // Status bar only (ignore trailing wnd[1] popup per plan)
    if let Ok(s) = crate::utils::sap_ctrl_utils::hit_ctrl(session, 0, "/sbar", "Text", "Get", "") {
        if !s.is_empty() {
            eprintln!("149 status bar: {}", s);
            let lower = s.to_lowercase();
            if lower.contains("no data")
                || lower.contains("not found")
                || lower.contains("no items")
            {
                println!("149 returned no data: {}", s);
                return Ok(None);
            }
        }
    }

    // Select existing layout, or set up default inbond columns and optionally save
    match ensure_inbond_layout_149(session, &params.layout, &params.layout_columns) {
        Ok((true, Some(saved_name))) => {
            if let Err(e) = crate::inbond_shipment::persist_inbond_layout_149(&saved_name) {
                println!(
                    "Layout saved in SAP as '{}', but failed to update config.toml: {}",
                    saved_name, e
                );
            }
        }
        Ok((true, None)) => {}
        Ok((false, _)) => {
            println!(
                "Warning: could not apply or set up layout '{}'; exporting as-is",
                params.layout
            );
        }
        Err(e) => {
            println!("Error ensuring layout '{}': {}", params.layout, e);
            return Ok(None);
        }
    }

    // Open export dialog then save
    if let Ok(menu) = session.find_by_id("wnd[0]/mbar/menu[0]/menu[3]/menu[2]".to_string()) {
        if let Some(menu_item) = menu.downcast::<GuiMenu>() {
            menu_item.select()?;
        }
    }

    let suffix = params.filename_suffix.as_deref();
    match export_local_file(session, "y_149", params.export_type, suffix) {
        Ok(path) if !path.is_empty() => {
            println!("149 by-delivery export completed: {}", path);
            Ok(Some(path))
        }
        Ok(_) => {
            println!("149 export completed but no file path returned");
            Ok(None)
        }
        Err(e) => {
            println!("Error exporting 149 data: {}", e);
            Ok(None)
        }
    }
}

/// Export the current data to a file
fn export_to_file(session: &GuiSession, plant: &str, export_type: u8) -> Result<()> {
    // Select Export menu to open the local file export dialog
    if let Ok(menu) = session.find_by_id("wnd[0]/mbar/menu[0]/menu[3]/menu[2]".to_string()) {
        if let Some(menu_item) = menu.downcast::<GuiMenu>() {
            menu_item.select()?;
        }
    }

    // Delegate to shared utility to handle radio selection and saving
    let _ = export_local_file(session, "y_149", export_type, Some(plant))?;
    Ok(())
}
