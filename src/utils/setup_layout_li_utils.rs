use sap_scripting::*;
use std::time::Duration;
use std::thread;

use crate::utils::utils::*;
use crate::utils::sap_ctrl_utils::*;

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

/// Setup a layout with list items
///
/// This function is a port of the VBA function SetupLayout_li
pub fn setup_layout_li(session: &GuiSession, tcode: &str, n_wnd: i32, base_obj_name: &str, 
                      layout_name: &str, list: &[String], limit: i32, no_save: bool) -> windows::core::Result<bool> {
    println!("Setting up layout with list items: {}", layout_name);
    
    let wnd = format!("wnd[{}]", n_wnd);
    let obj_name = base_obj_name;
    let obj_list_left = format!("{}{}/tblSAPLSKBHTC_WRITE_LIST", wnd, obj_name);
    let obj_list_right = format!("{}/usr/tblSAPLSKBHTC_FIELD_LIST", wnd);
    let obj_button_to_left = format!("{}/usr/btnAPP_WL_SING", wnd);
    let obj_button_to_right = format!("{}/usr/btnAPP_FL_SING", wnd);
    
    let nw_obj_list_left = format!("{}/tblSAPLSKBHTC_WRITE_LIST", obj_name);
    let _nw_obj_list_right = "/usr/tblSAPLSKBHTC_FIELD_LIST";
    
    // Get the left grid
    let left_grid = session.find_by_id(obj_list_left.clone())?;
    
    // Try to downcast to GuiGridView
    if let Some(grid_left) = left_grid.downcast::<GuiGridView>() {
        // Get row count
        let rc = grid_left.row_count()?;
        let vrc = grid_left.visible_row_count()?;
        let curr_row = grid_left.current_cell_row()?;
        let scrl_pos = (curr_row as f64 / rc as f64 * rc as f64).floor() as i32;
        
        println!("Row count is {}", rc);
        println!("Visible rowcount is {}", vrc);
        println!("Vertical Scrollbar Position is {}", scrl_pos);
        
        // Clear left grid
        println!("{} items found in current layout. Clearing...", rc);
        
        // Select all rows
        for j in 1..rc {
            let ctrl_id = format!("{}/txtGT_WRITE_LIST-SELTEXT[0,{}]", nw_obj_list_left, j);
            let err_ctrl = exist_ctrl(session, 1, &ctrl_id, true)?;
            
            if err_ctrl.cband {
                if !contains(&err_ctrl.ctext, "**_**", Some(false)) {
                    // Select row
                    grid_left.set_selected_rows(j.to_string())?;
                    
                    // Scroll down if needed
                    if j % 12 == 0 {
                        let err_ctrl = exist_ctrl(session, 1, &nw_obj_list_left, true)?;
                        if err_ctrl.cband {
                            let _scroll_position = (curr_row as f64 / rc as f64 * rc as f64).floor() as i32;
                            println!("Vertical Scrollbar Position is {}", scrl_pos);
                            
                            // Scroll down
                            grid_left.set_current_cell(scrl_pos + 12, "down".into())?;
                        }
                    }
                } else {
                    break;
                }
            }
        }
        
        // Move to right (clear)
        if let Ok(button) = session.find_by_id(obj_button_to_right.clone()) {
            if let Some(btn) = button.downcast::<GuiButton>() {
                btn.press()?;
            }
        }
        
        // Start working with right list
        // Order alphabetical
        if let Ok(button) = session.find_by_id("wnd[1]/usr/btn%#AUTOTEXT002".to_string()) {
            if let Some(btn) = button.downcast::<GuiButton>() {
                btn.press()?;
            }
        }
        
        // Get the right grid
        let right_grid = session.find_by_id(obj_list_right.clone())?;
        
        // Try to downcast to GuiGridView
        if let Some(grid_right) = right_grid.downcast::<GuiGridView>() {
            // Get row count
            let rc = grid_right.row_count()?;
            let _vrc = grid_right.visible_row_count()?;
            let _scrl_pos = (curr_row as f64 / rc as f64 * rc as f64).floor() as i32;
            
            // Add items from list to layout
            for item in list {
                if item.to_uppercase() != "SHIPMENT NUMBER" && 
                   item.to_uppercase() != "DELIVERY" && 
                   !contains(&item.to_uppercase(), "_LAYOUT", Some(false)) {
                    
                    let mut counter = 1;
                    let mut found = false;
                    
                    while counter < limit {
                        counter += 1;
                        
                        for i in 0..12 {
                            // Check if row exists and get cell value directly
                            if i < grid_right.row_count()? {
                                if let Ok(name) = grid_right.get_cell_value(i, "SELTEXT".to_string()) {
                                    let name = name.trim().to_string();
                                    
                                    if item.to_uppercase() == name.to_uppercase() {
                                        // Found, select row
                                        grid_right.set_selected_rows(i.to_string())?;
                                        println!("Item ({}) selected from right", name);
                                        thread::sleep(Duration::from_millis(100));
                                        
                                        // Move to left - create a fresh clone of the button path for each use
                                        if let Ok(button) = session.find_by_id(obj_button_to_left.clone()) {
                                            if let Some(btn) = button.downcast::<GuiButton>() {
                                                btn.press()?;
                                            }
                                        }
                                        
                                        println!("Item ({}) moved to left", name);
                                        
                                        // Reset scrollbar to top - Alternative to vertical_scrollbar
                                        grid_right.set_first_visible_row(0)?;
                                        
                                        found = true;
                                        break;
                                    }
                                }
                            } else {
                                // Row doesn't exist, item not found
                                println!("Item ({}) not found, skipping", item);
                                
                                // Reset scrollbar to top - Alternative to vertical_scrollbar
                                grid_right.set_first_visible_row(0)?;
                                
                                found = true;
                                break;
                            }
                        }
                        
                        if found {
                            break;
                        }
                        
                        // Scroll down - Alternative to vertical_scrollbar
                        let err_ctrl = exist_ctrl(session, 1, &nw_obj_list_left, true)?;
                        if err_ctrl.cband {
                            let current_first_row = grid_right.first_visible_row()?;
                            
                            // Scroll down by setting first visible row
                            grid_right.set_first_visible_row(current_first_row + 12)?;
                            
                            let new_first_row = grid_right.first_visible_row()?;
                            
                            // Check if scrollbar moved (if not it reached bottom)
                            if current_first_row == new_first_row {
                                println!("Item ({}) not found, skipping", item);
                                
                                // Reset scrollbar to top - Alternative to vertical_scrollbar
                                grid_right.set_first_visible_row(0)?;
                                
                                found = true;
                                break;
                            }
                        }
                    }
                }
            }
            
            println!("Added {} items to Layout", list.len());
            
            // Save the layout based on tcode
            if !no_save {
                match tcode.to_uppercase().as_str() {
                    "MB52" => {
                        // Enter
                        if let Ok(button) = session.find_by_id("wnd[1]/tbar[0]/btn[0]".to_string()) {
                            if let Some(btn) = button.downcast::<GuiButton>() {
                                btn.press()?;
                            }
                        }
                        
                        // Save
                        if let Ok(button) = session.find_by_id("wnd[0]/tbar[1]/btn[34]".to_string()) {
                            if let Some(btn) = button.downcast::<GuiButton>() {
                                btn.press()?;
                            }
                        }
                        
                        // Check layout name length
                        if layout_name.len() > 12 {
                            println!("Layout name is more than 12 characters, unable to save.");
                            
                            // Close window if exists
                            let err_ctrl = exist_ctrl(session, 1, "", true)?;
                            if err_ctrl.cband {
                                if let Ok(window) = session.find_by_id("wnd[1]".to_string()) {
                                    if let Some(wnd) = window.downcast::<GuiFrameWindow>() {
                                        wnd.close()?;
                                    }
                                }
                            }
                        } else {
                            // Set layout name
                            if let Ok(textfield) = session.find_by_id("wnd[1]/usr/ctxtLTDX-VARIANT".to_string()) {
                                if let Some(txt) = textfield.downcast::<GuiTextField>() {
                                    txt.set_text(layout_name.to_string())?;
                                }
                            }
                            
                            // Set layout description
                            if let Ok(textfield) = session.find_by_id("wnd[1]/usr/txtLTDXT-TEXT".to_string()) {
                                if let Some(txt) = textfield.downcast::<GuiTextField>() {
                                    txt.set_text(layout_name.to_string())?;
                                }
                            }
                            
                            // Enter
                            if let Ok(window) = session.find_by_id("wnd[1]".to_string()) {
                                if let Some(wnd) = window.downcast::<GuiFrameWindow>() {
                                    wnd.send_v_key(0)?;
                                }
                            }
                            
                            // LayoutExists Overwrite Y/N
                            let err_ctrl = exist_ctrl(session, 2, "", true)?;
                            if err_ctrl.cband {
                                if let Ok(window) = session.find_by_id("wnd[2]".to_string()) {
                                    if let Some(wnd) = window.downcast::<GuiFrameWindow>() {
                                        wnd.send_v_key(0)?;
                                    }
                                }
                            }
                        }
                    },
                    "LX03" | "LX02" => {
                        // Enter
                        if let Ok(button) = session.find_by_id("wnd[1]/tbar[0]/btn[0]".to_string()) {
                            if let Some(btn) = button.downcast::<GuiButton>() {
                                btn.press()?;
                            }
                        }
                        
                        // Save
                        if let Ok(button) = session.find_by_id("wnd[0]/tbar[1]/btn[36]".to_string()) {
                            if let Some(btn) = button.downcast::<GuiButton>() {
                                btn.press()?;
                            }
                        }
                        
                        // Check layout name length
                        if layout_name.len() > 12 {
                            println!("Layout name is more than 12 characters, unable to save.");
                            
                            // Close window if exists
                            let err_ctrl = exist_ctrl(session, 1, "", true)?;
                            if err_ctrl.cband {
                                if let Ok(window) = session.find_by_id("wnd[1]".to_string()) {
                                    if let Some(wnd) = window.downcast::<GuiFrameWindow>() {
                                        wnd.close()?;
                                    }
                                }
                            }
                        } else {
                            // Set layout name
                            if let Ok(textfield) = session.find_by_id("wnd[1]/usr/ctxtLTDX-VARIANT".to_string()) {
                                if let Some(txt) = textfield.downcast::<GuiTextField>() {
                                    txt.set_text(layout_name.to_string())?;
                                }
                            }
                            
                            // Set layout description
                            if let Ok(textfield) = session.find_by_id("wnd[1]/usr/txtLTDXT-TEXT".to_string()) {
                                if let Some(txt) = textfield.downcast::<GuiTextField>() {
                                    txt.set_text(layout_name.to_string())?;
                                }
                            }
                            
                            // Enter
                            if let Ok(window) = session.find_by_id("wnd[1]".to_string()) {
                                if let Some(wnd) = window.downcast::<GuiFrameWindow>() {
                                    wnd.send_v_key(0)?;
                                }
                            }
                            
                            // LayoutExists Overwrite Y/N
                            let err_ctrl = exist_ctrl(session, 2, "", true)?;
                            if err_ctrl.cband {
                                if let Ok(window) = session.find_by_id("wnd[2]".to_string()) {
                                    if let Some(wnd) = window.downcast::<GuiFrameWindow>() {
                                        wnd.send_v_key(0)?;
                                    }
                                }
                            }
                        }
                    },
                    "LT23" => {
                        // Enter (Close window)
                        if let Ok(window) = session.find_by_id("wnd[1]".to_string()) {
                            if let Some(wnd) = window.downcast::<GuiFrameWindow>() {
                                wnd.send_v_key(0)?;
                            }
                        }
                        
                        // Save layout button
                        if let Ok(menu) = session.find_by_id("wnd[0]/mbar/menu[3]/menu[2]/menu[3]".to_string()) {
                            if let Some(menu_item) = menu.downcast::<GuiMenu>() {
                                menu_item.select()?;
                            }
                        }
                        
                        // User Specific
                        if let Ok(checkbox) = session.find_by_id("wnd[1]/usr/chkG_FOR_USER".to_string()) {
                            if let Some(chk) = checkbox.downcast::<GuiCheckBox>() {
                                chk.set_selected(true)?;
                            }
                        }
                        
                        // LayoutName
                        if let Ok(textfield) = session.find_by_id("wnd[1]/usr/ctxtLTDX-VARIANT".to_string()) {
                            if let Some(txt) = textfield.downcast::<GuiTextField>() {
                                txt.set_text(layout_name.to_string())?;
                            }
                        }
                        
                        // Layout Description
                        if let Ok(textfield) = session.find_by_id("wnd[1]/usr/txtLTDXT-TEXT".to_string()) {
                            if let Some(txt) = textfield.downcast::<GuiTextField>() {
                                txt.set_text(layout_name.to_string())?;
                            }
                        }
                        
                        // Enter (Save)
                        if let Ok(window) = session.find_by_id("wnd[1]".to_string()) {
                            if let Some(wnd) = window.downcast::<GuiFrameWindow>() {
                                wnd.send_v_key(0)?;
                            }
                        }
                        
                        // LayoutExists Overwrite Y/N
                        let err_ctrl = exist_ctrl(session, 2, "", true)?;
                        if err_ctrl.cband {
                            if let Ok(window) = session.find_by_id("wnd[2]".to_string()) {
                                if let Some(wnd) = window.downcast::<GuiFrameWindow>() {
                                    wnd.send_v_key(0)?;
                                }
                            }
                        }
                    },
                    "VT11" => {
                        // Enter (Close window)
                        if let Ok(window) = session.find_by_id("wnd[1]".to_string()) {
                            if let Some(wnd) = window.downcast::<GuiFrameWindow>() {
                                wnd.send_v_key(0)?;
                            }
                        }
                        
                        // Save layout button
                        if let Ok(menu) = session.find_by_id("wnd[0]/mbar/menu[3]/menu[0]/menu[3]".to_string()) {
                            if let Some(menu_item) = menu.downcast::<GuiMenu>() {
                                menu_item.select()?;
                            }
                        }
                        
                        // User Specific
                        if let Ok(checkbox) = session.find_by_id("wnd[1]/usr/chkG_FOR_USER".to_string()) {
                            if let Some(chk) = checkbox.downcast::<GuiCheckBox>() {
                                chk.set_selected(true)?;
                            }
                        }
                        
                        // Save as name
                        if let Ok(textfield) = session.find_by_id("wnd[1]/usr/ctxtLTDX-VARIANT".to_string()) {
                            if let Some(txt) = textfield.downcast::<GuiTextField>() {
                                txt.set_text(layout_name.to_string())?;
                            }
                        }
                        
                        // Save as Description
                        if let Ok(textfield) = session.find_by_id("wnd[1]/usr/txtLTDXT-TEXT".to_string()) {
                            if let Some(txt) = textfield.downcast::<GuiTextField>() {
                                txt.set_text(layout_name.to_string())?;
                            }
                        }
                        
                        // Enter (Save)
                        if let Ok(window) = session.find_by_id("wnd[1]".to_string()) {
                            if let Some(wnd) = window.downcast::<GuiFrameWindow>() {
                                wnd.send_v_key(0)?;
                            }
                        }
                        
                        // LayoutExists Overwrite Y/N
                        let err_ctrl = exist_ctrl(session, 2, "", true)?;
                        if err_ctrl.cband {
                            if let Ok(window) = session.find_by_id("wnd[2]".to_string()) {
                                if let Some(wnd) = window.downcast::<GuiFrameWindow>() {
                                    wnd.send_v_key(0)?;
                                }
                            }
                        }
                    },
                    "VL06O" => {
                        // Enter (Close window)
                        if let Ok(window) = session.find_by_id("wnd[1]".to_string()) {
                            if let Some(wnd) = window.downcast::<GuiFrameWindow>() {
                                wnd.send_v_key(0)?;
                            }
                        }
                        
                        // Save Layout button
                        if let Ok(menu) = session.find_by_id("wnd[0]/mbar/menu[3]/menu[2]/menu[3]".to_string()) {
                            if let Some(menu_item) = menu.downcast::<GuiMenu>() {
                                menu_item.select()?;
                            }
                        }
                        
                        // User Specific
                        if let Ok(checkbox) = session.find_by_id("wnd[1]/usr/chkG_FOR_USER".to_string()) {
                            if let Some(chk) = checkbox.downcast::<GuiCheckBox>() {
                                chk.set_selected(true)?;
                            }
                        }
                        
                        // Save layout name
                        if let Ok(textfield) = session.find_by_id("wnd[1]/usr/ctxtLTDX-VARIANT".to_string()) {
                            if let Some(txt) = textfield.downcast::<GuiTextField>() {
                                txt.set_text(layout_name.to_string())?;
                            }
                        }
                        
                        // Save layout description
                        if let Ok(textfield) = session.find_by_id("wnd[1]/usr/txtLTDXT-TEXT".to_string()) {
                            if let Some(txt) = textfield.downcast::<GuiTextField>() {
                                txt.set_text(layout_name.to_string())?;
                            }
                        }
                        
                        // Enter (Save)
                        if let Ok(window) = session.find_by_id("wnd[1]".to_string()) {
                            if let Some(wnd) = window.downcast::<GuiFrameWindow>() {
                                wnd.send_v_key(0)?;
                            }
                        }
                        
                        // LayoutExists Overwrite Y/N
                        let err_ctrl = exist_ctrl(session, 2, "", true)?;
                        if err_ctrl.cband {
                            if let Ok(window) = session.find_by_id("wnd[2]".to_string()) {
                                if let Some(wnd) = window.downcast::<GuiFrameWindow>() {
                                    wnd.send_v_key(0)?;
                                }
                            }
                        }
                    },
                    _ => {
                        println!("Tcode {} not implemented for layout saving", tcode);
                    }
                }
            }
            
            // Get status bar message
            let status_msg = hit_ctrl(session, 0, "/sbar", "Text", "Get", "")?;
            println!("Layout ({}) Successfully Setup: {}", layout_name, status_msg);
            
            return Ok(true);
        }
    }
    
    Ok(false)
}
