use anyhow::{anyhow, Result};
use dialoguer::{Input, Select};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::io::{self};
use std::path::Path;
use std::thread;
use std::time::Duration;

use crate::utils::config_types::*;

impl Default for SapConfig {
    fn default() -> Self {
        Self {
            config_path: "config.toml".to_string(),
            global: Some(GlobalConfig {
                instance_id: default_instance_id(),
                reports_dir: get_default_reports_dir(),
                default_tcode: None,
                additional_params: HashMap::new(),
            }),
            build: None,
            tcode: Some(HashMap::new()),
            loop_config: None,
            raw_config: None,
        }
    }
}

impl SapConfig {
    /// Create a new configuration with default values
    pub fn new() -> Self {
        Self::default()
    }

    /// Load configuration from config.toml file
    pub fn load() -> Result<Self> {
        Self::load_from_path("config.toml")
    }

    /// Load configuration from a specific path
    pub fn load_from_path(path: &str) -> Result<Self> {
        let mut config = Self::default();
        config.config_path = path.to_string();
        
        // Try to read from config file
        if let Ok(content) = fs::read_to_string(path) {
            // Parse the TOML content
            match toml::from_str::<toml::Value>(&content) {
                Ok(parsed) => {
                    config.raw_config = Some(parsed.clone());
                    
                    // Extract build section
                    if let Some(build) = parsed.get("build").and_then(|v| v.as_table()) {
                        let mut build_config = BuildConfig {
                            target: build.get("target")
                                .and_then(|v| v.as_str())
                                .unwrap_or("i686-pc-windows-msvc")
                                .to_string(),
                            additional_params: HashMap::new(),
                        };
                        
                        // Extract additional build parameters
                        for (key, value) in build {
                            if key != "target" {
                                if let Some(val_str) = value.as_str() {
                                    build_config.additional_params.insert(key.clone(), val_str.to_string());
                                }
                            }
                        }
                        
                        config.build = Some(build_config);
                    }
                    
                    // Check for new format (with global and tcode sections)
                    let is_new_format = parsed.get("global").is_some() || parsed.get("tcode").is_some();
                    
                    if is_new_format {
                        // Extract global section
                        if let Some(global) = parsed.get("global").and_then(|v| v.as_table()) {
                            let mut global_config = GlobalConfig {
                                instance_id: global.get("instance_id")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or(&default_instance_id())
                                    .to_string(),
                                reports_dir: global.get("reports_dir")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or(&get_default_reports_dir())
                                    .to_string(),
                                default_tcode: global.get("default_tcode")
                                    .and_then(|v| v.as_str())
                                    .map(|s| s.to_string()),
                                additional_params: HashMap::new(),
                            };
                            
                            // Extract additional global parameters
                            for (key, value) in global {
                                if !["instance_id", "reports_dir", "default_tcode"].contains(&key.as_str()) {
                                    if let Some(val_str) = value.as_str() {
                                        global_config.additional_params.insert(key.clone(), val_str.to_string());
                                    }
                                }
                            }
                            
                            config.global = Some(global_config);
                        }
                        
                        // Extract tcode sections
                        if let Some(tcode_table) = parsed.get("tcode").and_then(|v| v.as_table()) {
                            let mut tcode_configs = HashMap::new();
                            
                            for (tcode_name, tcode_value) in tcode_table {
                                if let Some(tcode_table) = tcode_value.as_table() {
                                    let mut tcode_config = TcodeConfig::default();
                                    
                                    // Extract standard fields
                                    tcode_config.variant = tcode_table.get("variant")
                                        .and_then(|v| v.as_str())
                                        .map(|s| s.to_string());
                                        
                                    tcode_config.layout = tcode_table.get("layout")
                                        .and_then(|v| v.as_str())
                                        .map(|s| s.to_string());
                                        
                                    tcode_config.column_name = tcode_table.get("column_name")
                                        .and_then(|v| v.as_str())
                                        .map(|s| s.to_string());
                                        
                                    tcode_config.date_range_start = tcode_table.get("date_range_start")
                                        .and_then(|v| v.as_str())
                                        .map(|s| s.to_string());
                                        
                                    tcode_config.date_range_end = tcode_table.get("date_range_end")
                                        .and_then(|v| v.as_str())
                                        .map(|s| s.to_string());
                                        
                                    tcode_config.by_date = tcode_table.get("by_date")
                                        .and_then(|v| v.as_str())
                                        .map(|s| s.to_string());
                                        
                                    tcode_config.serial_number = tcode_table.get("serial_number")
                                        .and_then(|v| v.as_str())
                                        .map(|s| s.to_string());
                                        
                                    tcode_config.tab_number = tcode_table.get("tab_number")
                                        .and_then(|v| v.as_str())
                                        .map(|s| s.to_string());
                                    
                                    // Extract additional parameters
                                    for (key, value) in tcode_table {
                                        if !["variant", "layout", "column_name", "date_range_start", 
                                             "date_range_end", "by_date", "serial_number", "tab_number"]
                                            .contains(&key.as_str()) {
                                            if let Some(val_str) = value.as_str() {
                                                tcode_config.additional_params.insert(key.clone(), val_str.to_string());
                                            }
                                        }
                                    }
                                    
                                    tcode_configs.insert(tcode_name.clone(), tcode_config);
                                }
                            }
                            
                            config.tcode = Some(tcode_configs);
                        }
                        
                        // Extract loop section
                        if let Some(loop_table) = parsed.get("loop").and_then(|v| v.as_table()) {
                            let mut loop_config = LoopConfig {
                                tcode: loop_table.get("tcode")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    .to_string(),
                                iterations: loop_table.get("iterations")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or(&default_iterations())
                                    .to_string(),
                                delay_seconds: loop_table.get("delay_seconds")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or(&default_delay_seconds())
                                    .to_string(),
                                params: HashMap::new(),
                            };
                            
                            // Extract additional loop parameters
                            for (key, value) in loop_table {
                                if !["tcode", "iterations", "delay_seconds"].contains(&key.as_str()) {
                                    if let Some(val_str) = value.as_str() {
                                        loop_config.params.insert(key.clone(), val_str.to_string());
                                    }
                                }
                            }
                            
                            config.loop_config = Some(loop_config);
                        }
                    } else {
                        // Handle legacy format (with sap_config section)
                        config = Self::load_legacy_format(parsed, config)?;
                    }
                },
                Err(e) => {
                    return Err(anyhow!("Failed to parse config file: {}", e));
                }
            }
        }
        
        Ok(config)
    }
    
    /// Load configuration from legacy format
    fn load_legacy_format(parsed: toml::Value, mut config: SapConfig) -> Result<SapConfig> {
        // Extract sap_config section
        if let Some(sap_config) = parsed.get("sap_config").and_then(|v| v.as_table()) {
            // Create global config
            let mut global_config = GlobalConfig {
                instance_id: sap_config.get("instance_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or(&default_instance_id())
                    .to_string(),
                reports_dir: sap_config.get("reports_dir")
                    .and_then(|v| v.as_str())
                    .unwrap_or(&get_default_reports_dir())
                    .to_string(),
                default_tcode: sap_config.get("tcode")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                additional_params: HashMap::new(),
            };
            
            // Get the default tcode from the config
            let default_tcode = global_config.default_tcode.clone().unwrap_or_else(|| "".to_string());
            
            // Create tcode config for the default tcode
            let mut tcode_config = TcodeConfig::default();
            
            // Extract standard fields
            tcode_config.variant = sap_config.get("variant")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
                
            tcode_config.layout = sap_config.get("layout")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
                
            tcode_config.column_name = sap_config.get("column_name")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
                
            tcode_config.date_range_start = sap_config.get("date_range_start")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
                
            tcode_config.date_range_end = sap_config.get("date_range_end")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            
            // Create loop config
            let mut loop_config = LoopConfig {
                tcode: sap_config.get("loop_tcode")
                    .and_then(|v| v.as_str())
                    .unwrap_or(&default_tcode)
                    .to_string(),
                iterations: sap_config.get("loop_iterations")
                    .and_then(|v| v.as_str())
                    .unwrap_or(&default_iterations())
                    .to_string(),
                delay_seconds: sap_config.get("loop_delay_seconds")
                    .and_then(|v| v.as_str())
                    .unwrap_or(&default_delay_seconds())
                    .to_string(),
                params: HashMap::new(),
            };
            
            // Extract additional parameters
            for (key, value) in sap_config {
                if !["instance_id", "reports_dir", "tcode", "variant", "layout", "column_name", 
                     "date_range_start", "date_range_end", "loop_tcode", "loop_iterations", 
                     "loop_delay_seconds"].contains(&key.as_str()) {
                    if let Some(val_str) = value.as_str() {
                        // Check if it's a loop parameter
                        if key.starts_with("loop_param_") {
                            let param_name = key.replacen("loop_param_", "", 1);
                            loop_config.params.insert(param_name, val_str.to_string());
                        } else if key.starts_with("loop_") {
                            // Other loop-related parameters
                            let param_name = key.replacen("loop_", "", 1);
                            loop_config.params.insert(param_name, val_str.to_string());
                        } else if !default_tcode.is_empty() && key.starts_with(&format!("{}_", default_tcode)) {
                            // TCode-specific parameters
                            let param_name = key.replacen(&format!("{}_", default_tcode), "", 1);
                            tcode_config.additional_params.insert(param_name, val_str.to_string());
                        } else {
                            // Global parameters
                            global_config.additional_params.insert(key.clone(), val_str.to_string());
                        }
                    }
                }
            }
            
            // Update config
            config.global = Some(global_config);
            
            if !default_tcode.is_empty() {
                let mut tcode_configs = HashMap::new();
                tcode_configs.insert(default_tcode, tcode_config);
                config.tcode = Some(tcode_configs);
            }
            
            if !loop_config.tcode.is_empty() {
                config.loop_config = Some(loop_config);
            }
        }
        
        Ok(config)
    }

    /// Save configuration to config.toml file
    pub fn save(&self) -> Result<()> {
        self.save_to_path(&self.config_path)
    }
    
    /// Save configuration to a specific path
    pub fn save_to_path(&self, path: &str) -> Result<()> {
        let mut content = String::new();
        
        // Preserve any sections from the original config that we don't explicitly handle
        if let Some(raw_config) = &self.raw_config {
            // Get all top-level keys that aren't "build", "global", "tcode", or "loop"
            let preserved_keys: Vec<&String> = raw_config.as_table()
                .map(|t| t.keys().filter(|k| !["build", "global", "tcode", "loop", "sap_config"].contains(&k.as_str())).collect())
                .unwrap_or_default();
            
            // Add preserved sections to the content
            for key in preserved_keys {
                if let Some(section) = raw_config.get(key) {
                    if let Some(table) = section.as_table() {
                        content.push_str(&format!("[{}]\n", key));
                        for (k, v) in table {
                            if let Some(val_str) = v.as_str() {
                                content.push_str(&format!("{} = \"{}\"\n", k, val_str));
                            } else {
                                // For non-string values, use the TOML representation
                                content.push_str(&format!("{} = {}\n", k, v));
                            }
                        }
                        content.push('\n');
                    }
                }
            }
        }
        
        // Add build section
        if let Some(build) = &self.build {
            content.push_str("[build]\n");
            content.push_str(&format!("target = \"{}\"\n", build.target));
            
            // Add additional build parameters
            for (key, value) in &build.additional_params {
                content.push_str(&format!("{} = \"{}\"\n", key, value));
            }
            
            content.push('\n');
        }
        
        // Add global section
        if let Some(global) = &self.global {
            content.push_str("[global]\n");
            content.push_str(&format!("instance_id = \"{}\"\n", global.instance_id));
            content.push_str(&format!("reports_dir = \"{}\"\n", global.reports_dir));
            
            if let Some(default_tcode) = &global.default_tcode {
                content.push_str(&format!("default_tcode = \"{}\"\n", default_tcode));
            }
            
            // Add additional global parameters
            for (key, value) in &global.additional_params {
                content.push_str(&format!("{} = \"{}\"\n", key, value));
            }
            
            content.push('\n');
        }
        
        // Add tcode sections
        if let Some(tcode_configs) = &self.tcode {
            for (tcode_name, tcode_config) in tcode_configs {
                content.push_str(&format!("[tcode.{}]\n", tcode_name));
                
                if let Some(variant) = &tcode_config.variant {
                    content.push_str(&format!("variant = \"{}\"\n", variant));
                }
                
                if let Some(layout) = &tcode_config.layout {
                    content.push_str(&format!("layout = \"{}\"\n", layout));
                }
                
                if let Some(column_name) = &tcode_config.column_name {
                    content.push_str(&format!("column_name = \"{}\"\n", column_name));
                }
                
                if let Some(date_range_start) = &tcode_config.date_range_start {
                    content.push_str(&format!("date_range_start = \"{}\"\n", date_range_start));
                }
                
                if let Some(date_range_end) = &tcode_config.date_range_end {
                    content.push_str(&format!("date_range_end = \"{}\"\n", date_range_end));
                }
                
                if let Some(by_date) = &tcode_config.by_date {
                    content.push_str(&format!("by_date = \"{}\"\n", by_date));
                }
                
                if let Some(serial_number) = &tcode_config.serial_number {
                    content.push_str(&format!("serial_number = \"{}\"\n", serial_number));
                }
                
                if let Some(tab_number) = &tcode_config.tab_number {
                    content.push_str(&format!("tab_number = \"{}\"\n", tab_number));
                }
                
                // Add additional tcode parameters
                for (key, value) in &tcode_config.additional_params {
                    content.push_str(&format!("{} = \"{}\"\n", key, value));
                }
                
                content.push('\n');
            }
        }
        
        // Add loop section
        if let Some(loop_config) = &self.loop_config {
            content.push_str("[loop]\n");
            content.push_str(&format!("tcode = \"{}\"\n", loop_config.tcode));
            content.push_str(&format!("iterations = \"{}\"\n", loop_config.iterations));
            content.push_str(&format!("delay_seconds = \"{}\"\n", loop_config.delay_seconds));
            
            // Add additional loop parameters
            for (key, value) in &loop_config.params {
                content.push_str(&format!("param_{} = \"{}\"\n", key, value));
            }
            
            content.push('\n');
        }
        
        // Write updated config
        fs::write(path, content)?;
        
        Ok(())
    }

    /// Get configuration for a specific tcode
    pub fn get_tcode_config(&self, tcode: &str, is_loop_run: Option<bool>) -> Option<HashMap<String, String>> {
        let is_loop_run = is_loop_run.unwrap_or(false);
        
        let mut config = HashMap::new();
        
        // Get the configured tcode based on whether this is a loop run or not
        let configured_tcode = if is_loop_run {
            // For loop runs, use loop_tcode if available
            self.loop_config.as_ref().map(|l| l.tcode.clone())
        } else {
            // For normal runs, use the default tcode from global config
            self.global.as_ref().and_then(|g| g.default_tcode.clone())
        };
        
        // If we have a configured tcode, add it to the config
        if let Some(t) = configured_tcode {
            config.insert("tcode".to_string(), t.clone());
        }
        
        // Get tcode-specific configuration
        if let Some(tcode_configs) = &self.tcode {
            if let Some(tcode_config) = tcode_configs.get(tcode) {
                // Add standard fields if they exist
                if let Some(variant) = &tcode_config.variant {
                    config.insert("variant".to_string(), variant.clone());
                }
                
                if let Some(layout) = &tcode_config.layout {
                    config.insert("layout".to_string(), layout.clone());
                }
                
                if let Some(column_name) = &tcode_config.column_name {
                    config.insert("column_name".to_string(), column_name.clone());
                }
                
                if let Some(date_range_start) = &tcode_config.date_range_start {
                    config.insert("date_range_start".to_string(), date_range_start.clone());
                }
                
                if let Some(date_range_end) = &tcode_config.date_range_end {
                    config.insert("date_range_end".to_string(), date_range_end.clone());
                }
                
                if let Some(by_date) = &tcode_config.by_date {
                    config.insert("by_date".to_string(), by_date.clone());
                }
                
                if let Some(serial_number) = &tcode_config.serial_number {
                    config.insert("serial_number".to_string(), serial_number.clone());
                }
                
                if let Some(tab_number) = &tcode_config.tab_number {
                    config.insert("tab_number".to_string(), tab_number.clone());
                }
                
                // Add additional parameters
                for (key, value) in &tcode_config.additional_params {
                    config.insert(key.clone(), value.clone());
                }
                
                return Some(config);
            }
        }
        
        // If we're in a loop run, add loop parameters
        if is_loop_run && self.loop_config.is_some() {
            let loop_config = self.loop_config.as_ref().unwrap();
            
            // Add loop parameters with tcode-specific prefix
            for (key, value) in &loop_config.params {
                if key.starts_with(&format!("{}_", tcode)) {
                    let param_name = key.replacen(&format!("{}_", tcode), "", 1);
                    config.insert(param_name, value.clone());
                } else {
                    config.insert(key.clone(), value.clone());
                }
            }
            
            if !config.is_empty() {
                return Some(config);
            }
        }
        
        // If we have any configuration, return it
        if !config.is_empty() {
            Some(config)
        } else {
            None
        }
    }
    
    /// Get the instance ID
    pub fn get_instance_id(&self) -> String {
        self.global.as_ref().map(|g| g.instance_id.clone()).unwrap_or_else(default_instance_id)
    }
    
    /// Get the reports directory
    pub fn get_reports_dir(&self) -> String {
        self.global.as_ref().map(|g| g.reports_dir.clone()).unwrap_or_else(get_default_reports_dir)
    }
    
    /// Set the instance ID
    pub fn set_instance_id(&mut self, instance_id: &str) {
        if let Some(global) = &mut self.global {
            global.instance_id = instance_id.to_string();
        } else {
            self.global = Some(GlobalConfig {
                instance_id: instance_id.to_string(),
                reports_dir: get_default_reports_dir(),
                default_tcode: None,
                additional_params: HashMap::new(),
            });
        }
    }
    
    /// Set the reports directory
    pub fn set_reports_dir(&mut self, reports_dir: &str) {
        if let Some(global) = &mut self.global {
            global.reports_dir = reports_dir.to_string();
        } else {
            self.global = Some(GlobalConfig {
                instance_id: default_instance_id(),
                reports_dir: reports_dir.to_string(),
                default_tcode: None,
                additional_params: HashMap::new(),
            });
        }
    }
}

/// Gets the configured reports directory or returns the default
pub fn get_reports_dir() -> String {
    // Try to read from config file first
    if let Ok(config) = SapConfig::load() {
        return config.get_reports_dir();
    }

    // If loading config fails, use default path
    get_default_reports_dir()
}

/// Handle configuring the reports directory
pub fn handle_configure_reports_dir() -> Result<()> {
    println!("Configure Reports Directory");
    println!("==========================");

    // Get current reports directory
    let mut config = SapConfig::load()?;
    let current_dir = config.get_reports_dir();
    println!("Current reports directory: {}", current_dir);

    // Present options to the user
    let options = vec![
        "Enter a custom directory",
        "Reset to default (userprofile/documents/reports)",
        "Cancel (keep current)",
    ];

    let selection = Select::new()
        .with_prompt("Choose an option")
        .items(&options)
        .default(0)
        .interact()
        .unwrap();

    let mut new_dir = String::new();

    match selection {
        0 => {
            // User wants to enter a custom directory
            new_dir = Input::new()
                .with_prompt("Enter new reports directory")
                .allow_empty(true)
                .default(current_dir.clone())
                .interact()
                .unwrap();

            // Handle empty input
            if new_dir.is_empty() || new_dir == current_dir {
                println!("No changes made to reports directory.");
                thread::sleep(Duration::from_secs(2));
                return Ok(());
            }

            // Handle "../" at the beginning (up one directory)
            if new_dir.starts_with("../") || new_dir.starts_with("..\\") {
                let current_path = Path::new(&current_dir);
                if let Some(parent) = current_path.parent() {
                    let rest_of_path = if new_dir.starts_with("../") {
                        &new_dir[3..]
                    } else {
                        // starts_with("..\\")
                        &new_dir[3..]
                    };

                    new_dir = format!("{}\\{}", parent.to_string_lossy(), rest_of_path);
                    println!("Using parent directory path: {}", new_dir);
                }
            }
            // Handle slug (no path separators)
            else {
                let needles = ["\\", "/", "\\\\"];
                if !needles.iter().any(|n| new_dir.contains(n)) {
                    println!("Attempting to use relative path: {}", new_dir);
                    new_dir = format!("{}\\{}", current_dir, new_dir);
                }
            }
        }
        1 => {
            // User wants to reset to default
            new_dir = get_default_reports_dir();
            println!("Resetting to default reports directory: {}", new_dir);
        }
        _ => {
            // User wants to cancel
            println!("No changes made to reports directory.");
            thread::sleep(Duration::from_secs(2));
            return Ok(());
        }
    }

    // Validate directory
    let path = Path::new(&new_dir);
    if !path.exists() {
        println!("Directory does not exist. Create it? (y/n)");
        let mut create_choice = String::new();
        io::stdin().read_line(&mut create_choice).unwrap();

        if create_choice.trim().to_lowercase() == "y" {
            if let Err(e) = fs::create_dir_all(&new_dir) {
                eprintln!("Failed to create directory: {}", e);
                thread::sleep(Duration::from_secs(2));
                return Ok(());
            }
        } else {
            println!("Directory not created. No changes made.");
            thread::sleep(Duration::from_secs(2));
            return Ok(());
        }
    }

    // Update config
    config.set_reports_dir(&new_dir);
    if let Err(e) = config.save() {
        eprintln!("Failed to update config file: {}", e);
        thread::sleep(Duration::from_secs(2));
        return Err(anyhow!("Failed to update config file: {}", e));
    }

    println!("Reports directory updated to: {}", new_dir);
    thread::sleep(Duration::from_secs(2));

    Ok(())
}
