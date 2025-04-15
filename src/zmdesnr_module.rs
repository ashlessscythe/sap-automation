use crossterm::{
    execute,
    terminal::{Clear, ClearType},
};
use dialoguer::Input;
use sap_scripting::*;
use std::collections::HashMap;
use std::io::{self};
use windows::core::Result;

use crate::utils::config_ops::SapConfig;
use crate::zmdesnr::{run_export, ZMDESNRParams};

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
    let tcode_config = match config.get_tcode_config("ZMDESNR") {
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
    let params = create_zmdesnr_params_from_config(&tcode_config);

    println!("Running ZMDESNR with the following parameters:");
    println!("--------------------------------------------");
    println!("Variant: {:?}", params.sap_variant_name);
    println!("Layout: {:?}", params.layout_row);
    println!("Serial Number: {:?}", params.serial_number);
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

    // Wait for user to press enter before returning to main menu
    println!("\nPress Enter to return to main menu...");
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();

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

    // Set serial_number if available
    if let Some(serial_number) = config.get("serial_number") {
        params.serial_number = Some(serial_number.clone());
    }

    params
}

fn clear_screen() {
    execute!(std::io::stdout(), Clear(ClearType::All)).unwrap();
}

fn get_zmdesnr_parameters() -> Result<ZMDESNRParams> {
    let mut params = ZMDESNRParams::default();

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
