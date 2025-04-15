use chrono::NaiveDate;
use crossterm::{
    execute,
    terminal::{Clear, ClearType},
};
use dialoguer::{Input, Select};
use sap_scripting::*;
use std::collections::HashMap;
use std::io::{self};
use windows::core::Result;

use crate::utils::config_ops::SapConfig;
use crate::vt11::{run_export, VT11Params};

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
        }
        Ok(false) => {
            println!("VT11 export failed or was cancelled.");
        }
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

pub fn run_vt11_auto(session: &GuiSession) -> Result<()> {
    clear_screen();
    println!("VT11 - Auto Run from Configuration");
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

    // Get VT11 specific configuration
    let tcode_config = match config.get_tcode_config("VT11") {
        Some(cfg) => cfg,
        None => {
            println!("No configuration found for VT11.");
            println!("Please configure VT11 parameters first.");
            println!("\nPress Enter to return to main menu...");
            let mut input = String::new();
            io::stdin().read_line(&mut input).unwrap();
            return Ok(());
        }
    };

    // Create VT11Params from configuration
    let params = create_vt11_params_from_config(&tcode_config);

    println!("Running VT11 with the following parameters:");
    println!("------------------------------------------");
    println!("Variant: {:?}", params.sap_variant_name);
    println!("Layout: {:?}", params.layout_row);
    println!(
        "Date Range: {} to {}",
        params.start_date.format("%m/%d/%Y"),
        params.end_date.format("%m/%d/%Y")
    );
    println!("Filter by Date: {}", params.by_date);
    println!("Limiter: {:?}", params.limiter);
    println!("------------------------------------------");

    // Run the export
    match run_export(session, &params) {
        Ok(true) => {
            println!("VT11 export completed successfully!");
        }
        Ok(false) => {
            println!("VT11 export failed or was cancelled.");
        }
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

fn create_vt11_params_from_config(config: &HashMap<String, String>) -> VT11Params {
    let mut params = VT11Params::default();

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

    // Set limiter if available
    if let Some(limiter) = config.get("limiter") {
        params.limiter = Some(limiter.clone());
    }

    params
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

    clear_screen();

    println!("-------------------------------");
    println!("Running VT11 with params: {:#?}", params);
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
