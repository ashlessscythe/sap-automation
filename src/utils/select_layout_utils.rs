//! Layout selection helpers; much of the surface is for T-code flows and tests, not the binary alone.
#![allow(dead_code)]

use dialoguer::{Input, Select};
use sap_scripting::*;
use std::collections::HashMap;
use std::thread;
use std::time::Duration; // Added for interactive user input

use crate::utils::sap_ctrl_utils::*;
use crate::utils::sap_wnd_utils::*;
use crate::utils::utils::*;

/// Struct to hold layout parameters
#[derive(Debug, Clone, Default)]
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
pub fn select_layout(
    session: &GuiSession,
    n_wnd: i32,
    object_name: &str,
    layout_name: &str,
) -> windows::core::Result<bool> {
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
            println!("Object ({object_name}) not found.");
            // Some systems show the classic list dialog (`lbl[col,row]` labels with a Find
            // button) instead of the ALV grid; try that before giving up.
            return select_layout_from_list(session, n_wnd, &current_layout);
        }

        // Get the object
        let obj_path = format!("wnd[{n_wnd}]{object_name}");
        let obj = session.find_by_id(obj_path)?;

        // Try to downcast to GuiGridView
        if let Some(grid) = obj.downcast::<GuiGridView>() {
            // Get row count
            let row_count = grid.row_count()?;
            println!("Object has {row_count} rows");

            // Scroll down to end (in case long)
            if row_count > 0 {
                grid.set_first_visible_row(row_count - 1)?;
                let r = grid.first_visible_row()?;
                println!("Scrolldown - First visible row = {r}");
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
            if let Some(index) = layout_names
                .iter()
                .position(|name| name == &current_layout.to_uppercase())
            {
                grid.set_current_cell(index as i32, "VARIANT".to_string())?;
                grid.set_selected_rows(index.to_string())?;
                grid.double_click_current_cell()?;
                println!("Selected layout: {current_layout}");
                return Ok(true);
            } else {
                println!("Layout ({current_layout}) not found.");

                // Ask user for a new layout name or to exit
                println!("Layout '{current_layout}' not found.");

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
                        if let Ok(window) = session.find_by_id(format!("wnd[{n_wnd}]")) {
                            if let Some(wnd) = window.downcast::<GuiModalWindow>() {
                                println!("Closing window since layout selection was cancelled.");
                                wnd.close()?;
                            }
                        }

                        return Ok(false);
                    }

                    // Update current_layout and try again
                    current_layout = new_layout;
                    println!("Trying with new layout name: {current_layout}");

                    // Continue to next iteration of the loop
                    continue;
                } else {
                    // User wants to exit
                    println!("Layout selection cancelled");

                    // Close the window
                    if let Ok(window) = session.find_by_id(format!("wnd[{n_wnd}]")) {
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

/// Does the text of a list label name `layout`? Case-insensitive; the label may be just the
/// name or the name followed by a description (first whitespace-separated token).
pub fn list_label_matches(label_text: &str, layout: &str) -> bool {
    let t = label_text.trim();
    let want = layout.trim();
    if want.is_empty() {
        return false;
    }
    t.eq_ignore_ascii_case(want)
        || t.split_whitespace()
            .next()
            .is_some_and(|first| first.eq_ignore_ascii_case(want))
}

/// Find the label (column 1) of window `n_wnd` whose text names `layout`; returns its id.
fn find_list_label(session: &GuiSession, n_wnd: i32, layout: &str) -> Option<String> {
    for row in 1..=80 {
        let id = format!("wnd[{n_wnd}]/usr/lbl[1,{row}]");
        if let Ok(c) = session.find_by_id(id.clone()) {
            if let Some(l) = c.downcast::<GuiLabel>() {
                if list_label_matches(&l.text().unwrap_or_default(), layout) {
                    return Some(id);
                }
            }
        }
    }
    None
}

fn press_button(session: &GuiSession, id: &str) -> bool {
    session
        .find_by_id(id.to_string())
        .ok()
        .and_then(|c| c.downcast::<GuiButton>())
        .map(|b| b.press().is_ok())
        .unwrap_or(false)
}

fn send_key(session: &GuiSession, n_wnd: i32, key: i16) -> windows::core::Result<()> {
    if let Ok(w) = session.find_by_id(format!("wnd[{n_wnd}]")) {
        if let Some(m) = w.downcast::<GuiModalWindow>() {
            m.send_v_key(key)?;
        }
    }
    Ok(())
}

fn focus_label(session: &GuiSession, id: &str) -> bool {
    session
        .find_by_id(id.to_string())
        .ok()
        .and_then(|c| c.downcast::<GuiLabel>())
        .map(|l| l.set_focus().is_ok())
        .unwrap_or(false)
}

/// Select a layout in the classic list-style "Layout: Choose" dialog (labels `lbl[1,row]`, a
/// Find button `btn[71]`), as recorded on the owner's system for VL06O:
/// 1. the layout is visible in `wnd[n]` -> focus its label and F2;
/// 2. else Find (`wnd[n]/tbar[0]/btn[71]`),
///    type the name in `wnd[n+1]/usr/txtRSYSF-STRING`, Enter, pick the hit in `wnd[n+2]`
///    (F2), which puts the cursor on the layout in `wnd[n]`, then F2 there.
///
/// Returns `Ok(true)` when the dialog closed after the choice, `Ok(false)` (popups closed)
/// when the layout cannot be found.
pub fn select_layout_from_list(
    session: &GuiSession,
    n_wnd: i32,
    layout_name: &str,
) -> windows::core::Result<bool> {
    println!("Trying the list-style layout dialog for '{layout_name}'.");
    let settle = || thread::sleep(Duration::from_millis(300));
    let dialog_open = |n: i32| exist_ctrl(session, n, "", true).map(|c| c.cband);

    // 1. visible in the list
    if let Some(id) = find_list_label(session, n_wnd, layout_name) {
        focus_label(session, &id);
        send_key(session, n_wnd, 2)?;
        settle();
        let done = !dialog_open(n_wnd)?;
        println!("Selected layout from the list: {layout_name} (closed: {done})");
        return Ok(done);
    }

    // 2. Find
    let tbar = format!("wnd[{n_wnd}]/tbar[0]");
    if !press_button(session, &format!("{tbar}/btn[71]")) {
        println!("List dialog has no Find button; layout '{layout_name}' not found.");
        return Ok(false);
    }
    settle();
    let find_wnd = n_wnd + 1;
    let field = session
        .find_by_id(format!("wnd[{find_wnd}]/usr/txtRSYSF-STRING"))
        .ok()
        .and_then(|c| c.downcast::<GuiTextField>());
    let Some(field) = field else {
        println!("Find dialog did not open; layout '{layout_name}' not found.");
        return Ok(false);
    };
    field.set_text(layout_name.to_string())?;
    press_button(session, &format!("wnd[{find_wnd}]/tbar[0]/btn[0]"));
    settle();

    // hit list in wnd[n+2]
    let hits = n_wnd + 2;
    if !dialog_open(hits)? {
        println!("No hit list for '{layout_name}'.");
        close_popups_above(session, n_wnd)?;
        return Ok(false);
    }
    let Some(hit) = find_list_label(session, hits, layout_name) else {
        println!("Layout '{layout_name}' not in the hit list.");
        close_popups_above(session, n_wnd)?;
        return Ok(false);
    };
    focus_label(session, &hit);
    send_key(session, hits, 2)?;
    settle();

    // the cursor now sits on the layout in the list: prefer an explicit focus when visible
    if let Some(id) = find_list_label(session, n_wnd, layout_name) {
        focus_label(session, &id);
    }
    send_key(session, n_wnd, 2)?;
    settle();
    let done = !dialog_open(n_wnd)?;
    println!("Selected layout via Find: {layout_name} (closed: {done})");
    Ok(done)
}

/// F12 (cancel) the popups stacked above `n_wnd`, highest first.
fn close_popups_above(session: &GuiSession, n_wnd: i32) -> windows::core::Result<()> {
    for n in ((n_wnd + 1)..=(n_wnd + 3)).rev() {
        if exist_ctrl(session, n, "", true)?.cband {
            send_key(session, n, 12)?;
            thread::sleep(Duration::from_millis(150));
        }
    }
    Ok(())
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
            if let Ok(button) = session.find_by_id("wnd[0]/tbar[1]/btn[33]".to_string()) {
                if let Some(btn) = button.downcast::<GuiButton>() {
                    btn.press()?;
                }
            }
        }
        "zmdesnr" => {
            // Open via mbar
            println!("DEBUG: pressing layout button for zmdesnr");
            if let Ok(button) =
                session.find_by_id("wnd[0]/mbar/menu[4]/menu[0]/menu[1]".to_string())
            {
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
        }
        "mb52" => {
            // Check if button exists
            if let Ok(button) = session.find_by_id("wnd[0]/tbar[1]/btn[33]".to_string()) {
                if let Some(btn) = button.downcast::<GuiButton>() {
                    btn.press()?;
                }
            }
        }
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
        "zmdesnr" | "mb52" => {
            "/usr/ssubD0500_SUBSCREEN:SAPLSLVC_DIALOG:0501/cntlG51_CONTAINER/shellcont/shell"
                .to_string()
        }
        "vl06o" => {
            "/usr/ssubD0500_SUBSCREEN:SAPLSLVC_DIALOG:0501/cntlG51_CONTAINER/shellcont/shell"
                .to_string()
        }
        "vt11" => "/usr/ssubD0500_SUBSCREEN:SAPLSLVC_DIALOG:0501/cntlG51_CONTAINER/shellcont/shell"
            .to_string(),
        "lx03" | "lx02" | "lt23" | "vt22" => {
            "/usr/ssubD0500_SUBSCREEN:SAPLSLVC_DIALOG:0501/cntlG51_CONTAINER/shellcont/shell"
                .to_string()
        }
        // Default object name (can be adjusted as needed)
        _ => "/usr/ssubD0500_SUBSCREEN:SAPLSLVC_DIALOG:0501/cntlG51_CONTAINER/shellcont/shell"
            .to_string(),
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
pub fn select_layout_interactive(
    session: &GuiSession,
    tcode: &str,
    layout_name: &str,
) -> windows::core::Result<bool> {
    println!("Selecting layout interactively for tcode: {tcode}, initial layout: {layout_name}");

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
pub fn check_select_layout(
    session: &GuiSession,
    tcode: &str,
    layout_row: &str,
    args: Option<HashMap<String, String>>,
) -> windows::core::Result<Params> {
    let mut local_r_val = Params::default();

    println!("Checking / selecting layout for tCode ({tcode})");

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
    println!("DEBUG: layout_row is: {layout_row}");

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
            }
            "vt11" => {
                // Choose Layout Button
                if let Ok(menu) =
                    session.find_by_id("wnd[0]/mbar/menu[3]/menu[0]/menu[1]".to_string())
                {
                    if let Some(menu_item) = menu.downcast::<GuiComponent>() {
                        if let Some(btn) = menu_item.downcast::<GuiButton>() {
                            btn.press()?;
                        }
                    }
                }
            }
            "mb52" | "vl06o" => {
                // Check if button exists
                if let Ok(button) = session.find_by_id("wnd[0]/tbar[1]/btn[33]".to_string()) {
                    if let Some(btn) = button.downcast::<GuiButton>() {
                        btn.press()?;
                    }
                }
            }
            "zmdesnr" => {
                // open via mbar
                println!("DEBUG:pressing layout button for zmdesnr");
                if let Ok(button) =
                    session.find_by_id("wnd[0]/mbar/menu[4]/menu[0]/menu[1]".to_string())
                {
                    if let Some(btn) = button.downcast::<GuiButton>() {
                        btn.press()?;
                    }
                }
            }
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
            println!("Layout ({layout_row}) is empty or zero-length. Exporting as-is.");
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
                        let err_ctl = exist_ctrl(session, 1, &format!("/usr/lbl[1,{i}]"), true)?;
                        if err_ctl.cband {
                            let ctrl_msg = if let Ok(label) =
                                session.find_by_id(format!("wnd[1]/usr/lbl[1,{i}]"))
                            {
                                if let Some(lbl) = label.downcast::<GuiLabel>() {
                                    lbl.text()?
                                } else {
                                    String::new()
                                }
                            } else {
                                String::new()
                            };

                            if ctrl_msg.to_uppercase() == layout_row.to_uppercase() {
                                if let Ok(label) =
                                    session.find_by_id(format!("wnd[1]/usr/lbl[1,{i}]"))
                                {
                                    if let Some(lbl) = label.downcast::<GuiLabel>() {
                                        lbl.set_focus()?;
                                    }
                                }

                                if let Ok(window) = session.find_by_id("wnd[1]".to_string()) {
                                    if let Some(wnd) = window.downcast::<GuiModalWindow>() {
                                        wnd.send_v_key(2)?;
                                    }
                                }

                                println!("Layout number ({i}), ({layout_row}) selected.");
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

                        println!("Layout ({layout_row}) not found. Setting up layout");

                        // Setup layout based on tcode
                        match tcode.to_lowercase().as_str() {
                            "zmdesnr" | "zvt11" => {
                                if let Ok(button) =
                                    session.find_by_id("wnd[0]/tbar[1]/btn[32]".to_string())
                                {
                                    if let Some(btn) = button.downcast::<GuiButton>() {
                                        btn.press()?;
                                    }
                                }

                                // Setup layout
                                // This would call setup_layout from setup_layout_utils.rs
                                println!("Setting up layout for {tcode}");
                            }
                            "vt11" => {
                                println!("Layout ({layout_row}) not found. Setting up layout");

                                if let Ok(menu) = session
                                    .find_by_id("wnd[0]/mbar/menu[3]/menu[0]/menu[0]".to_string())
                                {
                                    if let Some(btn) = menu.downcast::<GuiButton>() {
                                        btn.press()?;
                                    }
                                }

                                // Setup layout_li
                                // This would call setup_layout_li from setup_layout_utils.rs
                                println!("Setting up layout_li for {tcode}");
                            }
                            _ => {
                                if let Ok(menu) = session
                                    .find_by_id("wnd[0]/mbar/menu[3]/menu[2]/menu[0]".to_string())
                                {
                                    if let Some(menu_item) = menu.downcast::<GuiComponent>() {
                                        if let Some(btn) = menu_item.downcast::<GuiButton>() {
                                            btn.press()?;
                                        }
                                    }
                                }

                                // Setup layout_li
                                // This would call setup_layout_li from setup_layout_utils.rs
                                println!("Setting up layout_li for {tcode}");
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
            println!("Status bar message: ({bar_msg})");
        }

        // Make sure all windows are closed
        close_popups(session, None, None)?;

        // Export based on tcode
        let export_wnd_name = match tcode.to_lowercase().as_str() {
            "lx03" | "lx02" => {
                if let Ok(menu) =
                    session.find_by_id("wnd[0]/mbar/menu[0]/menu[1]/menu[1]".to_string())
                {
                    if let Some(menu_item) = menu.downcast::<GuiComponent>() {
                        if let Some(btn) = menu_item.downcast::<GuiButton>() {
                            btn.press()?;
                        }
                    }
                }
                "BIN STATUS REPORT: OVERVIEW"
            }
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

                if let Ok(menu) =
                    session.find_by_id("wnd[0]/mbar/menu[0]/menu[10]/menu[0]".to_string())
                {
                    if let Some(menu_item) = menu.downcast::<GuiComponent>() {
                        if let Some(btn) = menu_item.downcast::<GuiButton>() {
                            btn.press()?;
                        }
                    }
                }
                "SHIPMENT LIST: PLANNING"
            }
            "zmdesnr" | "zvt11" => {
                if let Ok(menu) =
                    session.find_by_id("wnd[0]/mbar/menu[0]/menu[3]/menu[1]".to_string())
                {
                    if let Some(menu_item) = menu.downcast::<GuiComponent>() {
                        if let Some(btn) = menu_item.downcast::<GuiButton>() {
                            btn.press()?;
                        }
                    }
                }
                "ZMDEMAIN SERIAL NUMBER HISTORY CONTENTS"
            }
            "vl06o" => {
                if let Ok(menu) =
                    session.find_by_id("wnd[0]/mbar/menu[0]/menu[5]/menu[1]".to_string())
                {
                    if let Some(menu_item) = menu.downcast::<GuiComponent>() {
                        if let Some(btn) = menu_item.downcast::<GuiButton>() {
                            btn.press()?;
                        }
                    }
                }
                "LIST OF OUTBOUND DELIVERIES"
            }
            _ => "",
        };

        // Check export window
        if !export_wnd_name.is_empty() {
            local_r_val.run_check = check_export_window(session, tcode, export_wnd_name)?;
            if !local_r_val.run_check {
                local_r_val.err = format!("Failed to check export window for {tcode}");
                return Ok(local_r_val);
            }
        }

        local_r_val.type_name = "export_window".to_string();
    }

    Ok(local_r_val)
}

/// Helper function for goto_setup
fn goto_setup(_session: &GuiSession, tcode: &str, layout_row: &str) -> windows::core::Result<()> {
    // Implementation would go here
    println!("Going to setup for tcode: {tcode}, layout: {layout_row}");
    Ok(())
}

/// Helper function for goto_choose
fn goto_choose(_session: &GuiSession, tcode: &str, layout_row: &str) -> windows::core::Result<()> {
    // Implementation would go here
    println!("Going to choose for tcode: {tcode}, layout: {layout_row}");
    Ok(())
}

/// Check export window
///
/// This function checks if the export window is present and has the expected title
fn check_export_window(
    session: &GuiSession,
    _tcode: &str,
    expected_title: &str,
) -> windows::core::Result<bool> {
    // Check if window exists
    let err_wnd = exist_ctrl(session, 0, "", true)?;
    if !err_wnd.cband {
        println!("Export window not found.");
        return Ok(false);
    }

    // Check if window title matches expected title
    if !contains(&err_wnd.ctext, expected_title, Some(false)) {
        println!(
            "Export window title ({}) does not match expected title ({}).",
            err_wnd.ctext, expected_title
        );
        return Ok(false);
    }

    println!("Export window found with expected title: {expected_title}");
    Ok(true)
}

#[cfg(test)]
mod list_label_tests {
    use super::list_label_matches;

    #[test]
    fn matches_name_or_first_token_case_insensitively() {
        assert!(list_label_matches("xdock", "XDOCK"));
        assert!(list_label_matches("  XDOCK  ", "xdock"));
        assert!(list_label_matches("XDOCK  Cross dock view", "xdock"));
        assert!(!list_label_matches("XDOCK2", "xdock"));
        assert!(!list_label_matches("MY XDOCK", "xdock"));
        assert!(!list_label_matches("xdock", ""));
        assert!(!list_label_matches("", "xdock"));
    }
}
