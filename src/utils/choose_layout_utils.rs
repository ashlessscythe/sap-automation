//! Layout chooser ported from VBA; binary does not construct every helper type.
#![allow(dead_code)]

use sap_scripting::*;
use std::thread;
use std::time::Duration;

use crate::utils::sap_ctrl_utils::*;
use crate::utils::sap_wnd_utils::*;
use crate::utils::utils::*;

/// Struct to hold layout parameters
#[derive(Debug, Clone, Default)]
pub struct LayoutParams {
    pub run_check: bool,
    pub err: String,
    pub name: String,
    pub type_name: String,
}

/// Choose a layout from the layout selection window
///
/// This function is a port of the VBA function choose_layout
/// If layout not found, it will ask the user to type in another layout name or exit
pub fn choose_layout(
    session: &GuiSession,
    tcode: &str,
    layout_row: &str,
) -> windows::core::Result<String> {
    eprintln!(
        "DEBUG: Entering choose_layout function with tcode={}, layout_row={}",
        tcode, layout_row
    );

    // Special handling for 149 tcode (y_dn3_47000149)
    if tcode.to_lowercase() == "y_dn3_47000149" {
        eprintln!("DEBUG: Using special 149 layout selection method");
        return choose_layout_149(session, layout_row);
    }

    // Special handling for ZVT11 tcode
    if tcode.to_lowercase() == "zvt11" {
        eprintln!("DEBUG: Using special ZVT11 layout selection method");
        return choose_layout_zvt11(session, layout_row);
    }

    // Create a mutable copy of layout_row that we can modify in the loop
    let mut current_layout = layout_row.to_string();

    // Loop until a valid layout is found or user chooses to exit
    loop {
        let msg;

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
            eprintln!(
                "DEBUG: Window is neither 'choose' nor 'change' layout window: {}",
                err_wnd.ctext
            );
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
                    eprintln!(
                        "DEBUG: Text field found in window 1, setting text to '{}'",
                        current_layout
                    );
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
                if let Ok(checkbox) =
                    session.find_by_id("wnd[2]/usr/chkSCAN_STRING-START".to_string())
                {
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
                    eprintln!(
                        "DEBUG: Text field found, setting text to '{}'",
                        current_layout
                    );
                    txt.set_text(current_layout.clone())?;
                } else {
                    eprintln!("DEBUG: Text field found but downcast failed");
                }
            } else {
                eprintln!("DEBUG: Text field not found");

                // Try alternative text field ID
                if let Ok(text_field) =
                    session.find_by_id("wnd[2]/usr/txtGS_SEARCH-VALUE".to_string())
                {
                    if let Some(txt) = text_field.downcast::<GuiTextField>() {
                        eprintln!(
                            "DEBUG: Alternative text field found, setting text to '{}'",
                            current_layout
                        );
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
                eprintln!(
                    "DEBUG: Result label exists with text: {}",
                    result_exists.ctext
                );
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
                use dialoguer::{Input, Select};

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
    eprintln!(
        "DEBUG: Exiting choose_layout function with message: {}",
        msg
    );
    Ok(msg)
}

/// Special layout selection method for 149 tcode (y_dn3_47000149)
///
/// This method uses a grid-based approach instead of search dialog
/// Based on the VBA code:
/// session.findById("wnd[0]/tbar[1]/btn[33]").press
/// session.findById("wnd[1]/usr/ssubD0500_SUBSCREEN:SAPLSLVC_DIALOG:0501/cntlG51_CONTAINER/shellcont/shell").setCurrentCell 6,"TEXT"
/// session.findById("wnd[1]/usr/ssubD0500_SUBSCREEN:SAPLSLVC_DIALOG:0501/cntlG51_CONTAINER/shellcont/shell").selectedRows = "6"
/// session.findById("wnd[1]/usr/ssubD0500_SUBSCREEN:SAPLSLVC_DIALOG:0501/cntlG51_CONTAINER/shellcont/shell").clickCurrentCell
pub fn choose_layout_149(session: &GuiSession, layout_name: &str) -> windows::core::Result<String> {
    eprintln!(
        "DEBUG: Starting 149 layout selection for layout: {}",
        layout_name
    );

    // Step 1: Press the layout button (wnd[0]/tbar[1]/btn[33])
    eprintln!("DEBUG: Pressing layout button");
    if let Ok(button) = session.find_by_id("wnd[0]/tbar[1]/btn[33]".to_string()) {
        if let Some(btn) = button.downcast::<GuiButton>() {
            btn.press()?;
            eprintln!("DEBUG: Layout button pressed successfully");
        } else {
            eprintln!("DEBUG: Layout button found but downcast failed");
            return Ok("Failed to press layout button".to_string());
        }
    } else {
        eprintln!("DEBUG: Layout button not found");
        return Ok("Failed to find layout button".to_string());
    }

    // Wait for window 1 to appear
    thread::sleep(Duration::from_millis(1000));

    // Step 2: Find the grid container
    let grid_id =
        "wnd[1]/usr/ssubD0500_SUBSCREEN:SAPLSLVC_DIALOG:0501/cntlG51_CONTAINER/shellcont/shell";
    eprintln!("DEBUG: Looking for grid at: {}", grid_id);

    if let Ok(grid_obj) = session.find_by_id(grid_id.to_string()) {
        if let Some(grid) = grid_obj.downcast::<GuiGridView>() {
            eprintln!("DEBUG: Grid found successfully");

            // Step 3: Search for the layout in the grid
            let mut layout_found = false;
            let mut layout_row = -1;

            // Get the number of rows in the grid
            let row_count = grid.row_count()?;
            eprintln!("DEBUG: Grid has {} rows", row_count);

            // Search through the grid rows to find the layout
            for i in 0..row_count {
                if let Ok(cell_text) = grid.get_cell_value(i, "TEXT".to_string()) {
                    eprintln!("DEBUG: Row {} has text: '{}'", i, cell_text);
                    if cell_text.trim().to_lowercase() == layout_name.trim().to_lowercase() {
                        eprintln!("DEBUG: Layout '{}' found at row {}", layout_name, i);
                        layout_found = true;
                        layout_row = i;
                        break;
                    }
                }
            }

            if layout_found {
                // Step 4: Set current cell to the found row
                eprintln!(
                    "DEBUG: Setting current cell to row {} with column TEXT",
                    layout_row
                );
                grid.set_current_cell(layout_row, "TEXT".to_string())?;

                // Step 5: Select the row
                eprintln!("DEBUG: Selecting row {}", layout_row);
                grid.set_selected_rows(layout_row.to_string())?;

                // Step 6: Click on the current cell
                eprintln!("DEBUG: Clicking on current cell");
                grid.click_current_cell()?;

                eprintln!("DEBUG: 149 layout selection completed successfully");
                Ok("Layout selected successfully".to_string())
            } else {
                eprintln!("DEBUG: Layout '{}' not found in grid", layout_name);

                // Ask user for a new layout name or to exit
                use dialoguer::{Input, Select};

                println!("Layout '{}' not found in the grid.", layout_name);

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
                        close_popups(session, None, None)?;
                        return Ok("Layout selection cancelled".to_string());
                    }

                    // Close any remaining popups and try again
                    close_popups(session, None, None)?;

                    // Recursively call the function with the new layout name
                    choose_layout_149(session, &new_layout)
                } else {
                    // User wants to exit
                    close_popups(session, None, None)?;
                    Ok("Layout selection cancelled".to_string())
                }
            }
        } else {
            eprintln!("DEBUG: Grid object found but downcast failed");
            Ok("Failed to access grid object".to_string())
        }
    } else {
        eprintln!("DEBUG: Grid not found at: {}", grid_id);
        Ok("Failed to find layout grid".to_string())
    }
}

/// Default Displayed Columns for inbond material.php paste (Plant … Logistics Reference).
pub fn inbond_default_layout_columns() -> Vec<String> {
    vec![
        "Plant".to_string(),
        "Delivery Number".to_string(),
        "Material".to_string(),
        "Quantity".to_string(),
        "UOM".to_string(),
        "Logistics Reference Number".to_string(),
    ]
}

/// Try to select an existing 149 layout by name. Returns `true` if selected.
/// Closes the choose-layout popup when not found (no interactive prompt).
pub fn try_select_layout_149(
    session: &GuiSession,
    layout_name: &str,
) -> windows::core::Result<bool> {
    if layout_name.trim().is_empty() {
        return Ok(false);
    }

    eprintln!(
        "DEBUG: try_select_layout_149 for layout: {}",
        layout_name
    );

    if let Ok(button) = session.find_by_id("wnd[0]/tbar[1]/btn[33]".to_string()) {
        if let Some(btn) = button.downcast::<GuiButton>() {
            btn.press()?;
        } else {
            return Ok(false);
        }
    } else {
        println!("149 layout button (btn[33]) not found");
        return Ok(false);
    }

    thread::sleep(Duration::from_millis(1000));

    let grid_id =
        "wnd[1]/usr/ssubD0500_SUBSCREEN:SAPLSLVC_DIALOG:0501/cntlG51_CONTAINER/shellcont/shell";

    if let Ok(grid_obj) = session.find_by_id(grid_id.to_string()) {
        if let Some(grid) = grid_obj.downcast::<GuiGridView>() {
            let row_count = grid.row_count()?;
            for i in 0..row_count {
                if let Ok(cell_text) = grid.get_cell_value(i, "TEXT".to_string()) {
                    if cell_text.trim().eq_ignore_ascii_case(layout_name.trim()) {
                        grid.set_current_cell(i, "TEXT".to_string())?;
                        grid.set_selected_rows(i.to_string())?;
                        grid.click_current_cell()?;
                        eprintln!("DEBUG: Layout '{}' selected", layout_name);
                        return Ok(true);
                    }
                }
            }
        }
    }

    println!(
        "Layout '{}' not found. Closing choose-layout dialog...",
        layout_name
    );
    close_popups(session, None, None)?;
    Ok(false)
}

fn open_change_layout_149(session: &GuiSession) -> windows::core::Result<bool> {
    // Change Layout toolbar button (same pattern as VBA SetupLayout for ALV reports)
    if let Ok(button) = session.find_by_id("wnd[0]/tbar[1]/btn[32]".to_string()) {
        if let Some(btn) = button.downcast::<GuiButton>() {
            btn.press()?;
            thread::sleep(Duration::from_millis(800));
            return Ok(true);
        }
    }

    // Fallback: Settings → Layout → Change (common ALV menu path)
    if let Ok(menu) = session.find_by_id("wnd[0]/mbar/menu[3]/menu[0]/menu[0]".to_string()) {
        if let Some(menu_item) = menu.downcast::<GuiMenu>() {
            menu_item.select()?;
            thread::sleep(Duration::from_millis(800));
            return Ok(true);
        }
    }

    println!("Could not open Change Layout dialog for 149");
    Ok(false)
}

const LAYOUT_149_BASE: &str =
    "/usr/tabsG_TS_ALV/tabpALV_M_R1/ssubSUB_DYN0510:SAPLSKBH:0620";

/// Ensure 149 has the inbond layout: select if present, otherwise set up default columns and save.
///
/// Returns `(ok, saved_layout_name)`:
/// - existing layout selected → `(true, None)`
/// - columns set up and saved in SAP → `(true, Some(name))` so callers can persist to config
/// - columns set up but not saved → `(true, None)`
/// - failure → `(false, None)`
pub fn ensure_inbond_layout_149(
    session: &GuiSession,
    layout_name: &str,
    columns: &[String],
) -> windows::core::Result<(bool, Option<String>)> {
    use crate::utils::setup_layout_utils::setup_layout;
    use dialoguer::{Confirm, Input};

    let save_name = if layout_name.trim().is_empty() {
        "inb_ship".to_string()
    } else {
        layout_name.trim().to_string()
    };

    let cols: Vec<String> = if columns.is_empty() {
        inbond_default_layout_columns()
    } else {
        columns.to_vec()
    };

    if !layout_name.trim().is_empty() {
        if try_select_layout_149(session, layout_name)? {
            println!("Using existing layout '{}'", layout_name);
            return Ok((true, None));
        }
        println!(
            "Provided layout '{}' not found. Setting up default inbond columns...",
            layout_name
        );
    } else {
        println!("No layout provided. Setting up default inbond columns...");
    }

    if !open_change_layout_149(session)? {
        return Ok((false, None));
    }

    // Confirm save name when config layout was missing/invalid
    let do_save = Confirm::new()
        .with_prompt(format!(
            "Save this layout as '{}' for next time?",
            save_name
        ))
        .default(true)
        .interact()
        .unwrap_or(true);

    let mut final_name = save_name.clone();
    if do_save {
        let custom: String = Input::new()
            .with_prompt("Layout name to save")
            .with_initial_text(&save_name)
            .interact_text()
            .unwrap_or_else(|_| save_name.clone());
        if !custom.trim().is_empty() {
            final_name = custom.trim().to_string();
        }
    }

    println!(
        "Setting up layout columns {:?} (save={} as '{}')",
        cols, do_save, final_name
    );

    match setup_layout(
        session,
        1,
        LAYOUT_149_BASE,
        &final_name,
        &cols,
        200,
        !do_save, // no_save when user declines
    ) {
        Ok(true) => {
            println!(
                "Default inbond layout ready{}",
                if do_save {
                    format!(" and saved as '{}'", final_name)
                } else {
                    " (not saved)".to_string()
                }
            );
            if do_save {
                Ok((true, Some(final_name)))
            } else {
                Ok((true, None))
            }
        }
        Ok(false) => {
            println!("setup_layout returned false for 149 inbond columns");
            close_popups(session, None, None)?;
            Ok((false, None))
        }
        Err(e) => {
            println!("Error setting up 149 inbond layout: {}", e);
            let _ = close_popups(session, None, None);
            Err(e)
        }
    }
}

/// Special layout selection method for ZVT11 tcode
///
/// This method uses a grid-based approach similar to 149
/// Based on the VBA code pattern for ZVT11 layout selection
pub fn choose_layout_zvt11(
    session: &GuiSession,
    layout_name: &str,
) -> windows::core::Result<String> {
    eprintln!(
        "DEBUG: Starting ZVT11 layout selection for layout: {}",
        layout_name
    );

    // Step 1: Press the layout button (wnd[0]/tbar[1]/btn[33])
    eprintln!("DEBUG: Pressing layout button");
    if let Ok(button) = session.find_by_id("wnd[0]/tbar[1]/btn[33]".to_string()) {
        if let Some(btn) = button.downcast::<GuiButton>() {
            btn.press()?;
            eprintln!("DEBUG: Layout button pressed successfully");
        } else {
            eprintln!("DEBUG: Layout button found but downcast failed");
            return Ok("Failed to press layout button".to_string());
        }
    } else {
        eprintln!("DEBUG: Layout button not found");
        return Ok("Failed to find layout button".to_string());
    }

    // Wait for window 1 to appear
    thread::sleep(Duration::from_millis(1000));

    // Step 2: Find the grid container - ZVT11 uses a different grid ID
    let grid_id =
        "wnd[1]/usr/ssubD0500_SUBSCREEN:SAPLSLVC_DIALOG:0501/cntlG51_CONTAINER/shellcont/shell";
    eprintln!("DEBUG: Looking for ZVT11 grid at: {}", grid_id);

    if let Ok(grid_obj) = session.find_by_id(grid_id.to_string()) {
        if let Some(grid) = grid_obj.downcast::<GuiGridView>() {
            eprintln!("DEBUG: ZVT11 grid found successfully");

            // Step 3: Search for the layout in the grid
            let mut layout_found = false;
            let mut layout_row = -1;

            // Get the number of rows in the grid
            let row_count = grid.row_count()?;
            eprintln!("DEBUG: ZVT11 grid has {} rows", row_count);

            // Search through the grid rows to find the layout
            // ZVT11 might use different column names, try common ones
            let column_names = ["LAYOUT", "NAME", "TEXT", "DESCRIPTION"];

            for i in 0..row_count {
                for col_name in &column_names {
                    if let Ok(cell_text) = grid.get_cell_value(i, col_name.to_string()) {
                        eprintln!(
                            "DEBUG: Row {} column {} has text: '{}'",
                            i, col_name, cell_text
                        );
                        if cell_text.trim().to_lowercase() == layout_name.trim().to_lowercase() {
                            eprintln!(
                                "DEBUG: Layout '{}' found at row {} column {}",
                                layout_name, i, col_name
                            );
                            layout_found = true;
                            layout_row = i;
                            break;
                        }
                    }
                }
                if layout_found {
                    break;
                }
            }

            if layout_found {
                // Step 4: Set current cell to the found row
                eprintln!("DEBUG: Setting current cell to row {}", layout_row);
                // Try to set current cell with the first available column
                if grid
                    .set_current_cell(layout_row, "TEXT".to_string())
                    .is_ok()
                {
                    eprintln!("DEBUG: Current cell set successfully");
                }

                // Step 5: Select the row
                eprintln!("DEBUG: Selecting row {}", layout_row);
                grid.set_selected_rows(layout_row.to_string())?;

                // Step 6: Click on the current cell
                eprintln!("DEBUG: Clicking on current cell");
                grid.click_current_cell()?;

                eprintln!("DEBUG: ZVT11 layout selection completed successfully");
                Ok("Layout selected successfully".to_string())
            } else {
                eprintln!("DEBUG: Layout '{}' not found in ZVT11 grid", layout_name);

                // Ask user for a new layout name or to exit
                use dialoguer::{Input, Select};

                println!("Layout '{}' not found in the ZVT11 grid.", layout_name);

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
                        close_popups(session, None, None)?;
                        return Ok("Layout selection cancelled".to_string());
                    }

                    // Close any remaining popups and try again
                    close_popups(session, None, None)?;

                    // Recursively call the function with the new layout name
                    choose_layout_zvt11(session, &new_layout)
                } else {
                    // User wants to exit
                    close_popups(session, None, None)?;
                    Ok("Layout selection cancelled".to_string())
                }
            }
        } else {
            eprintln!("DEBUG: ZVT11 grid object found but downcast failed");
            Ok("Failed to access ZVT11 grid object".to_string())
        }
    } else {
        eprintln!("DEBUG: ZVT11 grid not found at: {}", grid_id);
        Ok("Failed to find ZVT11 layout grid".to_string())
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
        }
        "vt11" => {
            // Choose Layout Button
            if let Ok(menu) = session.find_by_id("wnd[0]/mbar/menu[3]/menu[0]/menu[1]".to_string())
            {
                if let Some(menu_item) = menu.downcast::<GuiMenu>() {
                    menu_item.select()?;
                }
            }
        }
        "vl06o" => {
            // Choose Layout Button for VL06O
            if let Ok(menu) = session.find_by_id("wnd[0]/mbar/menu[3]/menu[2]/menu[1]".to_string())
            {
                if let Some(menu_item) = menu.downcast::<GuiMenu>() {
                    menu_item.select()?;
                }
            }
        }
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
        }
        "y_dn3_47000149" => {
            // Layout button for 149 tcode
            if let Ok(button) = session.find_by_id("wnd[0]/tbar[1]/btn[33]".to_string()) {
                if let Some(btn) = button.downcast::<GuiButton>() {
                    btn.press()?;
                }
            }
        }
        _ => {}
    }

    Ok(true)
}
