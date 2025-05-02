use anyhow::{anyhow, Result};
use dialoguer::{Input, Select};
use sap_scripting::*;
use std::collections::HashMap;
use std::io;
use std::thread;
use std::time::Duration;

use crate::utils::config_types::SapConfig;
use crate::utils::config_types::{SequenceConfig as ConfigSequenceConfig, default_iterations, default_delay_seconds, default_interval_seconds};
use crate::vl06o_delivery_module::run_vl06o_delivery_packages_auto;
use crate::zmdesnr_module::run_zmdesnr_auto;

/// Structure to map menu options to their names and functions
#[derive(Debug, Clone)]
pub struct MenuOption {
    pub id: String,
    pub name: String,
}

/// Get available menu options for sequences
pub fn get_available_menu_options() -> Vec<MenuOption> {
    vec![
        MenuOption {
            id: "9".to_string(),
            name: "ZMDESNR - Auto Run".to_string(),
        },
        MenuOption {
            id: "7".to_string(),
            name: "VL06O - Auto Run Delivery Packages".to_string(),
        },
    ]
}

/// Get menu option name by ID
pub fn get_menu_option_name(id: &str) -> String {
    for option in get_available_menu_options() {
        if option.id == id {
            return option.name;
        }
    }
    format!("Unknown option: {}", id)
}

/// Execute a menu option by ID
pub fn execute_menu_option(session: &GuiSession, id: &str) -> Result<()> {
    match id {
        "9" => {
            println!("Running ZMDESNR Auto...");
            run_zmdesnr_auto(session)?;
        },
        "7" => {
            println!("Running VL06O Delivery Packages Auto...");
            run_vl06o_delivery_packages_auto(session)?;
        },
        _ => {
            println!("Unknown option: {}", id);
        }
    }
    Ok(())
}

/// Structure to hold sequence configuration
#[derive(Debug, Clone)]
pub struct SequenceConfig {
    pub options: Vec<String>,
    pub iterations: usize,
    pub delay_seconds: u64,
    pub interval_seconds: u64,
    pub params: HashMap<String, String>,
}

impl Default for SequenceConfig {
    fn default() -> Self {
        Self {
            options: Vec::new(),
            iterations: 1,
            delay_seconds: 60,
            interval_seconds: 10,
            params: HashMap::new(),
        }
    }
}

impl SequenceConfig {
    /// Create a new sequence configuration with default values
    pub fn new() -> Self {
        Self::default()
    }

    /// Load sequence configuration from config.toml file
    pub fn load() -> Result<Self> {
        let mut config = Self::default();
        
        // Try to read from config file via SapConfig
        if let Ok(sap_config) = SapConfig::load() {
            // Check if sequence configuration exists
            if let Some(sequence_config) = &sap_config.sequence {
                // Get options
                config.options = sequence_config.options.clone();
                
                // Get iterations
                if let Ok(iter_val) = sequence_config.iterations.parse::<usize>() {
                    config.iterations = iter_val;
                }
                
                // Get delay seconds
                if let Ok(delay_val) = sequence_config.delay_seconds.parse::<u64>() {
                    config.delay_seconds = delay_val;
                }
                
                // Get interval seconds
                if let Ok(interval_val) = sequence_config.interval_seconds.parse::<u64>() {
                    config.interval_seconds = interval_val;
                }
                
                // Get parameters
                for (key, value) in &sequence_config.params {
                    // If key starts with param_, remove it to get the actual parameter name
                    if key.starts_with("param_") {
                        let param_name = key.replacen("param_", "", 1);
                        config.params.insert(param_name, value.clone());
                    } else {
                        // Otherwise, use the key as is
                        config.params.insert(key.clone(), value.clone());
                    }
                }
            }
        }
        
        Ok(config)
    }
    
    /// Save sequence configuration to config.toml file
    pub fn save(&self) -> Result<()> {
        let mut sap_config = SapConfig::load()?;
        
        // Create or update sequence configuration
        let mut sequence_params = HashMap::new();
        
        // Add parameters with param_ prefix
        for (key, value) in &self.params {
            sequence_params.insert(format!("param_{}", key), value.clone());
        }
        
        let sequence_config = ConfigSequenceConfig {
            options: self.options.clone(),
            iterations: self.iterations.to_string(),
            delay_seconds: self.delay_seconds.to_string(),
            interval_seconds: self.interval_seconds.to_string(),
            params: sequence_params,
        };
        
        sap_config.sequence = Some(sequence_config);
        
        // Save the updated configuration
        sap_config.save()?;
        
        Ok(())
    }
}

/// Handle configuring sequence parameters
pub fn handle_configure_sequence() -> Result<()> {
    println!("Configure Sequence Parameters");
    println!("============================");
    
    // Load current configuration
    let mut config = SequenceConfig::load()?;
    
    // Present options to the user
    let options = vec![
        "Configure Sequence Options",
        "Configure Iterations",
        "Configure Delay (seconds)",
        "Configure Interval (seconds)",
        "Add/Edit Parameter",
        "Remove Parameter",
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
                // Configure Sequence Options
                println!("Available options:");
                
                let available_options = get_available_menu_options();
                for option in &available_options {
                    println!("{} - {}", option.id, option.name);
                }
                
                // Create a selection menu for options
                let mut selected_options: Vec<String> = Vec::new();
                
                loop {
                    // Show current selection
                    if !selected_options.is_empty() {
                        println!("\nCurrently selected options:");
                        for (i, option_id) in selected_options.iter().enumerate() {
                            println!("{}. {}", i + 1, get_menu_option_name(option_id));
                        }
                    }
                    
                    // Create options for selection menu
                    let mut menu_options = Vec::new();
                    for option in &available_options {
                        menu_options.push(format!("{} - {}", option.id, option.name));
                    }
                    menu_options.push("Done selecting options".to_string());
                    
                    let selection = Select::new()
                        .with_prompt("Select an option to add to the sequence (or 'Done' when finished)")
                        .items(&menu_options)
                        .default(0)
                        .interact()
                        .unwrap();
                        
                    if selection == menu_options.len() - 1 {
                        // User selected "Done"
                        break;
                    } else {
                        // User selected an option, add it to the list
                        let option_id = available_options[selection].id.clone();
                        selected_options.push(option_id);
                    }
                }
                
                if !selected_options.is_empty() {
                    config.options = selected_options;
                    println!("\nSequence options set to:");
                    for (i, option_id) in config.options.iter().enumerate() {
                        println!("{}. {}", i + 1, get_menu_option_name(option_id));
                    }
                } else {
                    println!("\nNo options selected. Keeping current options.");
                }
            },
            1 => {
                // Configure Iterations
                let current = config.iterations.to_string();
                let iterations_str: String = Input::new()
                    .with_prompt("Enter number of iterations (0 for infinite until Ctrl+C)")
                    .allow_empty(false)
                    .default(current)
                    .interact()
                    .unwrap();
                    
                if let Ok(iterations) = iterations_str.parse::<usize>() {
                    config.iterations = iterations;
                    if iterations == 0 {
                        println!("Iterations set to: infinite (until Ctrl+C)");
                    } else {
                        println!("Iterations set to: {}", iterations);
                    }
                } else {
                    println!("Invalid number. Keeping current value: {}", config.iterations);
                }
            },
            2 => {
                // Configure Delay
                let current = config.delay_seconds.to_string();
                let delay_str: String = Input::new()
                    .with_prompt("Enter delay between iterations (seconds)")
                    .allow_empty(false)
                    .default(current)
                    .interact()
                    .unwrap();
                    
                if let Ok(delay) = delay_str.parse::<u64>() {
                    config.delay_seconds = delay;
                    println!("Delay set to: {} seconds", delay);
                } else {
                    println!("Invalid number. Keeping current value: {} seconds", config.delay_seconds);
                }
            },
            3 => {
                // Configure Interval
                let current = config.interval_seconds.to_string();
                let interval_str: String = Input::new()
                    .with_prompt("Enter interval between sequence steps (seconds)")
                    .allow_empty(false)
                    .default(current)
                    .interact()
                    .unwrap();
                    
                if let Ok(interval) = interval_str.parse::<u64>() {
                    config.interval_seconds = interval;
                    println!("Interval set to: {} seconds", interval);
                } else {
                    println!("Invalid number. Keeping current value: {} seconds", config.interval_seconds);
                }
            },
            4 => {
                // Add/Edit Parameter
                let param_name: String = Input::new()
                    .with_prompt("Enter Parameter Name")
                    .allow_empty(false)
                    .interact()
                    .unwrap();
                    
                let current_value = config.params.get(&param_name).cloned().unwrap_or_default();
                let param_value: String = Input::new()
                    .with_prompt("Enter Parameter Value")
                    .allow_empty(true)
                    .default(current_value)
                    .interact()
                    .unwrap();
                    
                if param_value.is_empty() {
                    config.params.remove(&param_name);
                    println!("Parameter '{}' removed.", param_name);
                } else {
                    config.params.insert(param_name.clone(), param_value.clone());
                    println!("Parameter '{}' set to: {}", param_name, param_value);
                }
            },
            5 => {
                // Remove Parameter
                let mut param_names: Vec<String> = Vec::new();
                
                // Add parameters
                for key in config.params.keys() {
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
                config.params.remove(param_name);
                println!("Parameter '{}' removed.", param_name);
            },
            6 => {
                // Show Current Configuration
                println!("\nCurrent Sequence Configuration:");
                println!("-------------------------------");
                println!("Options:");
                if config.options.is_empty() {
                    println!("  No options configured");
                } else {
                    for (i, option_id) in config.options.iter().enumerate() {
                        println!("  {}. {}", i + 1, get_menu_option_name(option_id));
                    }
                }
                
                if config.iterations == 0 {
                    println!("Iterations: infinite (until Ctrl+C)");
                } else {
                    println!("Iterations: {}", config.iterations);
                }
                println!("Delay: {} seconds", config.delay_seconds);
                println!("Interval: {} seconds", config.interval_seconds);
                
                if !config.params.is_empty() {
                    println!("\nParameters:");
                    for (key, value) in &config.params {
                        println!("  {}: {}", key, value);
                    }
                }
                
                println!("\nPress Enter to continue...");
                let mut input = String::new();
                io::stdin().read_line(&mut input).unwrap();
                continue;
            },
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

/// Run a sequence of operations with the specified configuration
pub fn run_sequence(session: &GuiSession) -> Result<()> {
    println!("Run Sequence from Configuration");
    println!("==============================");
    
    // Load sequence configuration
    let config = match SequenceConfig::load() {
        Ok(cfg) => cfg,
        Err(e) => {
            println!("Error loading sequence configuration: {}", e);
            println!("\nPress Enter to return to main menu...");
            let mut input = String::new();
            io::stdin().read_line(&mut input).unwrap();
            return Ok(());
        }
    };
    
    // Check if sequence options are configured
    if config.options.is_empty() {
        println!("No sequence options configured.");
        println!("Please configure sequence parameters first.");
        println!("\nPress Enter to return to main menu...");
        let mut input = String::new();
        io::stdin().read_line(&mut input).unwrap();
        return Ok(());
    }
    
    println!("Running sequence with the following configuration:");
    println!("Options:");
    if config.options.is_empty() {
        println!("  No options configured");
    } else {
        for (i, option_id) in config.options.iter().enumerate() {
            println!("  {}. {}", i + 1, get_menu_option_name(option_id));
        }
    }
    
    if config.iterations == 0 {
        println!("Iterations: infinite (until Ctrl+C)");
    } else {
        println!("Iterations: {}", config.iterations);
    }
    println!("Delay between iterations: {} seconds", config.delay_seconds);
    println!("Interval between steps: {} seconds", config.interval_seconds);
    
    if !config.params.is_empty() {
        println!("\nParameters:");
        for (key, value) in &config.params {
            println!("  {}: {}", key, value);
        }
    }
    
    println!("\nPress Enter to start the sequence or Ctrl+C to cancel...");
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();
    
    // Run the sequence in a loop
    let mut iteration = 1;
    loop {
        // Display iteration information
        if config.iterations == 0 {
            println!("\nIteration {} (infinite loop, press Ctrl+C to stop)", iteration);
        } else {
            println!("\nIteration {}/{}", iteration, config.iterations);
        }
        
        // Run each step in the sequence
        for (step_index, option) in config.options.iter().enumerate() {
            println!("\nRunning step {} of {}: Option {}", step_index + 1, config.options.len(), option);
            
            // Execute the selected option
            println!("Running: {}", get_menu_option_name(option));
            if let Err(e) = execute_menu_option(session, option) {
                eprintln!("Error executing option: {}", e);
            }
            
            // If this is not the last step, wait for the interval
            if step_index < config.options.len() - 1 {
                println!("Waiting {} seconds before next step...", config.interval_seconds);
                thread::sleep(Duration::from_secs(config.interval_seconds));
            }
        }
        
        // Check if we should continue the loop
        if config.iterations > 0 && iteration >= config.iterations {
            break;
        }
        
        // Increment iteration counter
        iteration += 1;
        
        // Wait for the specified delay before the next iteration
        println!("Waiting {} seconds before next iteration...", config.delay_seconds);
        thread::sleep(Duration::from_secs(config.delay_seconds));
    }
    
    println!("\nSequence execution completed.");
    println!("\nPress Enter to return to main menu...");
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();
    
    Ok(())
}
