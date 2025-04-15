use anyhow::{anyhow, Result};
use dialoguer::{Input, Select};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::io::{self};
use std::path::Path;
use std::thread;
use std::time::Duration;

/// Configuration structure for SAP automation
#[derive(Debug, Clone)]
pub struct SapConfig {
    pub reports_dir: String,
    pub tcode: Option<String>,
    pub variant: Option<String>,
    pub layout: Option<String>,
    pub column_name: Option<String>,
    pub date_range: Option<(String, String)>,
    pub additional_params: HashMap<String, String>,
}

impl Default for SapConfig {
    fn default() -> Self {
        Self {
            reports_dir: get_default_reports_dir(),
            tcode: None,
            variant: None,
            layout: None,
            column_name: None,
            date_range: None,
            additional_params: HashMap::new(),
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
        let mut config = Self::default();

        // Try to read from config file
        if let Ok(content) = fs::read_to_string("config.toml") {
            // Parse reports_dir
            if let Some(reports_dir) = parse_config_value(&content, "reports_dir") {
                config.reports_dir = reports_dir;
            }

            // Parse tcode
            if let Some(tcode) = parse_config_value(&content, "tcode") {
                config.tcode = Some(tcode);
            }

            // Parse variant
            if let Some(variant) = parse_config_value(&content, "variant") {
                config.variant = Some(variant);
            }

            // Parse layout
            if let Some(layout) = parse_config_value(&content, "layout") {
                config.layout = Some(layout);
            }

            // Parse column_name
            if let Some(column_name) = parse_config_value(&content, "column_name") {
                config.column_name = Some(column_name);
            }

            // Parse date_range_start and date_range_end
            let start_date = parse_config_value(&content, "date_range_start");
            let end_date = parse_config_value(&content, "date_range_end");

            if start_date.is_some() && end_date.is_some() {
                config.date_range = Some((start_date.unwrap(), end_date.unwrap()));
            }

            // Parse loop_tcode and add to additional_params
            if let Some(loop_tcode) = parse_config_value(&content, "loop_tcode") {
                config.additional_params.insert("loop_tcode".to_string(), loop_tcode);
            } else if let Some(tcode) = &config.tcode {
                // If loop_tcode is not specified, use tcode as the default
                config.additional_params.insert("loop_tcode".to_string(), tcode.clone());
            }

            // Parse any additional parameters (those not explicitly handled above)
            for line in content.lines() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') || line.starts_with('[') {
                    continue;
                }

                if let Some(pos) = line.find('=') {
                    let key = line[..pos].trim();

                    // Skip keys we've already processed
                    if [
                        "reports_dir",
                        "tcode",
                        "variant",
                        "layout",
                        "column_name",
                        "date_range_start",
                        "date_range_end",
                        "loop_tcode",
                    ]
                    .contains(&key)
                    {
                        continue;
                    }

                    if let Some(value) = parse_config_value(&content, key) {
                        config.additional_params.insert(key.to_string(), value);
                    }
                }
            }
        }

        Ok(config)
    }

    /// Save configuration to config.toml file
    pub fn save(&self) -> Result<()> {
        let mut content = String::new();

        // Read existing content to preserve sections like [build]
        let existing_content = fs::read_to_string("config.toml").unwrap_or_default();
        let mut sections: HashMap<String, Vec<String>> = HashMap::new();
        let mut current_section = String::new();
        let mut non_section_lines = Vec::new();

        for line in existing_content.lines() {
            if line.starts_with('[') && line.ends_with(']') {
                current_section = line.to_string();
                sections.entry(current_section.clone()).or_default();
            } else if !current_section.is_empty() {
                sections
                    .get_mut(&current_section)
                    .unwrap()
                    .push(line.to_string());
            } else {
                non_section_lines.push(line.to_string());
            }
        }

        // Add non-section lines first
        for line in non_section_lines {
            content.push_str(&line);
            content.push('\n');
        }

        // Add sections except [sap_config]
        for (section, lines) in &sections {
            if section != "[sap_config]" {
                content.push_str(section);
                content.push('\n');
                for line in lines {
                    content.push_str(line);
                    content.push('\n');
                }
            }
        }

        // Add [sap_config] section with our values
        content.push_str("[sap_config]\n");
        content.push_str(&format!("reports_dir = \"{}\"\n", self.reports_dir));

        if let Some(tcode) = &self.tcode {
            content.push_str(&format!("tcode = \"{}\"\n", tcode));
        }

        if let Some(variant) = &self.variant {
            content.push_str(&format!("variant = \"{}\"\n", variant));
        }

        if let Some(layout) = &self.layout {
            content.push_str(&format!("layout = \"{}\"\n", layout));
        }

        if let Some(column_name) = &self.column_name {
            content.push_str(&format!("column_name = \"{}\"\n", column_name));
        }

        if let Some((start, end)) = &self.date_range {
            content.push_str(&format!("date_range_start = \"{}\"\n", start));
            content.push_str(&format!("date_range_end = \"{}\"\n", end));
        }

        // Add loop_tcode from additional_params if it exists
        if let Some(loop_tcode) = self.additional_params.get("loop_tcode") {
            content.push_str(&format!("loop_tcode = \"{}\"\n", loop_tcode));
        }

        // Add additional parameters
        for (key, value) in &self.additional_params {
            // Skip loop_tcode as we've already handled it
            if key != "loop_tcode" {
                content.push_str(&format!("{} = \"{}\"\n", key, value));
            }
        }

        // Write updated config
        fs::write("config.toml", content)?;

        Ok(())
    }

    /// Get configuration for a specific tcode
    pub fn get_tcode_config(&self, tcode: &str, is_loop_run: Option<bool>) -> Option<HashMap<String, String>> {
        let is_loop_run = is_loop_run.unwrap_or(false);

        let mut config = HashMap::new();
        
        // Get the configured tcode based on whether this is a loop run or not
        let configured_tcode = if is_loop_run {
            // For loop runs, use loop_tcode if available, otherwise fall back to regular tcode
            self.additional_params.get("loop_tcode").or(self.tcode.as_ref())
        } else {
            // For normal runs, use the regular tcode
            self.tcode.as_ref()
        };
        
        // If we have a configured tcode, add it to the config
        if let Some(t) = configured_tcode {
            config.insert("tcode".to_string(), t.clone());
            
            // If the configured tcode matches the requested one, add the configuration
            if t == tcode {
                if let Some(variant) = &self.variant {
                    config.insert("variant".to_string(), variant.clone());
                }

                if let Some(layout) = &self.layout {
                    config.insert("layout".to_string(), layout.clone());
                }

                if let Some(column_name) = &self.column_name {
                    config.insert("column_name".to_string(), column_name.clone());
                }

                if let Some((start, end)) = &self.date_range {
                    config.insert("date_range_start".to_string(), start.clone());
                    config.insert("date_range_end".to_string(), end.clone());
                }

                // Add any additional parameters
                for (key, value) in &self.additional_params {
                    if key.starts_with(&format!("{}_", tcode)) {
                        let param_name = key.replacen(&format!("{}_", tcode), "", 1);
                        config.insert(param_name, value.clone());
                    } else {
                        config.insert(key.clone(), value.clone());
                    }
                }

                return Some(config);
            }
        }

        // Check for tcode-specific parameters even if tcode doesn't match the configured one
        let mut has_tcode_params = false;
        for (key, value) in &self.additional_params {
            if key.starts_with(&format!("{}_", tcode)) {
                let param_name = key.replacen(&format!("{}_", tcode), "", 1);
                config.insert(param_name, value.clone());
                has_tcode_params = true;
            }
        }

        if has_tcode_params {
            Some(config)
        } else {
            None
        }
    }
}

/// Gets the configured reports directory or returns the default
pub fn get_reports_dir() -> String {
    // Try to read from config file first
    if let Ok(config) = SapConfig::load() {
        return config.reports_dir;
    }

    // If loading config fails, use default path
    get_default_reports_dir()
}

/// Gets the default reports directory path
fn get_default_reports_dir() -> String {
    match env::var("USERPROFILE") {
        Ok(profile) => format!("{}\\Documents\\Reports", profile),
        Err(_) => {
            eprintln!("Could not determine user profile directory");
            String::from(".\\Reports")
        }
    }
}

/// Parse a value from the config file
fn parse_config_value(content: &str, key: &str) -> Option<String> {
    for line in content.lines() {
        let line = line.trim();
        if line.starts_with(&format!("{} =", key)) || line.starts_with(&format!("{}=", key)) {
            if let Some(pos) = line.find('=') {
                let value = line[pos + 1..].trim().trim_matches('"').trim_matches('\'');
                if !value.is_empty() {
                    return Some(value.to_string());
                }
            }
        }
    }
    None
}

/// Handle configuring the reports directory
pub fn handle_configure_reports_dir() -> Result<()> {
    println!("Configure Reports Directory");
    println!("==========================");

    // Get current reports directory
    let mut config = SapConfig::load()?;
    let current_dir = &config.reports_dir;
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
            if new_dir.is_empty() || new_dir == *current_dir {
                println!("No changes made to reports directory.");
                thread::sleep(Duration::from_secs(2));
                return Ok(());
            }

            // Handle "../" at the beginning (up one directory)
            if new_dir.starts_with("../") || new_dir.starts_with("..\\") {
                let current_path = Path::new(current_dir);
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
    config.reports_dir = new_dir.clone();
    if let Err(e) = config.save() {
        eprintln!("Failed to update config file: {}", e);
        thread::sleep(Duration::from_secs(2));
        return Err(anyhow!("Failed to update config file: {}", e));
    }

    println!("Reports directory updated to: {}", new_dir);
    thread::sleep(Duration::from_secs(2));

    Ok(())
}

/// Handle configuring SAP automation parameters
pub fn handle_configure_sap_params() -> Result<()> {
    println!("Configure SAP Automation Parameters");
    println!("==================================");

    // Load current configuration
    let mut config = SapConfig::load()?;

    // Present options to the user
    let options = vec![
        "Configure TCode",
        "Configure Variant",
        "Configure Layout",
        "Configure Column Name",
        "Configure Date Range",
        "Add Custom Parameter",
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
                let current = config.tcode.clone().unwrap_or_default();
                let tcode: String = Input::new()
                    .with_prompt("Enter TCode (e.g., VT11, VL06O, ZMDESNR)")
                    .allow_empty(true)
                    .default(current)
                    .interact()
                    .unwrap();

                if tcode.is_empty() {
                    config.tcode = None;
                    println!("TCode configuration cleared.");
                } else {
                    config.tcode = Some(tcode.clone());
                    println!("TCode set to: {}", tcode);
                }
            }
            1 => {
                // Configure Variant
                let current = config.variant.clone().unwrap_or_default();
                let variant: String = Input::new()
                    .with_prompt("Enter SAP Variant Name")
                    .allow_empty(true)
                    .default(current)
                    .interact()
                    .unwrap();

                if variant.is_empty() {
                    config.variant = None;
                    println!("Variant configuration cleared.");
                } else {
                    config.variant = Some(variant.clone());
                    println!("Variant set to: {}", variant);
                }
            }
            2 => {
                // Configure Layout
                let current = config.layout.clone().unwrap_or_default();
                let layout: String = Input::new()
                    .with_prompt("Enter Layout Name")
                    .allow_empty(true)
                    .default(current)
                    .interact()
                    .unwrap();

                if layout.is_empty() {
                    config.layout = None;
                    println!("Layout configuration cleared.");
                } else {
                    config.layout = Some(layout.clone());
                    println!("Layout set to: {}", layout);
                }
            }
            3 => {
                // Configure Column Name
                let current = config.column_name.clone().unwrap_or_default();
                let column_name: String = Input::new()
                    .with_prompt("Enter Column Name")
                    .allow_empty(true)
                    .default(current)
                    .interact()
                    .unwrap();

                if column_name.is_empty() {
                    config.column_name = None;
                    println!("Column Name configuration cleared.");
                } else {
                    config.column_name = Some(column_name.clone());
                    println!("Column Name set to: {}", column_name);
                }
            }
            4 => {
                // Configure Date Range
                let current_start = config
                    .date_range
                    .as_ref()
                    .map(|(s, _)| s.clone())
                    .unwrap_or_default();
                let current_end = config
                    .date_range
                    .as_ref()
                    .map(|(_, e)| e.clone())
                    .unwrap_or_default();

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
                    config.date_range = None;
                    println!("Date Range configuration cleared.");
                } else {
                    config.date_range = Some((start_date.clone(), end_date.clone()));
                    println!("Date Range set to: {} - {}", start_date, end_date);
                }
            }
            5 => {
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
                    config.additional_params.remove(&param_name);
                    println!("Parameter '{}' removed.", param_name);
                } else {
                    config
                        .additional_params
                        .insert(param_name.clone(), param_value.clone());
                    println!("Parameter '{}' set to: {}", param_name, param_value);
                }
            }
            6 => {
                // Remove Parameter
                let mut param_names: Vec<String> = Vec::new();

                // Add standard parameters if they exist
                if config.tcode.is_some() {
                    param_names.push("tcode".to_string());
                }
                if config.variant.is_some() {
                    param_names.push("variant".to_string());
                }
                if config.layout.is_some() {
                    param_names.push("layout".to_string());
                }
                if config.column_name.is_some() {
                    param_names.push("column_name".to_string());
                }
                if config.date_range.is_some() {
                    param_names.push("date_range".to_string());
                }

                // Add additional parameters
                for key in config.additional_params.keys() {
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
                    "tcode" => {
                        config.tcode = None;
                        println!("TCode configuration cleared.");
                    }
                    "variant" => {
                        config.variant = None;
                        println!("Variant configuration cleared.");
                    }
                    "layout" => {
                        config.layout = None;
                        println!("Layout configuration cleared.");
                    }
                    "column_name" => {
                        config.column_name = None;
                        println!("Column Name configuration cleared.");
                    }
                    "date_range" => {
                        config.date_range = None;
                        println!("Date Range configuration cleared.");
                    }
                    _ => {
                        config.additional_params.remove(param_name);
                        println!("Parameter '{}' removed.", param_name);
                    }
                }
            }
            7 => {
                // Show Current Configuration
                println!("\nCurrent Configuration:");
                println!("---------------------");
                println!("Reports Directory: {}", config.reports_dir);

                if let Some(tcode) = &config.tcode {
                    println!("TCode: {}", tcode);
                }

                if let Some(variant) = &config.variant {
                    println!("Variant: {}", variant);
                }

                if let Some(layout) = &config.layout {
                    println!("Layout: {}", layout);
                }

                if let Some(column_name) = &config.column_name {
                    println!("Column Name: {}", column_name);
                }

                if let Some((start, end)) = &config.date_range {
                    println!("Date Range: {} - {}", start, end);
                }

                if !config.additional_params.is_empty() {
                    println!("\nAdditional Parameters:");
                    for (key, value) in &config.additional_params {
                        println!("  {}: {}", key, value);
                    }
                }

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
