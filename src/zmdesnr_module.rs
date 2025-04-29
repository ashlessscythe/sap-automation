use crossterm::{
    execute,
    terminal::{Clear, ClearType},
};
use dialoguer::Input;
use sap_scripting::*;
use std::collections::HashMap;
use std::fs;
use std::io::{self};
use std::path::Path;
use windows::core::Result;

use crate::utils::config_ops::get_reports_dir;
use crate::utils::config_types::SapConfig;
use crate::utils::excel_file_ops::read_excel_column;
use crate::utils::excel_path_utils::get_newest_file;
use crate::zmdesnr::{run_export, ZMDESNRParams};

pub fn run_zmdesnr_module(session: &GuiSession) -> Result<()> {
    clear_screen();
    println!("ZMDESNR - Serial Number History");
    println!("==============================");

    // Get parameters from use
    let params = get_zmdesnr_parameters()?;

    // Run the export
    match run_export(session, &params) {
        Ok(true) => {
            println!("ZMDESNR export completed successfully!");
        }
        Ok(false) => {
            println!("ZMDESNR export failed or was cancelled.");
        }
        Err(e) => {
            println!("Error running ZMDESNR export: {}", e);
        }
    }

    // Wait for user to press enter before returning to main menu
    println!("\nPress Enter to return to main menu...");
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();

    Ok(())
}

pub fn run_zmdesnr_auto(session: &GuiSession) -> Result<()> {
    clear_screen();
    println!("ZMDESNR - Auto Run from Configuration");
    println!("===================================");

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

    // Get ZMDESNR specific configuration
    let tcode_config = match config.get_tcode_config("ZMDESNR", Some(true)) {
        Some(cfg) => cfg,
        None => {
            println!("No configuration found for ZMDESNR.");
            println!("Please configure ZMDESNR parameters first.");
            println!("\nPress Enter to return to main menu...");
            let mut input = String::new();
            io::stdin().read_line(&mut input).unwrap();
            return Ok(());
        }
    };

    // Create ZMDESNRParams from configuration
    println!("Getting zmdesnr params from config");
    let mut params = create_zmdesnr_params_from_config(&tcode_config);

    // Check if we need to get delivery numbers from Excel
    if let Some(column_name) = &params.column_name {
        println!(
            "Reading delivery numbers from Excel column: {}",
            column_name
        );

        // Get the reports directory
        let reports_dir = get_reports_dir();

        // Create the ZMDESNR subdirectory path
        let zmdesnr_dir = format!("{}\\zmdesnr", reports_dir);

        // Check if the ZMDESNR directory exists
        let zmdesnr_path = Path::new(&zmdesnr_dir);
        if !zmdesnr_path.exists() {
            println!("ZMDESNR directory not found: {}", zmdesnr_dir);
            println!("Creating directory...");
            if let Err(e) = fs::create_dir_all(&zmdesnr_dir) {
                println!("Error creating directory: {}", e);
            }
        }

        // Get the newest Excel file in the ZMDESNR directory
        let excel_path = get_newest_file(&zmdesnr_dir, "xlsx")?;

        if excel_path.is_empty() {
            println!("No Excel files found in ZMDESNR directory.");
            println!("Please run ZMDESNR export first to generate an Excel file.");
        } else {
            println!("Using newest Excel file: {}", excel_path);

            // Read the delivery numbers from the Excel file
            match read_excel_column(&excel_path, "Sheet1", column_name) {
                Ok(delivery_numbers) => {
                    if delivery_numbers.is_empty() {
                        println!("No delivery numbers found in Excel file.");
                    } else {
                        println!(
                            "Found {} delivery numbers in Excel file.",
                            delivery_numbers.len()
                        );
                        params.delivery_numbers = delivery_numbers;
                    }
                }
                Err(e) => {
                    println!("Error reading Excel file: {}", e);
                }
            }
        }
    }

    println!("Running ZMDESNR with the following parameters:");
    println!("--------------------------------------------");
    println!("Variant: {:?}", params.sap_variant_name);
    println!("Layout: {:?}", params.layout_row);
    println!("Serial Number: {:?}", params.serial_number);
    println!("Delivery Numbers: {} found", params.delivery_numbers.len());
    if let Some(pre_export_back) = &params.additional_params.pre_export_back {
        if !pre_export_back.is_empty() {
            println!("Additional param: pre_export_back: {}", pre_export_back.to_string());
        }
    }
    if let Some(add_layout_columns) = &params.additional_params.add_layout_columns {
        if !add_layout_columns.is_empty() {
            println!("Additional param: add_layout_columns: {:?}", add_layout_columns);
        }
    }
    println!("--------------------------------------------");

    // Run the export
    match run_export(session, &params) {
        Ok(true) => {
            println!("ZMDESNR export completed successfully!");
        }
        Ok(false) => {
            println!("ZMDESNR export failed or was cancelled.");
        }
        Err(e) => {
            println!("Error running ZMDESNR export: {}", e);
        }
    }

    Ok(())
}

fn create_zmdesnr_params_from_config(config: &HashMap<String, String>) -> ZMDESNRParams {
    let mut params = ZMDESNRParams::default();

    // Set variant if available
    if let Some(variant) = config.get("variant") {
        params.sap_variant_name = Some(variant.clone());
    }

    // Set layout if available
    if let Some(layout) = config.get("layout") {
        params.layout_row = Some(layout.clone());
    }

    // Set column_name if available
    if let Some(column_name) = config.get("column_name") {
        params.column_name = Some(column_name.clone());
    }

    // Set serial_number if available
    if let Some(serial_number) = config.get("serial_number") {
        params.serial_number = Some(serial_number.clone());
    }

    if let Some(tab_number) = config.get("tab_number") {
        if let Ok(tab_number) = tab_number.parse::<i32>() {
            params.tab_number = Some(tab_number);
        }
    }

    if let Some(pre_export_back) = config.get("pre_export_back") {
        params.additional_params.pre_export_back = Some(pre_export_back.to_string());
    }

    // Set add_layout_columns if available
    if let Some(add_layout_columns) = config.get("add_layout_columns") {
        // Parse the string value as a TOML array
        match toml::from_str::<Vec<String>>(add_layout_columns) {
            Ok(columns) => {
                params.additional_params.add_layout_columns = Some(columns);
            }
            Err(e) => {
                println!("Error parsing add_layout_columns: {}", e);
                // Use default values from the task
                params.additional_params.add_layout_columns = Some(vec![
                    "Created By".to_string(),
                    "Shipment Number".to_string(),
                ]);
            }
        }
    }

    params
}

fn clear_screen() {
    execute!(std::io::stdout(), Clear(ClearType::All)).unwrap();
}

fn get_zmdesnr_parameters() -> Result<ZMDESNRParams> {
    let mut params = ZMDESNRParams::default();

    // Get Delivery Numbers
    let column_name: String = Input::new()
        .with_prompt("Column name for delivery numbers (leave empty for none)")
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
            // Read delivery numbers from Excel file
            match read_excel_column(&excel_path, "Sheet1", col_name) {
                Ok(delivery_numbers) => {
                    if delivery_numbers.is_empty() {
                        println!("No delivery numbers found in Excel file.");
                    } else {
                        println!(
                            "Found {} delivery numbers in Excel file.",
                            delivery_numbers.len()
                        );
                        params.delivery_numbers = delivery_numbers;
                    }
                }
                Err(e) => {
                    println!("Error reading Excel file: {}", e);
                }
            }
        } else {
            // Ask for delivery numbers
            let delivery_numbers_str: String = Input::new()
                .with_prompt("Enter delivery numbers (comma-separated, or press Enter to skip)")
                .allow_empty(true)
                .interact_text()
                .unwrap();

            if !delivery_numbers_str.is_empty() {
                let delivery_numbers: Vec<String> = delivery_numbers_str
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();

                if delivery_numbers.is_empty() {
                    println!("No delivery numbers entered.");
                } else {
                    println!("Found {} delivery numbers.", delivery_numbers.len());
                    params.delivery_numbers = delivery_numbers;
                }
            }
        }
    }

    // Get serial number
    let serial_number: String = Input::new()
        .with_prompt("Serial Number")
        .allow_empty(true)
        .interact_text()
        .unwrap();

    params.serial_number = if serial_number.is_empty() {
        None
    } else {
        Some(serial_number)
    };

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

    clear_screen();

    println!("-------------------------------");
    println!("Running ZMDESNR with params: {:#?}", params);
    println!("-------------------------------");

    Ok(params)
}
