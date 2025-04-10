use sap_scripting::*;
use std::time::Duration;
use std::thread;
use std::collections::HashMap;

use crate::utils::*;
use crate::utils::utils::*;

/// Struct to hold layout parameters
#[derive(Debug, Clone)]
pub struct LayoutParams {
    pub run_check: bool,
    pub err: String,
    pub name: String,
    pub type_name: String,
}

impl Default for LayoutParams {
    fn default() -> Self {
        Self {
            run_check: false,
            err: String::new(),
            name: String::new(),
            type_name: String::new(),
        }
    }
}

/// Select a layout from the layout selection window
///
/// This function is a port of the VBA function SelectLayout
pub fn select_layout(session: &GuiSession, n_wnd: i32, object_name: &str, layout_name: &str) -> windows::core::Result<bool> {
    println!("Checking if layout select window is present.");
    
    // Check if window exists
    let err_wnd = exist_ctrl(session, n_wnd, "", true)?;
    if err_wnd.cband {
        println!("Window with title ({}) found.", err_wnd.ctext);
    } else {
        println!("Window not open, exiting...");
        return Ok(false);
    }
    
    println!("Checking if layout exists...");
    
    // Check if object exists
    let err_wnd = exist_ctrl(session, n_wnd, object_name, true)?;
    if !err_wnd.cband {
        return Ok(false);
    }
    
    // Get the object
    let obj_path = format!("wnd[{}]{}", n_wnd, object_name);
    let obj = session.find_by_id(obj_path)?;
    
    // Try to downcast to GuiGridView
    if let Some(grid) = obj.downcast::<GuiGridView>() {
        // Get row count
        let row_count = grid.row_count()?;
        println!("Object has {} rows", row_count);
        
        // Scroll down to end (in case long)
        if row_count > 0 {
            grid.set_first_visible_row(row_count - 1)?;
            let r = grid.first_visible_row()?;
            println!("Scrolldown - First visible row = {}", r);
        }
        
        // Collect layout names
        let mut layout_names = Vec::new();
        for i in 0..row_count {
            if let Ok(name) = grid.get_cell_value(i, "VARIANT".to_string()) {
                layout_names.push(name.to_uppercase());
            }
        }
        
        println!("Found {} Layouts", layout_names.len());
        
        // Check if layout exists
        if let Some(index) = layout_names.iter().position(|name| name == &layout_name.to_uppercase()) {
            grid.set_current_cell(index as i32, "VARIANT".to_string())?;
            grid.set_selected_rows(index.to_string())?;
            grid.double_click_current_cell()?;
            println!("Selected.");
            return Ok(true);
        } else {
            println!("Layout ({}) not found.", layout_name);
            return Ok(false);
        }
    }
    
    Ok(false)
}

/// Check and select a layout
///
/// This function is a port of the VBA function check_select_layout
pub fn check_select_layout(session: &GuiSession, tcode: &str, layout_row: &str, 
                          args: Option<HashMap<String, String>>, run_pre: bool) -> windows::core::Result<LayoutParams> {
    let mut local_r_val = LayoutParams::default();
    
    println!("Checking / selecting layout for tCode ({})", tcode);
    
    // Get layout_row from args if provided
    let layout_row = if let Some(args) = &args {
        if args.contains_key("layout") {
            args.get("layout").unwrap()
        } else {
            layout_row
        }
    } else {
        layout_row
    };
    
    // Run pre-processing if needed
    let layout_row = if run_pre {
        // Check if layout_row contains "layout"
        if layout_row.contains("layout") {
            layout_row.split_whitespace().next().unwrap_or(layout_row).replace("layout:", "")
        } else if let Some(args) = &args {
            if args.contains_key("layout") {
                args.get("layout").unwrap().to_string()
            } else {
                layout_row.to_string()
            }
        } else if layout_row.is_empty() {
            // Close popup if exists
            let err_wnd = exist_ctrl(session, 1, "", true)?;
            if err_wnd.cband {
                if let Ok(window) = session.find_by_id("wnd[1]".to_string()) {
                    if let Some(wnd) = window.downcast::<GuiFrameWindow>() {
                        wnd.close()?;
                    }
                }
            }
            String::new()
        } else {
            layout_row.replace("layout:", "")
        }
    } else {
        layout_row.to_string()
    };
    
    // Check window
    if !layout_row.is_empty() && layout_row.len() > 1 {
        // Select layout based on tcode
        match tcode.to_lowercase().as_str() {
            "lx03" | "lx02" | "lt23" | "vt22" => {
                // Select Layout
                if let Ok(button) = session.find_by_id("wnd[0]/tbar[1]/btn[33]".to_string()) {
                    if let Some(btn) = button.downcast::<GuiButton>() {
                        btn.press()?;
                    }
                }
            },
            "vt11" => {
                // Choose Layout Button
                if let Ok(menu) = session.find_by_id("wnd[0]/mbar/menu[3]/menu[0]/menu[1]".to_string()) {
                    if let Some(menu_item) = menu.downcast::<GuiComponent>() {
                        if let Some(btn) = menu_item.downcast::<GuiButton>() {
                            btn.press()?;
                        }
                    }
                }
            },
            "zmdesnr" | "mb52" => {
                // Check if button exists
                let err_ctl = exist_ctrl(session, 0, "/tbar[1]/btn[33]", true)?;
                if err_ctl.cband {
                    if let Ok(button) = session.find_by_id("wnd[0]/tbar[1]/btn[33]".to_string()) {
                        if let Some(btn) = button.downcast::<GuiButton>() {
                            btn.press()?;
                        }
                    }
                }
            },
            _ => {}
        }
        
        // Check for error in status bar
        let bar_msg = hit_ctrl(session, 0, "/sbar", "Text", "Get", "")?;
        if contains(&bar_msg, "valid function", Some(false)) {
            local_r_val.err = bar_msg;
            return Ok(local_r_val);
        }
        
        // Check if layout exists
        if layout_row.is_empty() {
            // If layout is empty or zero-length, close popup window and export as-is
            let err_ctl = exist_ctrl(session, 1, "", true)?;
            if err_ctl.cband {
                if let Ok(window) = session.find_by_id("wnd[1]".to_string()) {
                    if let Some(wnd) = window.downcast::<GuiFrameWindow>() {
                        wnd.close()?;
                    }
                }
            }
            println!("Layout ({}) is empty or zero-length. Exporting as-is.", layout_row);
        } else if layout_row.parse::<i32>().is_ok() {
            // Numeric layout row
            let layout_num = layout_row.parse::<i32>().unwrap();
            let err_ctl = exist_ctrl(session, 1, &format!("/usr/lbl[1,{}]", layout_num), true)?;
            
            if !err_ctl.cband {
                println!("Layout number ({}) not found, decreasing", layout_num);
                // Recursive call with decreased layout number
                let mut new_args = HashMap::new();
                if let Some(args) = &args {
                    for (k, v) in args {
                        new_args.insert(k.clone(), v.clone());
                    }
                }
                new_args.insert("layout".to_string(), (layout_num - 1).to_string());
                return check_select_layout(session, tcode, &(layout_num - 1).to_string(), Some(new_args), run_pre);
            } else {
                let err_msg = err_ctl.ctext.clone();
                
                // Highlight layout row
                if let Ok(label) = session.find_by_id(format!("wnd[1]/usr/lbl[1,{}]", layout_num)) {
                    if let Some(lbl) = label.downcast::<GuiLabel>() {
                        lbl.set_focus()?;
                    }
                }
                
                // Select
                if let Ok(window) = session.find_by_id("wnd[1]".to_string()) {
                    if let Some(wnd) = window.downcast::<GuiFrameWindow>() {
                        wnd.send_v_key(2)?;
                    }
                }
                
                println!("Layout number ({}), ({}) selected.", layout_num, err_msg);
                local_r_val.run_check = true;
            }
        } else {
            // String layout name
            // Check if window exists
            let err_ctl = exist_ctrl(session, 1, "", true)?;
            
            if err_ctl.cband {
                if contains(&err_ctl.ctext, "change layout", Some(false)) {
                    // Setup layout
                    goto_setup(session, tcode, &layout_row)?;
                } else if contains(&err_ctl.ctext, "choose", Some(false)) {
                    // Choose layout
                    goto_choose(session, tcode, &layout_row)?;
                } else {
                    // Loop through available saved layouts
                    for i in 1..=60 {
                        let err_ctl = exist_ctrl(session, 1, &format!("/usr/lbl[1,{}]", i), true)?;
                        if err_ctl.cband {
                            let ctrl_msg = if let Ok(label) = session.find_by_id(format!("wnd[1]/usr/lbl[1,{}]", i)) {
                                if let Some(lbl) = label.downcast::<GuiLabel>() {
                                    lbl.text()?
                                } else {
                                    String::new()
                                }
                            } else {
                                String::new()
                            };
                            
                            if ctrl_msg.to_uppercase() == layout_row.to_uppercase() {
                                if let Ok(label) = session.find_by_id(format!("wnd[1]/usr/lbl[1,{}]", i)) {
                                    if let Some(lbl) = label.downcast::<GuiLabel>() {
                                        lbl.set_focus()?;
                                    }
                                }
                                
                                if let Ok(window) = session.find_by_id("wnd[1]".to_string()) {
                                    if let Some(wnd) = window.downcast::<GuiFrameWindow>() {
                                        wnd.send_v_key(2)?;
                                    }
                                }
                                
                                println!("Layout number ({}), ({}) selected.", i, layout_row);
                                local_r_val.run_check = true;
                                break;
                            }
                        }
                    }
                    
                    // Check status bar message
                    let bar_msg = hit_ctrl(session, 0, "/sbar", "Text", "Get", "")?;
                    if !contains(&bar_msg, "layout applied", Some(false)) {
                        // If layout not found, setup layout
                        let err_ctl = exist_ctrl(session, 1, "", true)?;
                        if err_ctl.cband {
                            if let Ok(window) = session.find_by_id("wnd[1]".to_string()) {
                                if let Some(wnd) = window.downcast::<GuiFrameWindow>() {
                                    wnd.close()?;
                                }
                            }
                        }
                        
                        println!("Layout ({}) not found. Setting up layout", layout_row);
                        
                        // Setup layout based on tcode
                        match tcode.to_lowercase().as_str() {
                            "zmdesnr" | "zvt11" => {
                                if let Ok(button) = session.find_by_id("wnd[0]/tbar[1]/btn[32]".to_string()) {
                                    if let Some(btn) = button.downcast::<GuiButton>() {
                                        btn.press()?;
                                    }
                                }
                                
                                // Setup layout
                                // This would call setup_layout from setup_layout_utils.rs
                                println!("Setting up layout for {}", tcode);
                            },
                            "vt11" => {
                                println!("Layout ({}) not found. Setting up layout", layout_row);
                                
                                if let Ok(menu) = session.find_by_id("wnd[0]/mbar/menu[3]/menu[0]/menu[0]".to_string()) {
                                    if let Some(btn) = menu.downcast::<GuiButton>() {
                                        btn.press()?;
                                    }
                                }
                                
                                // Setup layout_li
                                // This would call setup_layout_li from setup_layout_utils.rs
                                println!("Setting up layout_li for {}", tcode);
                            },
                            _ => {
                                if let Ok(menu) = session.find_by_id("wnd[0]/mbar/menu[3]/menu[2]/menu[0]".to_string()) {
                                    if let Some(menu_item) = menu.downcast::<GuiComponent>() {
                                        if let Some(btn) = menu_item.downcast::<GuiButton>() {
                                            btn.press()?;
                                        }
                                    }
                                }
                                
                                // Setup layout_li
                                // This would call setup_layout_li from setup_layout_utils.rs
                                println!("Setting up layout_li for {}", tcode);
                            }
                        }
                        
                        local_r_val.run_check = true;
                    }
                }
            }
        }
        
        // Check status bar message
        let bar_msg = hit_ctrl(session, 0, "/sbar", "Text", "Get", "")?;
        if contains(&bar_msg, "Layout", Some(false)) {
            println!("Status bar message: ({})", bar_msg);
        }
        
        // Make sure window disappears
        let err_ctl = exist_ctrl(session, 1, "", true)?;
        if err_ctl.cband {
            if let Ok(window) = session.find_by_id("wnd[1]".to_string()) {
                if let Some(wnd) = window.downcast::<GuiFrameWindow>() {
                    wnd.close()?;
                }
            }
        }
        
        // Export based on tcode
        let export_wnd_name = match tcode.to_lowercase().as_str() {
            "lx03" | "lx02" => {
                if let Ok(menu) = session.find_by_id("wnd[0]/mbar/menu[0]/menu[1]/menu[1]".to_string()) {
                    if let Some(menu_item) = menu.downcast::<GuiComponent>() {
                        if let Some(btn) = menu_item.downcast::<GuiButton>() {
                            btn.press()?;
                        }
                    }
                }
                "BIN STATUS REPORT: OVERVIEW"
            },
            "vt11" => {
                // Check for no results
                let err_wnd = exist_ctrl(session, 0, "/usr/lbl[2,4]", true)?;
                if err_wnd.cband {
                    let msg = hit_ctrl(session, 0, "/usr/lbl[2,4]", "Text", "Get", "")?;
                    if contains(&msg, "no data", Some(false)) {
                        local_r_val.run_check = false;
                        local_r_val.err = msg;
                        return Ok(local_r_val);
                    }
                }
                
                if let Ok(menu) = session.find_by_id("wnd[0]/mbar/menu[0]/menu[10]/menu[0]".to_string()) {
                    if let Some(menu_item) = menu.downcast::<GuiComponent>() {
                        if let Some(btn) = menu_item.downcast::<GuiButton>() {
                            btn.press()?;
                        }
                    }
                }
                "SHIPMENT LIST: PLANNING"
            },
            "zmdesnr" | "zvt11" => {
                if let Ok(menu) = session.find_by_id("wnd[0]/mbar/menu[0]/menu[3]/menu[1]".to_string()) {
                    if let Some(menu_item) = menu.downcast::<GuiComponent>() {
                        if let Some(btn) = menu_item.downcast::<GuiButton>() {
                            btn.press()?;
                        }
                    }
                }
                "ZMDEMAIN SERIAL NUMBER HISTORY CONTENTS"
            },
            "vl06o" => {
                if let Ok(menu) = session.find_by_id("wnd[0]/mbar/menu[0]/menu[5]/menu[1]".to_string()) {
                    if let Some(menu_item) = menu.downcast::<GuiComponent>() {
                       if let Some(btn) = menu_item.downcast::<GuiButton>() {
                            btn.press()?;
                        }
                    }
                }
                "LIST OF OUTBOUND DELIVERIES"
            },
            _ => ""
        };
        
        // Check export window
        if !export_wnd_name.is_empty() {
            local_r_val.run_check = check_export_window(session, tcode, export_wnd_name)?;
            if !local_r_val.run_check {
                local_r_val.err = format!("Failed to check export window for {}", tcode);
                return Ok(local_r_val);
            }
        }
        
        local_r_val.type_name = "export_window".to_string();
    }
    
    Ok(local_r_val)
}

/// Helper function for goto_setup
fn goto_setup(session: &GuiSession, tcode: &str, layout_row: &str) -> windows::core::Result<()> {
    // Implementation would go here
    println!("Going to setup for tcode: {}, layout: {}", tcode, layout_row);
    Ok(())
}

/// Helper function for goto_choose
fn goto_choose(session: &GuiSession, tcode: &str, layout_row: &str) -> windows::core::Result<()> {
    // Implementation would go here
    println!("Going to choose for tcode: {}, layout: {}", tcode, layout_row);
    Ok(())
}

/// Choose a layout from the layout selection window
///
/// This function is a port of the VBA function choose_layout
pub fn choose_layout(session: &GuiSession, tcode: &str, layout_row: &str) -> windows::core::Result<String> {
    let mut msg = String::new();
    
    // Check if window exists
    let err_wnd = exist_ctrl(session, 1, "", true)?;
    if !err_wnd.cband {
        // If window doesn't exist, trigger layout popup
        layout_popup(session, tcode)?;
    }
    
    // Check title based on window content
    if contains(&err_wnd.ctext.to_lowercase(), "choose", Some(false)) {
        // Window is a "choose" layout window
    } else if contains(&err_wnd.ctext.to_lowercase(), "change", Some(false)) {
        // Window is a "change" layout window
    }
    
    // Find button
    if let Ok(button) = session.find_by_id("wnd[1]/tbar[0]/btn[71]".to_string()) {
        if let Some(btn) = button.downcast::<GuiButton>() {
            btn.press()?;
        }
    }
    
    // Handle checkbox if it exists
    let checkbox_exists = exist_ctrl(session, 2, "/usr/chkSCAN_STRING-START", true)?;
    if checkbox_exists.cband {
        if let Ok(checkbox) = session.find_by_id("wnd[2]/usr/chkSCAN_STRING-START".to_string()) {
            if let Some(chk) = checkbox.downcast::<GuiCheckBox>() {
                chk.set_selected(false)?;
            }
        }
    }
    
    // Set layout name in text field
    if let Ok(text_field) = session.find_by_id("wnd[2]/usr/txtRSYSF-STRING".to_string()) {
        if let Some(txt) = text_field.downcast::<GuiTextField>() {
            txt.set_text(layout_row.to_string())?;
        }
    }
    
    // Press Enter
    if let Ok(window) = session.find_by_id("wnd[2]".to_string()) {
        if let Some(wnd) = window.downcast::<GuiFrameWindow>() {
            wnd.send_v_key(0)?;
        }
    }
    
    // Check window 3
    let err_wnd = exist_ctrl(session, 3, "", true)?;
    if err_wnd.cband {
        // Check if result exists
        let result_exists = exist_ctrl(session, 3, "/usr/lbl[1,2]", true)?;
        if result_exists.cband {
            // Highlight
            if let Ok(label) = session.find_by_id("wnd[3]/usr/lbl[1,2]".to_string()) {
                if let Some(lbl) = label.downcast::<GuiLabel>() {
                    lbl.set_focus()?;
                }
            }
            
            // Click
            if let Ok(window) = session.find_by_id("wnd[3]".to_string()) {
                if let Some(wnd) = window.downcast::<GuiFrameWindow>() {
                    wnd.send_v_key(2)?;
                }
            }
        } else {
            // Error info window
            msg = "No Layout".to_string();
            close_popups(session)?;
            return Ok(msg);
        }
    }
    
    // Enter (close window)
    if let Ok(button) = session.find_by_id("wnd[1]/tbar[0]/btn[0]".to_string()) {
        if let Some(btn) = button.downcast::<GuiButton>() {
            btn.press()?;
        }
    }
    
    // Check if window closed
    let err_wnd = exist_ctrl(session, 1, "", true)?;
    if err_wnd.cband {
        if let Ok(window) = session.find_by_id("wnd[1]".to_string()) {
            if let Some(wnd) = window.downcast::<GuiFrameWindow>() {
                wnd.close()?;
            }
        }
    }
    
    // Get status bar message
    msg = hit_ctrl(session, 0, "/sbar", "Text", "Get", "")?;
    
    println!("{}", msg);
    Ok(msg)
}

/// Trigger layout popup based on transaction code
///
/// This function is a port of the VBA function layout_popup
pub fn layout_popup(session: &GuiSession, tcode: &str) -> windows::core::Result<bool> {
    match tcode.to_lowercase().as_str() {
        "lx03" | "lx02" => {
            // Select Layout
            if let Ok(button) = session.find_by_id("wnd[0]/tbar[1]/btn[33]".to_string()) {
                if let Some(btn) = button.downcast::<GuiButton>() {
                    btn.press()?;
                }
            }
        },
        "vt11" => {
            // Choose Layout Button
            if let Ok(menu) = session.find_by_id("wnd[0]/mbar/menu[3]/menu[0]/menu[1]".to_string()) {
                if let Some(menu_item) = menu.downcast::<GuiComponent>() {
                    if let Some(btn) = menu_item.downcast::<GuiButton>() {
                        btn.press()?;
                    }
                }
            }
        },
        "zmdesnr" => {
            // Check if button exists
            let err_ctl = exist_ctrl(session, 0, "/tbar[1]/btn[33]", true)?;
            if err_ctl.cband {
                if let Ok(button) = session.find_by_id("wnd[0]/tbar[1]/btn[33]".to_string()) {
                    if let Some(btn) = button.downcast::<GuiButton>() {
                        btn.press()?;
                    }
                }
            }
        },
        _ => {}
    }
    
    Ok(true)
}
