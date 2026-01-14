use chrono::NaiveDate;
use crossterm::{
    execute,
    terminal::{Clear, ClearType},
};
use dialoguer::{Input, Select};
use sap_scripting::*;
use std::collections::HashMap;
use std::fs;
use std::io::{self};
use std::path::Path;
use windows::core::Result;

use crate::utils::config_types::SapConfig;
use crate::utils::excel_file_ops::read_excel_column;
use crate::utils::excel_path_utils::{get_excel_file_path, get_newest_file};
use crate::utils::{config_ops::get_reports_dir, excel_path_utils::resolve_path};
use crate::vl06o::run_export_delivery_packages;
use crate::vl06o::VL06ODeliveryParams;
use crate::vl06o::{run_date_update, run_export, VL06ODateUpdateParams, VL06OParams};

pub fn run_vl06o_module(session: &GuiSession) -> Result<()> {
    clear_screen();
    println!("VL06O - List of Outbound Deliveries");
    println!("==================================");

    // Get parameters from user
    let params = get_vl06o_parameters()?;

    // Run the export
    match run_export(session, &params) {
        Ok(true) => {
            println!("VL06O export completed successfully!");
        }
        Ok(false) => {
            println!("VL06O export failed or was cancelled.");
        }
        Err(e) => {
            println!("Error running VL06O export: {}", e);
        }
    }

    // Wait for user to press enter before returning to main menu
    println!("\nPress Enter to return to main menu...");
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();

    Ok(())
}

pub fn run_vl06o_auto(session: &GuiSession) -> Result<()> {
    clear_screen();
    println!("VL06O - Auto Run from Configuration");
    println!("==================================");

    // Load configuration
    let config = match SapConfig::load() {
        Ok(cfg) => cfg,
        Err(e) => {
            println!("Error loading configuration: {}", e);
            println!("\nPress Enter to return to main menu...");
            let mut input = String::new();
            io::stdin().read_line(&mut input).unwrap();
            return Ok(());
        }
    };

    // Get VL06O specific configuration
    let tcode_config = match config.get_tcode_config("VL06O", Some(false)) {
        Some(cfg) => {
            println!("VL06O configuration found: {:?}", cfg);
            cfg
        }
        None => {
            println!("No configuration found for VL06O.");
            println!("Please configure VL06O parameters first.");
            println!("\nPress Enter to return to main menu...");
            let mut input = String::new();
            io::stdin().read_line(&mut input).unwrap();
            return Ok(());
        }
    };

    // Detect by-delivery mode from config
    let by_delivery = tcode_config
        .get("by_delivery")
        .map(|v| v.to_lowercase() == "true")
        .unwrap_or(false);

    if by_delivery {
        // Build delivery-params path using ZMDESNR + ListCheck merged
        let mut dparams = VL06ODeliveryParams::default();
        if let Some(variant) = tcode_config.get("variant") {
            dparams.sap_variant_name = Some(variant.clone());
        }
        if let Some(layout) = tcode_config.get("layout") {
            dparams.layout_row = Some(layout.clone());
        }

        // Get delivery numbers from ZMDESNR
        let mut delivery_numbers = get_delivery_numbers_from_zmdesnr_for_vl06o()?;
        // Append from newest VT11 ListCheck CSV (if present); do not mark here
        let (listcheck_numbers, listcheck_path_opt) =
            get_delivery_numbers_from_listcheck_for_vl06o()?;
        if let Some(ref p) = listcheck_path_opt {
            println!("Using VT11 ListCheck deliveries from: {}", p);
        }
        if !listcheck_numbers.is_empty() {
            println!(
                "Appending {} deliveries from VT11 ListCheck CSV",
                listcheck_numbers.len()
            );
            delivery_numbers.extend(listcheck_numbers);
        } else {
            println!("No VT11 ListCheck deliveries to append.");
        }

        // Dedup, sanitize
        let mut delivery_numbers = delivery_numbers
            .into_iter()
            .filter(|s| !s.trim().is_empty())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        delivery_numbers.sort();

        if delivery_numbers.is_empty() {
            println!("No delivery numbers available for VL06O by_delivery mode.");
            return Ok(());
        }

        dparams.delivery_numbers = delivery_numbers;

        println!("Running VL06O (by_delivery) with the following parameters:");
        println!("-------------------------------------------");
        println!("Variant: {:?}", dparams.sap_variant_name);
        println!("Layout: {:?}", dparams.layout_row);
        println!("Delivery Numbers: {} found", dparams.delivery_numbers.len());
        println!("-------------------------------------------");

        match run_export_delivery_packages(session, &dparams) {
            Ok(true) => {
                println!("VL06O (by_delivery) export completed successfully!");
                // Mark the VT11 ListCheck file as used if one was consumed
                if let Some(path) = listcheck_path_opt {
                    if let Some(dot) = path.rfind('.') {
                        let (prefix, suffix) = path.split_at(dot);
                        if suffix.eq_ignore_ascii_case(".csv") {
                            let new_path = format!("{}_.csv", prefix);
                            match std::fs::rename(&path, &new_path) {
                                Ok(_) => println!(
                                    "Marked VT11 ListCheck as used: {} -> {}",
                                    path, new_path
                                ),
                                Err(e) => eprintln!(
                                    "Failed to mark VT11 ListCheck as used ({}): {}",
                                    path, e
                                ),
                            }
                        }
                    }
                }
            }
            Ok(false) => {
                println!("VL06O (by_delivery) export failed or was cancelled.");
            }
            Err(e) => {
                println!("Error running VL06O (by_delivery) export: {}", e);
            }
        }

        return Ok(());
    }

    // Create VL06OParams from configuration (default path)
    println!("Getting vl06o params from config");
    let mut params = create_vl06o_params_from_config(&tcode_config);

    // Check if we need to get shipment numbers from Excel
    if let Some(column_name) = &params.column_name {
        println!(
            "Reading shipment numbers from Excel column: {}",
            column_name
        );

        // Get the reports directory
        let reports_dir = get_reports_dir();

        // Create the VL06O subdirectory path
        let vl06o_dir = format!("{}\\vl06o", reports_dir);

        // Check if the VL06O directory exists
        let vl06o_path = Path::new(&vl06o_dir);
        if !vl06o_path.exists() {
            println!("VL06O directory not found: {}", vl06o_dir);
            println!("Creating directory...");
            if let Err(e) = fs::create_dir_all(&vl06o_dir) {
                println!("Error creating directory: {}", e);
            }
        }

        // Get the newest Excel file in the VL06O directory
        let vt11_dir = format!("{}\\vt11", get_reports_dir());
        let excel_path = get_newest_file(&vt11_dir, "xlsx")?;

        if excel_path.is_empty() {
            println!("No Excel files found in VT11 directory.");
            println!("Please run VT11 export first to generate an Excel file.");
        } else {
            println!("Using newest Excel file: {}", excel_path);

            // Read the shipment numbers from the Excel file
            match read_excel_column(&excel_path, "Sheet1", column_name) {
                Ok(shipment_numbers) => {
                    if shipment_numbers.is_empty() {
                        println!("No shipment numbers found in Excel file.");
                    } else {
                        println!(
                            "Found {} shipment numbers in Excel file.",
                            shipment_numbers.len()
                        );
                        params.shipment_numbers = shipment_numbers;
                    }
                }
                Err(e) => {
                    println!("Error reading Excel file: {}", e);
                }
            }
        }
    }

    println!("Running VL06O with the following parameters:");
    println!("-------------------------------------------");
    println!("Variant: {:?}", params.sap_variant_name);
    println!("Layout: {:?}", params.layout_row);
    // Get the configured date format
    let config = SapConfig::load().ok();
    let date_format = config
        .as_ref()
        .and_then(|c| c.global.as_ref())
        .map(|g| g.date_format.as_str())
        .unwrap_or("mm/dd/yyyy");

    // Format dates according to configuration
    let format_str = if date_format.to_lowercase() == "yyyy-mm-dd" {
        "%Y-%m-%d"
    } else {
        "%m/%d/%Y"
    };

    println!(
        "Date Range: {} to {}",
        params.start_date.format(format_str),
        params.end_date.format(format_str)
    );
    println!("Filter by Date: {}", params.by_date);
    println!("Column Name: {:?}", params.column_name);
    println!("Shipment Numbers: {} found", params.shipment_numbers.len());
    println!("-------------------------------------------");

    // Run the export
    match run_export(session, &params) {
        Ok(true) => {
            println!("VL06O export completed successfully!");
        }
        Ok(false) => {
            println!("VL06O export failed or was cancelled.");
        }
        Err(e) => {
            println!("Error running VL06O export: {}", e);
        }
    }

    Ok(())
}

/// Read deliveries from latest ZMDESNR export for VL06O (with header fallbacks)
fn get_delivery_numbers_from_zmdesnr_for_vl06o() -> Result<Vec<String>> {
    let reports_dir = get_reports_dir();
    let zmdesnr_dir = format!("{}\\zmdesnr", reports_dir);

    // Load configuration to get ZMDESNR effective export type
    let config = match SapConfig::load() {
        Ok(cfg) => cfg,
        Err(e) => {
            println!("Error loading configuration: {}", e);
            return Ok(Vec::new());
        }
    };

    let ext = match config.get_effective_export_type("ZMDESNR") {
        Some(0) | Some(1) => "txt",
        Some(2) => "rtf",
        Some(3) => "html",
        Some(4) => {
            println!("ZMDESNR export is set to clipboard; using Excel fallback...");
            "xlsx"
        }
        _ => "txt",
    };

    let newest_path = get_newest_file(&zmdesnr_dir, ext)?;
    if newest_path.is_empty() {
        println!("No ZMDESNR export files found in: {}", zmdesnr_dir);
        return Ok(Vec::new());
    }

    let header_candidates = ["Delivery", "delivery", "delivery number", "delivery_number"];

    let nums = if ext.eq_ignore_ascii_case("xlsx") {
        let mut out: Vec<String> = Vec::new();
        for h in header_candidates.iter() {
            let v = read_excel_column(&newest_path, "Sheet1", h).unwrap_or_default();
            if !v.is_empty() {
                out = v;
                break;
            }
        }
        out
    } else if ext.eq_ignore_ascii_case("txt") {
        let mut out: Vec<String> = Vec::new();
        for h in header_candidates.iter() {
            match crate::vl06o_delivery_module::read_tab_delimited_column(&newest_path, h) {
                Ok(v) => {
                    if !v.is_empty() {
                        out = v;
                        break;
                    }
                }
                Err(e) => {
                    println!("Error reading text file: {}", e);
                    return Ok(Vec::new());
                }
            }
        }
        out
    } else {
        let mut out: Vec<String> = Vec::new();
        for h in header_candidates.iter() {
            if let Ok(v) = read_excel_column(&newest_path, "Sheet1", h) {
                if !v.is_empty() {
                    out = v;
                    break;
                }
            }
        }
        if out.is_empty() {
            println!("Failed to read file with extension .{}", ext);
        }
        out
    };

    Ok(nums)
}

/// Get delivery numbers from newest, unused VT11 ListCheck CSV (do not mark here)
fn get_delivery_numbers_from_listcheck_for_vl06o() -> Result<(Vec<String>, Option<String>)> {
    let mut results: Vec<String> = Vec::new();
    let reports_dir = get_reports_dir();
    let subdir = format!("{}\\vt11_listcheck", reports_dir);

    // Find newest CSV that is not marked used (no "_.csv" suffix)
    let mut newest_path = String::new();
    if let Ok(entries) = std::fs::read_dir(&subdir) {
        let mut newest_time: Option<std::time::SystemTime> = None;
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
                if ext.eq_ignore_ascii_case("csv") {
                    if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
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
        return Ok((results, None));
    }

    if let Ok(contents) = std::fs::read_to_string(&newest_path) {
        for (idx, line) in contents.lines().enumerate() {
            if idx == 0 {
                continue;
            }
            let cols: Vec<&str> = line.split(',').collect();
            if let Some(deliv) = cols.get(1) {
                let d = deliv.trim().trim_matches('"').to_string();
                if !d.is_empty() {
                    results.push(d);
                }
            }
        }
        println!(
            "Read {} delivery numbers from VT11 ListCheck CSV",
            results.len()
        );
    }

    Ok((results, Some(newest_path)))
}

pub fn run_vl06o_date_update_module(session: &GuiSession) -> Result<()> {
    clear_screen();
    println!("VL06O - Change Delivery Date");
    println!("===========================");

    // Get parameters from user
    let params = get_vl06o_date_update_parameters()?;

    // Get the configured date format
    let config = SapConfig::load().ok();
    let date_format = config
        .as_ref()
        .and_then(|c| c.global.as_ref())
        .map(|g| g.date_format.as_str())
        .unwrap_or("mm/dd/yyyy");

    // Format date according to configuration
    let format_str = if date_format.to_lowercase() == "yyyy-mm-dd" {
        "%Y-%m-%d"
    } else {
        "%m/%d/%Y"
    };

    // Confirm with user
    let item_type = if params.is_shipment {
        "shipments"
    } else {
        "deliveries"
    };
    println!(
        "Starting date update for {} {}",
        params.entries.len(),
        item_type
    );
    println!("Target date: {}", params.target_date.format(format_str));

    let options = vec!["Yes, proceed", "No, cancel"];
    let choice = Select::new()
        .with_prompt("Do you want to proceed with the date update?")
        .items(&options)
        .default(0)
        .interact()
        .unwrap();

    if choice == 1 {
        println!("Date update cancelled.");
        println!("\nPress Enter to return to main menu...");
        let mut input = String::new();
        io::stdin().read_line(&mut input).unwrap();
        return Ok(());
    }

    // Run the date update
    match run_date_update(session, &params) {
        Ok((count, changes)) => {
            println!("VL06O date update completed successfully!");
            println!("Processed {} deliveries", count);
            println!("Changed {} delivery dates", changes.len());

            // Display changes
            if !changes.is_empty() {
                println!("\nDelivery Date Changes:");
                println!("----------------------");
                for (delivery, original_date) in changes {
                    println!(
                        "Delivery: {}, Original Date: {} -> New Date: {}",
                        delivery,
                        original_date,
                        params.target_date.format(format_str)
                    );
                }
            }
        }
        Err(e) => {
            println!("Error running VL06O date update: {}", e);
        }
    }

    // Wait for user to press enter before returning to main menu
    println!("\nPress Enter to return to main menu...");
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();

    Ok(())
}

fn create_vl06o_params_from_config(config: &HashMap<String, String>) -> VL06OParams {
    let mut params = VL06OParams::default();

    // Display the default values loaded from config
    println!("Default values from config:");
    println!("  Variant: {:?}", params.sap_variant_name);
    println!("  Layout: {:?}", params.layout_row);
    println!("  Column Name: {:?}", params.column_name);

    // Set variant if available
    if let Some(variant) = config.get("variant") {
        params.sap_variant_name = Some(variant.clone());
    }

    // Set layout if available
    if let Some(layout) = config.get("layout") {
        params.layout_row = Some(layout.clone());
    }

    // Set date range if available
    if let Some(start_date) = config.get("date_range_start") {
        if let Ok(date) = parse_date(start_date) {
            params.start_date = date;
        }
    }

    if let Some(end_date) = config.get("date_range_end") {
        if let Ok(date) = parse_date(end_date) {
            params.end_date = date;
        }
    }

    // Set by_date if available
    if let Some(by_date) = config.get("by_date") {
        params.by_date = by_date.to_lowercase() == "true";
    }

    // Set column_name if available
    if let Some(column_name) = config.get("column_name") {
        params.column_name = Some(column_name.clone());
    }

    params
}

fn clear_screen() {
    execute!(std::io::stdout(), Clear(ClearType::All)).unwrap();
}

fn get_vl06o_parameters() -> Result<VL06OParams> {
    let mut params = VL06OParams::default();

    // Display the default values loaded from config
    println!("Default values from config:");
    println!("  Variant: {:?}", params.sap_variant_name);
    println!("  Layout: {:?}", params.layout_row);
    println!("  Column Name: {:?}", params.column_name);

    // Get the configured date format
    let config = SapConfig::load().ok();
    let date_format = config
        .as_ref()
        .and_then(|c| c.global.as_ref())
        .map(|g| g.date_format.as_str())
        .unwrap_or("mm/dd/yyyy");

    // Format date according to configuration
    let format_str = if date_format.to_lowercase() == "yyyy-mm-dd" {
        "%Y-%m-%d"
    } else {
        "%m/%d/%Y"
    };
    let prompt_format = if date_format.to_lowercase() == "yyyy-mm-dd" {
        "YYYY-MM-DD"
    } else {
        "MM/DD/YYYY"
    };

    // Get start date
    let start_date_str: String = Input::new()
        .with_prompt(format!("Start date ({})", prompt_format))
        .default(chrono::Local::now().format(format_str).to_string())
        .interact_text()
        .unwrap();

    params.start_date =
        parse_date(&start_date_str).unwrap_or_else(|_| chrono::Local::now().date_naive());

    // Get end date
    let end_date_str: String = Input::new()
        .with_prompt(format!("End date ({})", prompt_format))
        .default(chrono::Local::now().format(format_str).to_string())
        .interact_text()
        .unwrap();

    params.end_date =
        parse_date(&end_date_str).unwrap_or_else(|_| chrono::Local::now().date_naive());

    // Get variant name
    let variant_prompt = match &params.sap_variant_name {
        Some(variant) => format!("SAP variant name (default: {})", variant),
        None => "SAP variant name (leave empty for none)".to_string(),
    };

    let variant_initial = params.sap_variant_name.clone().unwrap_or_default();

    let variant_name: String = Input::new()
        .with_prompt(&variant_prompt)
        .with_initial_text(variant_initial)
        .allow_empty(true)
        .interact_text()
        .unwrap();

    params.sap_variant_name = if variant_name.is_empty() {
        None
    } else {
        Some(variant_name)
    };

    // Get layout row
    let layout_prompt = match &params.layout_row {
        Some(layout) => format!("Layout row (default: {})", layout),
        None => "Layout row (leave empty for default)".to_string(),
    };

    let layout_initial = params.layout_row.clone().unwrap_or_default();

    let layout_row: String = Input::new()
        .with_prompt(&layout_prompt)
        .with_initial_text(layout_initial)
        .allow_empty(true)
        .interact_text()
        .unwrap();

    params.layout_row = if layout_row.is_empty() {
        None
    } else {
        Some(layout_row)
    };

    // Get by_date option
    let by_date_options = vec!["Yes", "No"];
    let by_date_choice = Select::new()
        .with_prompt("Filter by date?")
        .items(&by_date_options)
        .default(1)
        .interact()
        .unwrap();

    params.by_date = by_date_choice == 0;

    // Get column name
    let column_name: String = Input::new()
        .with_prompt("Column name (leave empty for default)")
        .with_initial_text("Shipment Number")
        .allow_empty(false)
        .interact_text()
        .unwrap();

    params.column_name = if column_name.is_empty() {
        Some("Shipment Number".to_string()) // default
    } else {
        Some(column_name)
    };

    // If column name is provided, ask how to input shipment numbers
    if let Some(col_name) = &params.column_name {
        println!("Column name provided: {}", col_name);

        // Ask how to input shipment numbers
        let input_options = vec![
            "Read from Excel file",
            "Enter manually",
            "Paste from clipboard",
        ];
        let input_choice = Select::new()
            .with_prompt("How would you like to input shipment numbers?")
            .items(&input_options)
            .default(0)
            .interact()
            .unwrap();

        match input_choice {
            2 => {
                // Paste from clipboard
                println!("Please paste shipment numbers from clipboard (one per line):");
                println!("When finished, press Enter twice.");

                let mut shipment_numbers = Vec::new();
                let mut buffer = String::new();

                loop {
                    let mut line = String::new();
                    io::stdin().read_line(&mut line).unwrap();

                    if line.trim().is_empty() && buffer.trim().is_empty() {
                        break;
                    }

                    if line.trim().is_empty() {
                        // Process buffer
                        for number in buffer.lines() {
                            let trimmed = number.trim();
                            if !trimmed.is_empty() {
                                shipment_numbers.push(trimmed.to_string());
                            }
                        }
                        buffer.clear();
                        break;
                    }

                    buffer.push_str(&line);
                }

                if shipment_numbers.is_empty() {
                    println!("No shipment numbers entered.");
                } else {
                    println!("Found {} shipment numbers.", shipment_numbers.len());
                    params.shipment_numbers = shipment_numbers;
                }
            }
            1 => {
                // Enter manually
                let shipment_numbers_str: String = Input::new()
                    .with_prompt("Enter shipment numbers (comma-separated)")
                    .interact_text()
                    .unwrap();

                let shipment_numbers: Vec<String> = shipment_numbers_str
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();

                if shipment_numbers.is_empty() {
                    println!("No shipment numbers entered.");
                } else {
                    println!("Found {} shipment numbers.", shipment_numbers.len());
                    params.shipment_numbers = shipment_numbers;
                }
            }
            0 => {
                // Read from Excel file
                println!("Select an Excel file containing shipment numbers:");

                // Get the reports directory as the default starting point
                let reports_dir = get_reports_dir();
                println!("Current reports directory: {}", reports_dir);

                // Ask if user wants to use a subdirectory
                println!("You can enter a subdirectory name to navigate to a specific folder.");
                println!(
                    "For example, entering 'subpath' will navigate to {}\\subpath",
                    reports_dir
                );
                println!("Or press Enter to use the current reports directory.");

                let subdir: String = Input::new()
                    .with_prompt("Enter subdirectory (optional)")
                    .allow_empty(true)
                    .interact_text()
                    .unwrap();

                // Determine the directory to use
                let dir_to_use = if subdir.is_empty() {
                    reports_dir.clone()
                } else {
                    // Handle the case where the user entered a subdirectory
                    let mut path = format!("{}\\{}", reports_dir, subdir);
                    path = resolve_path(&path);
                    println!("Using directory: {}", path);
                    path
                };

                // Use the get_excel_file_path function to select an Excel file
                println!("Please select an Excel file from the dialog...");
                println!("Press Enter to continue to file selection...");
                let mut input = String::new();
                io::stdin().read_line(&mut input).unwrap();

                match get_excel_file_path(&dir_to_use) {
                    Ok(excel_path) => {
                        println!("Selected Excel file: {}", excel_path);

                        // Loop until we get a valid column name or user chooses to exit
                        let mut column_valid = false;

                        while !column_valid {
                            // Get the column name
                            let column_name: String = Input::new()
                                .with_prompt("Enter column name containing shipment numbers")
                                .default(col_name.clone())
                                .interact_text()
                                .unwrap();

                            if column_name.is_empty() {
                                println!("Column name is empty.");

                                // Ask if user wants to try again or return to main menu
                                let options = vec!["Try again", "Return to main menu"];
                                let choice = Select::new()
                                    .with_prompt("What would you like to do?")
                                    .items(&options)
                                    .default(0)
                                    .interact()
                                    .unwrap();

                                if choice == 1 {
                                    // User wants to return to main menu
                                    println!("Returning to main menu...");
                                    break;
                                }
                                // Otherwise, loop continues for another attempt
                            } else {
                                println!("Reading from column: {}", column_name);

                                // Read the shipment numbers from the Excel file
                                match read_excel_column(&excel_path, "Sheet1", &column_name) {
                                    Ok(shipment_numbers) => {
                                        if shipment_numbers.is_empty() {
                                            println!("No shipment numbers found in column '{}' of the Excel file.", column_name);

                                            // Ask if user wants to try again or return to main menu
                                            let options =
                                                vec!["Try another column", "Return to main menu"];
                                            let choice = Select::new()
                                                .with_prompt("What would you like to do?")
                                                .items(&options)
                                                .default(0)
                                                .interact()
                                                .unwrap();

                                            if choice == 1 {
                                                // User wants to return to main menu
                                                println!("Returning to main menu...");
                                                break;
                                            }
                                            // Otherwise, loop continues for another attempt
                                        } else {
                                            println!(
                                                "Found {} shipment numbers in Excel file.",
                                                shipment_numbers.len()
                                            );
                                            params.shipment_numbers = shipment_numbers;
                                            column_valid = true; // Exit the loop
                                        }
                                    }
                                    Err(e) => {
                                        println!("Error reading Excel file: {}", e);
                                        println!(
                                            "Column '{}' may not exist in the Excel file.",
                                            column_name
                                        );

                                        // Ask if user wants to try again or return to main menu
                                        let options =
                                            vec!["Try another column", "Return to main menu"];
                                        let choice = Select::new()
                                            .with_prompt("What would you like to do?")
                                            .items(&options)
                                            .default(0)
                                            .interact()
                                            .unwrap();

                                        if choice == 1 {
                                            // User wants to return to main menu
                                            println!("Returning to main menu...");
                                            break;
                                        }
                                        // Otherwise, loop continues for another attempt
                                    }
                                }
                            }
                        }

                        // Wait for user to acknowledge before continuing
                        println!("Press Enter to continue...");
                        let mut input = String::new();
                        io::stdin().read_line(&mut input).unwrap();
                    }
                    Err(e) => {
                        println!("Error selecting Excel file: {}", e);
                        println!("Error details: {}", e);

                        // Wait for user to acknowledge before continuing
                        println!("Press Enter to continue...");
                        let mut input = String::new();
                        io::stdin().read_line(&mut input).unwrap();
                    }
                }
            }
            _ => {
                println!("Unexpected option selected.");

                // Wait for user to acknowledge before continuing
                println!("Press Enter to continue...");
                let mut input = String::new();
                io::stdin().read_line(&mut input).unwrap();
            }
        }
    }

    clear_screen();

    println!("-------------------------------");
    println!("Running VL06O with params: {:#?}", params);
    println!("-------------------------------");

    Ok(params)
}

fn get_vl06o_date_update_parameters() -> Result<VL06ODateUpdateParams> {
    let mut params = VL06ODateUpdateParams::default();

    // Display the default values loaded from config
    println!("Default values from config:");
    println!("  Variant: {:?}", params.sap_variant_name);
    println!("  Target Date: {}", params.target_date);

    // Get the configured date format
    let config = SapConfig::load().ok();
    let date_format = config
        .as_ref()
        .and_then(|c| c.global.as_ref())
        .map(|g| g.date_format.as_str())
        .unwrap_or("mm/dd/yyyy");

    // Format date according to configuration
    let format_str = if date_format.to_lowercase() == "yyyy-mm-dd" {
        "%Y-%m-%d"
    } else {
        "%m/%d/%Y"
    };
    let prompt_format = if date_format.to_lowercase() == "yyyy-mm-dd" {
        "YYYY-MM-DD"
    } else {
        "MM/DD/YYYY"
    };

    // Get target date
    let target_date_str: String = Input::new()
        .with_prompt(format!("Target date ({})", prompt_format))
        .default(
            chrono::Local::now()
                .date_naive()
                .succ_opt()
                .unwrap()
                .format(format_str)
                .to_string(),
        )
        .interact_text()
        .unwrap();

    params.target_date =
        parse_date(&target_date_str).unwrap_or_else(|_| chrono::Local::now().date_naive().succ_opt().unwrap());

    // Get variant name
    let variant_value = params
        .sap_variant_name
        .clone()
        .unwrap_or_else(|| "blank_".to_string());
    let variant_prompt = format!("SAP variant name (default: {})", variant_value);

    let variant_name: String = Input::new()
        .with_prompt(&variant_prompt)
        .with_initial_text(variant_value)
        .allow_empty(true)
        .interact_text()
        .unwrap();

    params.sap_variant_name = if variant_name.is_empty() {
        Some("blank_".to_string())
    } else {
        Some(variant_name)
    };

    // Ask if user wants to provide shipments or deliveries
    let item_type_options = vec!["Shipment(s)", "Deliveries"];
    let item_type_choice = Select::new()
        .with_prompt("Do you want to provide shipment(s) or deliveries?")
        .items(&item_type_options)
        .default(1)
        .interact()
        .unwrap();

    let is_shipment = item_type_choice == 0;
    let item_type_name = if is_shipment { "shipment" } else { "delivery" };

    // Ask how to input numbers
    let input_options = vec!["Read from Excel file", "Enter manually"];
    let input_choice = Select::new()
        .with_prompt(format!(
            "How would you like to input {} numbers?",
            item_type_name
        ))
        .items(&input_options)
        .default(1)
        .interact()
        .unwrap();

    match input_choice {
        1 => {
            // Enter manually
            let numbers_str: String = Input::new()
                .with_prompt(format!(
                    "Enter {} numbers (space or comma-separated)",
                    item_type_name
                ))
                .interact_text()
                .unwrap();

            // Check if input contains commas
            let numbers: Vec<String> = if numbers_str.contains(',') {
                // Split by commas if present
                numbers_str
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect()
            } else {
                // Otherwise split by spaces
                numbers_str
                    .split_whitespace()
                    .map(|s| s.to_string())
                    .collect()
            };

            if numbers.is_empty() {
                println!("No {} numbers entered.", item_type_name);
            } else {
                println!("Found {} {} numbers.", numbers.len(), item_type_name);
                params.entries = numbers;
                params.is_shipment = is_shipment;
            }
        }
        0 => {
            // Read from Excel file
            println!(
                "Select an Excel file containing {} numbers:",
                item_type_name
            );

            // Get the reports directory as the default starting point
            let reports_dir = get_reports_dir();
            println!("Current reports directory: {}", reports_dir);

            // Ask if user wants to use a subdirectory
            println!("You can enter a subdirectory name to navigate to a specific folder.");
            println!(
                "For example, entering 'subpath' will navigate to {}\\subpath",
                reports_dir
            );
            println!("Or press Enter to use the current reports directory.");

            let subdir: String = Input::new()
                .with_prompt("Enter subdirectory (optional)")
                .allow_empty(true)
                .interact_text()
                .unwrap();

            // Determine the directory to use
            let dir_to_use = if subdir.is_empty() {
                reports_dir.clone()
            } else {
                // Handle the case where the user entered a subdirectory
                let mut path = format!("{}\\{}", reports_dir, subdir);
                path = resolve_path(&path);
                println!("Using directory: {}", path);
                path
            };

            // Use the get_excel_file_path function to select an Excel file
            println!("Please select an Excel file from the dialog...");
            println!("Press Enter to continue to file selection...");
            let mut input = String::new();
            io::stdin().read_line(&mut input).unwrap();

            match get_excel_file_path(&dir_to_use) {
                Ok(excel_path) => {
                    println!("Selected Excel file: {}", excel_path);

                    // Loop until we get a valid column name or user chooses to exit
                    let mut column_valid = false;

                    while !column_valid {
                        // Get the column name
                        let column_name: String = Input::new()
                            .with_prompt(format!(
                                "Enter column name containing {} numbers",
                                item_type_name
                            ))
                            .interact_text()
                            .unwrap();

                        if column_name.is_empty() {
                            println!("Column name is empty.");

                            // Ask if user wants to try again or return to main menu
                            let options = vec!["Try again", "Return to main menu"];
                            let choice = Select::new()
                                .with_prompt("What would you like to do?")
                                .items(&options)
                                .default(0)
                                .interact()
                                .unwrap();

                            if choice == 1 {
                                // User wants to return to main menu
                                println!("Returning to main menu...");
                                break;
                            }
                            // Otherwise, loop continues for another attempt
                        } else {
                            println!("Reading from column: {}", column_name);

                            // Read the numbers from the Excel file
                            match read_excel_column(&excel_path, "Sheet1", &column_name) {
                                Ok(numbers) => {
                                    if numbers.is_empty() {
                                        println!(
                                            "No {} numbers found in column '{}' of the Excel file.",
                                            item_type_name, column_name
                                        );

                                        // Ask if user wants to try again or return to main menu
                                        let options =
                                            vec!["Try another column", "Return to main menu"];
                                        let choice = Select::new()
                                            .with_prompt("What would you like to do?")
                                            .items(&options)
                                            .default(0)
                                            .interact()
                                            .unwrap();

                                        if choice == 1 {
                                            // User wants to return to main menu
                                            println!("Returning to main menu...");
                                            break;
                                        }
                                        // Otherwise, loop continues for another attempt
                                    } else {
                                        println!(
                                            "Found {} {} numbers in Excel file.",
                                            numbers.len(),
                                            item_type_name
                                        );
                                        params.entries = numbers;
                                        params.is_shipment = is_shipment;
                                        column_valid = true; // Exit the loop
                                    }
                                }
                                Err(e) => {
                                    println!("Error reading Excel file: {}", e);
                                    println!(
                                        "Column '{}' may not exist in the Excel file.",
                                        column_name
                                    );

                                    // Ask if user wants to try again or return to main menu
                                    let options = vec!["Try another column", "Return to main menu"];
                                    let choice = Select::new()
                                        .with_prompt("What would you like to do?")
                                        .items(&options)
                                        .default(0)
                                        .interact()
                                        .unwrap();

                                    if choice == 1 {
                                        // User wants to return to main menu
                                        println!("Returning to main menu...");
                                        break;
                                    }
                                    // Otherwise, loop continues for another attempt
                                }
                            }
                        }
                    }

                    // Wait for user to acknowledge before continuing
                    println!("Press Enter to continue...");
                    let mut input = String::new();
                    io::stdin().read_line(&mut input).unwrap();
                }
                Err(e) => {
                    println!("Error selecting Excel file: {}", e);
                    println!("Error details: {}", e);

                    // Wait for user to acknowledge before continuing
                    println!("Press Enter to continue...");
                    let mut input = String::new();
                    io::stdin().read_line(&mut input).unwrap();
                }
            }
        }
        _ => {
            println!("Unexpected option selected.");

            // Wait for user to acknowledge before continuing
            println!("Press Enter to continue...");
            let mut input = String::new();
            io::stdin().read_line(&mut input).unwrap();
        }
    }

    clear_screen();

    println!("-------------------------------");
    println!("Running VL06O Date Update with params: {:#?}", params);
    println!("-------------------------------");

    Ok(params)
}

fn parse_date(date_str: &str) -> Result<NaiveDate> {
    // Try to load the configuration to get the date format
    let config = SapConfig::load().ok();
    let date_format = config
        .as_ref()
        .and_then(|c| c.global.as_ref())
        .map(|g| g.date_format.as_str())
        .unwrap_or("mm/dd/yyyy");

    // Try to parse the date based on the configured format
    match date_format.to_lowercase().as_str() {
        "yyyy-mm-dd" => {
            // Try YYYY-MM-DD format first
            if let Ok(date) = NaiveDate::parse_from_str(date_str, "%Y-%m-%d") {
                return Ok(date);
            }

            // Fallback to other formats
            if let Ok(date) = NaiveDate::parse_from_str(date_str, "%m/%d/%Y") {
                return Ok(date);
            }

            if let Ok(date) = NaiveDate::parse_from_str(date_str, "%m-%d-%Y") {
                return Ok(date);
            }
        }
        _ => {
            // Default to mm/dd/yyyy
            // Try MM/DD/YYYY format first
            if let Ok(date) = NaiveDate::parse_from_str(date_str, "%m/%d/%Y") {
                return Ok(date);
            }

            // Fallback to other formats
            if let Ok(date) = NaiveDate::parse_from_str(date_str, "%m-%d-%Y") {
                return Ok(date);
            }

            if let Ok(date) = NaiveDate::parse_from_str(date_str, "%Y-%m-%d") {
                return Ok(date);
            }
        }
    }

    // If all parsing attempts fail, return an error
    Err(windows::core::Error::from_win32())
}
