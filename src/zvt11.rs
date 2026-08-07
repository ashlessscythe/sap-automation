use chrono::NaiveDate;
use sap_scripting::*;
use windows::core::Result;

use crate::utils::{choose_layout, sap_file_utils::*};
// Import specific functions to avoid ambiguity
use crate::utils::config_ops::get_reports_dir;
use crate::utils::excel_file_ops::read_excel_column;
use crate::utils::excel_path_utils::get_newest_file;
use crate::utils::sap_ctrl_utils::exist_ctrl;
use crate::utils::sap_export_utils::export_local_file;
use crate::utils::sap_tcode_utils::*;
use crate::utils::sap_wnd_utils::*;

/// Struct to hold ZVT11 export parameters
#[derive(Debug)]
pub struct ZVT11Params {
    pub start_date: NaiveDate,
    pub end_date: NaiveDate,
    pub sap_variant_name: Option<String>,
    pub layout_row: Option<String>,
    pub by_date: bool,
    pub by_delivery: bool,
    pub limiter: Option<String>,
    pub t_code: String,
}

impl Default for ZVT11Params {
    fn default() -> Self {
        Self {
            start_date: chrono::Local::now().date_naive(),
            end_date: chrono::Local::now().date_naive(),
            sap_variant_name: None,
            layout_row: None,
            by_date: false,
            by_delivery: false,
            limiter: None,
            t_code: "ZVT11".to_string(),
        }
    }
}

/// Get delivery numbers from the latest ZMDESNR export file (same as VT11 delivery module)
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

/// Try to open the local file export dialog for ZVT11
fn try_open_local_file_export(session: &GuiSession) -> bool {
    // Prioritize VB-observed path for 'Save list in file...'
    let candidates = [
        "wnd[0]/mbar/menu[0]/menu[3]/menu[2]", // List -> Export -> Local file (VB example)
        "wnd[0]/mbar/menu[0]/menu[9]/menu[2]", // List -> Export -> Local file (common)
        "wnd[0]/mbar/menu[0]/menu[9]/menu[0]", // Alt path
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

/// Run ZVT11 export with the given parameters
///
/// This function is a port of the VBA function ZVT11_From_Deliv
pub fn run_export(session: &GuiSession, params: &ZVT11Params) -> Result<bool> {
    println!("Running ZVT11 export...");

    // Check if tCode is active
    if !assert_tcode(session, "ZVT11", Some(0))? {
        println!("Failed to activate ZVT11 transaction");
        return Ok(false);
    }

    // select tab 1
    if let Ok(tab) = session.find_by_id("wnd[0]/usr/tabsTABSTRIP_BLK1/tabpTAB1".to_string()) {
        if let Some(tab_item) = tab.downcast::<GuiTab>() {
            tab_item.select()?;
        }
    }

    // Format dates for SAP
    let start_date_str = params.start_date.format("%m/%d/%Y").to_string();
    let end_date_str = params.end_date.format("%m/%d/%Y").to_string();

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
        if let Ok(txt) = session.find_by_id("wnd[0]/usr/ctxtK_DATEN-LOW".to_string()) {
            if let Some(text_field) = txt.downcast::<GuiTextField>() {
                text_field.set_text(start_date_str.clone())?;
            }
        }

        // Set end date (leave blank if same as start date)
        if let Ok(txt) = session.find_by_id("wnd[0]/usr/ctxtK_DATEN-HIGH".to_string()) {
            if let Some(text_field) = txt.downcast::<GuiTextField>() {
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
        let mut delivery_numbers =
            match crate::utils::source_overrides::cli_delivery_numbers_override() {
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
                    } else {
                        println!("No VT11 ListCheck deliveries to append.");
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

        // pause for 2 seconds
        std::thread::sleep(std::time::Duration::from_secs(2));

        if !delivery_numbers.is_empty() {
            // Press the multi delivery button - using the path from zvt11.md
            if let Ok(btn) = session.find_by_id(
                "wnd[0]/usr/tabsTABSTRIP_BLK1/tabpTAB1/ssub%_SUBSCREEN_BLK1:ZSDR_SHIPMENT_REPORT:0101/btn%_S_VBELN1_%_APP_%-VALU_PUSH".to_string()
            ) {
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

                // Use the same table ID pattern as VT11
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
                    "ctxt",
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
            println!("ZVT11 export cannot proceed without delivery numbers.");
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
            println!("Selecting layout: {}", layout_row);

            // Use the choose_layout function which will automatically use the ZVT11-specific method
            let msg = choose_layout(session, &params.t_code, layout_row);
            match msg {
                Ok(message) if message.is_empty() => {
                    println!("Layout selected successfully");
                }
                Ok(message) => {
                    println!("Layout selection result: {}", message);
                }
                Err(e) => {
                    eprintln!("Error selecting layout {}: {:?}", layout_row, e);
                    println!("Continuing without layout selection...");
                }
            }
        } else {
            println!("No layout specified, exporting as-is.");
        }
    }

    // Export preference: try local file export if configured; otherwise Excel
    if let Ok(config) = crate::utils::config_types::SapConfig::load() {
        if let Some(exp_type) = config.get_effective_export_type("ZVT11") {
            if try_open_local_file_export(session)
                && export_local_file(session, "ZVT11", exp_type, None).is_ok()
            {
                return Ok(true);
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
    let run_check = check_export_window(session, "ZVT11", "SHIPMENT REPORT")?;
    match run_check {
        true => {
            println!("Export window opened successfully.");
        }
        false => {
            eprintln!("Error checking export window.");
        }
    }

    // Get file path using the utility function
    let (file_path, file_name) = get_tcode_file_path("ZVT11", "xlsx");

    // save sap file with prevent_excel_open set to true
    let run_check = save_sap_file(session, &file_path, &file_name, Some(true))?;

    Ok(run_check)
}
