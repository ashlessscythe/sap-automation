use sap_scripting::*;
use windows::core::Result;

use crate::utils::choose_layout_utils::choose_layout_149;
use crate::utils::sap_ctrl_utils::exist_ctrl;
use crate::utils::sap_export_utils::export_local_file;
use crate::utils::sap_tcode_utils::{assert_tcode, variant_select};

/// Struct to hold 149 material export parameters
#[derive(Debug)]
pub struct Report149MaterialParams {
    pub variant: Option<String>,
    pub layout: Option<String>,
    pub material: String,
    pub plant: String,
    pub signi: String,
    pub date_low: String,
    pub date_high: String,
    pub export_type: u8,
}

impl Default for Report149MaterialParams {
    fn default() -> Self {
        Self {
            variant: None,
            layout: None,
            material: String::new(),
            plant: String::new(),
            signi: String::new(),
            date_low: String::new(),
            date_high: String::new(),
            export_type: 1, // Default to text with tabs
        }
    }
}

/// Run 149 material report export with the given parameters
///
/// This function is a port of the VBA code from docs/149_not_tsp.md
pub fn run_export(session: &GuiSession, params: &Report149MaterialParams) -> Result<bool> {
    println!("Running 149 material report export...");

    // Check if tCode is active
    if !assert_tcode(session, "y_dn3_47000149", Some(0))? {
        println!("Failed to activate y_dn3_47000149 transaction");
        return Ok(false);
    }

    // Use variant if provided
    if let Some(ref variant) = params.variant {
        if !variant_select(session, "y_dn3_47000149", variant.as_str())? {
            println!(
                "Failed to select variant '{}' for tCode '{}'",
                variant, "y_dn3_47000149"
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

    // Use layout if provided
    if let Some(layout) = &params.layout {
        if let Err(e) = choose_layout_149(session, layout) {
            println!("Error choosing layout: {}", e);
        }
    }

    println!("DEBUG: No error window found, continuing...");

    // Export to file
    if let Err(e) = export_material_to_file(session, &params.material, params.export_type) {
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
fn export_material_to_file(session: &GuiSession, material: &str, export_type: u8) -> Result<()> {
    // Select Export menu to open the local file export dialog
    if let Ok(menu) = session.find_by_id("wnd[0]/mbar/menu[0]/menu[3]/menu[2]".to_string()) {
        if let Some(menu_item) = menu.downcast::<GuiMenu>() {
            menu_item.select()?;
        }
    }

    // Delegate to shared utility to handle radio selection and saving
    export_local_file(session, "y_149_material", export_type, Some(material))
}
