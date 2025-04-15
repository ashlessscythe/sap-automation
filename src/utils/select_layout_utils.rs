use sap_scripting::*;
use std::time::Duration;
use std::thread;
use std::collections::HashMap;
use dialoguer::{Select, Input}; // Added for interactive user input

use crate::utils::utils::*;
use crate::utils::sap_ctrl_utils::*;
use crate::utils::sap_wnd_utils::*;

/// Struct to hold layout parameters
#[derive(Debug, Clone)]
#[derive(Default)]
pub struct Params {
    pub run_check: bool,
    pub err: String,
    pub name: String,
    pub type_name: String,
}


/// Select a layout from the layout selection window
///
/// This function is a port of the VBA function SelectLayout
/// If layout not found, it will ask the user to type in another layout name or exit
/// 
/// # Arguments
/// * `session` - The SAP GUI session
/// * `n_wnd` - The window number (typically 1)
/// * `object_name` - The path to the object (varies by tcode)
///   For example: "/usr/ssubD0500_SUBSCREEN:SAPLSLVC_DIALOG:0501/cntlG51_CONTAINER/shellcont/shell"
/// * `layout_name` - The name of the layout to select
///
/// # Returns
/// * `Ok(true)` if the layout was successfully selected
/// * `Ok(false)` if the layout was not found or could not be selected and user chose to exit
pub fn select_layout(session: &GuiSession, n_wnd: i32, object_name: &str, layout_name: &str) -> windows::core::Result<bool> {
    println!("Checking if layout select window is present.");
    
    // Create a mutable copy of layout_name that we can modify in the loop
    let mut current_layout = layout_name.to_string();
    
    // Loop until a valid layout is found or user chooses to exit
    loop {
        // Check if window exists
        let err_wnd = exist_ctrl(session, n_wnd, "", true)?;
        if !err_wnd.cband {
            println!("Window not open, exiting...");
            return Ok(false);
        }
        
        println!("Window with title ({}) found.", err_wnd.ctext);
        println!("Checking if layout exists...");
        
        // Check if object exists
        let err_wnd = exist_ctrl(session, n_wnd, object_name, true)?;
        if !err_wnd.cband {
            println!("Object ({}) not found.", object_name);
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
            if let Some(index) = layout_names.iter().position(|name| name == &current_layout.to_uppercase()) {
                grid.set_current_cell(index as i32, "VARIANT".to_string())?;
                grid.set_selected_rows(index.to_string())?;
                grid.double_click_current_cell()?;
                println!("Selected layout: {}", current_layout);
                return Ok(true);
            } else {
                println!("Layout ({}) not found.", current_layout);
                
                // Ask user for a new layout name or to exit
                println!("Layout '{}' not found.", current_layout);
                
                let options = vec!["Enter another layout name", "Exit layout selection"];
                let selection = Select::new()
                    .with_prompt("What would you like to do?")
                    .items(&options)
                    .default(0)
                    .interact()
                    .unwrap_or(1); // Default to exit if interaction fails
                
                if selection == 0 {
                    // User wants to try another layout name
                    let new_layout: String = Input::new()
                        .with_prompt("Enter new layout name")
                        .interact_text()
                        .unwrap_or_else(|_| String::new());
                    
                    if new_layout.is_empty() {
                        // If user entered empty string, exit
                        println!("Layout selection cancelled");
                        
                        // Close the window
                        if let Ok(window) = session.find_by_id(format!("wnd[{}]", n_wnd)) {
                            if let Some(wnd) = window.downcast::<GuiModalWindow>() {
                                println!("Closing window since layout selection was cancelled.");
                                wnd.close()?;
                            }
                        }
                        
                        return Ok(false);
                    }
                    
                    // Update current_layout and try again
                    current_layout = new_layout;
                    println!("Trying with new layout name: {}", current_layout);
                    
                    // Continue to next iteration of the loop
                    continue;
                } else {
                    // User wants to exit
                    println!("Layout selection cancelled");
                    
                    // Close the window
                    if let Ok(window) = session.find_by_id(format!("wnd[{}]", n_wnd)) {
                        if let Some(wnd) = window.downcast::<GuiModalWindow>() {
                            println!("Closing window since layout selection was cancelled.");
                            wnd.close()?;
                        }
                    }
                    
                    return Ok(false);
                }
            }
        } else {
            println!("Object is not a GuiGridView.");
            return Ok(false);
        }
    }
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
                if let Some(menu_item) = menu.downcast::<GuiMenu>() {
                    menu_item.select()?;
                }
            }
        },
        "vl06o" => {
            // Choose Layout Button for VL06O
            if let Ok(menu) = session.find_by_id("wnd[0]/mbar/menu[3]/menu[2]/menu[1]".to_string()) {
                if let Some(menu_item) = menu.downcast::<GuiMenu>() {
                    menu_item.select()?;
                }
            }
        },
        "zmdesnr" => {
            // Open via mbar
            println!("DEBUG: pressing layout button for zmdesnr");
            if let Ok(button) = session.find_by_id("wnd[0]/mbar/menu[4]/menu[0]/menu[1]".to_string()) {
                if let Some(btn) = button.downcast::<GuiButton>() {
                    btn.press()?;
                }
            } else {
                // Check if button exists in toolbar
                let err_ctl = exist_ctrl(session, 0, "/tbar[1]/btn[33]", true)?;
                if err_ctl.cband {
                    if let Ok(button) = session.find_by_id("wnd[0]/tbar[1]/btn[33]".to_string()) {
                        if let Some(btn) = button.downcast::<GuiButton>() {
                            btn.press()?;
                        }
                    }
                }
            }
        },
        "mb52" => {
            // Check if button exists
            if let Ok(button) = session.find_by_id("wnd[0]/tbar[1]/btn[33]".to_string()) {
                if let Some(btn) = button.downcast::<GuiButton>() {
                    btn.press()?;
                }
            }
        },
        _ => {
            // Try common layout buttons
            if let Ok(button) = session.find_by_id("wnd[0]/tbar[1]/btn[33]".to_string()) {
                if let Some(btn) = button.downcast::<GuiButton>() {
                    btn.press()?;
                }
            }
        }
    }
    
    // Wait for layout window to appear
    thread::sleep(Duration::from_millis(500));
    
    Ok(true)
}

/// Get the appropriate object name for a tcode
///
/// Different tcodes use different object names for the layout selection grid
///
/// # Arguments
/// * `tcode` - The transaction code
///
/// # Returns
/// * The object name for the layout selection grid
pub fn get_layout_object_name(tcode: &str) -> String {
    match tcode.to_lowercase().as_str() {
        "zmdesnr" | "mb52" => "/usr/ssubD0500_SUBSCREEN:SAPLSLVC_DIALOG:0501/cntlG51_CONTAINER/shellcont/shell".to_string(),
        "vl06o" => "/usr/ssubD0500_SUBSCREEN:SAPLSLVC_DIALOG:0501/cntlG51_CONTAINER/shellcont/shell".to_string(),
        "vt11" => "/usr/ssubD0500_SUBSCREEN:SAPLSLVC_DIALOG:0501/cntlG51_CONTAINER/shellcont/shell".to_string(),
        "lx03" | "lx02" | "lt23" | "vt22" => "/usr/ssubD0500_SUBSCREEN:SAPLSLVC_DIALOG:0501/cntlG51_CONTAINER/shellcont/shell".to_string(),
        // Default object name (can be adjusted as needed)
        _ => "/usr/ssubD0500_SUBSCREEN:SAPLSLVC_DIALOG:0501/cntlG51_CONTAINER/shellcont/shell".to_string(),
    }
}

/// Select a layout interactively
///
/// This function combines layout_popup and select_layout to provide an interactive
/// layout selection experience. It will trigger the layout popup, then allow the user
/// to select a layout or enter a new layout name if the initial one isn't found.
///
/// # Arguments
/// * `session` - The SAP GUI session
/// * `tcode` - The transaction code
/// * `layout_name` - The initial layout name to try
///
/// # Returns
/// * `Ok(true)` if a layout was successfully selected
/// * `Ok(false)` if the user cancelled or no layout could be selected
pub fn select_layout_interactive(session: &GuiSession, tcode: &str, layout_name: &str) -> windows::core::Result<bool> {
    println!("Selecting layout interactively for tcode: {}, initial layout: {}", tcode, layout_name);
    
    // Trigger layout popup
    layout_popup(session, tcode)?;
    
    // Wait for layout window to appear
    thread::sleep(Duration::from_millis(500));
    
    // Check if window exists
    let err_wnd = exist_ctrl(session, 1, "", true)?;
    if !err_wnd.cband {
        println!("Layout selection window did not appear.");
        return Ok(false);
    }
    
    // Get the appropriate object name for this tcode
    let object_name = get_layout_object_name(tcode);
    
    // Call select_layout with the appropriate object name
    let result = select_layout(session, 1, &object_name, layout_name)?;
    
    Ok(result)
}

/// Check and select a layout
///
/// This function is a port of the VBA function check_select_layout
pub fn check_select_layout(session: &GuiSession, tcode: &str, layout_row: &str, 
                          args: Option<HashMap<String, String>>) -> windows::core::Result<Params> {
    
    let mut local_r_val = Params::default();

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

    // debug
    println!("DEBUG: layout_row is: {}", layout_row);
    
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
            "mb52" => {
                // Check if button exists
                if let Ok(button) = session.find_by_id("wnd[0]/tbar[1]/btn[33]".to_string()) {
                    if let Some(btn) = button.downcast::<GuiButton>() {
                        btn.press()?;
                    }
                }
            },
            "zmdesnr" => {
                // open via mbar
                println!("DEBUG:pressing layout button for zmdesnr");
                if let Ok(button) = session.find_by_id("wnd[0]/mbar/menu[4]/menu[0]/menu[1]".to_string()) {
                    if let Some(btn) = button.downcast::<GuiButton>() {
                        btn.press()?;
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
            close_popups(session, None, None)?;
            println!("Layout ({}) is empty or zero-length. Exporting as-is.", layout_row);
        } else {
            // String layout name
            // Check if window exists
            let err_ctl = exist_ctrl(session, 1, "", true)?;
            
            if err_ctl.cband {
                if contains(&err_ctl.ctext, "change layout", Some(false)) {
                    // Setup layout
                    goto_setup(session, tcode, layout_row)?;
                } else if contains(&err_ctl.ctext, "choose", Some(false)) {
                    // Choose layout
                    println!("DEBUG:layout wnd is choose");
                    
                    // Use the interactive layout selection
                    let run_check = select_layout_interactive(session, tcode, layout_row)?;
                    
                    // Update run_check in local_r_val
                    local_r_val.run_check = run_check;
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
                                    if let Some(wnd) = window.downcast::<GuiModalWindow>() {
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
                        // If layout not found, close any popups and setup layout
                        close_popups(session, None, None)?;
                        
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
        
        // Make sure all windows are closed
        close_popups(session, None, None)?;
        
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

/// Check export window
///
/// This function checks if the export window is present and has the expected title
fn check_export_window(session: &GuiSession, tcode: &str, expected_title: &str) -> windows::core::Result<bool> {
    // Check if window exists
    let err_wnd = exist_ctrl(session, 0, "", true)?;
    if !err_wnd.cband {
        println!("Export window not found.");
        return Ok(false);
    }
    
    // Check if window title matches expected title
    if !contains(&err_wnd.ctext, expected_title, Some(false)) {
        println!("Export window title ({}) does not match expected title ({}).", err_wnd.ctext, expected_title);
        return Ok(false);
    }
    
    println!("Export window found with expected title: {}", expected_title);
    Ok(true)
}
