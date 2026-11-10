use crate::utils::export_outcome::ExportOutcome;
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
    let zmdesnr_dir = format!("{reports_dir}\\zmdesnr");

    // Load configuration to get ZMDESNR effective export type
    let config = match crate::utils::config_types::SapConfig::load() {
        Ok(cfg) => cfg,
        Err(e) => {
            println!("Error loading configuration: {e}");
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

    println!("Looking for ZMDESNR files with extension: .{ext}");

    // Get the newest file in the ZMDESNR directory with the chosen extension
    let newest_path = get_newest_file(&zmdesnr_dir, ext)?;

    if newest_path.is_empty() {
        println!("No ZMDESNR export files found in: {zmdesnr_dir}");
        return Ok(Vec::new());
    }

    println!("Reading delivery numbers from: {newest_path}");

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
                    println!("Error reading text file: {e}");
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
            println!("Failed to read file with extension .{ext}");
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
    let subdir = format!("{reports_dir}\\vt11_listcheck");

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
        println!("No unused VT11 ListCheck CSV found in {subdir}");
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
pub fn run_export_outcome(session: &GuiSession, params: &ZVT11Params) -> Result<ExportOutcome> {
    println!("Running ZVT11 export...");

    // Check if tCode is active
    if !assert_tcode(session, "ZVT11", Some(0))? {
        println!("Failed to activate ZVT11 transaction");
        return Ok(ExportOutcome::Error(
            "ZVT11: export step failed".to_string(),
        ));
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
                    println!("CLI delivery-source error: {e}; falling back to legacy merge");
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
                    return Ok(ExportOutcome::Error(
                        "ZVT11: export step failed".to_string(),
                    ));
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
            return Ok(ExportOutcome::Error(
                "ZVT11: export step failed".to_string(),
            ));
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
                    println!("Unknown limiter type: {limiter}");
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
                    println!("No shipments found from dates ({start_date_str} to {end_date_str})");

                    // Close window
                    if let Ok(window) = session.find_by_id("wnd[1]".to_string()) {
                        if let Some(modal_window) = window.downcast::<GuiFrameWindow>() {
                            modal_window.close()?;
                        }
                    }

                    return Ok(ExportOutcome::NoData(error_text));
                }
            }
        }
    }

    // Check if layout exists and select it
    if let Some(layout_row) = &params.layout_row {
        if !layout_row.is_empty() {
            println!("Selecting layout: {layout_row}");

            // Use the choose_layout function which will automatically use the ZVT11-specific method
            let msg = choose_layout(session, &params.t_code, layout_row);
            match msg {
                Ok(message) if message.is_empty() => {
                    println!("Layout selected successfully");
                }
                Ok(message) => {
                    println!("Layout selection result: {message}");
                }
                Err(e) => {
                    eprintln!("Error selecting layout {layout_row}: {e:?}");
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
            if try_open_local_file_export(session) {
                if let Ok(p) = export_local_file(session, "ZVT11", exp_type, None) {
                    return Ok(ExportOutcome::from_exported_path(&p));
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

    Ok(ExportOutcome::from_saved(run_check, &file_path, &file_name))
}

/// Boolean wrapper kept for menus/sequence; see [`run_export_outcome`].
pub fn run_export(session: &GuiSession, params: &ZVT11Params) -> Result<bool> {
    Ok(run_export_outcome(session, params)?.into_bool())
}

// ---------------------------------------------------------------------------------------------
// ZVT11 "Closed" (tab 2) for the headless ABC runner, spec 3.2.
//
// Config-agnostic: everything arrives through [`ZVT11ClosedParams`]; no `SapConfig`, no CLI
// overrides, no clock, no console prompt. The legacy flow above (tab 1, "Open") is untouched.
//
// NOT run against SAP by the author. Every control id below comes from the VBA spec only; see
// "Live verification checklist (k2xx)" in `docs/abc_headless_plan.md`. UNVERIFIED live:
// * tab `wnd[0]/usr/tabsTABSTRIP_BLK1/tabpTAB2` and the sub-screen prefix [`CLOSED_PREFIX`],
// * `ctxtS_DATE_2-LOW` / `-HIGH`, `btn%_S_VBELN2_%_APP_%-VALU_PUSH`, `radR1`, `chkP_ALV_2`,
//   `ctxtS_VSTEL2-LOW` (ship point, only typed when given),
// * the variant dialog (shared `variant_select`) while tab 2 is active,
// * the "No data exists in the database for this selection" popup (`wnd[1]/usr/txtMESSTXT1`),
// * the layout chooser behind `btn[33]` (grid [`CLOSED_LAYOUT_GRID`], name columns
//   [`CLOSED_LAYOUT_COLUMNS`]),
// * the local file export menu (candidates [`CLOSED_LOCAL_FILE_MENUS`]) and the header titles
//   of the exported text list.
// ---------------------------------------------------------------------------------------------

/// Sub-screen of the "Closed" tab (spec 3.2), relative to `wnd[0]/usr/`.
pub const CLOSED_PREFIX: &str =
    "tabsTABSTRIP_BLK1/tabpTAB2/ssub%_SUBSCREEN_BLK1:ZSDR_SHIPMENT_REPORT:0102/";
/// Layout chooser grid (the id the legacy ZVT11 layout code uses).
pub const CLOSED_LAYOUT_GRID: &str =
    "wnd[1]/usr/ssubD0500_SUBSCREEN:SAPLSLVC_DIALOG:0501/cntlG51_CONTAINER/shellcont/shell";
/// Grid columns that may hold the layout name (tried in this order).
pub const CLOSED_LAYOUT_COLUMNS: &[&str] = &["VARIANT", "LAYOUT", "NAME", "TEXT", "DESCRIPTION"];
/// "Local file" menu candidates (the list of the legacy flow).
pub const CLOSED_LOCAL_FILE_MENUS: &[&str] = &[
    "wnd[0]/mbar/menu[0]/menu[3]/menu[2]",
    "wnd[0]/mbar/menu[0]/menu[9]/menu[2]",
];
/// Days on either side of the run date typed into `S_DATE_2` (the delivery list is the filter).
pub const CLOSED_WINDOW_DAYS: i64 = 100;

/// ZVT11 Closed parameters (headless only).
#[derive(Debug, Clone)]
pub struct ZVT11ClosedParams {
    pub variant: Option<String>,
    pub layout_row: Option<String>,
    /// `S_DATE_2-LOW` / `-HIGH`, already widened by the caller ([`closed_window`]).
    pub date_start: NaiveDate,
    pub date_end: NaiveDate,
    /// Config-style date format typed into SAP; `None` = `mm/dd/yyyy`.
    pub date_format: Option<String>,
    /// Distinct, blank-free deliveries (the caller prepares the list).
    pub deliveries: Vec<String>,
    /// Optional ship point (`S_VSTEL2-LOW`); `None` leaves the field alone.
    pub ship_point: Option<String>,
    /// Headless target; required (this step has no legacy flow).
    pub export_target: Option<crate::utils::sap_export_utils::ExportTarget>,
    pub ignore_display_error: bool,
    pub t_code: String,
}

impl Default for ZVT11ClosedParams {
    fn default() -> Self {
        let epoch = NaiveDate::from_ymd_opt(1970, 1, 1).expect("valid date");
        Self {
            variant: None,
            layout_row: None,
            date_start: epoch,
            date_end: epoch,
            date_format: None,
            deliveries: Vec::new(),
            ship_point: None,
            export_target: None,
            ignore_display_error: false,
            t_code: "ZVT11".to_string(),
        }
    }
}

/// The ZVT11 Closed date window of a run: `start - days` .. `end + days` (spec 3.2: date1 - 100
/// and date1 + 100; for a range the end of the range is widened). Negative `days` count as 0.
pub fn closed_window(start: NaiveDate, end: NaiveDate, days: i64) -> (NaiveDate, NaiveDate) {
    let d = chrono::Duration::days(days.max(0));
    let end = end.max(start);
    (start - d, end + d)
}

/// The two date strings typed into `S_DATE_2-LOW` / `-HIGH` (pure, unit tested).
pub fn closed_date_fields(params: &ZVT11ClosedParams) -> (String, String) {
    let fmt = params.date_format.as_deref().unwrap_or("mm/dd/yyyy");
    let end = params.date_end.max(params.date_start);
    (
        crate::utils::config_ops::format_date_with(fmt, params.date_start),
        crate::utils::config_ops::format_date_with(fmt, end),
    )
}

/// Full id of a control on the Closed sub-screen.
pub fn closed_id(control: &str) -> String {
    format!("wnd[0]/usr/{CLOSED_PREFIX}{control}")
}

/// True for the "no data" popup text of ZVT11 (spec 3.2).
pub fn is_no_data_popup(text: &str) -> bool {
    text.to_lowercase().contains("no data exists")
}

fn closed_fail(msg: &str) -> Result<ExportOutcome> {
    Ok(ExportOutcome::Error(format!("ZVT11 Closed: {msg}")))
}

/// Select `layout` in the chooser behind `btn[33]` without any console prompt. Ok(false) when
/// it could not be applied (the chooser is closed again).
fn select_closed_layout(session: &GuiSession, layout: &str) -> Result<bool> {
    use crate::zmdetpc::{close_wnd1, press};
    if !press(session, "wnd[0]/tbar[1]/btn[33]")? {
        println!("ZVT11 Closed: layout button btn[33] not found");
        return Ok(false);
    }
    std::thread::sleep(std::time::Duration::from_millis(1000));
    let grid = session
        .find_by_id(CLOSED_LAYOUT_GRID.to_string())
        .ok()
        .and_then(|c| c.downcast::<GuiGridView>());
    let Some(grid) = grid else {
        println!("ZVT11 Closed: layout chooser grid {CLOSED_LAYOUT_GRID} not found");
        close_wnd1(session);
        return Ok(false);
    };
    let rows = grid.row_count()?;
    for column in CLOSED_LAYOUT_COLUMNS {
        let names: Vec<String> = (0..rows)
            .map(|i| {
                grid.get_cell_value(i, column.to_string())
                    .unwrap_or_default()
            })
            .collect();
        let Some(i) = crate::lt23::find_layout_row(&names, layout) else {
            continue;
        };
        let _ = grid.set_current_cell(i as i32, column.to_string());
        grid.set_selected_rows(i.to_string())?;
        grid.click_current_cell()?;
        if exist_ctrl(session, 1, "", true)?.cband {
            println!("ZVT11 Closed: layout chooser still open after selecting '{layout}'");
            close_wnd1(session);
            return Ok(false);
        }
        println!("ZVT11 Closed: layout '{layout}' selected");
        return Ok(true);
    }
    println!("ZVT11 Closed: layout '{layout}' not found among {rows} layouts");
    close_wnd1(session);
    Ok(false)
}

/// Run ZVT11 Closed for `params.deliveries` (spec 3.2) and export to the target.
pub fn run_closed_export_outcome(
    session: &GuiSession,
    params: &ZVT11ClosedParams,
) -> Result<ExportOutcome> {
    use crate::utils::export_outcome::{classify_after_execute, classify_screen, ClassifyOptions};
    use crate::zmdetpc::{close_wnd1, paste_multi, press, select_radio, set_ctext};

    println!("Running ZVT11 Closed export...");
    if params.deliveries.is_empty() {
        return closed_fail("no deliveries given");
    }
    let Some(target) = &params.export_target else {
        return closed_fail("no export target (this step only runs headless)");
    };
    if !assert_tcode(session, "ZVT11", Some(0))? {
        return closed_fail("could not start transaction ZVT11");
    }

    // tab 2 (Closed)
    match session
        .find_by_id("wnd[0]/usr/tabsTABSTRIP_BLK1/tabpTAB2".to_string())
        .ok()
        .and_then(|c| c.downcast::<GuiTab>())
    {
        Some(tab) => tab.select()?,
        None => return closed_fail("tab tabpTAB2 (Closed) not found"),
    }

    // variant first: the fields below overwrite what it carries
    if let Some(v) = params.variant.as_deref().filter(|v| !v.trim().is_empty()) {
        if !variant_select(session, &params.t_code, v)? {
            return closed_fail(&format!("variant '{v}' could not be selected"));
        }
    }

    if let Some(sp) = params
        .ship_point
        .as_deref()
        .filter(|v| !v.trim().is_empty())
    {
        if !set_ctext(session, &closed_id("ctxtS_VSTEL2-LOW"), sp)? {
            return closed_fail("ship point field ctxtS_VSTEL2-LOW not found");
        }
    }

    // dates (deliberately wide: the delivery list is the real filter)
    let (low, high) = closed_date_fields(params);
    if !set_ctext(session, &closed_id("ctxtS_DATE_2-LOW"), &low)? {
        return closed_fail("date field ctxtS_DATE_2-LOW not found");
    }
    if !set_ctext(session, &closed_id("ctxtS_DATE_2-HIGH"), &high)? {
        return closed_fail("date field ctxtS_DATE_2-HIGH not found");
    }

    // deliveries
    if !paste_multi(
        session,
        "ZVT11 Closed",
        &closed_id("btn%_S_VBELN2_%_APP_%-VALU_PUSH"),
        &params.deliveries,
    )? {
        return closed_fail("could not paste the deliveries");
    }

    // display all deliveries, ALV output
    if !select_radio(session, &closed_id("radR1"))? {
        return closed_fail("radio button radR1 not found");
    }
    match session
        .find_by_id(closed_id("chkP_ALV_2"))
        .ok()
        .and_then(|c| c.downcast::<GuiCheckBox>())
    {
        Some(chk) => chk.set_selected(true)?,
        None => return closed_fail("checkbox chkP_ALV_2 not found"),
    }

    // execute
    if !press(session, "wnd[0]/tbar[1]/btn[8]")? {
        return closed_fail("execute button btn[8] not found");
    }
    let popup = exist_ctrl(session, 1, "/usr/txtMESSTXT1", true)?;
    if popup.cband && is_no_data_popup(&popup.ctext) {
        close_wnd1(session);
        return Ok(ExportOutcome::NoData(popup.ctext.trim().to_string()));
    }
    let opts = ClassifyOptions {
        ignore_display_error: params.ignore_display_error,
    };
    if let Some(o) = classify_after_execute(session, "ZVT11", "/usr/tabsTABSTRIP_BLK1", opts)? {
        println!("ZVT11 Closed outcome after execute: {o:?}");
        return Ok(o);
    }

    // layout: a missing one is a step error (no prompt when headless)
    if let Some(layout) = params
        .layout_row
        .as_deref()
        .filter(|l| !l.trim().is_empty())
    {
        if !select_closed_layout(session, layout)? {
            return closed_fail(&format!("layout '{layout}' could not be selected"));
        }
        if let Some(o) = classify_screen(session, opts)? {
            return Ok(o);
        }
    }

    crate::utils::sap_export_utils::export_to_target(
        session,
        "ZVT11",
        CLOSED_LOCAL_FILE_MENUS,
        target,
    )
}

#[cfg(test)]
mod closed_tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn window_is_100_days_each_side() {
        assert_eq!(
            closed_window(d(2026, 11, 5), d(2026, 11, 5), CLOSED_WINDOW_DAYS),
            (d(2026, 7, 28), d(2027, 2, 13))
        );
        // a range widens its end; an inverted range and a negative count are tolerated
        assert_eq!(
            closed_window(d(2026, 11, 1), d(2026, 11, 7), 1),
            (d(2026, 10, 31), d(2026, 11, 8))
        );
        assert_eq!(
            closed_window(d(2026, 11, 7), d(2026, 11, 1), -5),
            (d(2026, 11, 7), d(2026, 11, 7))
        );
    }

    #[test]
    fn date_fields_and_ids() {
        let p = ZVT11ClosedParams {
            date_start: d(2026, 7, 28),
            date_end: d(2027, 2, 13),
            ..ZVT11ClosedParams::default()
        };
        assert_eq!(
            closed_date_fields(&p),
            ("07/28/2026".into(), "02/13/2027".into())
        );
        let iso = ZVT11ClosedParams {
            date_format: Some("yyyy-mm-dd".into()),
            ..p
        };
        assert_eq!(closed_date_fields(&iso).0, "2026-07-28");
        assert_eq!(
            closed_id("ctxtS_DATE_2-LOW"),
            "wnd[0]/usr/tabsTABSTRIP_BLK1/tabpTAB2/ssub%_SUBSCREEN_BLK1:ZSDR_SHIPMENT_REPORT:0102/ctxtS_DATE_2-LOW"
        );
        let def = ZVT11ClosedParams::default();
        assert_eq!(def.t_code, "ZVT11");
        assert!(def.export_target.is_none() && def.deliveries.is_empty());
    }

    #[test]
    fn no_data_popup_text() {
        assert!(is_no_data_popup(
            "No data exists in the database for this selection"
        ));
        assert!(!is_no_data_popup("Something else"));
    }
}
