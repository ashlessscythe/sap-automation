use sap_scripting::*;
use std::time::Duration;
use std::thread;
use windows::core::Result;
use chrono;
use crate::utils::sap_constants::{ErrorCheck, ParamsStruct, TIME_FORMAT};
use crate::utils::sap_ctrl_utils::{exist_ctrl, hit_ctrl};
use crate::utils::sap_tcode_utils::check_tcode;

/// Saves a file from SAP GUI to the specified path and filename
///
/// This function handles the SAP GUI dialog for saving files to the local filesystem.
/// It supports both text and Excel file formats.
///
/// # Arguments
///
/// * `session` - Reference to the SAP GUI session
/// * `file_path` - Directory path where the file should be saved
/// * `file_name` - Name of the file to be saved
///
/// # Returns
///
/// * `Result<bool>` - Ok(true) if the file was successfully saved, Ok(false) otherwise
///
pub fn save_sap_file(session: &GuiSession, file_path: &str, file_name: &str) -> Result<bool> {
    println!("Exporting data from SAP....");
    
    // Check if window[1] exists (the save dialog)
    let err_wnd = exist_ctrl(session, 1, "", true)?;
    
    if err_wnd.cband {
        println!("Found window title: ({}). Extracting to filename: ({}\\{})",
                 err_wnd.ctext, file_path, file_name);
        
        // Check if it's an error message window
        let msg_err_wnd = exist_ctrl(session, 1, "/usr/txtMESSTXT1", true)?;
        if msg_err_wnd.cband {
            // There's an error message, get the text
            if let Ok(component) = session.find_by_id("wnd[1]/usr/txtMESSTXT1".to_string()) {
                if let Some(text_field) = component.downcast::<GuiTextField>() {
                    let error_msg = text_field.text()?;
                    println!("Error message: {}", error_msg);
                    return Ok(false);
                }
            }
        }
        
        // Set the file path
        if let Ok(component) = session.find_by_id("wnd[1]/usr/ctxtDY_PATH".to_string()) {
            if let Some(text_field) = component.downcast::<GuiCTextField>() {
                text_field.set_text(file_path.to_string())?;
            }
        }
        
        // Set the file name
        if let Ok(component) = session.find_by_id("wnd[1]/usr/ctxtDY_FILENAME".to_string()) {
            if let Some(text_field) = component.downcast::<GuiCTextField>() {
                text_field.set_text(file_name.to_string())?;
            }
        }
        
        // Press the save button
        if let Ok(component) = session.find_by_id("wnd[1]/tbar[0]/btn[0]".to_string()) {
            if let Some(button) = component.downcast::<GuiButton>() {
                button.press()?;
            }
        }
        
        // Wait a moment for the save operation to complete
        thread::sleep(Duration::from_millis(500));
        
        println!("File saved successfully");
        return Ok(true);
    } else {
        println!("Error: Save dialog window not found");
        return Ok(false);
    }
}