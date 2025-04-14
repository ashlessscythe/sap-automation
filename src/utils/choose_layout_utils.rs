use sap_scripting::*;
use std::time::Duration;
use std::thread;
use std::collections::HashMap;

use crate::utils::utils::*;
use crate::utils::sap_constants::*;
use crate::utils::sap_ctrl_utils::*;
use crate::utils::sap_tcode_utils::*;
use crate::utils::sap_wnd_utils::*;

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

/// Choose a layout from the layout selection window
///
/// This function is a port of the VBA function choose_layout
/// If layout not found, it will ask the user to type in another layout name or exit
pub fn choose_layout(session: &GuiSession, tcode: &str, layout_row: &str) -> windows::core::Result<String> {
    eprintln!("DEBUG: Entering choose_layout function with tcode={}, layout_row={}", tcode, layout_row);
    
    // Create a mutable copy of layout_row that we can modify in the loop
    let mut current_layout = layout_row.to_string();
    
    // Loop until a valid layout is found or user chooses to exit
    loop {
        let mut msg;
        
        // Check if window exists
        eprintln!("DEBUG: Checking if window exists");
        let err_wnd = exist_ctrl(session, 1, "", true)?;
        if !err_wnd.cband {
            // If window doesn't exist, trigger layout popup
            eprintln!("DEBUG: Window doesn't exist, triggering layout popup");
            layout_popup(session, tcode)?;
            
            // Check again if window exists after triggering popup
            let err_wnd = exist_ctrl(session, 1, "", true)?;
            if !err_wnd.cband {
                eprintln!("DEBUG: Window still doesn't exist after triggering layout popup");
                return Ok("Failed to open layout selection window".to_string());
            }
        } else {
            eprintln!("DEBUG: Window exists with title: {}", err_wnd.ctext);
        }
        
        // Check title based on window content
        if contains(&err_wnd.ctext.to_lowercase(), "choose", Some(false)) {
            // Window is a "choose" layout window
            eprintln!("DEBUG: Window is a 'choose' layout window");
        } else if contains(&err_wnd.ctext.to_lowercase(), "change", Some(false)) {
            // Window is a "change" layout window
            eprintln!("DEBUG: Window is a 'change' layout window");
        } else {
            eprintln!("DEBUG: Window is neither 'choose' nor 'change' layout window: {}", err_wnd.ctext);
        }
        
        // Find button - try both possible button IDs
        eprintln!("DEBUG: Finding search button");
        let mut button_found = false;
        
        // First try the standard button ID
        if let Ok(button) = session.find_by_id("wnd[1]/tbar[0]/btn[71]".to_string()) {
            if let Some(btn) = button.downcast::<GuiButton>() {
                eprintln!("DEBUG: Button found at wnd[1]/tbar[0]/btn[71], pressing it");
                btn.press()?;
                button_found = true;
            }
        }
        
        // If standard button not found, try alternative button ID for vl06o
        if !button_found {
            if let Ok(button) = session.find_by_id("wnd[1]/tbar[0]/btn[16]".to_string()) {
                if let Some(btn) = button.downcast::<GuiButton>() {
                    eprintln!("DEBUG: Button found at wnd[1]/tbar[0]/btn[16], pressing it");
                    btn.press()?;
                    button_found = true;
                }
            }
        }
        
        if !button_found {
            eprintln!("DEBUG: Search button not found, trying to continue anyway");
        }
        
        // Wait for window 2 to appear
        thread::sleep(Duration::from_millis(500));
        
        // Check if window 2 exists
        let err_wnd2 = exist_ctrl(session, 2, "", true)?;
        if !err_wnd2.cband {
            eprintln!("DEBUG: Window 2 does not exist, trying alternative approach");
            
            // Try to find the search field directly in window 1
            if let Ok(text_field) = session.find_by_id("wnd[1]/usr/txtRSYSF-STRING".to_string()) {
                if let Some(txt) = text_field.downcast::<GuiTextField>() {
                    eprintln!("DEBUG: Text field found in window 1, setting text to '{}'", current_layout);
                    txt.set_text(current_layout.clone())?;
                    
                    // Press Enter
                    if let Ok(window) = session.find_by_id("wnd[1]".to_string()) {
                        if let Some(wnd) = window.downcast::<GuiFrameWindow>() {
                            eprintln!("DEBUG: Pressing Enter on window 1");
                            wnd.send_v_key(0)?;
                        }
                    }
                }
            } else {
                eprintln!("DEBUG: Text field not found in window 1");
                return Ok("Failed to find search field".to_string());
            }
        } else {
            eprintln!("DEBUG: Window 2 exists with title: {}", err_wnd2.ctext);
            
            // Handle checkbox if it exists
            eprintln!("DEBUG: Checking if checkbox exists");
            let checkbox_exists = exist_ctrl(session, 2, "/usr/chkSCAN_STRING-START", true)?;
            if checkbox_exists.cband {
                eprintln!("DEBUG: Checkbox exists, attempting to unselect it");
                if let Ok(checkbox) = session.find_by_id("wnd[2]/usr/chkSCAN_STRING-START".to_string()) {
                    if let Some(chk) = checkbox.downcast::<GuiCheckBox>() {
                        eprintln!("DEBUG: Checkbox found, setting to unselected");
                        chk.set_selected(false)?;
                    } else {
                        eprintln!("DEBUG: Checkbox found but downcast failed");
                    }
                } else {
                    eprintln!("DEBUG: Failed to find checkbox by ID");
                }
            } else {
                eprintln!("DEBUG: Checkbox does not exist");
            }
            
            // Set layout name in text field
            eprintln!("DEBUG: Setting layout name in text field");
            if let Ok(text_field) = session.find_by_id("wnd[2]/usr/txtRSYSF-STRING".to_string()) {
                if let Some(txt) = text_field.downcast::<GuiTextField>() {
                    eprintln!("DEBUG: Text field found, setting text to '{}'", current_layout);
                    txt.set_text(current_layout.clone())?;
                } else {
                    eprintln!("DEBUG: Text field found but downcast failed");
                }
            } else {
                eprintln!("DEBUG: Text field not found");
                
                // Try alternative text field ID
                if let Ok(text_field) = session.find_by_id("wnd[2]/usr/txtGS_SEARCH-VALUE".to_string()) {
                    if let Some(txt) = text_field.downcast::<GuiTextField>() {
                        eprintln!("DEBUG: Alternative text field found, setting text to '{}'", current_layout);
                        txt.set_text(current_layout.clone())?;
                    } else {
                        eprintln!("DEBUG: Alternative text field found but downcast failed");
                    }
                } else {
                    eprintln!("DEBUG: Alternative text field not found");
                    return Ok("Failed to find search field".to_string());
                }
            }
            
            // Press Enter
            eprintln!("DEBUG: Pressing Enter on window 2");
            if let Ok(window) = session.find_by_id("wnd[2]".to_string()) {
                if let Some(wnd) = window.downcast::<GuiModalWindow>() {
                    eprintln!("DEBUG: Window found, sending v_key(0)");
                    wnd.send_v_key(0)?;
                } else {
                    eprintln!("DEBUG: Window found but downcast failed");
                }
            } else {
                eprintln!("DEBUG: Window 2 not found");
            }
        }
        
        // Wait for search results
        thread::sleep(Duration::from_millis(500));
        
        // Check window 3
        eprintln!("DEBUG: Checking if window 3 exists");
        let err_wnd = exist_ctrl(session, 3, "", true)?;
        if err_wnd.cband {
            eprintln!("DEBUG: Window 3 exists with title: {}", err_wnd.ctext);
            // Check if result exists
            eprintln!("DEBUG: Checking if result label exists");
            let result_exists = exist_ctrl(session, 3, "/usr/lbl[1,2]", true)?;
            if result_exists.cband {
                eprintln!("DEBUG: Result label exists with text: {}", result_exists.ctext);
                // Highlight
                eprintln!("DEBUG: Setting focus on result label");
                if let Ok(label) = session.find_by_id("wnd[3]/usr/lbl[1,2]".to_string()) {
                    if let Some(lbl) = label.downcast::<GuiLabel>() {
                        eprintln!("DEBUG: Label found, setting focus");
                        lbl.set_focus()?;
                    } else {
                        eprintln!("DEBUG: Label found but downcast failed");
                    }
                } else {
                    eprintln!("DEBUG: Failed to find label by ID");
                }

                // Click
                eprintln!("DEBUG: Clicking on window 3 (send_v_key(2))");
                if let Ok(window) = session.find_by_id("wnd[3]".to_string()) {
                    if let Some(wnd) = window.downcast::<GuiModalWindow>() {
                        eprintln!("DEBUG: Window found, sending v_key(2)");
                        wnd.send_v_key(2)?;
                    } else {
                        eprintln!("DEBUG: Window found but downcast failed");
                    }
                } else {
                    eprintln!("DEBUG: Window 3 not found for clicking");
                }

                // make sure wnd3 is closed
                let wnd3 = exist_ctrl(session, 3, "", true)?;
                if wnd3.cband {
                    eprintln!("DEBUG: Window 3 still exists, closing it");
                    close_popups(session, Some(3), None)?;
                } else {
                   eprintln!("DEBUG: Window 3 does not exist");
                }
                
                // make sure wnd2 is closed
                let wnd2 = exist_ctrl(session, 2, "", true)?;
                if wnd2.cband {
                    eprintln!("DEBUG: Window 2 still exists, closing it");
                    close_popups(session, Some(2), None)?;
                } else {
                    eprintln!("DEBUG: Window 2 does not exist");
                }

                // click on wnd1
                eprintln!("DEBUG: Clicking on window 1 (send_v_key(2))");
                if let Ok(window) = session.find_by_id("wnd[1]".to_string()) {
                    if let Some(wnd) = window.downcast::<GuiModalWindow>() {
                        eprintln!("DEBUG: Window found, sending v_key(2)");
                        wnd.send_v_key(2)?;
                    } else {
                        eprintln!("DEBUG: Window found but downcast failed");
                    }
                } else {
                    eprintln!("DEBUG: Window 1 not found for clicking");
                }

                
                // Layout found, break out of the loop
                eprintln!("DEBUG: Break loop after wnd1");
                break;
            } else {
                // Error info window - layout not found
                eprintln!("DEBUG: Result label does not exist, layout not found");
                
                // Close error window if it exists
                close_popups(session, Some(-1), Some(1))?;
                
                // Ask user for a new layout name or to exit
                use dialoguer::{Select, Input};
                
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
                        msg = "Layout selection cancelled".to_string();
                        close_popups(session, None, None)?;
                        return Ok(msg);
                    }
                    
                    // Update current_layout and try again
                    current_layout = new_layout;
                    
                    // Close any remaining popups before retrying
                    close_popups(session, None, None)?;
                    
                    // Trigger layout popup again for the next iteration
                    eprintln!("LAYOUT:calling layout_popup");
                    layout_popup(session, tcode)?;
                    
                    // Continue to next iteration of the loop
                    continue;
                } else {
                    // User wants to exit
                    msg = "Layout selection cancelled".to_string();
                    close_popups(session, None, None)?;
                    return Ok(msg);
                }
            }
        } else {
            eprintln!("DEBUG: Window 3 does not exist");
        }
        
        // Close any remaining windows using the improved close_popups function
        eprintln!("DEBUG: Closing any remaining windows");
        close_popups(session, None, None)?;
        
        // Break out of the loop if we've reached this point
        break;
    }

    // pause for a couple secs
    thread::sleep(Duration::from_secs(2));
    
    // Get status bar message
    eprintln!("DEBUG: Getting status bar message");
    let msg = hit_ctrl(session, 0, "/sbar", "Text", "Get", "")?;
    
    eprintln!("DEBUG: Status bar message: {}", msg);
    println!("{}", msg);
    eprintln!("DEBUG: Exiting choose_layout function with message: {}", msg);
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
