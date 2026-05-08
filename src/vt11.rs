use chrono::NaiveDate;
use sap_scripting::*;
use windows::core::Result;

use crate::utils::{choose_layout, cli_overrides, sap_file_utils::*};
// Import specific functions to avoid ambiguity
use crate::utils::cli_overrides::cli_overrides;
use crate::utils::config_ops::get_reports_dir;
use crate::utils::excel_file_ops::read_excel_column;
use crate::utils::excel_path_utils::get_newest_file;
use crate::utils::sap_ctrl_utils::exist_ctrl;
use crate::utils::sap_export_utils::export_local_file;
use crate::utils::sap_tcode_utils::*;
use crate::utils::sap_wnd_utils::*;
use crate::utils::source_overrides::*;
use chrono::Local;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs::{create_dir_all, File};
use std::io::Write;

/// Struct to hold VT11 export parameters
#[derive(Debug)]
pub struct VT11Params {
    pub start_date: NaiveDate,
    pub end_date: NaiveDate,
    pub sap_variant_name: Option<String>,
    pub layout_row: Option<String>,
    pub by_date: bool,
    pub by_delivery: bool,
    pub limiter: Option<String>,
    pub t_code: String,
}

impl Default for VT11Params {
    fn default() -> Self {
        Self {
            start_date: chrono::Local::now().date_naive(),
            end_date: chrono::Local::now().date_naive(),
            sap_variant_name: None,
            layout_row: None,
            by_date: false,
            by_delivery: false,
            limiter: None,
            t_code: "VT11".to_string(),
        }
    }
}

/// Get delivery numbers from the latest ZMDESNR export file (same as VL06O delivery module)
fn get_delivery_numbers_from_zmdesnr() -> Result<Vec<String>> {
    let reports_dir = get_reports_dir();
    let zmdesnr_dir = format!("{}\\zmdesnr", reports_dir);

    // Load configuration to get ZMDESNR effective export type
    let config = match crate::utils::config_types::SapConfig::load() {
        Ok(cfg) => cfg,
        Err(e) => {
            println!("Error loading configuration: {}", e);
            return Ok(Vec::new());
        }
    };

    // Determine expected extension from ZMDESNR effective export type
    let ext = match config.get_effective_export_type("ZMDESNR") {
        Some(0) | Some(1) => "txt", // unconverted or text with tabs
        Some(2) => "rtf",           // rich text
        Some(3) => "html",          // HTML
        Some(4) => {
            println!("ZMDESNR export is set to clipboard; no file available. Looking for Excel fallback...");
            "xlsx"
        }
        _ => "txt", // Default to text
    };

    println!("Looking for ZMDESNR files with extension: .{}", ext);

    // Get the newest file in the ZMDESNR directory with the chosen extension
    let newest_path = get_newest_file(&zmdesnr_dir, ext)?;

    if newest_path.is_empty() {
        println!("No ZMDESNR export files found in: {}", zmdesnr_dir);
        return Ok(Vec::new());
    }

    println!("Reading delivery numbers from: {}", newest_path);

    // Read delivery numbers trying multiple header variants
    let header_candidates = ["Delivery", "delivery", "delivery number", "delivery_number"];
    let delivery_numbers = if ext.eq_ignore_ascii_case("xlsx") {
        let mut nums: Vec<String> = Vec::new();
        for h in header_candidates.iter() {
            let v = read_excel_column(&newest_path, "Sheet1", h).unwrap_or_default();
            if !v.is_empty() {
                nums = v;
                break;
            }
        }
        nums
    } else if ext.eq_ignore_ascii_case("txt") {
        let mut nums: Vec<String> = Vec::new();
        for h in header_candidates.iter() {
            match crate::vl06o_delivery_module::read_tab_delimited_column(&newest_path, h) {
                Ok(v) => {
                    if !v.is_empty() {
                        nums = v;
                        break;
                    }
                }
                Err(e) => {
                    println!("Error reading text file: {}", e);
                    return Ok(Vec::new());
                }
            }
        }
        nums
    } else {
        // For other file types, try Excel reader as fallback
        let mut nums: Vec<String> = Vec::new();
        for h in header_candidates.iter() {
            if let Ok(v) = read_excel_column(&newest_path, "Sheet1", h) {
                if !v.is_empty() {
                    nums = v;
                    break;
                }
            }
        }
        if nums.is_empty() {
            println!("Failed to read file with extension .{}", ext);
        }
        nums
    };

    if delivery_numbers.is_empty() {
        println!("No delivery numbers found in the 'Delivery' column");
    } else {
        println!("Found {} delivery numbers", delivery_numbers.len());
    }

    Ok(delivery_numbers)
}

/// Get delivery numbers from the newest, unused VT11 ListCheck CSV (do not mark here)
fn get_delivery_numbers_from_listcheck() -> Result<Vec<String>> {
    let mut results: Vec<String> = Vec::new();

    let reports_dir = get_reports_dir();
    let subdir = format!("{}\\vt11_listcheck", reports_dir);

    // Find newest CSV that is NOT already marked as used (filename not ending with "_.csv")
    let mut newest_path = String::new();
    if let Ok(entries) = std::fs::read_dir(&subdir) {
        let mut newest_time: Option<std::time::SystemTime> = None;
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
                if ext.eq_ignore_ascii_case("csv") {
                    if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
                        // Skip files already marked as used ("_.csv" suffix)
                        if name.ends_with("_.csv") {
                            continue;
                        }
                        if let Ok(meta) = entry.metadata() {
                            if let Ok(modified) = meta.modified() {
                                if newest_time.map(|t| modified > t).unwrap_or(true) {
                                    newest_time = Some(modified);
                                    newest_path = path.to_string_lossy().to_string();
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    if newest_path.is_empty() {
        println!("No unused VT11 ListCheck CSV found in {}", subdir);
        return Ok(results);
    }

    // Read CSV and collect the Delivery column (second column)
    if let Ok(contents) = std::fs::read_to_string(&newest_path) {
        for (idx, line) in contents.lines().enumerate() {
            if idx == 0 {
                continue; // skip header
            }
            let cols: Vec<&str> = line.split(',').collect();
            if let Some(deliv) = cols.get(1) {
                let d = deliv.trim().trim_matches('"').to_string();
                if !d.is_empty() {
                    results.push(d);
                }
            }
        }
    }

    Ok(results)
}

/// Try to open the local file export dialog for VT11
fn try_open_local_file_export(session: &GuiSession) -> bool {
    // Prioritize VB-observed path for 'Save list in file...'
    let candidates = [
        "wnd[0]/mbar/menu[0]/menu[9]/menu[2]", // List -> Export -> Local file (VB example)
        "wnd[0]/mbar/menu[0]/menu[9]/menu[0]", // List -> Export -> Local file (common)
        "wnd[0]/mbar/menu[0]/menu[3]/menu[2]", // Alt path (used in 149)
        "wnd[0]/mbar/menu[0]/menu[3]/menu[0]", // Alt path
    ];

    for path in candidates.iter() {
        if let Ok(menu) = session.find_by_id((*path).to_string()) {
            if let Some(menu_item) = menu.downcast::<GuiMenu>() {
                if menu_item.select().is_ok() {
                    // Consider success if a modal window appeared
                    if let Ok(err_wnd) = exist_ctrl(session, 1, "", true) {
                        if err_wnd.cband {
                            return true;
                        }
                    }
                }
            }
        }
    }

    false
}

/// Run VT11 export with the given parameters
///
/// This function is a port of the VBA function VT11_Run_Export
pub fn run_export(session: &GuiSession, params: &VT11Params) -> Result<bool> {
    println!("Running VT11 export...");

    // Check if tCode is active
    if !assert_tcode(session, "VT11", Some(0))? {
        println!("Failed to activate VT11 transaction");
        return Ok(false);
    }

    // get override format if exists
    let date_fmt = cli_overrides().date_format.as_deref().unwrap_or("%m/%d/%Y");

    // Format dates for SAP
    let start_date_str = params.start_date.format(date_fmt).to_string();
    let end_date_str = params.end_date.format(date_fmt).to_string();

    // Apply variant if provided
    if let Some(variant_name) = &params.sap_variant_name {
        if !variant_name.is_empty() && !variant_select(session, &params.t_code, variant_name)? {
            println!(
                "Failed to select variant '{}' for tCode '{}'",
                variant_name, params.t_code
            );
            // Continue with export even if variant selection failed
        }
    }

    // Set date fields based on by_date parameter
    if params.by_date {
        // Set start date
        println!("Setting start date to {:?}", start_date_str.clone());
        if let Ok(txt) = session.find_by_id("wnd[0]/usr/ctxtK_DATEN-LOW".to_string()) {
            if let Some(text_field) = txt.downcast::<GuiCTextField>() {
                println!("start date set to {:?}", start_date_str.clone());
                text_field.set_text(start_date_str.clone())?;
            } else {
                println!("Error with txt.downcast");
            }
        } else {
            println!("Error with find_by_id");
        }

        // Set end date (leave blank if same as start date)
        if let Ok(txt) = session.find_by_id("wnd[0]/usr/ctxtK_DATEN-HIGH".to_string()) {
            if let Some(text_field) = txt.downcast::<GuiCTextField>() {
                if params.start_date == params.end_date {
                    text_field.set_text("".to_string())?;
                } else {
                    text_field.set_text(end_date_str.clone())?;
                }
            }
        }
    }

    // Handle delivery limitation if by_delivery is true
    if params.by_delivery {
        println!("Filtering by delivery numbers...");

        // CLI override (--delivery-file / --delivery-col) wins over legacy merge.
        let mut delivery_numbers = match cli_delivery_numbers_override() {
            Ok(Some(nums)) => nums,
            Ok(None) => {
                let mut delivery_numbers = get_delivery_numbers_from_zmdesnr()?;
                let listcheck_numbers = get_delivery_numbers_from_listcheck()?;
                if !listcheck_numbers.is_empty() {
                    println!(
                        "Appending {} deliveries from VT11 ListCheck CSV",
                        listcheck_numbers.len()
                    );
                    delivery_numbers.extend(listcheck_numbers);
                }
                delivery_numbers
            }
            Err(e) => {
                println!(
                    "CLI delivery-source error: {}; falling back to legacy merge",
                    e
                );
                let mut delivery_numbers = get_delivery_numbers_from_zmdesnr()?;
                let listcheck_numbers = get_delivery_numbers_from_listcheck()?;
                if !listcheck_numbers.is_empty() {
                    delivery_numbers.extend(listcheck_numbers);
                }
                delivery_numbers
            }
        };

        delivery_numbers = delivery_numbers
            .into_iter()
            .filter(|s| !s.trim().is_empty())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        delivery_numbers.sort();
        println!("Sanitized delivery numbers: {}", delivery_numbers.len());

        if !delivery_numbers.is_empty() {
            // Press the multi delivery button
            if let Ok(btn) =
                session.find_by_id("wnd[0]/usr/btn%_S_VBELN_%_APP_%-VALU_PUSH".to_string())
            {
                if let Some(button) = btn.downcast::<GuiButton>() {
                    button.press()?;
                    println!("Pressed multi delivery button");
                }
            }

            // Wait for the popup window to appear
            std::thread::sleep(std::time::Duration::from_millis(1000));

            // Check if popup window exists
            let popup_exists = exist_ctrl(session, 1, "", true)?;
            if popup_exists.cband {
                println!(
                    "Pasting {} delivery numbers using scrollable paste...",
                    delivery_numbers.len()
                );

                // Use the same table ID pattern as VL06O
                let table_id =
                    "tabsTAB_STRIP/tabpSIVA/ssubSCREEN_HEADER:SAPLALDB:3010/tblSAPLALDBSINGLE";
                let batch_size = 7; // Number of visible rows in the table

                // Use paste_values_with_scroll for efficient pasting
                let paste_result = crate::utils::sap_ctrl_utils::paste_values_with_scroll(
                    session,
                    1, // Window index for popup
                    table_id,
                    &delivery_numbers,
                    batch_size,
                )?;

                if !paste_result {
                    println!("Failed to paste delivery numbers");
                    return Ok(false);
                }

                println!(
                    "Successfully pasted {} delivery numbers",
                    delivery_numbers.len()
                );

                // Close the popup by pressing Enter to confirm
                if let Ok(window) = session.find_by_id("wnd[1]".to_string()) {
                    if let Some(modal_window) = window.downcast::<GuiModalWindow>() {
                        modal_window.send_v_key(8)?; // (F8) to close modal
                        println!("Confirmed delivery selection and closed popup");
                    }
                }
            }
        } else {
            println!("No delivery numbers found, but delivery limitation is required.");
            println!("VT11 export cannot proceed without delivery numbers.");
            return Ok(false);
        }
    }

    // Handle other limiters if provided
    if let Some(limiter) = &params.limiter {
        if !limiter.is_empty() {
            match limiter.to_lowercase().as_str() {
                "date_range" => {
                    // Blank 2nd description to prevent issues
                    if let Ok(txt) = session.find_by_id("wnd[0]/usr/txtK_TPBEZ-HIGH".to_string()) {
                        if let Some(text_field) = txt.downcast::<GuiTextField>() {
                            text_field.set_text("".to_string())?;
                        }
                    }
                }
                _ => {
                    println!("Unknown limiter type: {}", limiter);
                }
            }
        }
    }

    // Execute the transaction
    if let Ok(wnd) = session.find_by_id("wnd[0]".to_string()) {
        if let Some(window) = wnd.downcast::<GuiMainWindow>() {
            window.send_v_key(8)?;
            println!("Sent Execute (F8) key");
        }
    }

    // Check for error (No Shipments Found)
    let err_ctl = exist_ctrl(session, 1, "/usr/txtMESSTXT1", false)?;
    if err_ctl.cband {
        if let Ok(txt) = session.find_by_id("wnd[1]/usr/txtMESSTXT1".to_string()) {
            if let Some(text_field) = txt.downcast::<GuiTextField>() {
                let error_text = text_field.text()?;
                if error_text.contains("No shipments were found for the selection criteria") {
                    println!(
                        "No shipments found from dates ({} to {})",
                        start_date_str, end_date_str
                    );

                    // Close window
                    if let Ok(window) = session.find_by_id("wnd[1]".to_string()) {
                        if let Some(modal_window) = window.downcast::<GuiFrameWindow>() {
                            modal_window.close()?;
                        }
                    }

                    return Ok(false);
                }
            }
        }
    }

    // Check if layout exists and select it
    if let Some(layout_row) = &params.layout_row {
        if !layout_row.is_empty() {
            // Choose Layout - only open layout selection if a layout is provided
            if let Ok(menu) = session.find_by_id("wnd[0]/mbar/menu[3]/menu[0]/menu[1]".to_string())
            {
                if let Some(menu_item) = menu.downcast::<GuiMenu>() {
                    menu_item.select()?;
                }
            }

            // Check if window exists
            let err_ctl = exist_ctrl(session, 1, "", true)?;

            if err_ctl.cband {
                // String layout name
                let msg = choose_layout(session, &params.t_code, layout_row);
                match msg {
                    Ok(message) if message.is_empty() => {} // no-op
                    Ok(message) => {
                        eprintln!("Message after choosing layout {}: {}", layout_row, message);
                    }
                    Err(e) => {
                        eprintln!("Error after choosing layout {}: {:?}", layout_row, e);
                    }
                }

                // If we get here and the layout window is still open, the layout wasn't found
                let err_ctl = exist_ctrl(session, 1, "", true)?;
                if err_ctl.cband {
                    if let Ok(window) = session.find_by_id("wnd[1]".to_string()) {
                        if let Some(modal_window) = window.downcast::<GuiFrameWindow>() {
                            modal_window.close()?;
                        }
                    }

                    println!("Layout ({}) not found. Setting up layout...", layout_row);
                    // Setup layout functionality would be implemented here
                }
            }
        } else {
            // If layout is empty or zero-length, close popup window and export as-is
            let err_ctl = exist_ctrl(session, 1, "", true)?;
            if err_ctl.cband {
                if let Ok(window) = session.find_by_id("wnd[1]".to_string()) {
                    if let Some(modal_window) = window.downcast::<GuiFrameWindow>() {
                        modal_window.close()?;
                    }
                }
            }

            println!("Layout is empty or zero-length. Exporting as-is.");
        }
    }

    // Export preference: try local file export if configured; otherwise Excel
    if let Ok(config) = crate::utils::config_types::SapConfig::load() {
        if let Some(exp_type) = config.get_effective_export_type("VT11") {
            if try_open_local_file_export(session) {
                if export_local_file(session, "VT11", exp_type, None).is_ok() {
                    return Ok(true);
                }
            }
            println!("Local file export path not available; falling back to Excel export...");
        }
    }

    // Export as Excel (fallback)
    if let Ok(menu) = session.find_by_id("wnd[0]/mbar/menu[0]/menu[10]/menu[0]".to_string()) {
        if let Some(menu_item) = menu.downcast::<GuiMenu>() {
            menu_item.select()?;
        }
    }

    // debug
    eprintln!("DEBUG: Exporting to Excel");
    // Check export window
    let run_check = check_export_window(session, "VT11", "SHIPMENT LIST: PLANNING")?;
    match run_check {
        true => {
            println!("Export window opened successfully.");
        }
        false => {
            eprintln!("Error checking export window.");
        }
    }

    // Get file path using the utility function
    let (file_path, file_name) = get_tcode_file_path("VT11", "xlsx");

    // save sap file with prevent_excel_open set to true
    let run_check = save_sap_file(session, &file_path, &file_name, Some(true))?;

    Ok(run_check)
}

/// Run VT11 and scan the shipment list to find deliveries/shipments that cannot be entered
/// Returns a vector of delivery numbers to act on
pub fn run_listcheck(session: &GuiSession, params: &VT11Params) -> Result<Vec<String>> {
    println!("Running VT11 listcheck...");

    // Ensure tcode is active
    if !assert_tcode(session, "VT11", Some(0))? {
        println!("Failed to activate VT11 transaction");
        return Ok(Vec::new());
    }

    // Determine effective variant/layout: prefer params, fallback to config
    let mut eff_variant = params.sap_variant_name.clone();
    let mut eff_layout = params.layout_row.clone();
    if eff_variant.is_none() || eff_layout.is_none() {
        if let Ok(config) = crate::utils::config_types::SapConfig::load() {
            // Prefer dedicated listcheck section first
            if let Some(cfg) = config.get_tcode_config("VT11.listcheck", Some(false)) {
                if eff_variant.is_none() {
                    eff_variant = cfg.get("variant").cloned();
                }
                if eff_layout.is_none() {
                    eff_layout = cfg.get("layout").cloned();
                }
            }
            // Fallback to standard VT11 section
            if eff_variant.is_none() || eff_layout.is_none() {
                if let Some(cfg) = config.get_tcode_config("VT11", Some(false)) {
                    if eff_variant.is_none() {
                        eff_variant = cfg.get("variant").cloned();
                    }
                    if eff_layout.is_none() {
                        eff_layout = cfg.get("layout").cloned();
                    }
                }
            }
        }
    }

    // Variant (if configured)
    if let Some(variant_name) = &eff_variant {
        if !variant_name.is_empty() && !variant_select(session, &params.t_code, variant_name)? {
            println!(
                "Failed to select variant '{}' for tCode '{}'",
                variant_name, params.t_code
            );
        }
    }

    // Date range (if by_date)
    println!("DEBUG: params.by_date: {}", params.by_date);
    if params.by_date {
        let today = chrono::Local::now().date_naive();
        let mut start = params.start_date;
        let mut end = params.end_date;
        // If no explicit high provided (same day), default to yesterday..tomorrow
        if start == end {
            start = today - chrono::Duration::days(1);
            end = today + chrono::Duration::days(1);
        }
        let start_date_str = format!("{}*", start.format("%m/%d/%Y").to_string());
        let end_date_str = format!("{}*", end.format("%m/%d/%Y").to_string());
        if let Ok(txt) = session.find_by_id("wnd[0]/usr/txtK_TPBEZ-LOW".to_string()) {
            if let Some(text_field) = txt.downcast::<GuiTextField>() {
                text_field.set_text(start_date_str)?;
            }
        }
        if let Ok(txt) = session.find_by_id("wnd[0]/usr/txtK_TPBEZ-HIGH".to_string()) {
            if let Some(text_field) = txt.downcast::<GuiTextField>() {
                text_field.set_text(end_date_str)?;
            }
        }
    }

    // Execute (F8)
    if let Ok(wnd) = session.find_by_id("wnd[0]".to_string()) {
        if let Some(window) = wnd.downcast::<GuiMainWindow>() {
            window.send_v_key(8)?;
        }
    }

    // Optional layout selection
    if let Some(layout_row) = &eff_layout {
        if !layout_row.is_empty() {
            if let Ok(menu) = session.find_by_id("wnd[0]/mbar/menu[3]/menu[0]/menu[1]".to_string())
            {
                if let Some(menu_item) = menu.downcast::<GuiMenu>() {
                    let _ = menu_item.select();
                }
            }
            let _ = exist_ctrl(session, 1, "", true)?;
            let _ = choose_layout(session, &params.t_code, layout_row);
            if let Ok(window) = session.find_by_id("wnd[1]".to_string()) {
                if let Some(modal_window) = window.downcast::<GuiFrameWindow>() {
                    let _ = modal_window.close();
                }
            }
        }
    }

    // Iterate list to collect blocked and unblocked deliveries from status bar
    // Structure: (shipment, delivery, user, timestamp, is_blocked)
    let mut shipments_data: Vec<(String, String, String, String, bool)> = Vec::new();
    let mut deliveries: Vec<String> = Vec::new();
    let mut rows: Vec<(String, String, String)> = Vec::new(); // (shipment, delivery, user) - blocked only
    let mut total_attempted: u32 = 0;
    let mut blocked_count: u32 = 0;
    let re_specific = Regex::new(r"(?s)This delivery \((\d+)\) is currently being processed[^\(]*\(([A-Za-z][A-Za-z0-9_]+)\)").unwrap();
    let re_any = Regex::new(r"\b\d{7,}\b").unwrap();
    let re_user = Regex::new(r"\(([A-Za-z][A-Za-z0-9_]+)\)").unwrap();

    // We will attempt a fixed range of row label positions as per VBA notes: lbl[8,n]
    // Try multiple pages; between pages send VKey 82 (Page Down) to scroll
    let mut seen_first_rows: HashSet<String> = HashSet::new();
    loop {
        // Detect short lists by checking the first visible row label text.
        let mut first_row_key = String::new();
        if let Ok(lbl0) = session.find_by_id("wnd[0]/usr/lbl[8,4]".to_string()) {
            if let Some(label0) = lbl0.downcast::<GuiLabel>() {
                first_row_key = label0.text().unwrap_or_default();
            }
        }
        if !first_row_key.is_empty() {
            if seen_first_rows.contains(&first_row_key) {
                // We are seeing the same first row again; break to avoid looping
                break;
            }
            seen_first_rows.insert(first_row_key);
        } else {
            // No first row visible; end
            break;
        }

        // Walk down visible rows until a row is not found
        let mut row = 4;
        loop {
            let lbl_path = format!("wnd[0]/usr/lbl[8,{}]", row);
            if let Ok(lbl) = session.find_by_id(lbl_path.clone()) {
                if let Some(label) = lbl.downcast::<GuiLabel>() {
                    let _ = label.set_focus();
                    let shipment_text = label.text().unwrap_or_default();
                    let shipment_number = re_any
                        .captures(&shipment_text)
                        .and_then(|c| c.get(0))
                        .map(|m| m.as_str().to_string())
                        .unwrap_or_default();
                    // Enter - count this as an attempt to go deeper
                    total_attempted += 1;
                    if let Ok(wnd0) = session.find_by_id("wnd[0]".to_string()) {
                        if let Some(win0) = wnd0.downcast::<GuiMainWindow>() {
                            win0.send_v_key(2)?; // F2 (not ENTER)
                        }
                    }

                    std::thread::sleep(std::time::Duration::from_millis(250));

                    // Handle popups that might appear after pressing F2
                    // There might be up to 3 popups, check and send vkey 0 if they exist
                    for _ in 0..3 {
                        if let Ok(popup_check) = exist_ctrl(session, 1, "", true) {
                            if popup_check.cband {
                                // Try to send vkey 0 to the popup window
                                if let Ok(popup_window) = session.find_by_id("wnd[1]".to_string()) {
                                    if let Some(modal_window) =
                                        popup_window.downcast::<GuiModalWindow>()
                                    {
                                        let _ = modal_window.send_v_key(0);
                                    }
                                }
                                // // Also try sending vkey 0 via main window as fallback
                                // if let Ok(wnd0) = session.find_by_id("wnd[0]".to_string()) {
                                //     if let Some(win0) = wnd0.downcast::<GuiMainWindow>() {
                                //         let _ = win0.send_v_key(0);
                                //     }
                                // }
                                // Small delay to allow popup to process
                                std::thread::sleep(std::time::Duration::from_millis(100));
                            } else {
                                // No popup found, break early
                                break;
                            }
                        } else {
                            // Error checking for popup, break
                            break;
                        }
                    }

                    let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

                    // Extract delivery and user from status bar (available for both blocked and unblocked)
                    // The status bar shows delivery and userid when pressing ENTER on shipment
                    let mut delivery = String::new();
                    let mut user = String::new();
                    let mut is_blocked = false;

                    if let Ok(sbar) = session.find_by_id("wnd[0]/sbar".to_string()) {
                        if let Some(status) = sbar.downcast::<GuiStatusbar>() {
                            let msg = status.text().unwrap_or_default();

                            // Try to extract delivery and user from status bar message
                            if let Some(caps) = re_specific.captures(&msg) {
                                // Specific pattern: "This delivery (1234567) is currently being processed... (USERID)"
                                delivery = caps
                                    .get(1)
                                    .map(|m| m.as_str().to_string())
                                    .unwrap_or_default();
                                user = caps
                                    .get(2)
                                    .map(|m| m.as_str().to_string())
                                    .unwrap_or_default();
                                is_blocked = true;
                            } else if msg.to_lowercase().contains("process")
                                || msg.to_lowercase().contains("lock")
                            {
                                // Generic blocked message - try to extract delivery and user
                                if let Some(cap) = re_any.captures(&msg) {
                                    if let Some(m) = cap.get(0) {
                                        delivery = m.as_str().to_string();
                                    }
                                }
                                if let Some(ucap) = re_user.captures(&msg) {
                                    if let Some(m) = ucap.get(1) {
                                        let candidate = m.as_str().to_string();
                                        if candidate != delivery {
                                            user = candidate;
                                        }
                                    }
                                }
                                is_blocked = true;
                            } else {
                                // No error message - shipment is unblocked
                                // Try to extract delivery/user if they're shown in status bar
                                // (delivery and userid are shown when pressing ENTER, even for unblocked)
                                if let Some(cap) = re_any.captures(&msg) {
                                    if let Some(m) = cap.get(0) {
                                        let candidate = m.as_str().to_string();
                                        // Check if it looks like a delivery number (7+ digits)
                                        if candidate.len() >= 7 {
                                            delivery = candidate;
                                        }
                                    }
                                }
                                if let Some(ucap) = re_user.captures(&msg) {
                                    if let Some(m) = ucap.get(1) {
                                        let candidate = m.as_str().to_string();
                                        if candidate != delivery {
                                            user = candidate;
                                        }
                                    }
                                }
                                is_blocked = false;
                            }
                        }
                    }

                    // Check for navigation vs status bar message
                    let mut went_deeper = false;
                    if let Ok(info) = session.info() {
                        if let Ok(tx) = info.transaction() {
                            went_deeper = !tx.contains("VT11");
                        }
                    }

                    if went_deeper {
                        // Successfully entered - this shipment is unblocked
                        // We already extracted delivery/user from status bar above
                        if !shipment_number.is_empty() {
                            shipments_data.push((
                                shipment_number.clone(),
                                delivery.clone(),
                                user.clone(),
                                timestamp.clone(),
                                false,
                            ));
                        }
                        if let Ok(wnd0) = session.find_by_id("wnd[0]".to_string()) {
                            if let Some(win0) = wnd0.downcast::<GuiMainWindow>() {
                                win0.send_v_key(3)?; // Back
                            }
                        }
                    } else {
                        // Check if blocked
                        if is_blocked {
                            blocked_count += 1;
                            if !delivery.is_empty() {
                                deliveries.push(delivery.clone());
                            }
                            if !shipment_number.is_empty() {
                                shipments_data.push((
                                    shipment_number.clone(),
                                    delivery.clone(),
                                    user.clone(),
                                    timestamp.clone(),
                                    true,
                                ));
                                if !delivery.is_empty() {
                                    rows.push((shipment_number.clone(), delivery, user));
                                }
                            }
                        } else {
                            // Not blocked - shipment is unblocked
                            if !shipment_number.is_empty() {
                                shipments_data.push((
                                    shipment_number.clone(),
                                    delivery.clone(),
                                    user.clone(),
                                    timestamp.clone(),
                                    false,
                                ));
                            }
                        }
                        if let Ok(w1) = session.find_by_id("wnd[1]".to_string()) {
                            if let Some(modal) = w1.downcast::<GuiModalWindow>() {
                                let _ = modal.send_v_key(3);
                            }
                        }
                    }

                    // Next row
                    row += 1;
                }
            } else {
                // Row not found in current viewport; proceed to next page
                break;
            }
        }
        // Page down to scroll list
        if let Ok(wnd0) = session.find_by_id("wnd[0]".to_string()) {
            if let Some(win0) = wnd0.downcast::<GuiMainWindow>() {
                let _ = win0.send_v_key(82); // Page Down
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(150));
    }

    // Dedup
    deliveries = deliveries
        .into_iter()
        .filter(|s| !s.trim().is_empty())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();

    // Write CSV if we captured any (shipment, delivery, user) rows
    let reports_dir = get_reports_dir();
    let subdir = format!("{}\\vt11_listcheck", reports_dir);
    let _ = create_dir_all(&subdir);
    let ts = Local::now().format("%Y%m%d_%H%M%S");
    let csv_path = format!("{}\\vt11_listcheck_{}.csv", subdir, ts);
    if let Ok(mut f) = File::create(&csv_path) {
        let _ = writeln!(f, "Shipment,Delivery,User");
        for (ship, deliv, user) in rows {
            let ship_s = ship.trim();
            let deliv_s = deliv.trim();
            let user_s = user.trim();
            if !ship_s.is_empty() && !deliv_s.is_empty() {
                let _ = writeln!(f, "{},{},{}", ship_s, deliv_s, user_s);
            }
        }
        println!("VT11 ListCheck CSV written: {}", csv_path);
    } else {
        eprintln!("Failed to create VT11 ListCheck CSV file");
    }

    // Write statistics to JSON file
    #[derive(Serialize, Deserialize)]
    struct ShipmentInfo {
        shipment: String,
        delivery: String,
        user: String,
        timestamp: String,
    }

    #[derive(Serialize, Deserialize)]
    struct ListCheckStats {
        total_attempted: u32,
        blocked_count: u32,
        unblocked_count: u32,
        blocked_shipments: Vec<ShipmentInfo>,
        unblocked_shipments: Vec<ShipmentInfo>,
        timestamp: String,
    }

    // Separate blocked and unblocked shipments, deduplicating by shipment number
    let mut blocked_shipments_dedup: Vec<ShipmentInfo> = Vec::new();
    let mut unblocked_shipments_dedup: Vec<ShipmentInfo> = Vec::new();
    let mut seen_blocked: HashSet<String> = HashSet::new();
    let mut seen_unblocked: HashSet<String> = HashSet::new();

    for (shipment, delivery, user, timestamp, is_blocked) in shipments_data {
        if is_blocked {
            if !seen_blocked.contains(&shipment) {
                seen_blocked.insert(shipment.clone());
                blocked_shipments_dedup.push(ShipmentInfo {
                    shipment: shipment.clone(),
                    delivery: delivery.clone(),
                    user: user.clone(),
                    timestamp: timestamp.clone(),
                });
            }
        } else {
            if !seen_unblocked.contains(&shipment) {
                seen_unblocked.insert(shipment.clone());
                unblocked_shipments_dedup.push(ShipmentInfo {
                    shipment: shipment.clone(),
                    delivery: delivery.clone(),
                    user: user.clone(),
                    timestamp: timestamp.clone(),
                });
            }
        }
    }

    let stats = ListCheckStats {
        total_attempted,
        blocked_count,
        unblocked_count: unblocked_shipments_dedup.len() as u32,
        blocked_shipments: blocked_shipments_dedup,
        unblocked_shipments: unblocked_shipments_dedup,
        timestamp: Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
    };

    let json_path = format!("{}\\vt11_listcheck_{}.json", subdir, ts);
    if let Ok(json_content) = serde_json::to_string_pretty(&stats) {
        if let Ok(mut f) = File::create(&json_path) {
            if writeln!(f, "{}", json_content).is_ok() {
                println!("VT11 ListCheck stats written: {}", json_path);
            } else {
                eprintln!("Failed to write VT11 ListCheck stats JSON file");
            }
        } else {
            eprintln!("Failed to create VT11 ListCheck stats JSON file");
        }
    } else {
        eprintln!("Failed to serialize VT11 ListCheck stats");
    }

    println!(
        "Found {} blocked shipments out of {} total attempted",
        blocked_count, total_attempted
    );
    println!("Found {} blocked deliveries", deliveries.len());
    Ok(deliveries)
}
