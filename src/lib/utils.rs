use crate::*;
use std::collections::HashMap;
use std::io;
use chrono;
use aes_gcm::{
    aead::{Aead, KeyInit, OsRng},
    Aes256Gcm, Nonce,
};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use rand::Rng;

// Constants for encryption
pub const KEY_FILE_SUFFIX: &str = "_key.bin";
pub const NONCE_SIZE: usize = 12; // 96 bits for AES-GCM

/// Represents a window title and type
pub struct WindowInfo {
    pub window_type: String,
    pub window_title: String,
}

/// Represents the result of a control check
pub struct ControlCheck {
    pub exists: bool,
    pub text: String,
    pub control_type: String,
}

/// Represents the result of an error check
pub struct ErrorCheck {
    pub success: bool,
    pub message: String,
}

/// Check if a control exists in a SAP window
pub fn control_exists(session: &GuiSession, window_id: i32, control_id: &str, return_text: bool) -> crate::Result<ControlCheck> {
    let window_path = format!("wnd[{}]{}", window_id, control_id);
    
    // Try to find the control
    let result = session.find_by_id(window_path.clone());
    
    match result {
        Ok(component) => {
            let mut check = ControlCheck {
                exists: true,
                text: String::new(),
                control_type: String::new(),
            };
            
            // If requested, get the text and type
            if return_text {
                if let Some(vcomp) = component.downcast::<GuiVComponent>() {
                    if let Ok(text) = vcomp.text() {
                        check.text = text;
                    }
                }
                
                if let Ok(control_type) = component.r_type() {
                    check.control_type = control_type;
                }
            }
            
            Ok(check)
        },
        Err(_) => {
            Ok(ControlCheck {
                exists: false,
                text: String::new(),
                control_type: String::new(),
            })
        }
    }
}

/// Interact with a SAP control
pub fn hit_control(session: &GuiSession, window_id: i32, control_id: &str, 
                  event_id: &str, event_opt: &str, event_value: &str) -> crate::Result<String> {
    let window_path = format!("wnd[{}]{}", window_id, control_id);
    let component = session.find_by_id(window_path)?;
    
    match event_id {
        "Maximize" => {
            if let Some(window) = component.downcast::<GuiFrameWindow>() {
                window.maximize()?;
            }
            Ok(String::new())
        },
        "Minimize" => {
            if let Some(window) = component.downcast::<GuiFrameWindow>() {
                window.iconify()?;
            }
            Ok(String::new())
        },
        "Press" => {
            if let Some(button) = component.downcast::<GuiButton>() {
                button.press()?;
            }
            Ok(String::new())
        },
        "Select" => {
            if let Some(radio) = component.downcast::<GuiRadioButton>() {
                radio.select()?;
            } else if let Some(tab) = component.downcast::<GuiTab>() {
                tab.select()?;
            } else if let Some(menu) = component.downcast::<GuiMenu>() {
                menu.select()?;
            }
            Ok(String::new())
        },
        "Selected" => {
            match event_opt {
                "Get" => {
                    if let Some(checkbox) = component.downcast::<GuiCheckBox>() {
                        let selected = checkbox.selected()?;
                        Ok(selected.to_string())
                    } else {
                        Ok(String::new())
                    }
                },
                "Set" => {
                    if let Some(checkbox) = component.downcast::<GuiCheckBox>() {
                        let value = event_value == "True";
                        checkbox.set_selected(value)?;
                    }
                    Ok(String::new())
                },
                _ => Ok(String::new())
            }
        },
        "Focus" => {
            if let Some(vcomp) = component.downcast::<GuiVComponent>() {
                vcomp.set_focus()?;
            }
            Ok(String::new())
        },
        "Text" => {
            match event_opt {
                "Get" => {
                    if let Some(vcomp) = component.downcast::<GuiVComponent>() {
                        let text = vcomp.text()?;
                        Ok(text)
                    } else {
                        Ok(String::new())
                    }
                },
                "Set" => {
                    if let Some(text_field) = component.downcast::<GuiTextField>() {
                        text_field.set_text(event_value.to_string())?;
                    } else if let Some(password_field) = component.downcast::<GuiPasswordField>() {
                        password_field.set_text(event_value.to_string())?;
                    } else if let Some(combo_box) = component.downcast::<GuiComboBox>() {
                        combo_box.set_text(event_value.to_string())?;
                    }
                    Ok(String::new())
                },
                _ => Ok(String::new())
            }
        },
        "Position" => {
            match &event_opt[..3] {
                "Get" => {
                    match &event_opt[3..] {
                        "V" => {
                            if let Some(container) = component.downcast::<GuiScrollContainer>() {
                                if let Ok(scrollbar) = container.vertical_scrollbar() {
                                    if let Some(scroll) = scrollbar.downcast::<GuiScrollbar>() {
                                        let position = scroll.position()?;
                                        return Ok(position.to_string());
                                    }
                                }
                            }
                        },
                        "H" => {
                            if let Some(container) = component.downcast::<GuiScrollContainer>() {
                                if let Ok(scrollbar) = container.horizontal_scrollbar() {
                                    if let Some(scroll) = scrollbar.downcast::<GuiScrollbar>() {
                                        let position = scroll.position()?;
                                        return Ok(position.to_string());
                                    }
                                }
                            }
                        },
                        _ => {}
                    }
                    Ok(String::new())
                },
                "Set" => {
                    match &event_opt[3..] {
                        "V" => {
                            if let Some(container) = component.downcast::<GuiScrollContainer>() {
                                if let Ok(scrollbar) = container.vertical_scrollbar() {
                                    if let Some(scroll) = scrollbar.downcast::<GuiScrollbar>() {
                                        let position = event_value.parse::<i32>().unwrap_or(0);
                                        scroll.set_position(position)?;
                                    }
                                }
                            }
                        },
                        "H" => {
                            if let Some(container) = component.downcast::<GuiScrollContainer>() {
                                if let Ok(scrollbar) = container.horizontal_scrollbar() {
                                    if let Some(scroll) = scrollbar.downcast::<GuiScrollbar>() {
                                        let position = event_value.parse::<i32>().unwrap_or(0);
                                        scroll.set_position(position)?;
                                    }
                                }
                            }
                        },
                        _ => {}
                    }
                    Ok(String::new())
                },
                _ => Ok(String::new())
            }
        },
        "ContextButton" => {
            // Since GuiToolbarControl doesn't implement HasSAPType, we need to handle it differently
            // We can use the component directly if we know it's a toolbar
            if let Ok(toolbar_type) = component.r_type() {
                if toolbar_type == "GuiToolbarControl" {
                    // We can't directly call methods on GuiToolbarControl
                    // For now, we'll just log that we would press the button
                    println!("Would press context button: {}", event_value);
                }
            }
            Ok(String::new())
        },
        "ContextMenu" => {
            if let Some(shell) = component.downcast::<GuiShell>() {
                shell.select_context_menu_item(event_value.to_string())?;
            }
            Ok(String::new())
        },
        _ => Ok(String::new())
    }
}

/// Check the window type and handle it accordingly
pub fn check_window(session: &GuiSession, window_id: i32, params: &HashMap<String, String>) -> crate::Result<ErrorCheck> {
    let window_path = format!("wnd[{}]", window_id);
    let window = session.find_by_id(window_path.clone())?;
    
    let window_type = window.r_type()?;
    let window_title = if let Some(vcomp) = window.downcast::<GuiVComponent>() {
        if let Ok(title) = vcomp.text() { 
            title 
        } else { 
            String::new() 
        }
    } else { 
        String::new() 
    };
    
    let _window_info = WindowInfo {
        window_type: window_type.clone(),
        window_title: window_title.clone(),
    };
    
    let mut error_check = ErrorCheck {
        success: false,
        message: String::new(),
    };
    
    match window_type.as_str() {
        "GuiFrameWindow" => {
            error_check.success = true;
        },
        "GuiMainWindow" => {
            match window_title.as_str() {
                "SAP Easy Access" => {
                    // Maximize SAP Easy Access Window
                    if let Ok(check) = control_exists(session, window_id, "", true) {
                        if check.exists {
                            hit_control(session, window_id, "", "Focus", "", "")?;
                            hit_control(session, window_id, "", "Maximize", "", "")?;
                        }
                    }
                    error_check.success = true;
                },
                "SAP R/3" | "SAP" => {
                    // Login screen
                    if let Some(client_id) = params.get("clientID") {
                        hit_control(session, window_id, "/usr/txtRSYST-MANDT", "Text", "Set", client_id)?;
                    }
                    
                    if let Some(user) = params.get("user") {
                        hit_control(session, window_id, "/usr/txtRSYST-BNAME", "Text", "Set", user)?;
                    }
                    
                    if let Some(pass) = params.get("pass") {
                        hit_control(session, window_id, "/usr/pwdRSYST-BCODE", "Text", "Set", pass)?;
                    }
                    
                    if let Some(language) = params.get("language") {
                        hit_control(session, window_id, "/usr/txtRSYST-LANGU", "Text", "Set", language)?;
                    }
                    
                    // Press Enter
                    hit_control(session, window_id, "/tbar[0]/btn[0]", "Press", "", "")?;
                    
                    error_check.success = true;
                    error_check.message = format!("{}", chrono::Local::now().format("%m-%d-%y %H:%M:%S"));
                },
                _ => {
                    error_check.success = true;
                }
            }
        },
        "GuiModalWindow" => {
            match window_title.as_str() {
                "Log Off" => {
                    // Press No button
                    hit_control(session, window_id, "/usr/btnSPOP-OPTION2", "Press", "", "")?;
                    error_check.success = true;
                    error_check.message = format!("{}", chrono::Local::now().format("%m-%d-%y %H:%M:%S"));
                },
                "System Messages" => {
                    // Press OK button
                    hit_control(session, window_id, "/tbar[0]/btn[0]", "Press", "", "")?;
                    error_check.success = true;
                    error_check.message = format!("{}", chrono::Local::now().format("%m-%d-%y %H:%M:%S"));
                },
                _ => {
                    // Handle other modal windows
                    error_check.success = true;
                }
            }
        },
        _ => {
            // Unknown window type
            error_check.success = false;
        }
    }
    
    Ok(error_check)
}

/// Check if a transaction code is active
pub fn check_tcode(session: &GuiSession, tcode: &str, run: bool, kill_popups: bool) -> crate::Result<bool> {
    // Get current transaction
    let current_tcode = session.info()?.transaction()?;
    
    // Check if on the right transaction
    if current_tcode.to_lowercase().contains(&tcode.to_lowercase()) {
        println!("tCode ({}) is active", tcode);
        return Ok(true);
    } else if run {
        // Run the transaction if requested
        println!("tCode mismatch, attempting to run tCode ({})", tcode);
        session.start_transaction(tcode.to_string())?;
        
        // Wait a bit for the transaction to load
        std::thread::sleep(std::time::Duration::from_millis(1000));
        
        // Recursively check if we're now on the right transaction
        return check_tcode(session, tcode, false, false);
    }
    
    // If option to kill popups is set, restart the transaction
    if kill_popups {
        println!("Option to killpopups passed, restarting tCode ({})", tcode);
        session.start_transaction(tcode.to_string())?;
        
        // Wait a bit for the transaction to load
        std::thread::sleep(std::time::Duration::from_millis(1000));
        
        // Recursively check if we're now on the right transaction
        return check_tcode(session, tcode, false, false);
    }
    
    println!("tCode mismatch. Current tCode is ({}), need ({})", current_tcode, tcode);
    Ok(false)
}

/// Close all popup windows
pub fn close_popups(session: &GuiSession) -> crate::Result<bool> {
    println!("Closing all popups");
    
    let max_tries = 5;
    
    // Start from window 5 and work down to 1
    for window_id in (1..=5).rev() {
        let mut try_count = 0;
        while try_count < max_tries {
            if let Ok(check) = control_exists(session, window_id, "", true) {
                if check.exists {
                    println!("Closing window ({})", window_id);
                    
                    // Try to close the window
                    if let Ok(window) = session.find_by_id(format!("wnd[{}]", window_id)) {
                        if let Some(frame) = window.downcast::<GuiFrameWindow>() {
                            frame.close()?;
                        }
                    }
                    
                    // Check if there's another popup that appeared
                    if let Ok(next_check) = control_exists(session, window_id + 1, "", true) {
                        if next_check.exists {
                            println!("Additional popup found.... retrying");
                            
                            // If it's a multiple selection popup, press No
                            if next_check.text.contains("multiple selection") {
                                if let Ok(check) = control_exists(session, window_id + 1, "/usr/btnSPOP-OPTION2", true) {
                                    if check.exists {
                                        hit_control(session, window_id + 1, "/usr/btnSPOP-OPTION2", "Press", "", "")?;
                                        
                                        // Get status bar message
                                        let _ = hit_control(session, 0, "/sbar", "Text", "Get", "")?;
                                    }
                                }
                            } else {
                                // Try again from the top
                                break;
                            }
                        }
                    }
                }
            }
            
            try_count += 1;
            if try_count >= max_tries {
                println!("Max retries, exiting....");
                return Ok(false);
            }
        }
    }
    
    Ok(true)
}

/// Encrypt data using AES-GCM
pub fn encrypt_data(data: &str, key: &[u8]) -> io::Result<String> {
    // Create cipher
    let cipher = match Aes256Gcm::new_from_slice(key) {
        Ok(c) => c,
        Err(_) => return Err(io::Error::new(io::ErrorKind::InvalidInput, "Invalid key length")),
    };
    
    // Generate a random nonce
    let mut nonce_bytes = [0u8; NONCE_SIZE];
    OsRng.fill(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    
    // Encrypt the data
    let ciphertext = match cipher.encrypt(nonce, data.as_bytes()) {
        Ok(c) => c,
        Err(_) => return Err(io::Error::new(io::ErrorKind::Other, "Encryption failed")),
    };
    
    // Combine nonce and ciphertext and encode with base64
    let mut combined = Vec::with_capacity(NONCE_SIZE + ciphertext.len());
    combined.extend_from_slice(&nonce_bytes);
    combined.extend_from_slice(&ciphertext);
    
    Ok(BASE64.encode(combined))
}

/// Decrypt data using AES-GCM
pub fn decrypt_data(encrypted_data: &str, key: &[u8]) -> io::Result<String> {
    // Decode base64
    let combined = match BASE64.decode(encrypted_data) {
        Ok(c) => c,
        Err(_) => return Err(io::Error::new(io::ErrorKind::InvalidData, "Invalid base64 data")),
    };
    
    // Extract nonce and ciphertext
    if combined.len() < NONCE_SIZE {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "Invalid encrypted data"));
    }
    
    let nonce_bytes = &combined[..NONCE_SIZE];
    let ciphertext = &combined[NONCE_SIZE..];
    
    // Create cipher
    let cipher = match Aes256Gcm::new_from_slice(key) {
        Ok(c) => c,
        Err(_) => return Err(io::Error::new(io::ErrorKind::InvalidInput, "Invalid key length")),
    };
    
    let nonce = Nonce::from_slice(nonce_bytes);
    
    // Decrypt the data
    let plaintext = match cipher.decrypt(nonce, ciphertext) {
        Ok(p) => p,
        Err(_) => return Err(io::Error::new(io::ErrorKind::Other, "Decryption failed")),
    };
    
    // Convert to string
    match String::from_utf8(plaintext) {
        Ok(s) => Ok(s),
        Err(_) => Err(io::Error::new(io::ErrorKind::InvalidData, "Invalid UTF-8 data")),
    }
}

/// Get text from SAP error messages
pub fn get_sap_text_errors(session: &GuiSession, window_id: i32, ctrl_id: &str, count: i32, start_index: i32) -> crate::Result<String> {
    let mut result = String::new();
    
    println!("Getting SAP errors...");
    
    for i in start_index..=count {
        let control_id = if ctrl_id.contains("[") {
            format!("{}{},0]", ctrl_id, i)
        } else if ctrl_id.contains("txtMESSTXT") {
            // Replace right numeric if exists
            let base_id = if ctrl_id.chars().last().unwrap_or('x').is_numeric() {
                &ctrl_id[..ctrl_id.len()-1]
            } else {
                ctrl_id
            };
            format!("{}{}", base_id, i)
        } else {
            format!("{}{}", ctrl_id, i)
        };
        
        if let Ok(check) = control_exists(session, window_id, &control_id, true) {
            if check.exists {
                let error_msg = hit_control(session, window_id, &control_id, "Text", "Get", "")?;
                println!("{}", error_msg);
                result.push_str(&format!("\n {}", error_msg));
            }
        }
    }
    
    println!("Error contents are: ({})", result);
    
    Ok(result)
}
