use crossterm::{
    execute,
    terminal::{Clear, ClearType},
};
use dialoguer::{Input, Select};
use sap_scripting::*;
use std::collections::HashMap;
use std::io::{self, Write};
use windows::core::Result;

use crate::utils::config_types::SapConfig;
use crate::y_149::{run_export, Report149Params};

pub fn run_149_module(session: &GuiSession) -> Result<()> {
    clear_screen();
    println!("149 Report - y_dn3_47000149");
    println!("===========================");

    // Get parameters from user
    let params = get_149_parameters()?;

    // Run the export
    match run_export(session, &params) {
        Ok(true) => {
            println!("149 report export completed successfully!");
        }
        Ok(false) => {
            println!("149 report export failed or was cancelled.");
        }
        Err(e) => {
            println!("Error running 149 report export: {}", e);
        }
    }

    // Wait for user to press enter before returning to main menu
    println!("\nPress Enter to return to main menu...");
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();

    Ok(())
}

pub fn run_149_auto(session: &GuiSession) -> Result<()> {
    clear_screen();
    println!("149 Report - Auto Run from Configuration");
    println!("======================================");

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

    // Get 149 specific configuration
    let tcode_config = match config.get_tcode_config("y_dn3_47000149", Some(true)) {
        Some(cfg) => cfg,
        None => {
            println!("No configuration found for y_dn3_47000149.");
            println!("Please configure 149 report parameters first.");
            println!("\nPress Enter to return to main menu...");
            let mut input = String::new();
            io::stdin().read_line(&mut input).unwrap();
            return Ok(());
        }
    };

    // Create Report149Params from configuration
    let params = create_149_params_from_config(&config);

    // Determine file extension for display
    let file_extension = match params.export_type {
        0 | 1 => "txt",   // unconverted or text with tabs
        2 => "rtf",       // rich text format
        3 => "html",      // HTML format
        4 => "clipboard", // clipboard - no file
        _ => "txt",       // default to txt for unknown values
    };

    println!("Running 149 report with the following parameters:");
    println!("-----------------------------------------------");
    println!("Variant: {}", params.variant);
    println!("Plants: {:?}", params.plants);
    println!(
        "Export Type: {} ({})",
        params.export_type,
        get_export_type_description(params.export_type)
    );
    println!("File Extension: {}", file_extension);
    println!("-----------------------------------------------");

    // Run the export
    match run_export(session, &params) {
        Ok(true) => {
            println!("149 report export completed successfully!");
        }
        Ok(false) => {
            println!("149 report export failed or was cancelled.");
        }
        Err(e) => {
            println!("Error running 149 report export: {}", e);
        }
    }

    // no wait for user since this is auto
    Ok(())
}

fn create_149_params_from_config(config: &SapConfig) -> Report149Params {
    let mut params = Report149Params::default();

    // Get the tcode config for y_dn3_47000149
    if let Some(tcode_config) = config.get_tcode_config("y_dn3_47000149", Some(true)) {
        // Set variant if available
        if let Some(variant) = tcode_config.get("variant") {
            params.variant = variant.clone();
        }
        // get export_type from config - now using the proper field
        if let Some(export_type) = tcode_config.get("export_type") {
            if let Ok(v) = export_type.parse::<u8>() {
                params.export_type = v;
            }
        }
    }

    // If tcode didn't specify an export type, fall back to global default
    if params.export_type == 1 {
        if let Some(v) = config.get_effective_export_type("y_dn3_47000149") {
            params.export_type = v;
        }
    }

    // Set plants if available - we need to get this from the raw config since it's an array
    if let Some(raw_config) = &config.raw_config {
        if let Some(tcode_section) = raw_config.get("tcode") {
            if let Some(y_dn3_section) = tcode_section.get("y_dn3_47000149") {
                if let Some(plants_array) = y_dn3_section.get("plants") {
                    if let Some(plants_vec) = plants_array.as_array() {
                        params.plants = plants_vec
                            .iter()
                            .filter_map(|v| v.as_str())
                            .map(|s| s.to_string())
                            .collect();
                    }
                }
            }
        }
    }

    params
}

fn clear_screen() {
    execute!(std::io::stdout(), Clear(ClearType::All)).unwrap();
}

fn get_export_type_description(export_type: u8) -> &'static str {
    match export_type {
        0 => "unconverted",
        1 => "text with tabs",
        2 => "rich text format",
        3 => "HTML format",
        4 => "clipboard",
        _ => "unknown",
    }
}

fn get_149_parameters() -> Result<Report149Params> {
    let mut params = Report149Params::default();

    // Get variant name
    let variant_name: String = Input::new()
        .with_prompt("SAP variant name")
        .default("149_unload".to_string())
        .interact_text()
        .unwrap();

    params.variant = variant_name;

    // Get plants
    println!("Enter plant codes (one per line, empty line to finish):");
    let mut plants = Vec::new();

    loop {
        print!("Plant {}: ", plants.len() + 1);
        io::stdout().flush().unwrap();
        let mut plant = String::new();
        io::stdin().read_line(&mut plant).unwrap();
        let plant = plant.trim().to_string();

        if plant.is_empty() {
            break;
        }

        plants.push(plant);
    }

    if plants.is_empty() {
        println!("No plants specified. Using default plants from config.");
        // Try to get plants from config
        if let Ok(config) = SapConfig::load() {
            if let Some(tcode_config) = config.get_tcode_config("y_dn3_47000149", Some(true)) {
                if let Some(plants_str) = tcode_config.get("plants") {
                    // Parse plants from config string (assuming comma-separated)
                    params.plants = plants_str
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect();
                }
            }
        }
    } else {
        params.plants = plants;
    }

    // Get export type (will be overridden by config on auto run)
    let export_type: u8 = Input::new()
        .with_prompt(
            "Export type (0=unconverted, 1=text with tabs, 2=rich text, 3=HTML, 4=clipboard)",
        )
        .default(1)
        .interact_text()
        .unwrap();

    params.export_type = export_type;

    clear_screen();

    println!("-------------------------------");
    println!("Running 149 report with params: {:#?}", params);

    Ok(params)
}
