use std::collections::HashMap;
use std::fs;
use std::path::Path;
use chrono::NaiveDate;

// Import the necessary modules from the crate
use sap_automation::utils::config_ops::SapConfig;

// Mock function to create VL06O params from config (similar to the one in vl06o_module.rs)
fn create_vl06o_params_from_config(config: &HashMap<String, String>) -> TestVL06OParams {
    let mut params = TestVL06OParams::default();

    // Set variant if available
    if let Some(variant) = config.get("variant") {
        params.sap_variant_name = Some(variant.clone());
    }

    // Set layout if available
    if let Some(layout) = config.get("layout") {
        params.layout_row = Some(layout.clone());
    }

    // Set date range if available
    if let Some(start_date) = config.get("date_range_start") {
        if let Ok(date) = parse_date(start_date) {
            params.start_date = date;
        }
    }

    if let Some(end_date) = config.get("date_range_end") {
        if let Ok(date) = parse_date(end_date) {
            params.end_date = date;
        }
    }

    // Set by_date if available
    if let Some(by_date) = config.get("by_date") {
        params.by_date = by_date.to_lowercase() == "true";
    }

    // Set column_name if available
    if let Some(column_name) = config.get("column_name") {
        params.column_name = Some(column_name.clone());
    }

    params
}

// Mock function to create VT11 params from config (similar to the one in vt11_module.rs)
fn create_vt11_params_from_config(config: &HashMap<String, String>) -> TestVT11Params {
    let mut params = TestVT11Params::default();

    // Set variant if available
    if let Some(variant) = config.get("variant") {
        params.sap_variant_name = Some(variant.clone());
    }

    // Set layout if available
    if let Some(layout) = config.get("layout") {
        params.layout_row = Some(layout.clone());
    }

    // Set date range if available
    if let Some(start_date) = config.get("date_range_start") {
        if let Ok(date) = parse_date(start_date) {
            params.start_date = date;
        }
    }

    if let Some(end_date) = config.get("date_range_end") {
        if let Ok(date) = parse_date(end_date) {
            params.end_date = date;
        }
    }

    // Set column_name if available
    if let Some(column_name) = config.get("column_name") {
        params.column_name = Some(column_name.clone());
    }

    params
}

// Mock function to create ZMDESNR params from config (similar to the one in zmdesnr_module.rs)
fn create_zmdesnr_params_from_config(config: &HashMap<String, String>) -> TestZMDESNRParams {
    let mut params = TestZMDESNRParams::default();

    // Set variant if available
    if let Some(variant) = config.get("variant") {
        params.sap_variant_name = Some(variant.clone());
    }

    // Set layout if available
    if let Some(layout) = config.get("layout") {
        params.layout_row = Some(layout.clone());
    }

    // Set serial_number if available
    if let Some(serial_number) = config.get("serial_number") {
        params.serial_number = serial_number.clone();
    }

    params
}

// Helper function to parse date strings
fn parse_date(date_str: &str) -> Result<NaiveDate, &'static str> {
    // Try to parse the date in MM/DD/YYYY format
    if let Ok(date) = NaiveDate::parse_from_str(date_str, "%m/%d/%Y") {
        return Ok(date);
    }

    // Try to parse the date in MM-DD-YYYY format
    if let Ok(date) = NaiveDate::parse_from_str(date_str, "%m-%d-%Y") {
        return Ok(date);
    }

    // Try to parse the date in YYYY-MM-DD format
    if let Ok(date) = NaiveDate::parse_from_str(date_str, "%Y-%m-%d") {
        return Ok(date);
    }

    // If all parsing attempts fail, return an error
    Err("Failed to parse date")
}

// Test structs to mimic the real parameter structs without SAP dependencies
#[derive(Debug, Default, PartialEq, Clone)]
struct TestVL06OParams {
    pub start_date: NaiveDate,
    pub end_date: NaiveDate,
    pub sap_variant_name: Option<String>,
    pub layout_row: Option<String>,
    pub by_date: bool,
    pub column_name: Option<String>,
    pub shipment_numbers: Vec<String>,
}

#[derive(Debug, Default, PartialEq, Clone)]
struct TestVT11Params {
    pub start_date: NaiveDate,
    pub end_date: NaiveDate,
    pub sap_variant_name: Option<String>,
    pub layout_row: Option<String>,
    pub column_name: Option<String>,
}

#[derive(Debug, Default, PartialEq, Clone)]
struct TestZMDESNRParams {
    pub sap_variant_name: Option<String>,
    pub layout_row: Option<String>,
    pub serial_number: String,
}

// Create a temporary config file for testing
fn create_test_config(content: &str) -> String {
    let temp_dir = std::env::temp_dir();
    let config_path = temp_dir.join("test_config.toml");
    fs::write(&config_path, content).expect("Failed to write test config file");
    config_path.to_string_lossy().to_string()
}

// Helper function to create a SapConfig with a specific reports_dir
fn create_test_sap_config(reports_dir: &str) -> SapConfig {
    let mut config = SapConfig::default();
    config.reports_dir = reports_dir.to_string();
    config
}

#[test]
fn test_load_config() {
    // Create a test config file
    let config_content = r#"
[sap_config]
reports_dir = "C:\\Test\\Reports"
tcode = "VL06O"
variant = "TEST_VARIANT"
layout = "TEST_LAYOUT"
column_name = "Test Column"
date_range_start = "04/01/2025"
date_range_end = "04/15/2025"
"#;
    
    let config_path = create_test_config(config_content);
    
    // Create a backup of the current config.toml if it exists
    let backup_path = "config.toml.bak";
    let config_exists = Path::new("config.toml").exists();
    if config_exists {
        fs::copy("config.toml", backup_path).expect("Failed to backup config.toml");
    }
    
    // Copy our test config to the project directory
    fs::copy(&config_path, "config.toml").expect("Failed to copy test config to project directory");
    
    // Load the config
    let config = SapConfig::load().expect("Failed to load config");
    
    // Verify the config was loaded correctly
    assert_eq!(config.reports_dir, "C:\\Test\\Reports");
    assert_eq!(config.tcode, Some("VL06O".to_string()));
    assert_eq!(config.variant, Some("TEST_VARIANT".to_string()));
    assert_eq!(config.layout, Some("TEST_LAYOUT".to_string()));
    assert_eq!(config.column_name, Some("Test Column".to_string()));
    assert_eq!(config.date_range, Some(("04/01/2025".to_string(), "04/15/2025".to_string())));
    
    // Restore the original config.toml if it existed
    if config_exists {
        fs::copy(backup_path, "config.toml").expect("Failed to restore config.toml");
        fs::remove_file(backup_path).expect("Failed to remove backup file");
    } else {
        fs::remove_file("config.toml").expect("Failed to remove test config file");
    }
}

#[test]
fn test_get_tcode_config() {
    // Create a test SapConfig
    let mut config = SapConfig::new();
    config.tcode = Some("VL06O".to_string());
    config.variant = Some("TEST_VARIANT".to_string());
    config.layout = Some("TEST_LAYOUT".to_string());
    config.column_name = Some("Test Column".to_string());
    config.date_range = Some(("04/01/2025".to_string(), "04/15/2025".to_string()));
    
    // Add some additional parameters
    let mut additional_params = HashMap::new();
    additional_params.insert("VL06O_by_date".to_string(), "true".to_string());
    additional_params.insert("VT11_custom_param".to_string(), "custom_value".to_string());
    config.additional_params = additional_params;
    
    // Get the VL06O config
    let vl06o_config = config.get_tcode_config("VL06O").expect("Failed to get VL06O config");
    
    // Verify the VL06O config
    assert_eq!(vl06o_config.get("variant"), Some(&"TEST_VARIANT".to_string()));
    assert_eq!(vl06o_config.get("layout"), Some(&"TEST_LAYOUT".to_string()));
    assert_eq!(vl06o_config.get("column_name"), Some(&"Test Column".to_string()));
    assert_eq!(vl06o_config.get("date_range_start"), Some(&"04/01/2025".to_string()));
    assert_eq!(vl06o_config.get("date_range_end"), Some(&"04/15/2025".to_string()));
    assert_eq!(vl06o_config.get("by_date"), Some(&"true".to_string()));
    
    // Get the VT11 config
    let vt11_config = config.get_tcode_config("VT11").expect("Failed to get VT11 config");
    
    // Verify the VT11 config has the custom parameter but not the VL06O-specific ones
    assert_eq!(vt11_config.get("custom_param"), Some(&"custom_value".to_string()));
    assert!(vt11_config.get("by_date").is_none());
}

#[test]
fn test_vl06o_params_from_config() {
    // Create a test config HashMap
    let mut config = HashMap::new();
    config.insert("variant".to_string(), "TEST_VARIANT".to_string());
    config.insert("layout".to_string(), "TEST_LAYOUT".to_string());
    config.insert("column_name".to_string(), "Test Column".to_string());
    config.insert("date_range_start".to_string(), "04/01/2025".to_string());
    config.insert("date_range_end".to_string(), "04/15/2025".to_string());
    config.insert("by_date".to_string(), "true".to_string());
    
    // Create VL06O params from the config
    let params = create_vl06o_params_from_config(&config);
    
    // Verify the params
    assert_eq!(params.sap_variant_name, Some("TEST_VARIANT".to_string()));
    assert_eq!(params.layout_row, Some("TEST_LAYOUT".to_string()));
    assert_eq!(params.column_name, Some("Test Column".to_string()));
    assert_eq!(params.by_date, true);
    
    // Verify the dates
    let expected_start_date = NaiveDate::parse_from_str("04/01/2025", "%m/%d/%Y").unwrap();
    let expected_end_date = NaiveDate::parse_from_str("04/15/2025", "%m/%d/%Y").unwrap();
    assert_eq!(params.start_date, expected_start_date);
    assert_eq!(params.end_date, expected_end_date);
}

#[test]
fn test_vt11_params_from_config() {
    // Create a test config HashMap
    let mut config = HashMap::new();
    config.insert("variant".to_string(), "TEST_VARIANT".to_string());
    config.insert("layout".to_string(), "TEST_LAYOUT".to_string());
    config.insert("column_name".to_string(), "Test Column".to_string());
    config.insert("date_range_start".to_string(), "04/01/2025".to_string());
    config.insert("date_range_end".to_string(), "04/15/2025".to_string());
    
    // Create VT11 params from the config
    let params = create_vt11_params_from_config(&config);
    
    // Verify the params
    assert_eq!(params.sap_variant_name, Some("TEST_VARIANT".to_string()));
    assert_eq!(params.layout_row, Some("TEST_LAYOUT".to_string()));
    assert_eq!(params.column_name, Some("Test Column".to_string()));
    
    // Verify the dates
    let expected_start_date = NaiveDate::parse_from_str("04/01/2025", "%m/%d/%Y").unwrap();
    let expected_end_date = NaiveDate::parse_from_str("04/15/2025", "%m/%d/%Y").unwrap();
    assert_eq!(params.start_date, expected_start_date);
    assert_eq!(params.end_date, expected_end_date);
}

#[test]
fn test_zmdesnr_params_from_config() {
    // Create a test config HashMap
    let mut config = HashMap::new();
    config.insert("variant".to_string(), "TEST_VARIANT".to_string());
    config.insert("layout".to_string(), "TEST_LAYOUT".to_string());
    config.insert("serial_number".to_string(), "123456789".to_string());
    
    // Create ZMDESNR params from the config
    let params = create_zmdesnr_params_from_config(&config);
    
    // Verify the params
    assert_eq!(params.sap_variant_name, Some("TEST_VARIANT".to_string()));
    assert_eq!(params.layout_row, Some("TEST_LAYOUT".to_string()));
    assert_eq!(params.serial_number, "123456789".to_string());
}

#[test]
fn test_config_with_different_date_formats() {
    // Test with MM/DD/YYYY format
    let mut config1 = HashMap::new();
    config1.insert("date_range_start".to_string(), "04/01/2025".to_string());
    config1.insert("date_range_end".to_string(), "04/15/2025".to_string());
    
    let params1 = create_vl06o_params_from_config(&config1);
    let expected_start1 = NaiveDate::parse_from_str("04/01/2025", "%m/%d/%Y").unwrap();
    let expected_end1 = NaiveDate::parse_from_str("04/15/2025", "%m/%d/%Y").unwrap();
    assert_eq!(params1.start_date, expected_start1);
    assert_eq!(params1.end_date, expected_end1);
    
    // Test with MM-DD-YYYY format
    let mut config2 = HashMap::new();
    config2.insert("date_range_start".to_string(), "04-01-2025".to_string());
    config2.insert("date_range_end".to_string(), "04-15-2025".to_string());
    
    let params2 = create_vl06o_params_from_config(&config2);
    let expected_start2 = NaiveDate::parse_from_str("04-01-2025", "%m-%d-%Y").unwrap();
    let expected_end2 = NaiveDate::parse_from_str("04-15-2025", "%m-%d-%Y").unwrap();
    assert_eq!(params2.start_date, expected_start2);
    assert_eq!(params2.end_date, expected_end2);
    
    // Test with YYYY-MM-DD format
    let mut config3 = HashMap::new();
    config3.insert("date_range_start".to_string(), "2025-04-01".to_string());
    config3.insert("date_range_end".to_string(), "2025-04-15".to_string());
    
    let params3 = create_vl06o_params_from_config(&config3);
    let expected_start3 = NaiveDate::parse_from_str("2025-04-01", "%Y-%m-%d").unwrap();
    let expected_end3 = NaiveDate::parse_from_str("2025-04-15", "%Y-%m-%d").unwrap();
    assert_eq!(params3.start_date, expected_start3);
    assert_eq!(params3.end_date, expected_end3);
}

#[test]
fn test_tcode_specific_params() {
    // Create a test SapConfig
    let mut config = SapConfig::new();
    
    // Add tcode-specific parameters
    let mut additional_params = HashMap::new();
    additional_params.insert("VL06O_by_date".to_string(), "true".to_string());
    additional_params.insert("VL06O_custom_param".to_string(), "vl06o_value".to_string());
    additional_params.insert("VT11_custom_param".to_string(), "vt11_value".to_string());
    additional_params.insert("ZMDESNR_serial_number".to_string(), "123456789".to_string());
    config.additional_params = additional_params;
    
    // Get the VL06O config
    let vl06o_config = config.get_tcode_config("VL06O").expect("Failed to get VL06O config");
    
    // Verify the VL06O config has the VL06O-specific parameters
    assert_eq!(vl06o_config.get("by_date"), Some(&"true".to_string()));
    assert_eq!(vl06o_config.get("custom_param"), Some(&"vl06o_value".to_string()));
    assert!(vl06o_config.get("serial_number").is_none());
    
    // Get the VT11 config
    let vt11_config = config.get_tcode_config("VT11").expect("Failed to get VT11 config");
    
    // Verify the VT11 config has the VT11-specific parameters
    assert_eq!(vt11_config.get("custom_param"), Some(&"vt11_value".to_string()));
    assert!(vt11_config.get("by_date").is_none());
    
    // Get the ZMDESNR config
    let zmdesnr_config = config.get_tcode_config("ZMDESNR").expect("Failed to get ZMDESNR config");
    
    // Verify the ZMDESNR config has the ZMDESNR-specific parameters
    assert_eq!(zmdesnr_config.get("serial_number"), Some(&"123456789".to_string()));
    assert!(zmdesnr_config.get("by_date").is_none());
}

#[test]
fn test_missing_config_params() {
    // Create an empty config HashMap
    let config = HashMap::new();
    
    // Create params from the empty config
    let vl06o_params = create_vl06o_params_from_config(&config);
    let vt11_params = create_vt11_params_from_config(&config);
    let zmdesnr_params = create_zmdesnr_params_from_config(&config);
    
    // Verify default values are used
    assert_eq!(vl06o_params.sap_variant_name, None);
    assert_eq!(vl06o_params.layout_row, None);
    assert_eq!(vl06o_params.column_name, None);
    assert_eq!(vl06o_params.by_date, false);
    
    assert_eq!(vt11_params.sap_variant_name, None);
    assert_eq!(vt11_params.layout_row, None);
    assert_eq!(vt11_params.column_name, None);
    
    assert_eq!(zmdesnr_params.sap_variant_name, None);
    assert_eq!(zmdesnr_params.layout_row, None);
    assert_eq!(zmdesnr_params.serial_number, "");
}
