use sap_scripting::*;
use windows::core::Result;

use crate::utils::sap_ctrl_utils::exist_ctrl;
use crate::utils::sap_file_utils::{get_tcode_file_path, save_sap_file};
use crate::utils::sap_tcode_utils::*;

/// Struct to hold 149 material export parameters
#[derive(Debug)]
pub struct Report149MaterialParams {
    pub variant: Option<String>,
    pub material: String,
    pub plant: String,
    pub trailer: String,
    pub date_low: String,
    pub date_high: String,
    pub t_code: String,
}

impl Default for Report149MaterialParams {
    fn default() -> Self {
        Self {
            variant: None,
            material: String::new(),
            plant: String::new(),
            trailer: "trl".to_string(),
            date_low: String::new(),
            date_high: String::new(),
            t_code: "y_dn3_47000149".to_string(),
        }
    }
}

/// Run 149 material report export with the given parameters
///
/// This function is a port of the VBA code from docs/149_not_tsp.md
pub fn run_material_export(session: &GuiSession, params: &Report149MaterialParams) -> Result<bool> {
    println!("Running 149 material report export...");

    // Check if tCode is active
    if !assert_tcode(session, &params.t_code, Some(0))? {
        println!("Failed to activate {} transaction", params.t_code);
        return Ok(false);
    }

    // Use variant if provided
    if let Some(ref variant) = params.variant {
        if !variant_select(session, &params.t_code, variant.as_str())? {
            println!(
                "Failed to select variant '{}' for tCode '{}'",
                variant, &params.t_code
            );
            return Ok(false);
        }
        println!("[DEBUG] Variant selection complete");
    }

    // Set material number
    if let Ok(txt) = session.find_by_id("wnd[0]/usr/ctxtS_MATNR-LOW".to_string()) {
        if let Some(text_field) = txt.downcast::<GuiCTextField>() {
            text_field.set_text(params.material.clone())?;
        }
    }

    // Set date low
    if let Ok(txt) = session.find_by_id("wnd[0]/usr/ctxtS_D_DATE-LOW".to_string()) {
        if let Some(text_field) = txt.downcast::<GuiCTextField>() {
            text_field.set_text(params.date_low.clone())?;
        }
    }

    // Set date high
    if let Ok(txt) = session.find_by_id("wnd[0]/usr/ctxtS_D_DATE-HIGH".to_string()) {
        if let Some(text_field) = txt.downcast::<GuiCTextField>() {
            text_field.set_text(params.date_high.clone())?;
        }
    }

    // Set focus back to "All" radio button
    if let Ok(radio) = session.find_by_id("wnd[0]/usr/radR_ALL".to_string()) {
        if let Some(radio_button) = radio.downcast::<GuiRadioButton>() {
            radio_button.set_focus()?;
        }
    }

    // Send F8 to execute
    if let Ok(wnd) = session.find_by_id("wnd[0]".to_string()) {
        if let Some(window) = wnd.downcast::<GuiMainWindow>() {
            window.send_v_key(8)?; // F8
        }
    }

    // Wait a moment for any error windows to appear
    std::thread::sleep(std::time::Duration::from_millis(1000));

    // Check for error windows more robustly
    let mut error_detected = false;

    // Try to check for error window at wnd[1]
    if let Ok(err_wnd) = exist_ctrl(session, 1, "", true) {
        if err_wnd.ctext.contains("Information")
            || err_wnd.ctext.contains("Error")
            || err_wnd.ctext.contains("Warning")
            || err_wnd.ctext.contains("Message")
        {
            println!(
                "DEBUG: Error window found with text: '{}', sending enter and exiting...",
                err_wnd.ctext
            );
            if let Ok(wnd) = session.find_by_id("wnd[1]".to_string()) {
                if let Some(window) = wnd.downcast::<GuiModalWindow>() {
                    window.send_v_key(0)?; // Enter
                    error_detected = true;
                }
            }
        }
    }

    // Also check for error messages in the main window status bar
    if !error_detected {
        if let Ok(status) = session.find_by_id("wnd[0]/sbar".to_string()) {
            if let Some(statusbar) = status.downcast::<GuiStatusbar>() {
                if let Ok(status_text) = statusbar.text() {
                    if status_text.contains("Error") || status_text.contains("No data") {
                        println!("DEBUG: Error detected in status bar: '{}'", status_text);
                        error_detected = true;
                    }
                }
            }
        }
    }

    if error_detected {
        return Ok(false);
    }

    println!("DEBUG: No error window found, continuing...");

    // Export to file
    if let Err(e) = export_material_to_file(session, &params.material) {
        println!("Error exporting material data: {}", e);
        return Ok(false);
    }

    println!(
        "Successfully exported material data for: {}",
        params.material
    );
    Ok(true)
}

/// Export the current material data to a file
fn export_material_to_file(session: &GuiSession, material: &str) -> Result<()> {
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
    let (file_path, base_file_name) = get_tcode_file_path("y_149_material", "txt");
    // Insert material into the filename before the extension
    let file_name = if let Some(stripped) = base_file_name.strip_suffix(".txt") {
        format!("{}-{}.txt", stripped, material)
    } else {
        format!("{}-{}.txt", base_file_name, material)
    };

    // Save SAP file using the utility (handles setting path and filename in dialog)
    save_sap_file(session, &file_path, &file_name, Some(false))?;

    // Wait a bit for export to complete
    std::thread::sleep(std::time::Duration::from_millis(2000));

    Ok(())
}
