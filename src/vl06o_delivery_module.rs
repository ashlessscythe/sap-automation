use chrono::NaiveDate;
use crossterm::{
    execute,
    terminal::{Clear, ClearType},
};
use dialoguer::{Input, Select};
use sap_scripting::*;
use std::fs;
use std::io::{self};
use std::path::Path;
use windows::core::Result;

use crate::utils::{config_ops::get_reports_dir, excel_path_utils::resolve_path};
use crate::utils::config_types::SapConfig;
use crate::utils::excel_file_ops::read_excel_column;
use crate::utils::excel_path_utils::{get_excel_file_path, get_newest_file};
use crate::vl06o::{run_export_delivery_packages, VL06ODeliveryParams};

/// Run VL06O export with delivery numbers to get package counts
pub fn run_vl06o_delivery_packages_module(session: &GuiSession) -> Result<()> {
    clear_screen();
    println!("VL06O - List of Delivery Packages");
    println!("================================");

    // Get parameters from user
    let params = get_vl06o_delivery_parameters()?;

    // Run the export
    match run_export_delivery_packages(session, &params) {
        Ok(true) => {
            println!("VL06O delivery packages export completed successfully!");
        }
        Ok(false) => {
            println!("VL06O delivery packages export failed or was cancelled.");
        }
        Err(e) => {
            println!("Error running VL06O delivery packages export: {}", e);
        }
    }

    // Wait for user to press enter before returning to main menu
    println!("\nPress Enter to return to main menu...");
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();

    Ok(())
}

/// Run VL06O delivery packages auto using default configs
/// This function automatically gets deliveries from the "Delivery" column
/// in the latest Excel file in the zmdesnr subdirectory
pub fn run_vl06o_delivery_packages_auto(session: &GuiSession) -> Result<()> {
    clear_screen();
    println!("VL06O - Auto Run Delivery Packages");
    println!("=================================");

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

    // Create default parameters
    let mut params = VL06ODeliveryParams::default();

    // Get VL06O specific configuration
    if let Some(tcode_config) = config.get_tcode_config("VL06O", Some(true)) {
        // Override with config if available
        if let Some(variant) = tcode_config.get("variant") {
            params.sap_variant_name = Some(variant.clone());
        }
        if let Some(layout) = tcode_config.get("layout") {
            params.layout_row = Some(layout.clone());
        }
        if let Some(subdir) = tcode_config.get("subdir") {
            params.subdir = Some(subdir.clone());
        }
    } else {
        println!("No configuration found for VL06O.");
        println!("Using default parameters.");
    }

    // Set column name to "Delivery" as specified
    params.column_name = Some("Delivery".to_string());

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
            println!("\nPress Enter to return to main menu...");
            let mut input = String::new();
            io::stdin().read_line(&mut input).unwrap();
            return Ok(());
        }
    }

    // Get the newest Excel file in the ZMDESNR directory
    let excel_path = get_newest_file(&zmdesnr_dir, "xlsx")?;

    if excel_path.is_empty() {
        println!("No Excel files found in ZMDESNR directory.");
        println!("Please run ZMDESNR export first to generate an Excel file.");
        println!("\nPress Enter to return to main menu...");
        let mut input = String::new();
        io::stdin().read_line(&mut input).unwrap();
        return Ok(());
    }

    println!("Using newest Excel file: {}", excel_path);

    // Read the delivery numbers from the Excel file
    match read_excel_column(&excel_path, "Sheet1", "Delivery") {
        Ok(delivery_numbers) => {
            if delivery_numbers.is_empty() {
                println!("No delivery numbers found in Excel file.");
                println!("\nPress Enter to return to main menu...");
                let mut input = String::new();
                io::stdin().read_line(&mut input).unwrap();
                return Ok(());
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
            println!("\nPress Enter to return to main menu...");
            let mut input = String::new();
            io::stdin().read_line(&mut input).unwrap();
            return Ok(());
        }
    }

    println!("Running VL06O delivery packages with the following parameters:");
    println!("--------------------------------------------");
    println!("Variant: {:?}", params.sap_variant_name);
    println!("Layout: {:?}", params.layout_row);
    println!("Column Name: {:?}", params.column_name);
    println!("Delivery Numbers: {} found", params.delivery_numbers.len());
    println!("--------------------------------------------");

    // Run the export
    match run_export_delivery_packages(session, &params) {
        Ok(true) => {
            println!("VL06O delivery packages export completed successfully!");
        }
        Ok(false) => {
            println!("VL06O delivery packages export failed or was cancelled.");
        }
        Err(e) => {
            println!("Error running VL06O delivery packages export: {}", e);
        }
    }

    // Wait for user to press enter before returning to main menu
    println!("\nPress Enter to return to main menu...");
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();

    Ok(())
}

/// Get parameters for VL06O delivery packages export
fn get_vl06o_delivery_parameters() -> Result<VL06ODeliveryParams> {
    let mut params = VL06ODeliveryParams::default();

    // Display the default values loaded from config
    println!("Default values from config:");
    println!("  Variant: {:?}", params.sap_variant_name);
    println!("  Layout: {:?}", params.layout_row);
    println!("  Column Name: {:?}", params.column_name);
    println!("  Subdir: {:?}", params.subdir);

    // Get the configured date format
    let config = SapConfig::load().ok();
    let date_format = config
        .as_ref()
        .and_then(|c| c.global.as_ref())
        .map(|g| g.date_format.as_str())
        .unwrap_or("mm/dd/yyyy");
    
    // Format date according to configuration
    let format_str = if date_format.to_lowercase() == "yyyy-mm-dd" { "%Y-%m-%d" } else { "%m/%d/%Y" };
    let prompt_format = if date_format.to_lowercase() == "yyyy-mm-dd" { "YYYY-MM-DD" } else { "MM/DD/YYYY" };
    
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
        .with_initial_text("Delivery Number")
        .allow_empty(false)
        .interact_text()
        .unwrap();

    params.column_name = if column_name.is_empty() {
        Some("Delivery Number".to_string()) // default
    } else {
        Some(column_name)
    };

    // If column name is provided, ask how to input delivery numbers
    if let Some(col_name) = &params.column_name {
        println!("Column name provided: {}", col_name);

        // Ask how to input delivery numbers
        let input_options = vec!["Read from Excel file", "Enter manually", "Paste from clipboard"];
        let input_choice = Select::new()
            .with_prompt("How would you like to input delivery numbers?")
            .items(&input_options)
            .default(0)
            .interact()
            .unwrap();

        match input_choice {
            2 => {
                // Paste from clipboard
                println!("Please paste delivery numbers from clipboard (one per line):");
                println!("When finished, press Enter twice.");
                
                let mut delivery_numbers = Vec::new();
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
                                delivery_numbers.push(trimmed.to_string());
                            }
                        }
                        buffer.clear();
                        break;
                    }
                    
                    buffer.push_str(&line);
                }
                
                if delivery_numbers.is_empty() {
                    println!("No delivery numbers entered.");
                } else {
                    println!("Found {} delivery numbers.", delivery_numbers.len());
                    params.delivery_numbers = delivery_numbers;
                }
            },
            1 => {
                // Enter manually
                let delivery_numbers_str: String = Input::new()
                    .with_prompt("Enter delivery numbers (comma-separated)")
                    .interact_text()
                    .unwrap();
                
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
            },
            0 => {
                // Read from Excel file
                println!("Select an Excel file containing delivery numbers:");
                
                // Get the reports directory as the default starting point
                let reports_dir = get_reports_dir();
                println!("Current reports directory: {}", reports_dir);
                
                // Ask if user wants to use a subdirectory
                println!("You can enter a subdirectory name to navigate to a specific folder.");
                println!("For example, entering 'subpath' will navigate to {}\\subpath", reports_dir);
                println!("Or press Enter to use the current reports directory.");
                
                // Create prompt with default value
                let subdir_prompt = match &params.subdir {
                    Some(subdir) => format!("Enter subdirectory (default: {})", subdir),
                    None => "Enter subdirectory (optional)".to_string(),
                };
                
                let subdir_initial = params.subdir.clone().unwrap_or_default();

                let subdir: String = Input::new()
                    .with_prompt(&subdir_prompt)
                    .with_initial_text(subdir_initial)
                    .allow_empty(true)
                    .interact_text()
                    .unwrap();
                
                // Save the subdir value back to params for future use
                params.subdir = if subdir.is_empty() {
                    params.subdir.clone()  // Keep the default if empty
                } else {
                    Some(subdir.clone())
                };
                
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
                                .with_prompt("Enter column name containing delivery numbers")
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
                                
                                // Read the delivery numbers from the Excel file
                                match read_excel_column(&excel_path, "Sheet1", &column_name) {
                                    Ok(delivery_numbers) => {
                                        if delivery_numbers.is_empty() {
                                            println!("No delivery numbers found in column '{}' of the Excel file.", column_name);
                                            
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
                                        } else {
                                            println!("Found {} delivery numbers in Excel file.", delivery_numbers.len());
                                            params.delivery_numbers = delivery_numbers;
                                            column_valid = true; // Exit the loop
                                        }
                                    },
                                    Err(e) => {
                                        println!("Error reading Excel file: {}", e);
                                        println!("Column '{}' may not exist in the Excel file.", column_name);
                                        
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
                    },
                    Err(e) => {
                        println!("Error selecting Excel file: {}", e);
                        println!("Error details: {}", e);
                        
                        // Wait for user to acknowledge before continuing
                        println!("Press Enter to continue...");
                        let mut input = String::new();
                        io::stdin().read_line(&mut input).unwrap();
                    }
                }
            },
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
    println!("Running VL06O Delivery Packages with params: {:#?}", params);
    println!("-------------------------------");

    Ok(params)
}

/// Clear the screen
fn clear_screen() {
    execute!(std::io::stdout(), Clear(ClearType::All)).unwrap();
}

/// Parse a date string into a NaiveDate
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
        },
        _ => { // Default to mm/dd/yyyy
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
