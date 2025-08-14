use chrono::{Duration as ChronoDuration, Local};
use crossterm::{
    execute,
    terminal::{Clear, ClearType},
};
use dialoguer::Input;
use sap_scripting::*;
use std::io::{self};
use windows::core::Result;

use crate::utils::config_types::SapConfig;
use crate::y_149_rcv::{run_export, Report149RcvParams};

pub fn run_149_rcv_module(session: &GuiSession) -> Result<()> {
    clear_screen();
    println!("149 Report - RCV");
    println!("================");

    // Get parameters from user
    let params = get_149_rcv_parameters()?;

    // Run the export
    match run_export(session, &params) {
        Ok(true) => {
            println!("149 RCV report export completed successfully!");
        }
        Ok(false) => {
            println!("149 RCV report export failed or was cancelled.");
        }
        Err(e) => {
            println!("Error running 149 RCV report export: {}", e);
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

fn get_149_rcv_parameters() -> Result<Report149RcvParams> {
    let mut params = Report149RcvParams::default();

    // Check if we have a variant in config
    if let Ok(config) = SapConfig::load() {
        if let Some(tcode_config) = config.get_tcode_config("y_dn3_47000149", Some(true)) {
            if let Some(variant) = tcode_config.get("rcv_variant") {
                params.variant = Some(variant.clone());
                println!("Using RCV variant from config: {}", variant);
            }
            // get export_type from config
            if let Some(export_type) = tcode_config.get("export_type") {
                params.export_type = export_type.clone().parse::<u8>().unwrap();
                println!(
                    "Using export type from config: {} ({})",
                    export_type,
                    get_export_type_description(export_type.parse::<u8>().unwrap())
                );
            }
            // get rcv_layout from config
            if let Some(layout) = tcode_config.get("rcv_layout") {
                params.layout = Some(layout.clone());
                println!("Using RCV layout from config: {}", layout);
            }
        }
    }

    // Only get plant if no variant exists
    if params.variant.is_none() {
        // Get plant (required when no variant)
        let plant: String = Input::new()
            .with_prompt("Plant code (required)")
            .allow_empty(false)
            .interact_text()
            .unwrap();

        params.plant = plant;
    } else {
        // Set empty plant when variant is used
        params.plant = String::new();
    }

    // Check if we have date range in config
    let mut use_config_dates = false;
    if let Ok(config) = SapConfig::load() {
        if let Some(raw_config) = &config.raw_config {
            if let Some(tcode_section) = raw_config.get("tcode") {
                if let Some(y_dn3_section) = tcode_section.get("y_dn3_47000149") {
                    if let Some(rcv_days_back) = y_dn3_section.get("rcv_days_back") {
                        if let Some(days) = rcv_days_back.as_integer() {
                            let today = Local::now().naive_local().date();
                            let start_date = today - ChronoDuration::days(days);
                            let end_date = today;

                            params.date_low = start_date.format("%Y-%m-%d").to_string();
                            params.date_high = end_date.format("%Y-%m-%d").to_string();
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
        let end_date = today; // Today (not tomorrow like material)

        params.date_low = start_date.format("%Y-%m-%d").to_string();
        params.date_high = end_date.format("%Y-%m-%d").to_string();

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
    println!("Running 149 RCV report with params: {:#?}", params);
    println!("----------------------------------------");

    Ok(params)
}
