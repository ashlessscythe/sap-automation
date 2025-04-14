use sap_scripting::*;
use std::io::{self};
use windows::core::Result;
use dialoguer::*;
use crossterm::{execute, terminal::{Clear, ClearType}};

use crate::zmdesnr::{ZMDESNRParams, run_export};
use crate::utils::excel_file_ops::read_excel_column;
use crate::utils::excel_path_utils::get_newest_file;
use crate::utils::sap_file_utils::get_reports_dir;

pub fn run_zmdesnr_module(session: &GuiSession) -> Result<()> {
    clear_screen();
    println!("ZMDESNR - Serial Number History");
    println!("==============================");
    
    // Get parameters from user
    let params = get_zmdesnr_parameters()?;
    
    // Run the export
    match run_export(session, &params) {
        Ok(true) => {
            println!("ZMDESNR export completed successfully!");
        },
        Ok(false) => {
            println!("ZMDESNR export failed or was cancelled.");
        },
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

fn clear_screen() {
    execute!(std::io::stdout(), Clear(ClearType::All)).unwrap();
}

fn get_zmdesnr_parameters() -> Result<ZMDESNRParams> {
    let mut params = ZMDESNRParams::default();
    
    // Get variant name
    let variant_name: String = Input::new()
        .with_prompt("SAP variant name (leave empty for none)")
        .allow_empty(true)
        .interact_text()
        .unwrap();
    
    params.sap_variant_name = if variant_name.is_empty() { None } else { Some(variant_name) };
    
    // Get layout row
    let layout_row: String = Input::new()
        .with_prompt("Layout row (leave empty for default)")
        .allow_empty(true)
        .interact_text()
        .unwrap();
    
    params.layout_row = if layout_row.is_empty() { None } else { Some(layout_row) };
    
    // Get delivery numbers from Excel file
    let delivery_numbers = get_delivery_numbers_from_excel()?;
    params.delivery_numbers = delivery_numbers;
    
    // Ask if user wants to exclude serials
    let exclude_options = vec!["No", "Yes"];
    let exclude_choice = Select::new()
        .with_prompt("Do you want to exclude serials?")
        .items(&exclude_options)
        .default(0)
        .interact()
        .unwrap();
    
    if exclude_choice == 1 {
        // User wants to exclude serials
        let exclude_serials = get_exclude_serials_from_excel()?;
        params.exclude_serials = Some(exclude_serials);
    }
    
    clear_screen();

    println!("-------------------------------");
    println!("Running ZMDESNR with params: {:#?}", params);
    println!("-------------------------------");
    
    Ok(params)
}

fn get_delivery_numbers_from_excel() -> Result<Vec<String>> {
    let reports_dir = format!("{}\\{}", get_reports_dir(), "VL06O");
    
    // Get the newest Excel file in the reports directory
    let newest_file = get_newest_file(&reports_dir, "xlsx")?;
    
    if newest_file.is_empty() {
        println!("No Excel files found in reports directory: {}", reports_dir);
        return Ok(Vec::new());
    }
    
    println!("Using newest Excel file: {}", newest_file);
    
    // Ask user for sheet name and column header
    let sheet_name: String = Input::new()
        .with_prompt("Sheet name containing delivery numbers")
        .default("Sheet1".to_string())
        .interact_text()
        .unwrap();
    
    let column_header: String = Input::new()
        .with_prompt("Column header for delivery numbers")
        .default("Delivery".to_string())
        .interact_text()
        .unwrap();
    
    // Read the delivery numbers from the Excel file
    let delivery_numbers = read_excel_column(&newest_file, &sheet_name, &column_header)?;
    
    println!("Found {} delivery numbers", delivery_numbers.len());
    
    // If there are too many delivery numbers, ask user to limit
    let max_display = 10;
    if delivery_numbers.len() > max_display {
        println!("First {} delivery numbers:", max_display);
        for (i, number) in delivery_numbers.iter().take(max_display).enumerate() {
            println!("  {}. {}", i + 1, number);
        }
        println!("  ... and {} more", delivery_numbers.len() - max_display);
    } else if !delivery_numbers.is_empty() {
        println!("Delivery numbers:");
        for (i, number) in delivery_numbers.iter().enumerate() {
            println!("  {}. {}", i + 1, number);
        }
    } else {
        println!("No delivery numbers found in the specified column.");
        
        // Ask user to enter delivery numbers manually
        let manual_input: String = Input::new()
            .with_prompt("Enter delivery numbers manually (comma-separated)")
            .allow_empty(true)
            .interact_text()
            .unwrap();
        
        if !manual_input.is_empty() {
            return Ok(manual_input.split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect());
        }
    }
    
    Ok(delivery_numbers)
}

fn get_exclude_serials_from_excel() -> Result<Vec<String>> {
    // Ask user if they want to use an Excel file for exclude serials
    let options = vec!["Use Excel file", "Enter manually"];
    let choice = Select::new()
        .with_prompt("How do you want to provide exclude serials?")
        .items(&options)
        .default(0)
        .interact()
        .unwrap();
    
    if choice == 0 {
        // User wants to use an Excel file
        let reports_dir = get_reports_dir();
        
        // Ask for Excel file path or directory
        let path_input: String = Input::new()
            .with_prompt("Enter Excel file path or directory (leave empty to use reports directory)")
            .allow_empty(true)
            .interact_text()
            .unwrap();
        
        // Use reports directory if input is empty
        let path_or_dir = if path_input.is_empty() {
            reports_dir
        } else {
            path_input
        };
        
        // Get the newest Excel file in the directory
        let newest_file = get_newest_file(&path_or_dir, "xlsx")?;
        
        if newest_file.is_empty() {
            println!("No Excel files found in directory: {}", path_or_dir);
            return Ok(Vec::new());
        }
        
        println!("Using Excel file: {}", newest_file);
        
        // Ask user for sheet name and column header
        let sheet_name: String = Input::new()
            .with_prompt("Sheet name containing exclude serials")
            .default("Sheet1".to_string())
            .interact_text()
            .unwrap();
        
        let column_header: String = Input::new()
            .with_prompt("Column header for exclude serials")
            .default("Serial Number".to_string())
            .interact_text()
            .unwrap();
        
        // Read the exclude serials from the Excel file
        let exclude_serials = read_excel_column(&newest_file, &sheet_name, &column_header)?;
        
        println!("Found {} exclude serials", exclude_serials.len());
        
        // If there are too many exclude serials, ask user to limit
        let max_display = 10;
        if exclude_serials.len() > max_display {
            println!("First {} exclude serials:", max_display);
            for (i, serial) in exclude_serials.iter().take(max_display).enumerate() {
                println!("  {}. {}", i + 1, serial);
            }
            println!("  ... and {} more", exclude_serials.len() - max_display);
        } else if !exclude_serials.is_empty() {
            println!("Exclude serials:");
            for (i, serial) in exclude_serials.iter().enumerate() {
                println!("  {}. {}", i + 1, serial);
            }
        } else {
            println!("No exclude serials found in the specified column.");
        }
        
        Ok(exclude_serials)
    } else {
        // User wants to enter exclude serials manually
        let manual_input: String = Input::new()
            .with_prompt("Enter exclude serials manually (comma-separated)")
            .allow_empty(true)
            .interact_text()
            .unwrap();
        
        if !manual_input.is_empty() {
            let serials = manual_input.split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            
            Ok(serials)
        } else {
            Ok(Vec::new())
        }
    }
}
