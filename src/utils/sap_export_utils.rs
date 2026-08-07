use sap_scripting::*;
use windows::core::Result;

use crate::utils::sap_ctrl_utils::exist_ctrl;
use crate::utils::sap_file_utils::{get_tcode_file_path, save_sap_file};

/// Map export_type 0..=4 to file extension
pub fn export_type_to_extension(export_type: u8) -> &'static str {
    match export_type {
        0 | 1 => "txt", // unconverted or text with tabs
        2 => "rtf",     // rich text format
        3 => "html",    // HTML format
        4 => "",        // clipboard - no file
        _ => "txt",     // default to txt for unknown values
    }
}

/// Perform a "Save list in file" export for the active ALV grid.
/// Assumes the "SAVE LIST IN FILE..." dialog (wnd[1]) is already open.
/// Selects the radio option based on export_type, confirms, and saves to disk unless clipboard.
///
/// Returns the full path of the written file, or an empty string for clipboard export.
pub fn export_local_file(
    session: &GuiSession,
    tcode_for_path: &str,
    export_type: u8,
    filename_suffix: Option<&str>,
) -> Result<String> {
    // Select export format based on export_type (0..=4)
    let radio_index = match export_type {
        0 => "0", // unconverted
        1 => "1", // text with tabs (default)
        2 => "2", // rich text format
        3 => "3", // HTML format
        4 => "4", // clipboard
        _ => "1", // default to text with tabs for unknown values
    };

    let radio_id = format!(
        "wnd[1]/usr/subSUBSCREEN_STEPLOOP:SAPLSPO5:0150/sub:SAPLSPO5:0150/radSPOPLI-SELFLAG[{},0]",
        radio_index
    );

    if let Ok(radio) = session.find_by_id(radio_id) {
        if let Some(radio_button) = radio.downcast::<GuiRadioButton>() {
            radio_button.select()?;
            radio_button.set_focus()?;
        }
    }

    // Press Enter to confirm selection in dlg
    if let Ok(tbar) = session.find_by_id("wnd[1]/tbar[0]/btn[0]".to_string()) {
        if let Some(button) = tbar.downcast::<GuiButton>() {
            button.press()?;
        }
    }

    // Determine extension
    let file_extension = export_type_to_extension(export_type);

    // For clipboard export, don't create a file
    if export_type == 4 {
        println!("Exporting to clipboard - no file created");
        return Ok(String::new());
    }

    // Wait for save dialog to appear
    let _ = exist_ctrl(session, 1, "", true)?; // best-effort check

    // Build base path and name
    let (file_path, base_file_name) = get_tcode_file_path(tcode_for_path, file_extension);

    // Compose filename, optionally appending suffix before extension
    let file_name = if let Some(suffix) = filename_suffix {
        if file_extension.is_empty() {
            format!("{}-{}", base_file_name, suffix)
        } else if let Some(stripped) = base_file_name.strip_suffix(&format!(".{}", file_extension))
        {
            format!("{}-{}.{}", stripped, suffix, file_extension)
        } else {
            format!("{}-{}.{}", base_file_name, suffix, file_extension)
        }
    } else {
        base_file_name
    };

    // Save via utility
    save_sap_file(session, &file_path, &file_name, Some(false))?;

    // Small delay for export to finish
    std::thread::sleep(std::time::Duration::from_millis(2000));

    let full_path = format!("{}{}{}", file_path, std::path::MAIN_SEPARATOR, file_name);
    println!("Successfully exported data to {}", full_path);

    Ok(full_path)
}
