use anyhow::{anyhow, Result};
use dialoguer::{Input, Select};
use std::collections::HashMap;
use std::io;
use std::thread;
use std::time::Duration;

use crate::utils::config_types::SapConfig;
use crate::utils::config_types::*;

/// Handle configuring SAP automation parameters
pub fn handle_configure_sap_params() -> Result<()> {
    println!("Configure SAP Automation Parameters");
    println!("==================================");

    // Load current configuration
    let mut config = SapConfig::load()?;

    // Present options to the user
    let options = vec![
        "Configure Instance ID",
        "Configure Default TCode",
        "Configure TCode-specific Parameters",
        "Configure Loop Parameters",
        "Show Current Configuration",
        "Back to Main Menu",
    ];

    loop {
        let selection = Select::new()
            .with_prompt("Choose an option")
            .items(&options)
            .default(0)
            .interact()
            .unwrap();

        match selection {
            0 => {
                // Configure Instance ID
                let current = config.get_instance_id();
                let instance_id: String = Input::new()
                    .with_prompt("Enter Instance ID (default: rs)")
                    .allow_empty(false)
                    .default(current)
                    .interact()
                    .unwrap();

                config.set_instance_id(&instance_id);
                println!("Instance ID set to: {}", instance_id);
                println!("Note: This will use different credential files for each instance ID.");
            }
            1 => {
                // Configure Default TCode
                let current = config.global.as_ref()
                    .and_then(|g| g.default_tcode.clone())
                    .unwrap_or_default();
                
                let tcode: String = Input::new()
                    .with_prompt("Enter Default TCode (e.g., VT11, VL06O, ZMDESNR)")
                    .allow_empty(true)
                    .default(current)
                    .interact()
                    .unwrap();

                if tcode.is_empty() {
                    if let Some(global) = &mut config.global {
                        global.default_tcode = None;
                    }
                    println!("Default TCode configuration cleared.");
                } else {
                    if let Some(global) = &mut config.global {
                        global.default_tcode = Some(tcode.clone());
                    } else {
                        config.global = Some(GlobalConfig {
                            instance_id: config.get_instance_id(),
                            reports_dir: config.get_reports_dir(),
                            default_tcode: Some(tcode.clone()),
                            additional_params: HashMap::new(),
                        });
                    }
                    println!("Default TCode set to: {}", tcode);
                }
            }
            2 => {
                // Configure TCode-specific Parameters
                handle_configure_tcode_params(&mut config)?;
            }
            3 => {
                // Configure Loop Parameters
                handle_configure_loop_params(&mut config)?;
            }
            4 => {
                // Show Current Configuration
                show_current_configuration(&config);
                
                println!("\nPress Enter to continue...");
                let mut input = String::new();
                io::stdin().read_line(&mut input).unwrap();
                continue;
            }
            _ => {
                // Back to Main Menu
                break;
            }
        }

        // Save configuration after each change
        if let Err(e) = config.save() {
            eprintln!("Failed to save configuration: {}", e);
            thread::sleep(Duration::from_secs(2));
        } else {
            println!("Configuration saved successfully.");
            thread::sleep(Duration::from_secs(1));
        }
    }

    Ok(())
}

/// Handle configuring TCode-specific parameters
fn handle_configure_tcode_params(config: &mut SapConfig) -> Result<()> {
    println!("\nConfigure TCode-specific Parameters");
    println!("==================================");

    // Get list of available TCodes
    let mut tcode_names = vec![];
    
    // Clone the tcode map to avoid borrowing issues
    let tcode_configs = config.tcode.clone().unwrap_or_default();
    
    for tcode_name in tcode_configs.keys() {
        tcode_names.push(tcode_name.clone());
    }
    
    // Add option to add a new TCode
    tcode_names.push("Add New TCode".to_string());
    tcode_names.push("Back".to_string());

    let selection = Select::new()
        .with_prompt("Select TCode to configure")
        .items(&tcode_names)
        .default(0)
        .interact()
        .unwrap();

    if selection == tcode_names.len() - 1 {
        // User selected Back
        return Ok(());
    }

    let tcode_name = if selection == tcode_names.len() - 2 {
        // User selected Add New TCode
        let tcode: String = Input::new()
            .with_prompt("Enter TCode name (e.g., VT11, VL06O, ZMDESNR)")
            .allow_empty(false)
            .interact()
            .unwrap();
        
        // Ensure tcode map exists
        if config.tcode.is_none() {
            config.tcode = Some(HashMap::new());
        }
        
        // Add new TCode if it doesn't exist
        if let Some(tcode_configs) = &mut config.tcode {
            if !tcode_configs.contains_key(&tcode) {
                tcode_configs.insert(tcode.clone(), TcodeConfig::default());
            }
        }
        
        tcode
    } else {
        // User selected an existing TCode
        tcode_names[selection].clone()
    };

    // Configure TCode parameters
    configure_tcode_parameters(config, &tcode_name)?;

    Ok(())
}

/// Configure parameters for a specific TCode
fn configure_tcode_parameters(config: &mut SapConfig, tcode_name: &str) -> Result<()> {
    println!("\nConfiguring parameters for TCode: {}", tcode_name);

    // Ensure tcode map exists
    if config.tcode.is_none() {
        config.tcode = Some(HashMap::new());
    }
    
    // Ensure TCode config exists
    if let Some(tcode_configs) = &mut config.tcode {
        if !tcode_configs.contains_key(tcode_name) {
            tcode_configs.insert(tcode_name.to_string(), TcodeConfig::default());
        }
    }

    // Get a clone of the TCode config to avoid borrowing issues
    let mut tcode_config = if let Some(tcode_configs) = &config.tcode {
        tcode_configs.get(tcode_name).cloned().unwrap_or_default()
    } else {
        return Err(anyhow!("Failed to get TCode configuration"));
    };

    // Present options to the user
    let options = vec![
        "Configure Variant",
        "Configure Layout",
        "Configure Column Name",
        "Configure Date Range",
        "Configure By Date",
        "Configure Serial Number",
        "Configure Tab Number",
        "Add Custom Parameter",
        "Remove Parameter",
        "Delete This TCode Configuration",
        "Back",
    ];

    loop {
        let selection = Select::new()
            .with_prompt("Choose an option")
            .items(&options)
            .default(0)
            .interact()
            .unwrap();

        match selection {
            0 => {
                // Configure Variant
                let current = tcode_config.variant.clone().unwrap_or_default();
                let variant: String = Input::new()
                    .with_prompt("Enter SAP Variant Name")
                    .allow_empty(true)
                    .default(current)
                    .interact()
                    .unwrap();

                if variant.is_empty() {
                    tcode_config.variant = None;
                    println!("Variant configuration cleared.");
                } else {
                    tcode_config.variant = Some(variant.clone());
                    println!("Variant set to: {}", variant);
                }
            }
            1 => {
                // Configure Layout
                let current = tcode_config.layout.clone().unwrap_or_default();
                let layout: String = Input::new()
                    .with_prompt("Enter Layout Name")
                    .allow_empty(true)
                    .default(current)
                    .interact()
                    .unwrap();

                if layout.is_empty() {
                    tcode_config.layout = None;
                    println!("Layout configuration cleared.");
                } else {
                    tcode_config.layout = Some(layout.clone());
                    println!("Layout set to: {}", layout);
                }
            }
            2 => {
                // Configure Column Name
                let current = tcode_config.column_name.clone().unwrap_or_default();
                let column_name: String = Input::new()
                    .with_prompt("Enter Column Name")
                    .allow_empty(true)
                    .default(current)
                    .interact()
                    .unwrap();

                if column_name.is_empty() {
                    tcode_config.column_name = None;
                    println!("Column Name configuration cleared.");
                } else {
                    tcode_config.column_name = Some(column_name.clone());
                    println!("Column Name set to: {}", column_name);
                }
            }
            3 => {
                // Configure Date Range
                let current_start = tcode_config.date_range_start.clone().unwrap_or_default();
                let current_end = tcode_config.date_range_end.clone().unwrap_or_default();

                let start_date: String = Input::new()
                    .with_prompt("Enter Start Date (MM/DD/YYYY)")
                    .allow_empty(true)
                    .default(current_start)
                    .interact()
                    .unwrap();

                let end_date: String = Input::new()
                    .with_prompt("Enter End Date (MM/DD/YYYY)")
                    .allow_empty(true)
                    .default(current_end)
                    .interact()
                    .unwrap();

                if start_date.is_empty() || end_date.is_empty() {
                    tcode_config.date_range_start = None;
                    tcode_config.date_range_end = None;
                    println!("Date Range configuration cleared.");
                } else {
                    tcode_config.date_range_start = Some(start_date.clone());
                    tcode_config.date_range_end = Some(end_date.clone());
                    println!("Date Range set to: {} - {}", start_date, end_date);
                }
            }
            4 => {
                // Configure By Date
                let current = tcode_config.by_date.clone().unwrap_or_default();
                let by_date_options = vec!["true", "false"];
                let default_index = if current == "true" { 0 } else { 1 };
                
                let by_date_choice = Select::new()
                    .with_prompt("Filter by date?")
                    .items(&by_date_options)
                    .default(default_index)
                    .interact()
                    .unwrap();

                let by_date = by_date_options[by_date_choice].to_string();
                tcode_config.by_date = Some(by_date.clone());
                println!("By Date set to: {}", by_date);
            }
            5 => {
                // Configure Serial Number
                let current = tcode_config.serial_number.clone().unwrap_or_default();
                let serial_number: String = Input::new()
                    .with_prompt("Enter Serial Number")
                    .allow_empty(true)
                    .default(current)
                    .interact()
                    .unwrap();

                if serial_number.is_empty() {
                    tcode_config.serial_number = None;
                    println!("Serial Number configuration cleared.");
                } else {
                    tcode_config.serial_number = Some(serial_number.clone());
                    println!("Serial Number set to: {}", serial_number);
                }
            }
            6 => {
                // Configure Tab Number
                let current = tcode_config.tab_number.clone().unwrap_or_default();
                let tab_number: String = Input::new()
                    .with_prompt("Enter Tab Number")
                    .allow_empty(true)
                    .default(current)
                    .interact()
                    .unwrap();

                if tab_number.is_empty() {
                    tcode_config.tab_number = None;
                    println!("Tab Number configuration cleared.");
                } else {
                    tcode_config.tab_number = Some(tab_number.clone());
                    println!("Tab Number set to: {}", tab_number);
                }
            }
            7 => {
                // Add Custom Parameter
                let param_name: String = Input::new()
                    .with_prompt("Enter Parameter Name")
                    .allow_empty(false)
                    .interact()
                    .unwrap();

                let param_value: String = Input::new()
                    .with_prompt("Enter Parameter Value")
                    .allow_empty(true)
                    .interact()
                    .unwrap();

                if param_value.is_empty() {
                    tcode_config.additional_params.remove(&param_name);
                    println!("Parameter '{}' removed.", param_name);
                } else {
                    tcode_config.additional_params.insert(param_name.clone(), param_value.clone());
                    println!("Parameter '{}' set to: {}", param_name, param_value);
                }
            }
            8 => {
                // Remove Parameter
                let mut param_names: Vec<String> = Vec::new();

                // Add standard parameters if they exist
                if tcode_config.variant.is_some() {
                    param_names.push("variant".to_string());
                }
                if tcode_config.layout.is_some() {
                    param_names.push("layout".to_string());
                }
                if tcode_config.column_name.is_some() {
                    param_names.push("column_name".to_string());
                }
                if tcode_config.date_range_start.is_some() {
                    param_names.push("date_range".to_string());
                }
                if tcode_config.by_date.is_some() {
                    param_names.push("by_date".to_string());
                }
                if tcode_config.serial_number.is_some() {
                    param_names.push("serial_number".to_string());
                }
                if tcode_config.tab_number.is_some() {
                    param_names.push("tab_number".to_string());
                }

                // Add additional parameters
                for key in tcode_config.additional_params.keys() {
                    param_names.push(key.clone());
                }

                if param_names.is_empty() {
                    println!("No parameters to remove.");
                    thread::sleep(Duration::from_secs(2));
                    continue;
                }

                param_names.push("Cancel".to_string());

                let selection = Select::new()
                    .with_prompt("Select parameter to remove")
                    .items(&param_names)
                    .default(0)
                    .interact()
                    .unwrap();

                if selection == param_names.len() - 1 {
                    // User selected Cancel
                    continue;
                }

                let param_name = &param_names[selection];

                match param_name.as_str() {
                    "variant" => {
                        tcode_config.variant = None;
                        println!("Variant configuration cleared.");
                    }
                    "layout" => {
                        tcode_config.layout = None;
                        println!("Layout configuration cleared.");
                    }
                    "column_name" => {
                        tcode_config.column_name = None;
                        println!("Column Name configuration cleared.");
                    }
                    "date_range" => {
                        tcode_config.date_range_start = None;
                        tcode_config.date_range_end = None;
                        println!("Date Range configuration cleared.");
                    }
                    "by_date" => {
                        tcode_config.by_date = None;
                        println!("By Date configuration cleared.");
                    }
                    "serial_number" => {
                        tcode_config.serial_number = None;
                        println!("Serial Number configuration cleared.");
                    }
                    "tab_number" => {
                        tcode_config.tab_number = None;
                        println!("Tab Number configuration cleared.");
                    }
                    _ => {
                        tcode_config.additional_params.remove(param_name);
                        println!("Parameter '{}' removed.", param_name);
                    }
                }
            }
            9 => {
                // Delete This TCode Configuration
                println!("Are you sure you want to delete the configuration for TCode '{}'? (y/n)", tcode_name);
                let mut confirm = String::new();
                io::stdin().read_line(&mut confirm).unwrap();
                
                if confirm.trim().to_lowercase() == "y" {
                    if let Some(tcode_configs) = &mut config.tcode {
                        tcode_configs.remove(tcode_name);
                        println!("TCode '{}' configuration deleted.", tcode_name);
                        
                        // Save configuration
                        if let Err(e) = config.save() {
                            eprintln!("Failed to save configuration: {}", e);
                            thread::sleep(Duration::from_secs(2));
                        } else {
                            println!("Configuration saved successfully.");
                            thread::sleep(Duration::from_secs(1));
                        }
                        
                        return Ok(());
                    }
                } else {
                    println!("Deletion cancelled.");
                }
            }
            _ => {
                // Back
                // Update the tcode config in the main config
                if let Some(tcode_configs) = &mut config.tcode {
                    tcode_configs.insert(tcode_name.to_string(), tcode_config);
                }
                return Ok(());
            }
        }

        // Save configuration after each change
        // Update the tcode config in the main config
        if let Some(tcode_configs) = &mut config.tcode {
            tcode_configs.insert(tcode_name.to_string(), tcode_config.clone());
        }
        
        if let Err(e) = config.save() {
            eprintln!("Failed to save configuration: {}", e);
            thread::sleep(Duration::from_secs(2));
        } else {
            println!("Configuration saved successfully.");
            thread::sleep(Duration::from_secs(1));
        }
    }
}

/// Handle configuring loop parameters
fn handle_configure_loop_params(config: &mut SapConfig) -> Result<()> {
    println!("\nConfigure Loop Parameters");
    println!("========================");

    // Create loop config if it doesn't exist
    if config.loop_config.is_none() {
        config.loop_config = Some(LoopConfig {
            tcode: "".to_string(),
            iterations: default_iterations(),
            delay_seconds: default_delay_seconds(),
            params: HashMap::new(),
        });
    }

    // Get a clone of the loop config to avoid borrowing issues
    let mut loop_config = config.loop_config.clone().unwrap_or(LoopConfig {
        tcode: "".to_string(),
        iterations: default_iterations(),
        delay_seconds: default_delay_seconds(),
        params: HashMap::new(),
    });

    // Present options to the user
    let options = vec![
        "Configure TCode",
        "Configure Iterations",
        "Configure Delay (seconds)",
        "Add/Edit Parameter",
        "Remove Parameter",
        "Show Current Configuration",
        "Back",
    ];

    loop {
        let selection = Select::new()
            .with_prompt("Choose an option")
            .items(&options)
            .default(0)
            .interact()
            .unwrap();

        match selection {
            0 => {
                // Configure TCode
                let current = loop_config.tcode.clone();
                let tcode: String = Input::new()
                    .with_prompt("Enter TCode (e.g., VT11, VL06O, ZMDESNR)")
                    .allow_empty(true)
                    .default(current)
                    .interact()
                    .unwrap();

                loop_config.tcode = tcode.clone();
                println!("TCode set to: {}", tcode);
            }
            1 => {
                // Configure Iterations
                let current = loop_config.iterations.clone();
                let iterations_str: String = Input::new()
                    .with_prompt("Enter number of iterations")
                    .allow_empty(false)
                    .default(current)
                    .interact()
                    .unwrap();

                loop_config.iterations = iterations_str.clone();
                println!("Iterations set to: {}", iterations_str);
            }
            2 => {
                // Configure Delay
                let current = loop_config.delay_seconds.clone();
                let delay_str: String = Input::new()
                    .with_prompt("Enter delay between iterations (seconds)")
                    .allow_empty(false)
                    .default(current)
                    .interact()
                    .unwrap();

                loop_config.delay_seconds = delay_str.clone();
                println!("Delay set to: {} seconds", delay_str);
            }
            3 => {
                // Add/Edit Parameter
                let param_name: String = Input::new()
                    .with_prompt("Enter Parameter Name")
                    .allow_empty(false)
                    .interact()
                    .unwrap();

                let current_value = loop_config.params.get(&param_name).cloned().unwrap_or_default();
                let param_value: String = Input::new()
                    .with_prompt("Enter Parameter Value")
                    .allow_empty(true)
                    .default(current_value)
                    .interact()
                    .unwrap();

                if param_value.is_empty() {
                    loop_config.params.remove(&param_name);
                    println!("Parameter '{}' removed.", param_name);
                } else {
                    loop_config.params.insert(param_name.clone(), param_value.clone());
                    println!("Parameter '{}' set to: {}", param_name, param_value);
                }
            }
            4 => {
                // Remove Parameter
                let mut param_names: Vec<String> = Vec::new();

                // Add parameters
                for key in loop_config.params.keys() {
                    param_names.push(key.clone());
                }

                if param_names.is_empty() {
                    println!("No parameters to remove.");
                    thread::sleep(Duration::from_secs(2));
                    continue;
                }

                param_names.push("Cancel".to_string());

                let selection = Select::new()
                    .with_prompt("Select parameter to remove")
                    .items(&param_names)
                    .default(0)
                    .interact()
                    .unwrap();

                if selection == param_names.len() - 1 {
                    // User selected Cancel
                    continue;
                }

                let param_name = &param_names[selection];
                loop_config.params.remove(param_name);
                println!("Parameter '{}' removed.", param_name);
            }
            5 => {
                // Show Current Configuration
                println!("\nCurrent Loop Configuration:");
                println!("---------------------------");
                println!("TCode: {}", loop_config.tcode);
                println!("Iterations: {}", loop_config.iterations);
                println!("Delay: {} seconds", loop_config.delay_seconds);

                if !loop_config.params.is_empty() {
                    println!("\nParameters:");
                    for (key, value) in &loop_config.params {
                        println!("  {}: {}", key, value);
                    }
                }

                println!("\nPress Enter to continue...");
                let mut input = String::new();
                io::stdin().read_line(&mut input).unwrap();
                continue;
            }
            _ => {
                // Back
                // Update the loop config in the main config
                config.loop_config = Some(loop_config);
                return Ok(());
            }
        }

        // Save configuration after each change
        // Update the loop config in the main config
        config.loop_config = Some(loop_config.clone());
        
        if let Err(e) = config.save() {
            eprintln!("Failed to save configuration: {}", e);
            thread::sleep(Duration::from_secs(2));
        } else {
            println!("Configuration saved successfully.");
            thread::sleep(Duration::from_secs(1));
        }
    }
}

/// Show the current configuration
fn show_current_configuration(config: &SapConfig) {
    println!("\nCurrent Configuration:");
    println!("---------------------");
    
    // Show global configuration
    if let Some(global) = &config.global {
        println!("Instance ID: {}", global.instance_id);
        println!("Reports Directory: {}", global.reports_dir);
        
        if let Some(default_tcode) = &global.default_tcode {
            println!("Default TCode: {}", default_tcode);
        }
        
        if !global.additional_params.is_empty() {
            println!("\nGlobal Parameters:");
            for (key, value) in &global.additional_params {
                println!("  {}: {}", key, value);
            }
        }
    }
    
    // Show TCode configurations
    if let Some(tcode_configs) = &config.tcode {
        if !tcode_configs.is_empty() {
            println!("\nTCode Configurations:");
            
            for (tcode_name, tcode_config) in tcode_configs {
                println!("\n  {}:", tcode_name);
                
                if let Some(variant) = &tcode_config.variant {
                    println!("    Variant: {}", variant);
                }
                
                if let Some(layout) = &tcode_config.layout {
                    println!("    Layout: {}", layout);
                }
                
                if let Some(column_name) = &tcode_config.column_name {
                    println!("    Column Name: {}", column_name);
                }
                
                if let Some(date_range_start) = &tcode_config.date_range_start {
                    if let Some(date_range_end) = &tcode_config.date_range_end {
                        println!("    Date Range: {} - {}", date_range_start, date_range_end);
                    }
                }
                
                if let Some(by_date) = &tcode_config.by_date {
                    println!("    By Date: {}", by_date);
                }
                
                if let Some(serial_number) = &tcode_config.serial_number {
                    println!("    Serial Number: {}", serial_number);
                }
                
                if let Some(tab_number) = &tcode_config.tab_number {
                    println!("    Tab Number: {}", tab_number);
                }
                
                if !tcode_config.additional_params.is_empty() {
                    println!("    Additional Parameters:");
                    for (key, value) in &tcode_config.additional_params {
                        println!("      {}: {}", key, value);
                    }
                }
            }
        }
    }
    
    // Show Loop configuration
    if let Some(loop_config) = &config.loop_config {
        println!("\nLoop Configuration:");
        println!("  TCode: {}", loop_config.tcode);
        println!("  Iterations: {}", loop_config.iterations);
        println!("  Delay: {} seconds", loop_config.delay_seconds);
        
        if !loop_config.params.is_empty() {
            println!("  Parameters:");
            for (key, value) in &loop_config.params {
                println!("    {}: {}", key, value);
            }
        }
    }
}
