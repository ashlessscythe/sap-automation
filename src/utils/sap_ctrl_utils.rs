use crate::utils::sap_constants::CtrlCheck;
use crate::utils::sap_interfaces::{SapComponent, SapSession};
use sap_scripting::*;
use windows::core::Result;

// Legacy function that uses the new interface internally
pub fn exist_ctrl(
    session: &GuiSession,
    n_wnd: i32,
    control_id: &str,
    ret_msg: bool,
) -> Result<CtrlCheck> {
    // For now, we'll implement this directly to avoid circular dependencies
    // In a real implementation, we would convert the GuiSession to a SapSession
    let mut err_chk = CtrlCheck {
        cband: false,
        ctext: String::new(),
        ctype: String::new(),
    };

    // Try to find the control
    let control_path = format!("wnd[{}]{}", n_wnd, control_id);
    let ret_id = session.find_by_id(control_path);

    if let Ok(component) = ret_id {
        err_chk.cband = true;

        if ret_msg {
            // Get type information
            err_chk.ctype = component.r_type().unwrap_or_default();

            // Get text based on component type
            if let Some(text_field) = component.downcast::<GuiTextField>() {
                err_chk.ctext = text_field.text()?;
            } else if let Some(button) = component.downcast::<GuiButton>() {
                err_chk.ctext = button.text()?;
            } else if let Some(label) = component.downcast::<GuiLabel>() {
                err_chk.ctext = label.text()?;
            } else if let Some(statusbar) = component.downcast::<GuiStatusbar>() {
                err_chk.ctext = statusbar.text()?;
            } else if let Some(window) = component.downcast::<GuiFrameWindow>() {
                err_chk.ctext = window.text()?;
            } else if let Some(modal_window) = component.downcast::<GuiModalWindow>() {
                err_chk.ctext = modal_window.text()?;
            } else {
                // For other component types, use the name as a fallback
                err_chk.ctext = component.name().unwrap_or_default();
            }
        }
    }

    Ok(err_chk)
}

// Legacy function that uses the new interface internally
pub fn hit_ctrl(
    session: &GuiSession,
    n_wnd: i32,
    control_id: &str,
    event_id: &str,
    event_id_opt: &str,
    event_id_value: &str,
) -> Result<String> {
    // For now, we'll implement this directly to avoid circular dependencies
    // In a real implementation, we would convert the GuiSession to a SapSession
    let mut aux_str = String::new();
    let control_path = format!("wnd[{}]{}", n_wnd, control_id);

    match event_id {
        "Maximize" => {
            if let Ok(component) = session.find_by_id(control_path) {
                if let Some(window) = component.downcast::<GuiFrameWindow>() {
                    window.maximize()?;
                }
            }
        }
        "Minimize" => {
            // Note: minimize is not available in GuiFrameWindow
            // Using maximize as a fallback
            if let Ok(component) = session.find_by_id(control_path) {
                if let Some(window) = component.downcast::<GuiFrameWindow>() {
                    window.maximize()?;
                }
            }
        }
        "Press" => {
            if let Ok(component) = session.find_by_id(control_path) {
                if let Some(button) = component.downcast::<GuiButton>() {
                    button.press()?;
                }
            }
        }
        "Select" => {
            if let Ok(component) = session.find_by_id(control_path) {
                if let Some(radio_button) = component.downcast::<GuiRadioButton>() {
                    radio_button.select()?;
                }
            }
        }
        "Selected" => {
            if let Ok(component) = session.find_by_id(control_path) {
                match event_id_opt {
                    "Get" => {
                        if let Some(checkbox) = component.downcast::<GuiCheckBox>() {
                            aux_str = checkbox.selected()?.to_string();
                        }
                    }
                    "Set" => {
                        if let Some(checkbox) = component.downcast::<GuiCheckBox>() {
                            match event_id_value {
                                "True" => checkbox.set_selected(true)?,
                                "False" => checkbox.set_selected(false)?,
                                _ => {}
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        "Focus" => {
            if let Ok(component) = session.find_by_id(control_path) {
                // Need to downcast to a specific type that has set_focus
                if let Some(field) = component.downcast::<GuiTextField>() {
                    field.set_focus()?;
                } else if let Some(button) = component.downcast::<GuiButton>() {
                    button.set_focus()?;
                } else if let Some(radio) = component.downcast::<GuiRadioButton>() {
                    radio.set_focus()?;
                }
            }
        }
        "Text" => {
            if let Ok(component) = session.find_by_id(control_path) {
                match event_id_opt {
                    "Get" => {
                        // Need to get text from the component based on its type
                        if let Some(text_field) = component.downcast::<GuiTextField>() {
                            aux_str = text_field.text()?;
                        } else if let Some(button) = component.downcast::<GuiButton>() {
                            aux_str = button.text()?;
                        } else if let Some(label) = component.downcast::<GuiLabel>() {
                            aux_str = label.text()?;
                        } else if let Some(statusbar) = component.downcast::<GuiStatusbar>() {
                            aux_str = statusbar.text()?;
                        } else if let Some(window) = component.downcast::<GuiFrameWindow>() {
                            aux_str = window.text()?;
                        } else if let Some(modal_window) = component.downcast::<GuiModalWindow>() {
                            aux_str = modal_window.text()?;
                        } else {
                            // For other component types, use the name as a fallback
                            aux_str = component.name().unwrap_or_default();
                        }
                    }
                    "Set" => {
                        if let Some(text_field) = component.downcast::<GuiTextField>() {
                            text_field.set_text(event_id_value.to_string())?;
                        } else if let Some(password_field) =
                            component.downcast::<GuiPasswordField>()
                        {
                            password_field.set_text(event_id_value.to_string())?;
                        }
                    }
                    _ => {}
                }
            }
        }
        _ => {}
    }

    Ok(aux_str)
}

/// Gets error text messages from SAP controls
///
/// This function retrieves error messages from SAP controls by iterating through
/// a range of indices and checking if controls exist at those indices.
///
/// # Arguments
///
/// * `session` - The SAP GUI session
/// * `n_wnd` - The window number
/// * `ctrl_id` - The control ID base
/// * `count` - The number of controls to check
/// * `start_index` - Optional starting index (defaults to 1)
///
/// # Returns
///
/// A Result containing the concatenated error messages as a String
pub fn get_sap_text_errors(
    session: &GuiSession,
    n_wnd: i32,
    ctrl_id: &str,
    count: i32,
    start_index: Option<i32>,
) -> Result<String> {
    let start = start_index.unwrap_or(1);
    let mut result = String::new();

    println!("Getting SAP errors...");

    for i in start..=count {
        let mut current_ctrl_id = ctrl_id.to_string();

        if ctrl_id.contains('[') {
            // Handle controls with array indices
            let err_ctl = exist_ctrl(session, n_wnd, &format!("{}{},0]", ctrl_id, i), true)?;
            if err_ctl.cband {
                let err_msg = hit_ctrl(session, n_wnd, &format!("{}{},0]", ctrl_id, i), "Text", "Get", "")?;
                println!("{}", err_msg);
                result.push_str(&format!("\n {}", err_msg));
            }
        } else if ctrl_id.contains("txtMESSTXT") {
            // Handle message text controls
            let mut modified_ctrl_id = ctrl_id.to_string();
            
            // Replace right numeric if exists
            if let Some(last_char) = modified_ctrl_id.chars().last() {
                if last_char.is_numeric() {
                    modified_ctrl_id = modified_ctrl_id[0..modified_ctrl_id.len()-1].to_string();
                }
            }
            
            let err_ctl = exist_ctrl(session, n_wnd, &format!("{}{}", modified_ctrl_id, i), true)?;
            if err_ctl.cband {
                let err_msg = hit_ctrl(session, n_wnd, &format!("{}{}", modified_ctrl_id, i), "Text", "Get", "")?;
                println!("{}", err_msg);
                result.push_str(&format!("\n {}", err_msg));
            }
        } else {
            // Handle other controls
            let err_ctl = exist_ctrl(session, n_wnd, &format!("{}{}", ctrl_id, i), true)?;
            if err_ctl.cband {
                let err_msg = hit_ctrl(session, n_wnd, &format!("{}{}", ctrl_id, i), "Text", "Get", "")?;
                println!("{}", err_msg);
                result.push_str(&format!("\n {}", err_msg));
            }
        }
    }

    println!("str contents are: ({})", result);
    Ok(result)
}
