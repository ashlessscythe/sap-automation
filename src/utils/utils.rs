use sap_scripting::*;
use std::time::Duration;
use std::thread;
use windows::core::Result;
use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use base64::{Engine as _, engine::general_purpose};
use rand::Rng;

// Constants from VBA
pub const TIME_FORMAT: &str = "mm-dd-yy hh:mm:ss";
pub const STR_FORM: &str = "\n****************************************************************************\n";
pub const KEY_FILE_SUFFIX: &str = "_key.bin";

// Window types from VBA
pub const WORD: &str = "OpusApp";
pub const EXCEL: &str = "XLMAIN";
pub const IEXPLORER: &str = "IEFrame";
pub const MSVBASIC: &str = "wndclass_desked_gsk";
pub const NOTEPAD: &str = "Notepad";

// Windows message constants
pub const WM_CLOSE: u32 = 0x10;

// Resource types from VBA
#[derive(Debug, Clone, Copy)]
pub enum Resource {
    Connected = 0x1,
    Remembered = 0x3,
    GlobalNet = 0x2,
}

#[derive(Debug, Clone, Copy)]
pub enum ResourceType {
    Disk = 0x1,
    Print = 0x2,
    Any = 0x0,
}

#[derive(Debug, Clone, Copy)]
pub enum ResourceViewType {
    Domain = 0x1,
    Generic = 0x0,
    Server = 0x2,
    Share = 0x3,
}

#[derive(Debug, Clone, Copy)]
pub enum ResourceUseType {
    Connectable = 0x1,
    Container = 0x2,
}

// Structs from VBA
#[derive(Debug, Clone)]
pub struct WndTitleCaption {
    pub wnd_type: String,
    pub wnd_title: String,
}

#[derive(Debug, Clone)]
pub struct ErrorCheck {
    pub bchgb: bool,
    pub msg: String,
}

#[derive(Debug, Clone)]
pub struct CtrlCheck {
    pub cband: bool,
    pub ctext: String,
    pub ctype: String,
}

#[derive(Debug, Clone)]
pub struct ParamsStruct {
    pub client_id: String,
    pub user: String,
    pub pass: String,
    pub language: String,
}

// Encryption/Decryption functions
pub fn encrypt_data(data: &str, key: &[u8]) -> Result<String> {
    // Create a new AES-GCM cipher with the provided key
    let cipher = match Aes256Gcm::new_from_slice(key) {
        Ok(c) => c,
        Err(_) => return Err(windows::core::Error::from_win32()),
    };
    
    // Generate a random nonce (12 bytes for AES-GCM)
    let mut nonce_bytes = [0u8; 12];
    rand::thread_rng().fill(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    
    // Encrypt the data
    let plaintext = data.as_bytes();
    let ciphertext = match cipher.encrypt(nonce, plaintext.as_ref()) {
        Ok(c) => c,
        Err(_) => return Err(windows::core::Error::from_win32()),
    };
    
    // Combine nonce and ciphertext and encode as base64
    let mut combined = Vec::with_capacity(nonce_bytes.len() + ciphertext.len());
    combined.extend_from_slice(&nonce_bytes);
    combined.extend_from_slice(&ciphertext);
    
    Ok(general_purpose::STANDARD.encode(combined))
}

pub fn decrypt_data(encrypted_data: &str, key: &[u8]) -> Result<String> {
    // Decode the base64 data
    let combined = match general_purpose::STANDARD.decode(encrypted_data) {
        Ok(c) => c,
        Err(_) => return Err(windows::core::Error::from_win32()),
    };
    
    // Split into nonce and ciphertext
    if combined.len() < 12 {
        return Err(windows::core::Error::from_win32());
    }
    
    let nonce_bytes = &combined[..12];
    let ciphertext = &combined[12..];
    
    // Create cipher with the key
    let cipher = match Aes256Gcm::new_from_slice(key) {
        Ok(c) => c,
        Err(_) => return Err(windows::core::Error::from_win32()),
    };
    
    // Decrypt the data
    let nonce = Nonce::from_slice(nonce_bytes);
    let plaintext = match cipher.decrypt(nonce, ciphertext.as_ref()) {
        Ok(p) => p,
        Err(_) => return Err(windows::core::Error::from_win32()),
    };
    
    // Convert back to string
    match String::from_utf8(plaintext) {
        Ok(s) => Ok(s),
        Err(_) => Err(windows::core::Error::from_win32()),
    }
}

// SAP GUI interaction functions
pub fn assert_tcode(session: &GuiSession, tcode: &str, wnd: Option<i32>) -> Result<bool> {
    let wnd_num = wnd.unwrap_or(0);
    
    // Start the transaction
    session.start_transaction(tcode.to_string())?;
    
    // Check for errors in status bar
    let err_msg = hit_ctrl(session, wnd_num, "/sbar", "Text", "Get", "")?;
    
    if err_msg.contains("exist") || err_msg.contains("autho") {
        println!("Error: {}", err_msg);
        return Ok(false);
    }
    
    if err_msg.is_empty() {
        return Ok(true);
    }
    
    // Log error message
    println!("{}{}{}", STR_FORM, err_msg, STR_FORM);
    
    Ok(false)
}

pub fn check_tcode(session: &GuiSession, tcode: &str, run: Option<bool>, _kill_popups: Option<bool>) -> Result<bool> {
    let run_val = run.unwrap_or(true);
    
    println!("Checking if tCode ({}) is active", tcode);
    
    // Get current transaction
    let current = session.info()?.transaction()?;
    
    // Check if on tCode
    if current.contains(tcode) {
        println!("tCode ({}) is active", tcode);
        return Ok(true);
    } else if run_val {
        // Run if requested
        println!("tCode mismatch, attempting to run tCode ({})", tcode);
        let _ = assert_tcode(session, tcode, None)?;
        thread::sleep(Duration::from_millis(500)); // Time_Event equivalent
        
        // Recursive call to check again
        return check_tcode(session, tcode, Some(false), Some(false));
    } else {
        println!("tCode mismatch. Current tCode is ({}), need ({})", current, tcode);
        return Ok(false);
    }
}

pub fn exist_ctrl(session: &GuiSession, n_wnd: i32, control_id: &str, ret_msg: bool) -> Result<CtrlCheck> {
    let mut err_chk = CtrlCheck {
        cband: false,
        ctext: String::new(),
        ctype: String::new(),
    };
    
    // Try to find the control
    let control_path = format!("wnd[{}]{}", n_wnd, control_id);
    let ret_id = session.find_by_id(control_path.clone());
    
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

pub fn hit_ctrl(session: &GuiSession, n_wnd: i32, control_id: &str, event_id: &str, event_id_opt: &str, event_id_value: &str) -> Result<String> {
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

pub fn close_popups(session: &GuiSession) -> Result<bool> {
    println!("Closing all popups");
    
    let max_tries = 5;
    
    for i in (1..=5).rev() {
        let mut j = 0;
        while j < max_tries {
            let err_wnd = exist_ctrl(session, i, "", true)?;
            if err_wnd.cband {
                println!("Closing window ({})", i);
                if let Ok(component) = session.find_by_id(format!("wnd[{}]", i)) {
                    if let Some(window) = component.downcast::<GuiFrameWindow>() {
                        window.close()?;
                    }
                }
                
                // Check if another popup appeared
                let next_err_wnd = exist_ctrl(session, i + 1, "", true)?;
                if next_err_wnd.cband {
                    println!("Additional popup found.... retrying");
                    if next_err_wnd.ctext.contains("multiple selection") {
                        // Press no
                        let btn_err_wnd = exist_ctrl(session, i + 1, "/usr/btnSPOP-OPTION2", true)?;
                        if btn_err_wnd.cband {
                            if let Ok(component) = session.find_by_id(format!("wnd[{}]/usr/btnSPOP-OPTION2", i + 1)) {
                                if let Some(button) = component.downcast::<GuiButton>() {
                                    button.press()?;
                                }
                            }
                            let _ = hit_ctrl(session, 0, "/sbar", "Text", "Get", "")?;
                        }
                    } else {
                        continue; // Go back to the top of the loop
                    }
                }
            }
            
            j += 1;
            if j >= max_tries {
                println!("Max retries, exiting....");
                return Ok(false);
            }
        }
    }
    
    Ok(true)
}

pub fn check_export_window(session: &GuiSession, tcode: &str, correct_title: &str) -> Result<bool> {
    // Check if tcode is active
    if !check_tcode(session, tcode, Some(false), Some(false))? {
        println!("tCode ({}) not active, exiting....", tcode);
        return Ok(false);
    }
    
    loop {
        // Check for window
        let err_wnd = exist_ctrl(session, 1, "", true)?;
        
        if err_wnd.cband {
            println!("Looking for: {}", correct_title);
            
            if err_wnd.ctext.contains("SELECT SPREADSHEET") {
                // Press Excel button
                if let Ok(component) = session.find_by_id("wnd[1]/tbar[0]/btn[0]".to_string()) {
                    if let Some(button) = component.downcast::<GuiButton>() {
                        button.press()?;
                    }
                }
                return Ok(true);
            } else if err_wnd.ctext.contains("SAVE LIST IN FILE...") {
                println!("Window Title {}", err_wnd.ctext);
                println!("Saving as 'local file'");
                
                // Select local file radio button
                if let Ok(component) = session.find_by_id("wnd[1]/usr/subSUBSCREEN_STEPLOOP:SAPLSPO5:0150/sub:SAPLSPO5:0150/radSPOPLI-SELFLAG[1,0]".to_string()) {
                    if let Some(radio) = component.downcast::<GuiRadioButton>() {
                        radio.select()?;
                    }
                }
                
                // Press button
                if let Ok(component) = session.find_by_id("wnd[1]/tbar[0]/btn[0]".to_string()) {
                    if let Some(button) = component.downcast::<GuiButton>() {
                        button.press()?;
                    }
                }
                
                return Ok(true);
            } else if err_wnd.ctext.contains(correct_title) {
                return Ok(true);
            } else {
                println!("Error with Excel Export, trying Local File, trying to correct...");
                println!("Window title {}", err_wnd.ctext);
            }
        } else {
            // tCode Specific to get export window if non-existent
            let base_obj_id;
            let obj_id;
            
            match tcode {
                "VT11" => {
                    base_obj_id = "";
                    let _ = exist_ctrl(session, 0, base_obj_id, true)?;
                    
                    // Send F12 (key 44)
                    if let Ok(component) = session.find_by_id("wnd[0]".to_string()) {
                        if let Some(window) = component.downcast::<GuiFrameWindow>() {
                            window.send_v_key(44)?;
                        }
                    }
                    continue; // Go back to the top of the loop
                },
                "MB51" => {
                    base_obj_id = "/usr/cntlGRID1/shellcont/shell";
                    obj_id = format!("wnd[0]{}", base_obj_id);
                    let err_wnd = exist_ctrl(session, 0, base_obj_id, true)?;
                    
                    if err_wnd.cband {
                        if let Ok(component) = session.find_by_id(obj_id.clone()) {
                            if let Some(grid) = component.downcast::<GuiGridView>() {
                                grid.set_selected_rows("0".to_string())?;
                                grid.set_current_cell_row(-1)?;
                                grid.context_menu()?;
                                grid.select_context_menu_item("&XXL".to_string())?;
                            }
                        }
                        
                        if let Ok(component) = session.find_by_id("wnd[1]/usr/chkCB_ALWAYS".to_string()) {
                            if let Some(checkbox) = component.downcast::<GuiCheckBox>() {
                                checkbox.set_selected(false)?;
                            }
                        }
                        
                        continue; // Go back to the top of the loop
                    }
                },
                "ZWM_MDE_COMPARE" => {
                    base_obj_id = "/usr/cntlGRID1/shellcont/shell";
                    obj_id = format!("wnd[0]{}", base_obj_id);
                    let err_wnd = exist_ctrl(session, 0, base_obj_id, true)?;
                    
                    if err_wnd.cband {
                        // Similar to MB51 handling
                        if let Ok(component) = session.find_by_id(obj_id.clone()) {
                            if let Some(grid) = component.downcast::<GuiGridView>() {
                                grid.set_selected_rows("0".to_string())?;
                                grid.set_current_cell_row(-1)?;
                                grid.context_menu()?;
                                grid.select_context_menu_item("&XXL".to_string())?;
                            }
                        }
                        
                        if let Ok(component) = session.find_by_id("wnd[1]/usr/chkCB_ALWAYS".to_string()) {
                            if let Some(checkbox) = component.downcast::<GuiCheckBox>() {
                                checkbox.set_selected(false)?;
                            }
                        }
                        
                        continue; // Go back to the top of the loop
                    }
                },
                "ZMDESNR" => {
                    base_obj_id = "/usr/cntlGRID1/shellcont/shell/shellcont[1]/shell";
                    obj_id = format!("wnd[0]{}", base_obj_id);
                    let err_wnd = exist_ctrl(session, 0, base_obj_id, true)?;
                    
                    if err_wnd.cband {
                        // Similar to MB51 handling
                        if let Ok(component) = session.find_by_id(obj_id.clone()) {
                            if let Some(grid) = component.downcast::<GuiGridView>() {
                                grid.set_selected_rows("0".to_string())?;
                                grid.set_current_cell_row(-1)?;
                                grid.context_menu()?;
                                grid.select_context_menu_item("&XXL".to_string())?;
                            }
                        }
                        
                        if let Ok(component) = session.find_by_id("wnd[1]/usr/chkCB_ALWAYS".to_string()) {
                            if let Some(checkbox) = component.downcast::<GuiCheckBox>() {
                                checkbox.set_selected(false)?;
                            }
                        }
                        
                        continue; // Go back to the top of the loop
                    }
                },
                _ => {
                    println!("Grid for TCODE ({}) needs to be set up in Check_Export_Window", tcode);
                }
            }
        }
        
        break; // Exit the loop if we haven't continued
    }
    
    Ok(false)
}

pub fn check_wnd(session: &GuiSession, n_wnd: i32, params_in: &ParamsStruct) -> Result<ErrorCheck> {
    let get_time = chrono::Local::now().format(TIME_FORMAT).to_string();
    
    let mut err_chk = ErrorCheck {
        bchgb: false,
        msg: String::new(),
    };
    
    let err_wnd = exist_ctrl(session, n_wnd, "", true)?;
    
    if err_wnd.cband {
        let wnd_type = err_wnd.ctype.clone();
        let wnd_title = err_wnd.ctext.clone();
        
        match wnd_type.as_str() {
            "GuiFrameWindow" => {},
            "GuiMainWindow" => {
                match wnd_title.as_str() {
                    "SAP Easy Access" => {
                        err_chk.bchgb = true;
                        err_chk.msg = String::new();
                        
                        // Maximize SAP Easy Access Window
                        let err_ctl = exist_ctrl(session, n_wnd, "", true)?;
                        if err_ctl.cband {
                            let _ = hit_ctrl(session, n_wnd, "", "Focus", "", "")?;
                            let _ = hit_ctrl(session, n_wnd, "", "Maximize", "", "")?;
                        }
                    },
                    "SAP R/3" | "SAP" => {
                        // Login Initial Screen
                        let ctrl_id = "/usr/txtRSYST-MANDT";  // Client
                        let err_ctl = exist_ctrl(session, n_wnd, ctrl_id, true)?;
                        if err_ctl.cband {
                            let _ = hit_ctrl(session, n_wnd, ctrl_id, "Text", "Set", &params_in.client_id)?;
                        }
                        
                        let ctrl_id = "/usr/txtRSYST-BNAME";  // User
                        let err_ctl = exist_ctrl(session, n_wnd, ctrl_id, true)?;
                        if err_ctl.cband {
                            let _ = hit_ctrl(session, n_wnd, ctrl_id, "Text", "Set", &params_in.user)?;
                        }
                        
                        let ctrl_id = "/usr/pwdRSYST-BCODE";  // Password
                        let err_ctl = exist_ctrl(session, n_wnd, ctrl_id, true)?;
                        if err_ctl.cband {
                            let _ = hit_ctrl(session, n_wnd, ctrl_id, "Text", "Set", &params_in.pass)?;
                        }
                        
                        let ctrl_id = "/usr/txtRSYST-LANGU";  // Language
                        let err_ctl = exist_ctrl(session, n_wnd, ctrl_id, true)?;
                        if err_ctl.cband {
                            let _ = hit_ctrl(session, n_wnd, ctrl_id, "Text", "Set", &params_in.language)?;
                        }
                        
                        let ctrl_id = "/tbar[0]/btn[0]";      // Enter
                        let err_ctl = exist_ctrl(session, n_wnd, ctrl_id, true)?;
                        if err_ctl.cband {
                            let _ = hit_ctrl(session, n_wnd, ctrl_id, "Press", "", "")?;
                        }
                        
                        err_chk.bchgb = true;
                        err_chk.msg = get_time;
                    },
                    // Add other window title cases as needed
                    _ => {}
                }
            },
            "GuiModalWindow" => {
                match wnd_title.as_str() {
                    "Log Off" => {
                        let ctrl_id = "/usr/btnSPOP-OPTION2";
                        let err_ctl = exist_ctrl(session, n_wnd, ctrl_id, true)?;
                        if err_ctl.cband {
                            let _ = hit_ctrl(session, n_wnd, ctrl_id, "Press", "", "")?;
                        }
                        
                        err_chk.bchgb = true;
                        err_chk.msg = get_time;
                    },
                    "System Messages" => {
                        let ctrl_id = "/tbar[0]/btn[0]";
                        let err_ctl = exist_ctrl(session, n_wnd, ctrl_id, true)?;
                        if err_ctl.cband {
                            let _ = hit_ctrl(session, n_wnd, ctrl_id, "Press", "", "")?;
                        }
                        
                        err_chk.bchgb = true;
                        err_chk.msg = get_time;
                    },
                    // Add other modal window title cases as needed
                    _ => {}
                }
            },
            _ => {}
        }
    }
    
    Ok(err_chk)
}

// Helper function to check if a string contains a substring
pub fn contains(haystack: &str, needle: &str, case_sensitive: Option<bool>) -> bool {
    let case_sensitive_val = case_sensitive.unwrap_or(true);
    
    if case_sensitive_val {
        haystack.contains(needle)
    } else {
        haystack.to_lowercase().contains(&needle.to_lowercase())
    }
}

// Helper function to check if multiple strings contain a target
pub fn mult_contains(haystack: &str, needles: &[&str]) -> bool {
    for needle in needles {
        if haystack.contains(needle) {
            return true;
        }
    }
    false
}
