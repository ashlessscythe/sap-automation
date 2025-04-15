use crate::utils::sap_constants::{ErrorCheck, ParamsStruct, TIME_FORMAT};
use crate::utils::sap_ctrl_utils::{exist_ctrl, hit_ctrl};
use crate::utils::sap_tcode_utils::check_tcode;
use chrono;
use sap_scripting::*;
use windows::core::Result;

pub fn close_popups(
    session: &GuiSession,
    wnd_idx: Option<i32>,
    repeat: Option<i32>,
) -> Result<bool> {
    let repeat_count = repeat.unwrap_or(1);

    for _ in 0..repeat_count {
        match wnd_idx {
            Some(-1) => {
                // Close only the highest index popup
                println!("Closing highest index popup");

                // Find the highest index popup
                let mut highest_idx = 0;
                for i in 1..=5 {
                    let err_wnd = exist_ctrl(session, i, "", true)?;
                    if err_wnd.cband {
                        highest_idx = i;
                    }
                }

                if highest_idx > 0 {
                    close_specific_popup(session, highest_idx)?;
                } else {
                    println!("No popups found");
                }
            }
            Some(idx) if idx > 0 => {
                // Close only the specified popup
                println!("Closing popup at index {}", idx);
                close_specific_popup(session, idx)?;
            }
            _ => {
                // Original behavior: close all popups from 5 down to 1
                println!("Closing all popups");

                for i in (1..=5).rev() {
                    close_specific_popup(session, i)?;
                }
            }
        }
    }

    Ok(true)
}

fn close_specific_popup(session: &GuiSession, i: i32) -> Result<bool> {
    let max_tries = 5;
    let mut j = 0;

    while j < max_tries {
        let err_wnd = exist_ctrl(session, i, "", true)?;
        if err_wnd.cband {
            println!("Closing window ({})", i);

            // First attempt: try to close the window using close()
            if let Ok(component) = session.find_by_id(format!("wnd[{}]", i)) {
                if let Some(window) = component.downcast::<GuiModalWindow>() {
                    window.close()?;
                }
            }

            // Check if window is still open after first attempt
            let still_open = exist_ctrl(session, i, "", true)?;
            if still_open.cband {
                // Second attempt: try to close the window using F12
                if let Ok(component) = session.find_by_id(format!("wnd[{}]", i)) {
                    if let Some(window) = component.downcast::<GuiModalWindow>() {
                        window.send_v_key(0)?; // Send Enter key
                    } else if let Some(modal_window) = component.downcast::<GuiModalWindow>() {
                        modal_window.send_v_key(12)?; // Send F12 (Cancel)
                    }
                }

                // Check if window is still open after second attempt
                let still_open_after_second = exist_ctrl(session, i, "", true)?;
                if still_open_after_second.cband {
                    println!("Window {} still open, trying vkey0 (Enter)", i);
                    // Third attempt: try to close using vkey0 (Enter key)
                    if let Ok(component) = session.find_by_id(format!("wnd[{}]", i)) {
                        if let Some(window) = component.downcast::<GuiModalWindow>() {
                            window.send_v_key(0)?; // Send Enter key
                        } else if let Some(modal_window) = component.downcast::<GuiModalWindow>() {
                            modal_window.send_v_key(0)?; // Send Enter key
                        }
                    }
                }
            }

            // Check if another popup appeared
            let next_err_wnd = exist_ctrl(session, i + 1, "", true)?;
            if next_err_wnd.cband {
                println!(
                    "Additional popup found at window {}: {}",
                    i + 1,
                    next_err_wnd.ctext
                );
                if next_err_wnd.ctext.contains("multiple selection") {
                    // Press no
                    let btn_err_wnd = exist_ctrl(session, i + 1, "/usr/btnSPOP-OPTION2", true)?;
                    if btn_err_wnd.cband {
                        if let Ok(component) =
                            session.find_by_id(format!("wnd[{}]/usr/btnSPOP-OPTION2", i + 1))
                        {
                            if let Some(button) = component.downcast::<GuiButton>() {
                                button.press()?;
                            }
                        }
                        let _ = hit_ctrl(session, 0, "/sbar", "Text", "Get", "")?;
                    }
                } else {
                    // Try to close this new popup
                    if let Ok(component) = session.find_by_id(format!("wnd[{}]", i + 1)) {
                        if let Some(window) = component.downcast::<GuiModalWindow>() {
                            window.close()?;
                        } else if let Some(modal_window) = component.downcast::<GuiModalWindow>() {
                            modal_window.send_v_key(0)?; // Send Enter key
                        }
                    }
                    continue; // Go back to the top of the loop
                }
            }

            // Check if the window is still open after all attempts
            let final_check = exist_ctrl(session, i, "", true)?;
            if final_check.cband {
                println!(
                    "Warning: Window {} still open after multiple close attempts",
                    i
                );
            }
        }

        j += 1;
        if j >= max_tries {
            println!("Max retries, exiting....");
            return Ok(false);
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

            if err_wnd.ctext.contains("Select Spreadsheet") {
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
                }
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

                        if let Ok(component) =
                            session.find_by_id("wnd[1]/usr/chkCB_ALWAYS".to_string())
                        {
                            if let Some(checkbox) = component.downcast::<GuiCheckBox>() {
                                checkbox.set_selected(false)?;
                            }
                        }

                        continue; // Go back to the top of the loop
                    }
                }
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

                        if let Ok(component) =
                            session.find_by_id("wnd[1]/usr/chkCB_ALWAYS".to_string())
                        {
                            if let Some(checkbox) = component.downcast::<GuiCheckBox>() {
                                checkbox.set_selected(false)?;
                            }
                        }

                        continue; // Go back to the top of the loop
                    }
                }
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

                        if let Ok(component) =
                            session.find_by_id("wnd[1]/usr/chkCB_ALWAYS".to_string())
                        {
                            if let Some(checkbox) = component.downcast::<GuiCheckBox>() {
                                checkbox.set_selected(false)?;
                            }
                        }

                        continue; // Go back to the top of the loop
                    }
                }
                _ => {
                    println!(
                        "Grid for TCODE ({}) needs to be set up in Check_Export_Window",
                        tcode
                    );
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
            "GuiFrameWindow" => {}
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
                    }
                    "SAP R/3" | "SAP" => {
                        // Login Initial Screen
                        let ctrl_id = "/usr/txtRSYST-MANDT"; // Client
                        let err_ctl = exist_ctrl(session, n_wnd, ctrl_id, true)?;
                        if err_ctl.cband {
                            let _ = hit_ctrl(
                                session,
                                n_wnd,
                                ctrl_id,
                                "Text",
                                "Set",
                                &params_in.client_id,
                            )?;
                        }

                        let ctrl_id = "/usr/txtRSYST-BNAME"; // User
                        let err_ctl = exist_ctrl(session, n_wnd, ctrl_id, true)?;
                        if err_ctl.cband {
                            let _ =
                                hit_ctrl(session, n_wnd, ctrl_id, "Text", "Set", &params_in.user)?;
                        }

                        let ctrl_id = "/usr/pwdRSYST-BCODE"; // Password
                        let err_ctl = exist_ctrl(session, n_wnd, ctrl_id, true)?;
                        if err_ctl.cband {
                            let _ =
                                hit_ctrl(session, n_wnd, ctrl_id, "Text", "Set", &params_in.pass)?;
                        }

                        let ctrl_id = "/usr/txtRSYST-LANGU"; // Language
                        let err_ctl = exist_ctrl(session, n_wnd, ctrl_id, true)?;
                        if err_ctl.cband {
                            let _ = hit_ctrl(
                                session,
                                n_wnd,
                                ctrl_id,
                                "Text",
                                "Set",
                                &params_in.language,
                            )?;
                        }

                        let ctrl_id = "/tbar[0]/btn[0]"; // Enter
                        let err_ctl = exist_ctrl(session, n_wnd, ctrl_id, true)?;
                        if err_ctl.cband {
                            let _ = hit_ctrl(session, n_wnd, ctrl_id, "Press", "", "")?;
                        }

                        err_chk.bchgb = true;
                        err_chk.msg = get_time;
                    }
                    // Add other window title cases as needed
                    _ => {}
                }
            }
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
                    }
                    "System Messages" => {
                        let ctrl_id = "/tbar[0]/btn[0]";
                        let err_ctl = exist_ctrl(session, n_wnd, ctrl_id, true)?;
                        if err_ctl.cband {
                            let _ = hit_ctrl(session, n_wnd, ctrl_id, "Press", "", "")?;
                        }

                        err_chk.bchgb = true;
                        err_chk.msg = get_time;
                    }
                    // Add other modal window title cases as needed
                    _ => {}
                }
            }
            _ => {}
        }
    }

    Ok(err_chk)
}
