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
            "Failed to select variant '{}' for tCode '{}'",
            &params.variant, "y_dn3_47000149"
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
                    "Failed to select variant '{}' for tCode '{}'",
                    &params.variant, "y_dn3_47000149"
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

/// Export the current data to a file
fn export_to_file(session: &GuiSession, plant: &str, export_type: u8) -> Result<()> {
    // Select Export menu to open the local file export dialog
    if let Ok(menu) = session.find_by_id("wnd[0]/mbar/menu[0]/menu[3]/menu[2]".to_string()) {
        if let Some(menu_item) = menu.downcast::<GuiMenu>() {
            menu_item.select()?;
        }
    }

    // Delegate to shared utility to handle radio selection and saving
    export_local_file(session, "y_149", export_type, Some(plant))
}
