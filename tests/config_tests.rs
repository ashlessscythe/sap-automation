use std::fs;
use std::path::Path;

#[test]
fn test_config_file_exists() {
    // Check if config.toml exists
    assert!(Path::new("config.toml").exists(), "config.toml file should exist");
}

#[test]
fn test_config_file_has_sap_config_section() {
    // Read the config file
    let content = fs::read_to_string("config.toml").expect("Failed to read config.toml");
    
    // Check if it has the [sap_config] section
    assert!(content.contains("[sap_config]"), "config.toml should have [sap_config] section");
}

#[test]
fn test_config_file_has_reports_dir() {
    // Read the config file
    let content = fs::read_to_string("config.toml").expect("Failed to read config.toml");
    
    // Check if it has the reports_dir setting
    assert!(content.contains("reports_dir"), "config.toml should have reports_dir setting");
}
