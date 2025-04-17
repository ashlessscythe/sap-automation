use std::fs;
use std::path::Path;
use std::process;

fn main() {
    println!("SAP Automation Configuration Migration Tool");
    println!("==========================================");
    println!("This tool will migrate your config.toml file to the new format.");
    println!("A backup of your original config will be created as config.toml.bak");
    println!();

    // Check if config.toml exists
    if !Path::new("config.toml").exists() {
        println!("Error: config.toml not found in the current directory.");
        println!("Please run this tool from the directory containing your config.toml file.");
        process::exit(1);
    }

    // Create a backup
    match fs::copy("config.toml", "config.toml.bak") {
        Ok(_) => println!("Backup created: config.toml.bak"),
        Err(e) => {
            println!("Error creating backup: {}", e);
            process::exit(1);
        }
    }

    // Read the config file
    let content = match fs::read_to_string("config.toml") {
        Ok(content) => content,
        Err(e) => {
            println!("Error reading config.toml: {}", e);
            process::exit(1);
        }
    };

    // Parse the TOML content
    let parsed: toml::Value = match content.parse() {
        Ok(parsed) => parsed,
        Err(e) => {
            println!("Error parsing config.toml: {}", e);
            process::exit(1);
        }
    };

    // Create the new config structure
    let mut new_config = String::new();

    // Preserve build section if it exists
    if let Some(build) = parsed.get("build").and_then(|v| v.as_table()) {
        new_config.push_str("[build]\n");
        for (key, value) in build {
            if let Some(val_str) = value.as_str() {
                new_config.push_str(&format!("{} = \"{}\"\n", key, val_str));
            } else {
                new_config.push_str(&format!("{} = {}\n", key, value));
            }
        }
        new_config.push_str("\n");
    }

    // Create global section
    new_config.push_str("[global]\n");

    // Extract values from sap_config if it exists
    if let Some(sap_config) = parsed.get("sap_config").and_then(|v| v.as_table()) {
        // Extract instance_id
        if let Some(instance_id) = sap_config.get("instance_id").and_then(|v| v.as_str()) {
            new_config.push_str(&format!("instance_id = \"{}\"\n", instance_id));
        } else {
            new_config.push_str("instance_id = \"rs\"\n");
        }

        // Extract reports_dir
        if let Some(reports_dir) = sap_config.get("reports_dir").and_then(|v| v.as_str()) {
            new_config.push_str(&format!("reports_dir = \"{}\"\n", reports_dir));
        }

        // Extract tcode as default_tcode
        if let Some(tcode) = sap_config.get("tcode").and_then(|v| v.as_str()) {
            new_config.push_str(&format!("default_tcode = \"{}\"\n", tcode));
        }

        // Add any other global parameters
        for (key, value) in sap_config {
            if !["instance_id", "reports_dir", "tcode", "variant", "layout", "column_name", 
                 "date_range_start", "date_range_end", "loop_tcode", "loop_iterations", 
                 "loop_delay_seconds"].contains(&key.as_str()) && !key.starts_with("loop_") {
                if let Some(val_str) = value.as_str() {
                    new_config.push_str(&format!("{} = \"{}\"\n", key, val_str));
                }
            }
        }
    } else {
        // Default values if sap_config doesn't exist
        new_config.push_str("instance_id = \"rs\"\n");
    }
    new_config.push_str("\n");

    // Create tcode sections
    let mut has_tcode_section = false;
    if let Some(sap_config) = parsed.get("sap_config").and_then(|v| v.as_table()) {
        // Extract tcode
        if let Some(tcode) = sap_config.get("tcode").and_then(|v| v.as_str()) {
            has_tcode_section = true;
            new_config.push_str(&format!("[tcode.{}]\n", tcode));

            // Extract variant
            if let Some(variant) = sap_config.get("variant").and_then(|v| v.as_str()) {
                new_config.push_str(&format!("variant = \"{}\"\n", variant));
            }

            // Extract layout
            if let Some(layout) = sap_config.get("layout").and_then(|v| v.as_str()) {
                new_config.push_str(&format!("layout = \"{}\"\n", layout));
            }

            // Extract column_name
            if let Some(column_name) = sap_config.get("column_name").and_then(|v| v.as_str()) {
                new_config.push_str(&format!("column_name = \"{}\"\n", column_name));
            }

            // Extract date_range_start and date_range_end
            if let Some(date_range_start) = sap_config.get("date_range_start").and_then(|v| v.as_str()) {
                new_config.push_str(&format!("date_range_start = \"{}\"\n", date_range_start));
            }

            if let Some(date_range_end) = sap_config.get("date_range_end").and_then(|v| v.as_str()) {
                new_config.push_str(&format!("date_range_end = \"{}\"\n", date_range_end));
            }

            // Extract tcode-specific parameters
            for (key, value) in sap_config {
                if key.starts_with(&format!("{}_", tcode)) {
                    if let Some(val_str) = value.as_str() {
                        let param_name = key.replacen(&format!("{}_", tcode), "", 1);
                        new_config.push_str(&format!("{} = \"{}\"\n", param_name, val_str));
                    }
                }
            }

            new_config.push_str("\n");
        }
    }

    // Create loop section
    let mut has_loop_section = false;
    if let Some(sap_config) = parsed.get("sap_config").and_then(|v| v.as_table()) {
        // Check if there are any loop-related parameters
        let has_loop_params = sap_config.keys().any(|k| k.starts_with("loop_"));

        if has_loop_params {
            has_loop_section = true;
            new_config.push_str("[loop]\n");

            // Extract loop_tcode
            if let Some(loop_tcode) = sap_config.get("loop_tcode").and_then(|v| v.as_str()) {
                new_config.push_str(&format!("tcode = \"{}\"\n", loop_tcode));
            } else if let Some(tcode) = sap_config.get("tcode").and_then(|v| v.as_str()) {
                // Default to tcode if loop_tcode is not specified
                new_config.push_str(&format!("tcode = \"{}\"\n", tcode));
            } else {
                new_config.push_str("tcode = \"\"\n");
            }

            // Extract loop_iterations
            if let Some(loop_iterations) = sap_config.get("loop_iterations").and_then(|v| v.as_str()) {
                new_config.push_str(&format!("iterations = \"{}\"\n", loop_iterations));
            } else {
                new_config.push_str("iterations = \"1\"\n");
            }

            // Extract loop_delay_seconds
            if let Some(loop_delay_seconds) = sap_config.get("loop_delay_seconds").and_then(|v| v.as_str()) {
                new_config.push_str(&format!("delay_seconds = \"{}\"\n", loop_delay_seconds));
            } else {
                new_config.push_str("delay_seconds = \"60\"\n");
            }

            // Extract loop parameters
            for (key, value) in sap_config {
                if key.starts_with("loop_param_") {
                    if let Some(val_str) = value.as_str() {
                        let param_name = key.replacen("loop_param_", "", 1);
                        new_config.push_str(&format!("param_{} = \"{}\"\n", param_name, val_str));
                    }
                } else if key.starts_with("loop_") && !["loop_tcode", "loop_iterations", "loop_delay_seconds"].contains(&key.as_str()) {
                    if let Some(val_str) = value.as_str() {
                        let param_name = key.replacen("loop_", "", 1);
                        new_config.push_str(&format!("{} = \"{}\"\n", param_name, val_str));
                    }
                }
            }

            new_config.push_str("\n");
        }
    }

    // Write the new config file
    match fs::write("config.toml", new_config) {
        Ok(_) => {
            println!("Migration completed successfully!");
            println!("Your config.toml file has been updated to the new format.");
            if has_tcode_section {
                println!("- Added tcode-specific configuration section");
            }
            if has_loop_section {
                println!("- Added loop configuration section");
            }
            println!("\nIf you need to revert to the original configuration, rename config.toml.bak to config.toml");
        }
        Err(e) => {
            println!("Error writing new config.toml: {}", e);
            println!("Your original config.toml has been preserved as config.toml.bak");
            process::exit(1);
        }
    }
}
