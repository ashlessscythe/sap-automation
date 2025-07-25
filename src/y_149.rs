use chrono::{Local, NaiveDateTime};
use sap_scripting::*;
use std::path::Path;
use windows::core::Result;

use crate::utils::sap_file_utils::{get_tcode_file_path, save_sap_file};
use crate::utils::sap_tcode_utils::*;
use crate::utils::sap_wnd_utils::*;

/// Struct to hold 149 export parameters
#[derive(Debug)]
pub struct Report149Params {
    pub variant: String,
    pub plants: Vec<String>,
    pub t_code: String,
}

impl Default for Report149Params {
    fn default() -> Self {
        Self {
            variant: "149_unload".to_string(),
            plants: Vec::new(),
            t_code: "y_dn3_47000149".to_string(),
        }
    }
}

/// Run 149 report export with the given parameters
///
/// This function is a port of the VBA code from docs/149.md
pub fn run_export(session: &GuiSession, params: &Report149Params) -> Result<bool> {
    println!("Running 149 report export...");

    // Check if tCode is active
    if !assert_tcode(session, &params.t_code, Some(0))? {
        println!("Failed to activate {} transaction", params.t_code);
        return Ok(false);
    }

    // Process each plant
    for plant in &params.plants {
        println!("Processing plant: {}", plant);

        // Start tcode for each plant (ensures clean state)
        if !assert_tcode(session, &params.t_code, Some(0))? {
            println!("Failed to activate {} transaction", params.t_code);
            continue;
        }

        // Use the standard variant_select utility
        if !variant_select(session, &params.t_code, &params.variant)? {
            println!(
                "Failed to select variant '{}' for tCode '{}'",
                &params.variant, &params.t_code
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
                    println!("[DEBUG] Plant field found but could not downcast to GuiTextField");
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
        // Export to file
        if let Err(e) = export_to_file(session, plant) {
            println!("Error exporting for plant {}: {}", plant, e);
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

    Ok(true)
}

/// Export the current data to a file
fn export_to_file(session: &GuiSession, plant: &str) -> Result<()> {
    // Select Export menu
    if let Ok(menu) = session.find_by_id("wnd[0]/mbar/menu[0]/menu[3]/menu[2]".to_string()) {
        if let Some(menu_item) = menu.downcast::<GuiMenu>() {
            menu_item.select()?;
        }
    }

    // Select "Text with tabs" option
    if let Ok(radio) = session.find_by_id(
        "wnd[1]/usr/subSUBSCREEN_STEPLOOP:SAPLSPO5:0150/sub:SAPLSPO5:0150/radSPOPLI-SELFLAG[1,0]"
            .to_string(),
    ) {
        if let Some(radio_button) = radio.downcast::<GuiRadioButton>() {
            radio_button.select()?;
            radio_button.set_focus()?;
        }
    }

    // Press Enter
    if let Ok(tbar) = session.find_by_id("wnd[1]/tbar[0]/btn[0]".to_string()) {
        if let Some(button) = tbar.downcast::<GuiButton>() {
            button.press()?;
        }
    }

    // Use the standard file path and timestamp logic
    let (file_path, base_file_name) = get_tcode_file_path("y_149", "txt");
    // Insert plant into the filename before the extension
    let file_name = if let Some(stripped) = base_file_name.strip_suffix(".txt") {
        format!("{}-{}.txt", stripped, plant)
    } else {
        format!("{}-{}.txt", base_file_name, plant)
    };

    // Save SAP file using the utility (handles setting path and filename in dialog)
    save_sap_file(session, &file_path, &file_name, Some(false))?;

    // Wait a bit for export to complete
    std::thread::sleep(std::time::Duration::from_millis(2000));

    Ok(())
}
