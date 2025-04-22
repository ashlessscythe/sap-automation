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

use crate::utils::{config_ops::get_reports_dir, excel_path_utils::resolve_path};
use crate::utils::config_types::SapConfig;
use crate::utils::excel_file_ops::read_excel_column;
use crate::utils::excel_path_utils::{get_excel_file_path, get_newest_file};
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
    let tcode_config = match config.get_tcode_config("VL06O", Some(true)) {
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
    let format_str = if date_format.to_lowercase() == "yyyy-mm-dd" { "%Y-%m-%d" } else { "%m/%d/%Y" };
    
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
    let format_str = if date_format.to_lowercase() == "yyyy-mm-dd" { "%Y-%m-%d" } else { "%m/%d/%Y" };
    
    // Confirm with user
    println!("Starting date update for {} deliveries", params.delivery_numbers.len());
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
                    println!("Delivery: {}, Original Date: {} -> New Date: {}", 
                             delivery, original_date, params.target_date.format(format_str));
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

    // If column name is provided, ask how to input shipment numbers
    if let Some(col_name) = &params.column_name {
        println!("Column name provided: {}", col_name);

        // Ask how to input shipment numbers
        let input_options = vec!["Read from Excel file", "Enter manually", "Paste from clipboard"];
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
            },
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
            },
            0 => {
                // Read from Excel file
                println!("Select an Excel file containing shipment numbers:");
                
                // Get the reports directory as the default starting point
                let reports_dir = get_reports_dir();
                println!("Current reports directory: {}", reports_dir);
                
                // Ask if user wants to use a subdirectory
                println!("You can enter a subdirectory name to navigate to a specific folder.");
                println!("For example, entering 'subpath' will navigate to {}\\subpath", reports_dir);
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
                                            println!("Found {} shipment numbers in Excel file.", shipment_numbers.len());
                                            params.shipment_numbers = shipment_numbers;
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
    println!("Running VL06O with params: {:#?}", params);
    println!("-------------------------------");

    Ok(params)
}

fn get_vl06o_date_update_parameters() -> Result<VL06ODateUpdateParams> {
    let mut params = VL06ODateUpdateParams::default();

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
    
    // Get target date
    let target_date_str: String = Input::new()
        .with_prompt(format!("Target date ({})", prompt_format))
        .default(chrono::Local::now().date_naive().succ().format(format_str).to_string())
        .interact_text()
        .unwrap();

    params.target_date =
        parse_date(&target_date_str).unwrap_or_else(|_| chrono::Local::now().date_naive().succ());

    // Get variant name
    let variant_name: String = Input::new()
        .with_prompt("SAP variant name (leave empty for default 'blank_')")
        .default("blank_".to_string())
        .allow_empty(true)
        .interact_text()
        .unwrap();

    params.sap_variant_name = if variant_name.is_empty() {
        Some("blank_".to_string())
    } else {
        Some(variant_name)
    };

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
