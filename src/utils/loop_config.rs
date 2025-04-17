use anyhow::{anyhow, Result};
use dialoguer::{Input, Select};
use sap_scripting::*;
use std::collections::HashMap;
use std::io;
use std::thread;
use std::time::Duration;

use crate::utils::config_types::SapConfig;
use crate::utils::config_types::{LoopConfig as ConfigLoopConfig, default_iterations, default_delay_seconds};
use crate::utils::sap_tcode_utils::{assert_tcode, check_tcode, variant_select};
use crate::vl06o_module::run_vl06o_auto;
use crate::vt11_module::run_vt11_auto;
use crate::zmdesnr_module::run_zmdesnr_auto;

/// Structure to hold loop configuration
#[derive(Debug, Clone)]
pub struct LoopConfig {
    pub tcode: String,
    pub iterations: usize,
    pub delay_seconds: u64,
    pub params: HashMap<String, String>,
}

impl Default for LoopConfig {
    fn default() -> Self {
        Self {
            tcode: String::new(),
            iterations: 1,
            delay_seconds: 60,
            params: HashMap::new(),
        }
    }
}

impl LoopConfig {
    /// Create a new loop configuration with default values
    pub fn new() -> Self {
        Self::default()
    }

    /// Load loop configuration from config.toml file
    pub fn load() -> Result<Self> {
        let mut config = Self::default();
        
        // Try to read from config file via SapConfig
        if let Ok(sap_config) = SapConfig::load() {
            // Check if loop configuration exists in the new format
            if let Some(loop_config) = &sap_config.loop_config {
                // Get tcode
                config.tcode = loop_config.tcode.clone();
                
                // Get iterations
                if let Ok(iter_val) = loop_config.iterations.parse::<usize>() {
                    config.iterations = iter_val;
                }
                
                // Get delay seconds
                if let Ok(delay_val) = loop_config.delay_seconds.parse::<u64>() {
                    config.delay_seconds = delay_val;
                }
                
                // Get parameters
                for (key, value) in &loop_config.params {
                    config.params.insert(key.clone(), value.clone());
                }
            } else {
                // Fall back to legacy format
                // First, try to get tcode from the main config's default tcode
                if let Some(global) = &sap_config.global {
                    if let Some(tcode) = &global.default_tcode {
                        config.tcode = tcode.clone();
                    }
                }
                
                // Check if there are any loop-related parameters in the global additional_params
                if let Some(global) = &sap_config.global {
                    for (key, value) in &global.additional_params {
                        if key == "loop_tcode" {
                            config.tcode = value.clone();
                        } else if key == "loop_iterations" {
                            if let Ok(iter_val) = value.parse::<usize>() {
                                config.iterations = iter_val;
                            }
                        } else if key == "loop_delay_seconds" {
                            if let Ok(delay_val) = value.parse::<u64>() {
                                config.delay_seconds = delay_val;
                            }
                        } else if key.starts_with("loop_param_") {
                            let param_name = key.replacen("loop_param_", "", 1);
                            config.params.insert(param_name, value.clone());
                        }
                    }
                }
            }
        }
        
        Ok(config)
    }
    
    /// Save loop configuration to config.toml file
    pub fn save(&self) -> Result<()> {
        let mut sap_config = SapConfig::load()?;
        
        // Create or update loop configuration
        let loop_config = ConfigLoopConfig {
            tcode: self.tcode.clone(),
            iterations: self.iterations.to_string(),
            delay_seconds: self.delay_seconds.to_string(),
            params: self.params.clone(),
        };
        
        sap_config.loop_config = Some(loop_config);
        
        // Save the updated configuration
        sap_config.save()?;
        
        Ok(())
    }
}

/// Handle configuring loop parameters
pub fn handle_configure_loop() -> Result<()> {
    println!("Configure Loop Parameters");
    println!("========================");
    
    // Load current configuration
    let mut config = LoopConfig::load()?;
    
    // Present options to the user
    let options = vec![
        "Configure TCode",
        "Configure Iterations",
        "Configure Delay (seconds)",
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
                // Configure TCode
                let current = config.tcode.clone();
                let tcode: String = Input::new()
                    .with_prompt("Enter TCode (e.g., VT11, VL06O, ZMDESNR)")
                    .allow_empty(false)
                    .default(current)
                    .interact()
                    .unwrap();
                    
                config.tcode = tcode.clone();
                println!("TCode set to: {}", tcode);
            },
            1 => {
                // Configure Iterations
                let current = config.iterations.to_string();
                let iterations_str: String = Input::new()
                    .with_prompt("Enter number of iterations")
                    .allow_empty(false)
                    .default(current)
                    .interact()
                    .unwrap();
                    
                if let Ok(iterations) = iterations_str.parse::<usize>() {
                    config.iterations = iterations;
                    println!("Iterations set to: {}", iterations);
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
            4 => {
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
            5 => {
                // Show Current Configuration
                println!("\nCurrent Loop Configuration:");
                println!("---------------------------");
                println!("TCode: {}", config.tcode);
                println!("Iterations: {}", config.iterations);
                println!("Delay: {} seconds", config.delay_seconds);
                
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

/// Run a TCode in a loop with the specified configuration
pub fn run_loop(session: &GuiSession) -> Result<()> {
    println!("Run Loop from Configuration");
    println!("==========================");
    
    // Load loop configuration
    let config = match LoopConfig::load() {
        Ok(cfg) => cfg,
        Err(e) => {
            println!("Error loading loop configuration: {}", e);
            println!("\nPress Enter to return to main menu...");
            let mut input = String::new();
            io::stdin().read_line(&mut input).unwrap();
            return Ok(());
        }
    };
    
    // Check if TCode is configured
    if config.tcode.is_empty() {
        println!("No TCode configured for loop execution.");
        println!("Please configure loop parameters first.");
        println!("\nPress Enter to return to main menu...");
        let mut input = String::new();
        io::stdin().read_line(&mut input).unwrap();
        return Ok(());
    }
    
    println!("Running TCode '{}' in a loop with the following configuration:", config.tcode);
    println!("Iterations: {}", config.iterations);
    println!("Delay: {} seconds", config.delay_seconds);
    
    if !config.params.is_empty() {
        println!("\nParameters:");
        for (key, value) in &config.params {
            println!("  {}: {}", key, value);
        }
    }
    
    println!("\nPress Enter to start the loop or Ctrl+C to cancel...");
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();
    
    // Run the TCode in a loop
    for i in 1..=config.iterations {
        println!("\nIteration {}/{}", i, config.iterations);
        
        // Check if the TCode is active
        if !check_tcode(session, &config.tcode, Some(true), Some(true))? {
            println!("Failed to activate TCode '{}'", config.tcode);
            break;
        }
        
        // Run the TCode with the configured parameters
        match config.tcode.as_str() {
            "VL06O" => {
                run_vl06o_auto(session)?;
            },
            "VT11" => {
                run_vt11_auto(session)?;
            },
            "ZMDESNR" => {
                run_zmdesnr_auto(session)?;
            },
            _ => {
                // For other TCodes, just run the TCode and apply variant if specified
                if !assert_tcode(session, &config.tcode, Some(0))? {
                    println!("Failed to activate TCode '{}'", config.tcode);
                    break;
                }
                
                // Apply variant if specified
                if let Some(variant) = config.params.get("variant") {
                    if !variant.is_empty() && !variant_select(session, &config.tcode, variant)? {
                        println!("Failed to select variant '{}' for TCode '{}'", variant, config.tcode);
                    }
                }
                
                // Execute the TCode
                if let Ok(wnd) = session.find_by_id("wnd[0]".to_string()) {
                    if let Some(gui) = wnd.downcast::<GuiMainWindow>() {
                        gui.send_v_key(8)?;
                    }
                }
            }
        }
        
        // Wait for the specified delay before the next iteration
        if i < config.iterations {
            println!("Waiting {} seconds before next iteration...", config.delay_seconds);
            thread::sleep(Duration::from_secs(config.delay_seconds));
        }
    }
    
    println!("\nLoop execution completed.");
    println!("\nPress Enter to return to main menu...");
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();
    
    Ok(())
}
