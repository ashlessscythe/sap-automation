use crossterm::{
    execute,
    terminal::{Clear, ClearType},
};
use dialoguer::Input;
use sap_scripting::*;
use std::collections::HashMap;
use std::io::{self};
use windows::core::Result;

use crate::lx03::{run_export, LX03Params};
use crate::utils::config_types::SapConfig;

pub fn run_lx03_module(session: &GuiSession) -> Result<()> {
    clear_screen();
    println!("LX03 - Bin Status Report");
    println!("========================");

    let params = get_lx03_parameters()?;

    match run_export(session, &params) {
        Ok(true) => println!("LX03 export completed successfully!"),
        Ok(false) => println!("LX03 export failed or was cancelled."),
        Err(e) => println!("Error running LX03 export: {}", e),
    }

    println!("\nPress Enter to return to main menu...");
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();

    Ok(())
}

pub fn run_lx03_auto(session: &GuiSession) -> Result<()> {
    clear_screen();
    println!("LX03 - Auto Run from Configuration");
    println!("==================================");

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

    let tcode_config = match config.get_tcode_config("LX03", Some(true)) {
        Some(cfg) => cfg,
        None => {
            println!("No configuration found for LX03.");
            println!("Please configure LX03 parameters first.");
            println!("\nPress Enter to return to main menu...");
            let mut input = String::new();
            io::stdin().read_line(&mut input).unwrap();
            return Ok(());
        }
    };

    let params = create_lx03_params_from_config(&tcode_config);

    println!("Running LX03 with the following parameters:");
    println!("-------------------------------------------");
    println!("Variant: {:?}", params.sap_variant_name);
    println!("Layout: {:?}", params.layout_row);
    println!("Export Type: {:?}", params.export_type);
    println!("-------------------------------------------");

    match run_export(session, &params) {
        Ok(true) => println!("LX03 export completed successfully!"),
        Ok(false) => println!("LX03 export failed or was cancelled."),
        Err(e) => println!("Error running LX03 export: {}", e),
    }

    Ok(())
}

pub fn create_lx03_params_from_config(config: &HashMap<String, String>) -> LX03Params {
    let mut params = LX03Params::default();

    if let Some(variant) = config.get("variant") {
        params.sap_variant_name = Some(variant.clone());
    }

    if let Some(layout) = config.get("layout") {
        params.layout_row = Some(layout.clone());
    }

    if let Some(export_type) = config.get("export_type") {
        if let Ok(export_type) = export_type.parse::<u8>() {
            params.export_type = Some(export_type);
        }
    }

    params
}

fn clear_screen() {
    execute!(std::io::stdout(), Clear(ClearType::All)).unwrap();
}

fn get_lx03_parameters() -> Result<LX03Params> {
    let mut params = LX03Params::default();

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

    let layout_row: String = Input::new()
        .with_prompt("Layout search string (leave empty for default)")
        .allow_empty(true)
        .interact_text()
        .unwrap();

    params.layout_row = if layout_row.is_empty() {
        None
    } else {
        Some(layout_row)
    };

    let export_type: u8 = Input::new()
        .with_prompt("Export type (0=unconverted, 1=text with tabs, 2=rich text, 3=HTML, 4=clipboard)")
        .default(1)
        .interact_text()
        .unwrap();

    params.export_type = Some(export_type);

    clear_screen();
    println!("-------------------------------");
    println!("Running LX03 with params: {:#?}", params);
    println!("-------------------------------");

    Ok(params)
}
