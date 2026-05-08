//! SAP control probes; large API used from different T-code modules and tests.
#![allow(dead_code)]

use sap_scripting::*;
use windows::core::Result;

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

    Ok(CtrlBand {
        cband,
        ctext,
        ctype,
    })
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
                        text_field.text()
                    } else if let Some(text_field) = ctrl.downcast::<GuiCTextField>() {
                        text_field.text()
                    } else if let Some(label) = ctrl.downcast::<GuiLabel>() {
                        label.text()
                    } else if let Some(statusbar) = ctrl.downcast::<GuiStatusbar>() {
                        statusbar.text()
                    } else {
                        Ok("".to_string())
                    }
                } else {
                    Ok("".to_string())
                }
            } else if action == "Set" {
                if prop == "Text" {
                    if let Some(text_field) = ctrl.downcast::<GuiTextField>() {
                        text_field.set_text(value.to_string())?;
                    } else if let Some(text_field) = ctrl.downcast::<GuiCTextField>() {
                        text_field.set_text(value.to_string())?;
                    }
                }
                Ok("".to_string())
            } else {
                Ok("".to_string())
            }
        }
        Err(_) => {
            println!("Control not found: {}", full_id_for_log);
            Ok("".to_string())
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
                result.push('\n');
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
/// This is a faithful port of the VBA implementation from deliv_packages.md
pub fn paste_values_with_scroll(
    session: &GuiSession,
    wnd_idx: i32,
    table_id: &str,
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

    // Clean the values to ensure no trailing commas
    let clean_values: Vec<String> = values
        .iter()
        .map(|v| v.trim().trim_end_matches(',').to_string())
        .collect();

    println!(
        "Starting to paste {} values using VBA-style scrolling",
        clean_values.len()
    );

    // Process values in batches, similar to VBA approach
    let mut values_pasted = 0;
    let mut scroll_position = 0;

    while values_pasted < clean_values.len() {
        // Set scrollbar position to show the current batch
        // Based on VBA code, we increment by 7 for each batch
        if values_pasted > 0 {
            scroll_position += 7;
            // Try to scroll down using Page Down (more reliable)
            if let Ok(window) = session.find_by_id(format!("wnd[{}]", wnd_idx)) {
                if let Some(wnd) = window.downcast::<GuiModalWindow>() {
                    // Send Page Down keys until we find an empty row at index 1
                    let mut page_down_count = 0;
                    loop {
                        wnd.send_v_key(82)?; // Page Down key
                        page_down_count += 1;
                        std::thread::sleep(std::time::Duration::from_millis(50));
                        // Check if row 1 is empty (indicating we've scrolled to a new area)
                        let check_field_id = format!("{}/ctxtRSCSEL_255-SLOW_I[1,1]", table_id);
                        let check_full_id = format!("wnd[{}]/usr/{}", wnd_idx, check_field_id);
                        if let Ok(field) = session.find_by_id(check_full_id) {
                            if let Some(text_field) = field.downcast::<GuiCTextField>() {
                                if let Ok(text) = text_field.text() {
                                    if text.is_empty() {
                                        println!(
                                            "Found empty row at index 1 after {} Page Down presses",
                                            page_down_count
                                        );
                                        break;
                                    }
                                }
                            }
                        }
                        // Safety limit to prevent infinite loop
                        if page_down_count >= 5 {
                            println!("Reached safety limit of 5 Page Down presses");
                            break;
                        }
                    }
                }
            }
        }
        // Calculate how many values we can paste in this batch
        let remaining_values = clean_values.len() - values_pasted;
        let current_batch_size = std::cmp::min(batch_size, remaining_values);
        println!(
            "Pasting batch: positions {} to {} (scroll_pos: {})",
            values_pasted,
            values_pasted + current_batch_size - 1,
            scroll_position
        );
        // Determine correct start index after scrolling
        let mut start_index = 0;
        if values_pasted > 0 {
            let field_id = format!("{}/ctxtRSCSEL_255-SLOW_I[1,0]", table_id);
            let full_field_id = format!("wnd[{}]/usr/{}", wnd_idx, field_id);
            if let Ok(field) = session.find_by_id(full_field_id) {
                if let Some(text_field) = field.downcast::<GuiCTextField>() {
                    if let Ok(text) = text_field.text() {
                        if !text.trim().is_empty() {
                            start_index = 1;
                        }
                    }
                }
            }
        }
        let mut local_index = start_index;
        for i in 0..current_batch_size {
            let global_index = values_pasted + i;
            // Try to find the field using the correct pattern from VBA
            let field_id = format!("{}/ctxtRSCSEL_255-SLOW_I[1,{}]", table_id, local_index);
            let full_field_id = format!("wnd[{}]/usr/{}", wnd_idx, field_id);
            if let Ok(field) = session.find_by_id(full_field_id.clone()) {
                if let Some(text_field) = field.downcast::<GuiCTextField>() {
                    let clean_value = clean_values[global_index].clone();
                    println!(
                        "  Pasted value {} at local index {}",
                        clean_value, local_index
                    );
                    text_field.set_text(clean_value)?;
                    local_index += 1;
                } else {
                    println!("  Field found but not a text field: {}", full_field_id);
                    break;
                }
            } else {
                println!("  Field not found: {}", full_field_id);
                // If we can't find the field, we might need to scroll
                break;
            }
        }
        // Update counters
        values_pasted += local_index - start_index;
        // If we couldn't paste any values in this batch, we might be at the end
        if local_index == start_index {
            println!("Could not paste any values in this batch, stopping");
            break;
        }
        // Ensure we don't exceed the total number of values
        if values_pasted > clean_values.len() {
            values_pasted = clean_values.len();
        }
        println!(
            "Pasted {} values so far ({} remaining)",
            values_pasted,
            clean_values.len() - values_pasted
        );
        // If we've pasted all values, we're done
        if values_pasted >= clean_values.len() {
            break;
        }
    }

    println!(
        "Total values pasted: {}/{}",
        values_pasted,
        clean_values.len()
    );

    // Return true if we pasted at least some values
    Ok(values_pasted > 0)
}

/// Get the current vertical scrollbar position of a table
///
/// This function gets the current vertical scrollbar position of a table
/// using the same approach as the VBA implementation
pub fn get_scrollbar_position(session: &GuiSession, wnd_idx: i32, table_id: &str) -> Result<i32> {
    let scroll_result = hit_ctrl(
        session,
        wnd_idx,
        &format!("/usr/{}", table_id),
        "Position",
        "GetV",
        "",
    );

    match scroll_result {
        Ok(position_str) => {
            let position = position_str.parse::<i32>().unwrap_or(0);
            println!("Current scrollbar position: {}", position);
            Ok(position)
        }
        Err(e) => {
            println!("Failed to get scrollbar position: {}", e);
            Ok(0)
        }
    }
}
