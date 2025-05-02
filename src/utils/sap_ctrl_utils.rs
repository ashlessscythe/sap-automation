use sap_scripting::*;
use windows::core::Result;

use super::sap_wnd_utils::*;

/// Check if a control exists in the SAP GUI
///
/// This function checks if a control exists in the SAP GUI at the specified window index
/// and with the specified ID suffix.
pub fn exist_ctrl(
    session: &GuiSession,
    wnd_idx: i32,
    id_suffix: &str,
    silent: bool,
) -> Result<CtrlBand> {
    let wnd_id = format!("wnd[{}]", wnd_idx);
    let full_id = format!("{}{}", wnd_id, id_suffix);
    let full_id_for_log = full_id.clone();

    let ctrl_result = session.find_by_id(full_id);
    let cband = ctrl_result.is_ok();
    
    // Initialize default values for ctext and ctype
    let mut ctext = String::new();
    let mut ctype = String::new();
    
    // If control exists, try to get its text and type
    if cband {
        if let Ok(component) = ctrl_result {
            // Try to get component type using the component's name
            if let Ok(name) = component.name() {
                ctype = name;
            }
            
            // Try to get text based on component type
            if let Some(window) = component.downcast::<GuiFrameWindow>() {
                ctext = window.text().unwrap_or_default();
            } else if let Some(window) = component.downcast::<GuiMainWindow>() {
                ctext = window.text().unwrap_or_default();
            } else if let Some(window) = component.downcast::<GuiModalWindow>() {
                ctext = window.text().unwrap_or_default();
            } else if let Some(label) = component.downcast::<GuiLabel>() {
                ctext = label.text().unwrap_or_default();
            } else if let Some(text_field) = component.downcast::<GuiTextField>() {
                ctext = text_field.text().unwrap_or_default();
            } else if let Some(text_field) = component.downcast::<GuiCTextField>() {
                ctext = text_field.text().unwrap_or_default();
            } else if let Some(statusbar) = component.downcast::<GuiStatusbar>() {
                ctext = statusbar.text().unwrap_or_default();
            }
        }
    }

    if !cband && !silent {
        println!("Control not found: {}", full_id_for_log);
    }

    Ok(CtrlBand { cband, ctext, ctype })
}

/// Struct to hold the result of exist_ctrl
#[derive(Debug)]
pub struct CtrlBand {
    pub cband: bool,
    pub ctext: String,
    pub ctype: String,
}

/// Get text from SAP GUI controls
///
/// This function gets text from SAP GUI controls at the specified window index
/// and with the specified ID suffix.
pub fn hit_ctrl(
    session: &GuiSession,
    wnd_idx: i32,
    id_suffix: &str,
    prop: &str,
    action: &str,
    value: &str,
) -> Result<String> {
    let wnd_id = format!("wnd[{}]", wnd_idx);
    let full_id = format!("{}{}", wnd_id, id_suffix);
    let full_id_for_log = full_id.clone();

    let ctrl_result = session.find_by_id(full_id);
    match ctrl_result {
        Ok(ctrl) => {
            if action == "Get" {
                if prop == "Text" {
                    if let Some(text_field) = ctrl.downcast::<GuiTextField>() {
                        return text_field.text();
                    } else if let Some(text_field) = ctrl.downcast::<GuiCTextField>() {
                        return text_field.text();
                    } else if let Some(label) = ctrl.downcast::<GuiLabel>() {
                        return label.text();
                    } else if let Some(statusbar) = ctrl.downcast::<GuiStatusbar>() {
                        return statusbar.text();
                    } else {
                        return Ok("".to_string());
                    }
                } else {
                    return Ok("".to_string());
                }
            } else if action == "Set" {
                if prop == "Text" {
                    if let Some(text_field) = ctrl.downcast::<GuiTextField>() {
                        text_field.set_text(value.to_string())?;
                    } else if let Some(text_field) = ctrl.downcast::<GuiCTextField>() {
                        text_field.set_text(value.to_string())?;
                    }
                }
                return Ok("".to_string());
            } else {
                return Ok("".to_string());
            }
        }
        Err(_) => {
            println!("Control not found: {}", full_id_for_log);
            return Ok("".to_string());
        }
    }
}

/// Get text from SAP GUI error messages
///
/// This function gets text from SAP GUI error messages at the specified window index
/// and with the specified ID suffix.
pub fn get_sap_text_errors(
    session: &GuiSession,
    wnd_idx: i32,
    id_suffix: &str,
    max_lines: i32,
    prefix: Option<&str>,
) -> Result<String> {
    let mut result = String::new();
    let prefix_str = prefix.unwrap_or("");

    for i in 1..=max_lines {
        let id = format!("{}{}", id_suffix, i);
        let text = hit_ctrl(session, wnd_idx, &id, "Text", "Get", "")?;
        if !text.is_empty() {
            if !result.is_empty() {
                result.push_str("\n");
            }
            result.push_str(&format!("{}{}", prefix_str, text));
        }
    }

    Ok(result)
}

/// Paste values into a scrollable table in SAP GUI
///
/// This function pastes values into a scrollable table in SAP GUI at the specified window index.
/// It handles scrolling through the table to paste all values, even when there are thousands.
pub fn paste_values_with_scroll(
    session: &GuiSession,
    wnd_idx: i32,
    table_id: &str,
    field_pattern: &str,
    values: &[String],
    batch_size: usize,
) -> Result<bool> {
    if values.is_empty() {
        return Ok(true);
    }

    let full_table_id = format!("wnd[{}]/usr/{}", wnd_idx, table_id);
    
    // Check if table exists
    let table_exists = exist_ctrl(session, wnd_idx, &format!("/usr/{}", table_id), true)?;
    if !table_exists.cband {
        println!("Table not found: {}", full_table_id);
        return Ok(false);
    }

    let mut values_pasted = 0;
    let mut current_position = 0;
    let mut page_idx = 0;

    while values_pasted < values.len() {
        // Set scrollbar position if needed (not for the first batch)
        if values_pasted > 0 {
            // Try to set scrollbar position by sending key presses
            // This is a workaround since we can't directly set the scrollbar position
            // Send Page Down key to scroll down
            if let Ok(window) = session.find_by_id(format!("wnd[{}]", wnd_idx)) {
                if let Some(wnd) = window.downcast::<GuiModalWindow>() {
                    wnd.send_v_key(82)?; // Page Down key
                    page_idx += 1;
                }
            }
            println!("Scrolled down {} pages", page_idx);
        }

        // Paste a batch of values
        let end_idx = std::cmp::min(values_pasted + batch_size, values.len());
        let mut local_index = 0;

        for i in values_pasted..end_idx {
            let field_id = format!("{}/ctxtRSCSEL_255-SLOW_I[1,{}]", table_id, local_index);
            let full_field_id = format!("wnd[{}]/usr/{}", wnd_idx, field_id);
            
            if let Ok(field) = session.find_by_id(full_field_id) {
                if let Some(text_field) = field.downcast::<GuiCTextField>() {
                    text_field.set_text(values[i].clone())?;
                    local_index += 1;
                } else {
                    // Field not found or not the right type, might be at the end of visible area
                    break;
                }
            } else {
                // Field not found, might be at the end of visible area
                break;
            }
        }

        // Update counters
        values_pasted += local_index;
        current_position += batch_size as i32;

        // If we couldn't paste any values in this batch, we might be at the end of the table
        if local_index == 0 {
            println!("Could not paste any more values at position {}", current_position);
            break;
        }

        println!("Pasted {} values so far", values_pasted);
    }

    println!("Total values pasted: {}/{}", values_pasted, values.len());
    Ok(values_pasted > 0)
}
