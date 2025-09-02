use chrono::{Duration as ChronoDuration, Local};
use crossterm::{
    execute,
    terminal::{Clear, ClearType},
};
use dialoguer::Input;
use sap_scripting::*;
use std::collections::HashMap;
use std::io::{self};
use windows::core::Result;

use crate::utils::config_types::SapConfig;
use crate::y_149_material::{run_export, Report149MaterialParams};

pub fn run_149_material_module(session: &GuiSession) -> Result<()> {
    clear_screen();
    println!("149 Report - Material Not TSP");
    println!("=============================");

    // Get parameters from user
    let params = get_149_material_parameters()?;

    // Run the export
    match run_export(session, &params) {
        Ok(true) => {
            println!("149 material report export completed successfully!");
        }
        Ok(false) => {
            println!("149 material report export failed or was cancelled.");
        }
        Err(e) => {
            println!("Error running 149 material report export: {}", e);
        }
    }

    // Wait for user to press enter before returning to main menu
    println!("\nPress Enter to return to main menu...");
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();

    Ok(())
}

pub fn run_149_material_auto(session: &GuiSession) -> Result<()> {
    clear_screen();
    println!("149 Report - Material Not TSP Auto Run from Configuration");
    println!("=====================================================");

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

    // Get Y_149 Material specific configuration
    let tcode_config = match config.get_tcode_config("y_dn3_47000149", Some(true)) {
        Some(cfg) => cfg,
        None => {
            println!("No configuration found for y_dn3_47000149.");
            println!("Please configure y_dn3_47000149 parameters first.");
            println!("\nPress Enter to return to main menu...");
            let mut input = String::new();
            io::stdin().read_line(&mut input).unwrap();
            return Ok(());
        }
    };

    // Create Report149MaterialParams from configuration
    println!("Getting 149 Material params from config");
    let mut params = create_149_material_params_from_config(&tcode_config);

    // Set date range if available (149_days_range) - access raw config directly
    if let Some(raw_config) = &config.raw_config {
        if let Some(tcode_section) = raw_config.get("tcode") {
            if let Some(y_dn3_section) = tcode_section.get("y_dn3_47000149") {
                if let Some(days_range) = y_dn3_section.get("149_days_range") {
                    if let Some(days) = days_range.as_integer() {
                        let today = Local::now().naive_local().date();
                        let start_date = today - ChronoDuration::days(days);
                        let end_date = today + ChronoDuration::days(1); // Tomorrow for material

                        params.date_low = config.format_date(start_date);
                        params.date_high = config.format_date(end_date);
                        println!(
                            "Using date range from config: {} to {}",
                            params.date_low, params.date_high
                        );
                    }
                }
            }
        }
    }

    // If no date range was set from config, set default dates
    if params.date_low.is_empty() || params.date_high.is_empty() {
        let today = Local::now().naive_local().date();
        params.date_low = config.format_date(today);
        params.date_high = config.format_date(today + ChronoDuration::days(1));
        println!(
            "Using default date range: {} to {}",
            params.date_low, params.date_high
        );
    }

    println!("Running 149 Material with the following parameters:");
    println!("-------------------------------------------");
    println!("Variant: {:?}", params.variant);
    println!("Material: {}", params.material);
    println!("Plant: {}", params.plant);
    println!("Signi: {}", params.signi);
    println!("Date Range: {} to {}", params.date_low, params.date_high);
    println!(
        "Export Type: {} ({})",
        params.export_type,
        get_export_type_description(params.export_type)
    );
    println!("-------------------------------------------");

    // Run the export
    match run_export(session, &params) {
        Ok(true) => {
            println!("149 Material report export completed successfully!");
        }
        Ok(false) => {
            println!("149 Material report export failed or was cancelled.");
        }
        Err(e) => {
            println!("Error running 149 Material report export: {}", e);
        }
    }

    Ok(())
}

fn create_149_material_params_from_config(
    config: &HashMap<String, String>,
) -> Report149MaterialParams {
    let mut params = Report149MaterialParams::default();

    // Display the default values loaded from config
    println!("Default values from config:");
    println!("  Variant: {:?}", params.variant);
    println!("  Layout: {:?}", params.layout);
    println!("  Export Type: {:?}", params.export_type);

    // Set variant if available (mat_variant)
    if let Some(variant) = config.get("mat_variant") {
        params.variant = Some(variant.clone());
    }

    // Set layout if available (mat_layout)
    if let Some(layout) = config.get("mat_layout") {
        params.layout = Some(layout.clone());
    }

    // Set export type if available
    if let Some(export_type) = config.get("export_type") {
        if let Ok(export_type_val) = export_type.parse::<u8>() {
            params.export_type = export_type_val;
        }
    }

    // Set material if available
    if let Some(material) = config.get("material") {
        params.material = material.clone();
    } else {
        // Default material if not specified
        params.material = "".to_string();
    }

    // Set plant if available (required when no variant)
    if params.variant.is_none() {
        if let Some(plant) = config.get("plant") {
            params.plant = plant.clone();
        } else {
            // If no variant and no plant in config, use a default
            params.plant = "BRUH".to_string();
        }
    } else {
        // Set empty plant when variant is used
        params.plant = String::new();
    }

    // Set signi if available (required when no variant)
    if params.variant.is_none() {
        if let Some(signi) = config.get("signi") {
            params.signi = signi.clone();
        } else {
            // If no variant and no signi in config, use a default
            params.signi = "".to_string();
        }
    } else {
        // Set empty signi when variant is used
        params.signi = String::new();
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

fn config_has_export_type() -> bool {
    if let Ok(config) = SapConfig::load() {
        if let Some(tcode_config) = config.get_tcode_config("y_dn3_47000149", Some(true)) {
            return tcode_config.get("export_type").is_some();
        }
    }
    false
}

fn get_149_material_parameters() -> Result<Report149MaterialParams> {
    let mut params = Report149MaterialParams::default();

    // Check if we have a variant in config
    if let Ok(config) = SapConfig::load() {
        if let Some(tcode_config) = config.get_tcode_config("y_dn3_47000149", Some(true)) {
            if let Some(variant) = tcode_config.get("mat_variant") {
                params.variant = Some(variant.clone());
                println!("Using variant from config: {}", variant);
            }
            // get export_type from config - now using the proper field
            if let Some(export_type) = tcode_config.get("export_type") {
                params.export_type = export_type.clone().parse::<u8>().unwrap();
                println!(
                    "Using export type from config: {} ({})",
                    export_type,
                    get_export_type_description(export_type.parse::<u8>().unwrap())
                );
            }
        }
    }

    // Get material number
    let material: String = Input::new()
        .with_prompt("Material number (required)")
        .allow_empty(false)
        .interact_text()
        .unwrap();

    params.material = material;

    // Only get plant and signi if no variant exists
    if params.variant.is_none() {
        // Get plant
        let plant: String = Input::new()
            .with_prompt("Plant code (optional)")
            .allow_empty(true)
            .interact_text()
            .unwrap();

        params.plant = plant;

        // Get signi (default to "")
        let signi: String = Input::new()
            .with_prompt("Significance/Trailer (optional)")
            .default("".to_string())
            .allow_empty(true)
            .interact_text()
            .unwrap();

        params.signi = signi;
    } else {
        // Set empty values when variant is used
        params.plant = String::new();
        params.signi = String::new();
    }

    // Check if we have date range in config
    let mut use_config_dates = false;
    if let Ok(config) = SapConfig::load() {
        if let Some(raw_config) = &config.raw_config {
            if let Some(tcode_section) = raw_config.get("tcode") {
                if let Some(y_dn3_section) = tcode_section.get("y_dn3_47000149") {
                    if let Some(days_range) = y_dn3_section.get("149_days_range") {
                        if let Some(days) = days_range.as_integer() {
                            let today = Local::now().naive_local().date();
                            let start_date = today - ChronoDuration::days(days);
                            let end_date = today + ChronoDuration::days(1);

                            // Load config to get date format
                            if let Ok(config) = SapConfig::load() {
                                params.date_low = config.format_date(start_date);
                                params.date_high = config.format_date(end_date);
                            } else {
                                // Fallback to ISO format if config can't be loaded
                                params.date_low = start_date.format("%Y-%m-%d").to_string();
                                params.date_high = end_date.format("%Y-%m-%d").to_string();
                            }
                            use_config_dates = true;
                            println!(
                                "Using date range from config: {} to {}",
                                params.date_low, params.date_high
                            );
                        }
                    }
                }
            }
        }
    }

    if !use_config_dates {
        // Get number of days back
        let days_back: i64 = Input::new()
            .with_prompt("How many days back from today?")
            .interact_text()
            .unwrap();

        // Calculate start date by subtracting days from today
        let today = Local::now().naive_local().date();
        let start_date = today - ChronoDuration::days(days_back);
        let end_date = today + ChronoDuration::days(1); // Tomorrow

        // Load config to get date format
        if let Ok(config) = SapConfig::load() {
            params.date_low = config.format_date(start_date);
            params.date_high = config.format_date(end_date);
        } else {
            // Fallback to ISO format if config can't be loaded
            params.date_low = start_date.format("%Y-%m-%d").to_string();
            params.date_high = end_date.format("%Y-%m-%d").to_string();
        }

        println!("Date range: {} to {}", params.date_low, params.date_high);
    }

    // Get export type if not provided by config
    if params.export_type == 1 && !config_has_export_type() {
        let export_type: u8 = Input::new()
            .with_prompt(
                "Export type (0=unconverted, 1=text with tabs, 2=rich text, 3=HTML, 4=clipboard)",
            )
            .default(1)
            .interact_text()
            .unwrap();

        params.export_type = export_type;
    }

    clear_screen();

    println!("----------------------------------------");
    println!("Running 149 material report with params: {:#?}", params);
    println!("----------------------------------------");

    Ok(params)
}
