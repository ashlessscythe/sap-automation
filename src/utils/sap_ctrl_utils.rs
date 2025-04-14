use sap_scripting::*;
use windows::core::Result;
use crate::utils::sap_constants::CtrlCheck;
use crate::utils::sap_interfaces::{SapSession, SapComponent};

// Legacy function that uses the new interface internally
pub fn exist_ctrl(session: &GuiSession, n_wnd: i32, control_id: &str, ret_msg: bool) -> Result<CtrlCheck> {
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
pub fn hit_ctrl(session: &GuiSession, n_wnd: i32, control_id: &str, event_id: &str, event_id_opt: &str, event_id_value: &str) -> Result<String> {
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
        },
        "Minimize" => {
            // Note: minimize is not available in GuiFrameWindow
            // Using maximize as a fallback
            if let Ok(component) = session.find_by_id(control_path) {
                if let Some(window) = component.downcast::<GuiFrameWindow>() {
                    window.maximize()?;
                }
            }
        },
        "Press" => {
            if let Ok(component) = session.find_by_id(control_path) {
                if let Some(button) = component.downcast::<GuiButton>() {
                    button.press()?;
                }
            }
        },
        "Select" => {
            if let Ok(component) = session.find_by_id(control_path) {
                if let Some(radio_button) = component.downcast::<GuiRadioButton>() {
                    radio_button.select()?;
                }
            }
        },
        "Selected" => {
            if let Ok(component) = session.find_by_id(control_path) {
                match event_id_opt {
                    "Get" => {
                        if let Some(checkbox) = component.downcast::<GuiCheckBox>() {
                            aux_str = checkbox.selected()?.to_string();
                        }
                    },
                    "Set" => {
                        if let Some(checkbox) = component.downcast::<GuiCheckBox>() {
                            match event_id_value {
                                "True" => checkbox.set_selected(true)?,
                                "False" => checkbox.set_selected(false)?,
                                _ => {}
                            }
                        }
                    },
                    _ => {}
                }
            }
        },
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
        },
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
                    },
                    "Set" => {
                        if let Some(text_field) = component.downcast::<GuiTextField>() {
                            text_field.set_text(event_id_value.to_string())?;
                        } else if let Some(password_field) = component.downcast::<GuiPasswordField>() {
                            password_field.set_text(event_id_value.to_string())?;
                        }
                    },
                    _ => {}
                }
            }
        },
        _ => {}
    }
    
    Ok(aux_str)
}
