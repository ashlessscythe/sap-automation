use sap_scripting::*;
use std::time::Duration;
use std::thread;

use crate::utils::sap_ctrl_utils::*;

/// Struct to hold layout parameters
#[derive(Debug, Clone)]
#[derive(Default)]
pub struct LayoutParams {
    pub run_check: bool,
    pub err: String,
    pub name: String,
    pub type_name: String,
}


/// Setup a layout with the given parameters
///
/// This function is a port of the VBA function SetupLayout
pub fn setup_layout(session: &GuiSession, n_wnd: i32, base_obj_name: &str, layout_name: &str, 
                   list: &[String], limit: i32, no_save: bool) -> windows::core::Result<bool> {
    println!("Setting up layout: {}", layout_name);
    
    let obj_name = format!("wnd[{}]{}", n_wnd, base_obj_name);
    let obj_list_left = format!("{}/cntlCONTAINER2_LAYO/shellcont/shell", obj_name);
    let obj_list_right = format!("{}/cntlCONTAINER1_LAYO/shellcont/shell", obj_name);
    let obj_button_to_right = format!("{}/btnAPP_FL_SING", obj_name);
    
    // Get the left grid
    let left_grid = session.find_by_id(obj_list_left.clone())?;
    
    // Get the right grid
    let right_grid = session.find_by_id(obj_list_right.clone())?;
    
    // Try to downcast to GuiGridView
    if let (Some(grid_left), Some(grid_right)) = (left_grid.downcast::<GuiGridView>(), right_grid.downcast::<GuiGridView>()) {
        // Set current cell row to 0
        grid_left.set_current_cell_row(0)?;
        
        // Move current items from left grid to right grid
        grid_left.set_selected_rows(format!("0-{}", grid_left.row_count()? - 1))?;
        
        // Press the button to move to right
        if let Ok(button) = session.find_by_id(obj_button_to_right) {
            if let Some(btn) = button.downcast::<GuiButton>() {
                btn.press()?;
            }
        }
        
        // Reset selection
        grid_left.set_current_cell_row(-1)?;
        grid_right.set_current_cell_row(1)?;
        
        // Sort by column
        grid_right.select_column("SELTEXT".to_string())?;
        grid_right.press_column_header("SELTEXT".to_string())?;
        
        // Add items from list to layout
        for item in list {
            let mut found = false;
            for i in 0..limit {
                if let Ok(name) = grid_right.get_cell_value(i, "SELTEXT".to_string()) {
                    println!("Checking: {}", name);
                    if item.to_uppercase() == name.to_uppercase() {
                        grid_right.set_current_cell_row(i)?;
                        grid_right.double_click_current_cell()?;
                        thread::sleep(Duration::from_millis(100));
                        found = true;
                        break;
                    }
                }
            }
            
            if !found {
                println!("Item ({}) not found, skipping", item);
            }
        }
        
        println!("Added {} items to Layout", list.len());
        
        // Save the layout if not no_save
        if !no_save {
            // Save layout button
            if let Ok(button) = session.find_by_id("wnd[1]/tbar[0]/btn[5]".to_string()) {
                if let Some(btn) = button.downcast::<GuiButton>() {
                    btn.press()?;
                }
            }
            
            // User-Specific
            if let Ok(checkbox) = session.find_by_id("wnd[2]/usr/tabsG50_TABSTRIP/tabpTAB_D0501/ssubD0505_SUBSCREEN:SAPLSLVC_DIALOG:0501/chkG51_USPEC".to_string()) {
                if let Some(chk) = checkbox.downcast::<GuiCheckBox>() {
                    chk.set_selected(true)?;
                }
            }
            
            // Default Layout Yes/No
            if let Ok(checkbox) = session.find_by_id("wnd[2]/usr/tabsG50_TABSTRIP/tabpTAB_D0501/ssubD0505_SUBSCREEN:SAPLSLVC_DIALOG:0501/chkLTVARIANT-DEFAULTVAR".to_string()) {
                if let Some(chk) = checkbox.downcast::<GuiCheckBox>() {
                    chk.set_selected(false)?;
                }
            }
            
            // Save as name
            if let Ok(textfield) = session.find_by_id("wnd[2]/usr/tabsG50_TABSTRIP/tabpTAB_D0501/ssubD0505_SUBSCREEN:SAPLSLVC_DIALOG:0501/txtLTDX-VARIANT".to_string()) {
                if let Some(txt) = textfield.downcast::<GuiTextField>() {
                    txt.set_text(layout_name.to_string())?;
                }
            }
            
            // Save as Description
            if let Ok(textfield) = session.find_by_id("wnd[2]/usr/tabsG50_TABSTRIP/tabpTAB_D0501/ssubD0505_SUBSCREEN:SAPLSLVC_DIALOG:0501/txtLTDXT-TEXT".to_string()) {
                if let Some(txt) = textfield.downcast::<GuiTextField>() {
                    txt.set_text(layout_name.to_string())?;
                }
            }
            
            // Green Checkmark wnd2
            if let Ok(button) = session.find_by_id("wnd[2]/tbar[0]/btn[0]".to_string()) {
                if let Some(btn) = button.downcast::<GuiButton>() {
                    btn.press()?;
                }
            }
            
            // LayoutExists Overwrite Y/N
            let err_ctrl = exist_ctrl(session, 3, "", true)?;
            if err_ctrl.cband {
                if let Ok(button) = session.find_by_id("wnd[3]/usr/btnSPOP-OPTION1".to_string()) {
                    if let Some(btn) = button.downcast::<GuiButton>() {
                        btn.press()?;
                    }
                }
            }
        }
        
        // Green Checkmark wnd1
        if let Ok(button) = session.find_by_id("wnd[1]/tbar[0]/btn[0]".to_string()) {
            if let Some(btn) = button.downcast::<GuiButton>() {
                btn.press()?;
            }
        }
        
        // Get status bar message
        let status_msg = hit_ctrl(session, 0, "/sbar", "Text", "Get", "")?;
        println!("Layout ({}) Successfully Setup: {}", layout_name, status_msg);
        
        return Ok(true);
    }
    
    Ok(false)
}
