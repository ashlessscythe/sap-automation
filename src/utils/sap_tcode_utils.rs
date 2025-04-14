use sap_scripting::*;
use std::time::Duration;
use std::thread;
use windows::core::Result;
use crate::utils::close_popups;
use crate::utils::sap_constants::STR_FORM;
use crate::utils::sap_ctrl_utils::hit_ctrl;

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
    let run_val = run.unwrap_or(false);
    let b_kill_popups  = _kill_popups.unwrap_or(false);
    
    println!("Checking if tCode ({}) is active", tcode);
    
    match b_kill_popups {
        true => {
            close_popups(session, None, None)?;
        },
        _ => {}
    }
    
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

pub fn variant_select(session: &GuiSession, tcode: &str, variant_name: &str) -> Result<bool> {
    println!("Selecting variant '{}' for tCode '{}'", variant_name, tcode);
    
    // Choose variant
    if let Ok(btn) = session.find_by_id("wnd[0]/tbar[1]/btn[17]".to_string()) {
        if let Some(button) = btn.downcast::<GuiButton>() {
            button.press()?;
        } else {
            println!("Failed to downcast variant button to GuiButton");
            return Ok(false);
        }
    } else {
        println!("Variant button not found for tCode '{}'", tcode);
        return Ok(false);
    }
    
    // Check if variant selection window opened
    let err_msg = hit_ctrl(session, 0, "/sbar", "Text", "Get", "")?;
    if err_msg.contains("error") || err_msg.contains("not found") {
        println!("Error opening variant selection: {}", err_msg);
        return Ok(false);
    }
    
    // Enter variant name
    if let Ok(txt) = session.find_by_id("wnd[1]/usr/txtV-LOW".to_string()) {
        if let Some(text_field) = txt.downcast::<GuiTextField>() {
            text_field.set_text(variant_name.to_string())?;
        } else {
            println!("Failed to downcast variant name field to GuiTextField");
            return Ok(false);
        }
    } else {
        println!("Variant name field not found");
        return Ok(false);
    }
    
    // Blank username
    if let Ok(txt) = session.find_by_id("wnd[1]/usr/txtENAME-LOW".to_string()) {
        if let Some(text_field) = txt.downcast::<GuiTextField>() {
            text_field.set_text("".to_string())?;
        } else {
            // If we can't find or use the username field, just continue
            println!("Warning: Could not clear username field, continuing anyway");
        }
    }
    
    // Close variant select window
    if let Ok(btn) = session.find_by_id("wnd[1]/tbar[0]/btn[8]".to_string()) {
        if let Some(button) = btn.downcast::<GuiButton>() {
            button.press()?;
        } else {
            println!("Failed to downcast confirm button to GuiButton");
            return Ok(false);
        }
    } else {
        println!("Confirm button not found");
        return Ok(false);
    }
    
    // Check for errors in status bar after variant selection
    let err_msg = hit_ctrl(session, 0, "/sbar", "Text", "Get", "")?;
    if !err_msg.is_empty() && (err_msg.contains("error") || err_msg.contains("not found")) {
        println!("Error selecting variant: {}", err_msg);
        return Ok(false);
    }
    
    println!("Variant '{}' selected successfully for tCode '{}'", variant_name, tcode);
    Ok(true)
}
