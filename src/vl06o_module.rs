use sap_scripting::*;
use std::io::{self};
use windows::core::Result;
use dialoguer::*;
use crossterm::{execute, terminal::{Clear, ClearType}};

use crate::vl06o::{VL06OParams, run_export};
use crate::utils::excel_file_ops::read_excel_column;
use crate::utils::excel_path_utils::get_newest_file;
use crate::utils::sap_file_utils::get_reports_dir;

pub fn run_vl06o_module(session: &GuiSession) -> Result<()> {
    clear_screen();
    println!("VL06O - List of Outbound Deliveries");
    println!("===================================");
    
    // Get parameters from user
    let params = get_vl06o_parameters()?;
    
    // Run the export
    match run_export(session, &params) {
        Ok(true) => {
            println!("VL06O export completed successfully!");
        },
        Ok(false) => {
            println!("VL06O export failed or was cancelled.");
        },
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

fn clear_screen() {
    execute!(std::io::stdout(), Clear(ClearType::All)).unwrap();
}

fn get_vl06o_parameters() -> Result<VL06OParams> {
    let mut params = VL06OParams::default();
    
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
    
    // Get shipment numbers from Excel file
    let shipment_numbers = get_shipment_numbers_from_excel()?;
    params.shipment_numbers = shipment_numbers;
    
    clear_screen();

    println!("-------------------------------");
    println!("Running VL06O with params: {:#?}", params);
    println!("-------------------------------");
    
    Ok(params)
}

fn get_shipment_numbers_from_excel() -> Result<Vec<String>> {
    let reports_dir = format!("{}\\{}", get_reports_dir(), "VT11");
    
    // Get the newest Excel file in the reports directory
    let newest_file = get_newest_file(&reports_dir, "xlsx")?;
    
    if newest_file.is_empty() {
        println!("No Excel files found in reports directory: {}", reports_dir);
        return Ok(Vec::new());
    }
    
    println!("Using newest Excel file: {}", newest_file);
    
    // Ask user for sheet name and column header
    let sheet_name: String = Input::new()
        .with_prompt("Sheet name containing shipment numbers")
        .default("Sheet1".to_string())
        .interact_text()
        .unwrap();
    
    let column_header: String = Input::new()
        .with_prompt("Column header for shipment numbers")
        .default("Shipment Number".to_string())
        .interact_text()
        .unwrap();
    
    // Read the shipment numbers from the Excel file
    let shipment_numbers = read_excel_column(&newest_file, &sheet_name, &column_header)?;
    
    println!("Found {} shipment numbers", shipment_numbers.len());
    
    // If there are too many shipment numbers, ask user to limit
    let max_display = 10;
    if shipment_numbers.len() > max_display {
        println!("First {} shipment numbers:", max_display);
        for (i, number) in shipment_numbers.iter().take(max_display).enumerate() {
            println!("  {}. {}", i + 1, number);
        }
        println!("  ... and {} more", shipment_numbers.len() - max_display);
    } else if !shipment_numbers.is_empty() {
        println!("Shipment numbers:");
        for (i, number) in shipment_numbers.iter().enumerate() {
            println!("  {}. {}", i + 1, number);
        }
    } else {
        println!("No shipment numbers found in the specified column.");
        
        // Ask user to enter shipment numbers manually
        let manual_input: String = Input::new()
            .with_prompt("Enter shipment numbers manually (comma-separated)")
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
    
    Ok(shipment_numbers)
}
