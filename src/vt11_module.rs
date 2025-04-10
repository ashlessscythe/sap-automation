use sap_scripting::*;
use std::io::{self, Write};
use windows::core::Result;
use chrono::NaiveDate;
use dialoguer::{Input, Select};
use crossterm::{execute, terminal::{Clear, ClearType}};

use crate::utils::*;
use crate::vt11::{VT11Params, run_export};

pub fn run_vt11_module(session: &GuiSession) -> Result<()> {
    clear_screen();
    println!("VT11 - Shipment List Planning");
    println!("============================");
    
    // Get parameters from user
    let params = get_vt11_parameters()?;
    
    // Run the export
    match run_export(session, &params) {
        Ok(true) => {
            println!("VT11 export completed successfully!");
        },
        Ok(false) => {
            println!("VT11 export failed or was cancelled.");
        },
        Err(e) => {
            println!("Error running VT11 export: {}", e);
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

fn get_vt11_parameters() -> Result<VT11Params> {
    let mut params = VT11Params::default();
    
    // Get start date
    let start_date_str: String = Input::new()
        .with_prompt("Start date (MM/DD/YYYY)")
        .default(chrono::Local::now().format("%m/%d/%Y").to_string())
        .interact_text()
        .unwrap();
    
    params.start_date = parse_date(&start_date_str).unwrap_or_else(|_| chrono::Local::now().date_naive());
    
    // Get end date
    let end_date_str: String = Input::new()
        .with_prompt("End date (MM/DD/YYYY)")
        .default(chrono::Local::now().format("%m/%d/%Y").to_string())
        .interact_text()
        .unwrap();
    
    params.end_date = parse_date(&end_date_str).unwrap_or_else(|_| chrono::Local::now().date_naive());
    
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
    
    // Get by_date option
    let by_date_options = vec!["Yes", "No"];
    let by_date_choice = Select::new()
        .with_prompt("Filter by date?")
        .items(&by_date_options)
        .default(0)
        .interact()
        .unwrap();
    
    params.by_date = by_date_choice == 0;
    
    // Get limiter option
    let limiter_options = vec!["None", "Delivery", "Date Range"];
    let limiter_choice = Select::new()
        .with_prompt("Select limiter type")
        .items(&limiter_options)
        .default(0)
        .interact()
        .unwrap();
    
    params.limiter = match limiter_choice {
        0 => None,
        1 => Some("delivery".to_string()),
        2 => Some("date_range".to_string()),
        _ => None,
    };
    
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
