use chrono::NaiveDate;
use dialoguer::{Input, Select};
use sap_scripting::*;
use std::collections::HashMap;
use windows::core::Result;

use crate::utils::config_types::SapConfig;
use crate::zvt11::{run_export, ZVT11Params};

/// Run ZVT11 module with user input parameters
pub fn run_zvt11_module(session: &GuiSession) -> Result<()> {
    clear_screen();
    println!("ZVT11 - Shipment Report");

    // Get parameters from user
    let params = get_zvt11_parameters()?;

    // Run the export
    match run_export(session, &params) {
        Ok(true) => {
            println!("ZVT11 export completed successfully!");
        }
        Ok(false) => {
            println!("ZVT11 export failed or was cancelled.");
        }
        Err(e) => {
            eprintln!("Error running ZVT11 export: {}", e);
        }
    }

    println!("\nPress Enter to continue...");
    let mut input = String::new();
    std::io::stdin().read_line(&mut input).unwrap();

    Ok(())
}

/// Run ZVT11 Auto module using configuration
pub fn run_zvt11_auto(session: &GuiSession) -> Result<()> {
    clear_screen();
    println!("ZVT11 - Auto Run from Configuration");

    // Load configuration
    let config = match SapConfig::load() {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("Error loading configuration: {}", e);
            println!("Please configure ZVT11 parameters first.");
            println!("\nPress Enter to continue...");
            let mut input = String::new();
            std::io::stdin().read_line(&mut input).unwrap();
            return Ok(());
        }
    };

    // Get ZVT11 specific configuration
    let tcode_config = match config.get_tcode_config("ZVT11", Some(true)) {
        Some(cfg) => cfg,
        None => {
            println!("No configuration found for ZVT11.");
            println!("Please configure ZVT11 parameters first.");
            println!("\nPress Enter to continue...");
            let mut input = String::new();
            std::io::stdin().read_line(&mut input).unwrap();
            return Ok(());
        }
    };

    // Create ZVT11Params from configuration
    let params = create_zvt11_params_from_config(&tcode_config);

    println!("Running ZVT11 with the following parameters:");
    println!("  Start Date: {}", params.start_date.format("%m/%d/%Y"));
    println!("  End Date: {}", params.end_date.format("%m/%d/%Y"));
    println!("  Variant: {:?}", params.sap_variant_name);
    println!("  Layout: {:?}", params.layout_row);
    println!("  By Date: {}", params.by_date);
    println!("  By Delivery: {}", params.by_delivery);
    println!("  Limiter: {:?}", params.limiter);

    println!("\nPress Enter to start export...");
    let mut input = String::new();
    std::io::stdin().read_line(&mut input).unwrap();

    // Run the export
    match run_export(session, &params) {
        Ok(true) => {
            println!("ZVT11 export completed successfully!");
        }
        Ok(false) => {
            println!("ZVT11 export failed or was cancelled.");
        }
        Err(e) => {
            eprintln!("Error running ZVT11 export: {}", e);
        }
    }

    println!("\nPress Enter to continue...");
    let mut input = String::new();
    std::io::stdin().read_line(&mut input).unwrap();

    Ok(())
}

/// Create ZVT11Params from configuration HashMap
fn create_zvt11_params_from_config(config: &HashMap<String, String>) -> ZVT11Params {
    let mut params = ZVT11Params::default();

    // Parse dates if provided
    if let Some(start_date_str) = config.get("date_range_start") {
        if let Ok(date) = NaiveDate::parse_from_str(start_date_str, "%Y-%m-%d") {
            params.start_date = date;
        }
    }

    if let Some(end_date_str) = config.get("date_range_end") {
        if let Ok(date) = NaiveDate::parse_from_str(end_date_str, "%Y-%m-%d") {
            params.end_date = date;
        }
    }

    // Set variant if provided
    if let Some(variant) = config.get("variant") {
        params.sap_variant_name = Some(variant.clone());
    }

    // Set layout if provided
    if let Some(layout) = config.get("layout") {
        params.layout_row = Some(layout.clone());
    }

    // Set by_date flag
    if let Some(by_date_str) = config.get("by_date") {
        params.by_date = by_date_str.parse().unwrap_or(false);
    }

    // Set by_delivery flag
    if let Some(by_delivery_str) = config.get("by_delivery") {
        params.by_delivery = by_delivery_str.parse().unwrap_or(false);
    }

    // Set limiter if provided
    if let Some(limiter) = config.get("limiter") {
        params.limiter = Some(limiter.clone());
    }

    params
}

/// Get ZVT11 parameters from user input
fn get_zvt11_parameters() -> Result<ZVT11Params> {
    let mut params = ZVT11Params::default();

    println!("Enter ZVT11 Export Parameters:");
    println!("===============================");

    // Get start date
    let start_date_input: String = Input::new()
        .with_prompt("Start Date (MM/DD/YYYY) or press Enter for today")
        .allow_empty(true)
        .interact_text()
        .unwrap();

    if !start_date_input.trim().is_empty() {
        if let Ok(date) = NaiveDate::parse_from_str(&start_date_input, "%m/%d/%Y") {
            params.start_date = date;
        } else {
            println!("Invalid date format. Using today's date.");
        }
    }

    // Get end date
    let end_date_input: String = Input::new()
        .with_prompt("End Date (MM/DD/YYYY) or press Enter for same as start date")
        .allow_empty(true)
        .interact_text()
        .unwrap();

    if !end_date_input.trim().is_empty() {
        if let Ok(date) = NaiveDate::parse_from_str(&end_date_input, "%m/%d/%Y") {
            params.end_date = date;
        } else {
            println!("Invalid date format. Using start date.");
            params.end_date = params.start_date;
        }
    } else {
        params.end_date = params.start_date;
    }

    // Get variant name
    let variant_input: String = Input::new()
        .with_prompt("SAP Variant Name (or press Enter for none)")
        .allow_empty(true)
        .interact_text()
        .unwrap();

    if !variant_input.trim().is_empty() {
        params.sap_variant_name = Some(variant_input);
    }

    // Get layout row
    let layout_input: String = Input::new()
        .with_prompt("Layout Row (or press Enter for none)")
        .allow_empty(true)
        .interact_text()
        .unwrap();

    if !layout_input.trim().is_empty() {
        params.layout_row = Some(layout_input);
    }

    // Get filter type
    let filter_options = vec!["By Date", "By Delivery", "Both"];
    let filter_choice = Select::new()
        .with_prompt("Select filter type")
        .items(&filter_options)
        .default(0)
        .interact()
        .unwrap();

    match filter_choice {
        0 => {
            params.by_date = true;
            params.by_delivery = false;
        }
        1 => {
            params.by_date = false;
            params.by_delivery = true;
        }
        2 => {
            params.by_date = true;
            params.by_delivery = true;
        }
        _ => unreachable!(),
    }

    // Get limiter if needed
    if params.by_date {
        let limiter_input: String = Input::new()
            .with_prompt("Limiter (date_range, or press Enter for none)")
            .allow_empty(true)
            .interact_text()
            .unwrap();

        if !limiter_input.trim().is_empty() {
            params.limiter = Some(limiter_input);
        }
    }

    println!("\nZVT11 Parameters:");
    println!("  Start Date: {}", params.start_date.format("%m/%d/%Y"));
    println!("  End Date: {}", params.end_date.format("%m/%d/%Y"));
    println!("  Variant: {:?}", params.sap_variant_name);
    println!("  Layout: {:?}", params.layout_row);
    println!("  By Date: {}", params.by_date);
    println!("  By Delivery: {}", params.by_delivery);
    println!("  Limiter: {:?}", params.limiter);

    println!("\nRunning ZVT11 with params: {:#?}", params);

    Ok(params)
}

/// Clear the screen
fn clear_screen() {
    print!("\x1B[2J\x1B[1;1H");
}
