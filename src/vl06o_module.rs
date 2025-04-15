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

use crate::utils::config_ops::{get_reports_dir, SapConfig};
use crate::utils::excel_file_ops::read_excel_column;
use crate::utils::excel_path_utils::get_newest_file;
use crate::vl06o::{run_export, VL06OParams};

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
    let tcode_config = match config.get_tcode_config("VL06O") {
        Some(cfg) => cfg,
        None => {
            println!("No configuration found for VL06O.");
            println!("Please configure VL06O parameters first.");
            println!("\nPress Enter to return to main menu...");
            let mut input = String::new();
            io::stdin().read_line(&mut input).unwrap();
            return Ok(());
        }
    };

    // Create VL06OParams from configuration
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
        let vl06o_dir = format!("{}\\VL06O", reports_dir);

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
        let excel_path = get_newest_file(&vl06o_dir, "xlsx")?;

        if excel_path.is_empty() {
            println!("No Excel files found in VL06O directory.");
            println!("Please run VL06O export first to generate an Excel file.");
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
    println!(
        "Date Range: {} to {}",
        params.start_date.format("%m/%d/%Y"),
        params.end_date.format("%m/%d/%Y")
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

    // Wait for user to press enter before returning to main menu
    println!("\nPress Enter to return to main menu...");
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();

    Ok(())
}

fn create_vl06o_params_from_config(config: &HashMap<String, String>) -> VL06OParams {
    let mut params = VL06OParams::default();

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

    // Get start date
    let start_date_str: String = Input::new()
        .with_prompt("Start date (MM/DD/YYYY)")
        .default(chrono::Local::now().format("%m/%d/%Y").to_string())
        .interact_text()
        .unwrap();

    params.start_date =
        parse_date(&start_date_str).unwrap_or_else(|_| chrono::Local::now().date_naive());

    // Get end date
    let end_date_str: String = Input::new()
        .with_prompt("End date (MM/DD/YYYY)")
        .default(chrono::Local::now().format("%m/%d/%Y").to_string())
        .interact_text()
        .unwrap();

    params.end_date =
        parse_date(&end_date_str).unwrap_or_else(|_| chrono::Local::now().date_naive());

    // Get variant name
    let variant_name: String = Input::new()
        .with_prompt("SAP variant name (leave empty for none)")
        .allow_empty(true)
        .interact_text()
        .unwrap();

    params.sap_variant_name = if variant_name.is_empty() {
        None
    } else {
        Some(variant_name)
    };

    // Get layout row
    let layout_row: String = Input::new()
        .with_prompt("Layout row (leave empty for default)")
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
        .allow_empty(true)
        .interact_text()
        .unwrap();

    params.column_name = if column_name.is_empty() {
        None
    } else {
        Some(column_name)
    };

    // If column name is provided, ask for Excel file path
    if let Some(col_name) = &params.column_name {
        println!("Column name provided: {}", col_name);

        // Ask for Excel file path
        let excel_path: String = Input::new()
            .with_prompt("Enter Excel file path (or press Enter to skip)")
            .allow_empty(true)
            .interact_text()
            .unwrap();

        if !excel_path.is_empty() {
            // Ask for shipment numbers
            let shipment_numbers_str: String = Input::new()
                .with_prompt("Enter shipment numbers (comma-separated, or press Enter to skip)")
                .allow_empty(true)
                .interact_text()
                .unwrap();

            if !shipment_numbers_str.is_empty() {
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
        }
    }

    clear_screen();

    println!("-------------------------------");
    println!("Running VL06O with params: {:#?}", params);
    println!("-------------------------------");

    Ok(params)
}

fn parse_date(date_str: &str) -> Result<NaiveDate> {
    // Try to parse the date in MM/DD/YYYY format
    if let Ok(date) = NaiveDate::parse_from_str(date_str, "%m/%d/%Y") {
        return Ok(date);
    }

    // Try to parse the date in MM-DD-YYYY format
    if let Ok(date) = NaiveDate::parse_from_str(date_str, "%m-%d-%Y") {
        return Ok(date);
    }

    // Try to parse the date in YYYY-MM-DD format
    if let Ok(date) = NaiveDate::parse_from_str(date_str, "%Y-%m-%d") {
        return Ok(date);
    }

    // If all parsing attempts fail, return an error
    Err(windows::core::Error::from_win32())
}
