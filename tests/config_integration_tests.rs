use std::fs;
use std::path::Path;
use chrono::NaiveDate;

// Import the necessary modules from the crate
use sap_automation::utils::config_ops::SapConfig;

// Mock structs for testing
#[derive(Debug, Default, PartialEq, Clone)]
struct MockVL06OParams {
    pub start_date: NaiveDate,
    pub end_date: NaiveDate,
    pub sap_variant_name: Option<String>,
    pub layout_row: Option<String>,
    pub by_date: bool,
    pub column_name: Option<String>,
}

#[derive(Debug, Default, PartialEq, Clone)]
struct MockVT11Params {
    pub start_date: NaiveDate,
    pub end_date: NaiveDate,
    pub sap_variant_name: Option<String>,
    pub layout_row: Option<String>,
    pub column_name: Option<String>,
}

#[derive(Debug, Default, PartialEq, Clone)]
struct MockZMDESNRParams {
    pub sap_variant_name: Option<String>,
    pub layout_row: Option<String>,
    pub serial_number: String,
}

// Mock auto run functions
fn mock_auto_run_vl06o(config_path: &str) -> Result<MockVL06OParams, String> {
    // Load the config
    let config = match SapConfig::load() {
        Ok(cfg) => cfg,
        Err(e) => return Err(format!("Failed to load config: {}", e)),
    };

    // Get VL06O specific configuration
    let tcode_config = match config.get_tcode_config("VL06O") {
        Some(cfg) => cfg,
        None => return Err("No configuration found for VL06O".to_string()),
    };

    // Create params from config
    let mut params = MockVL06OParams::default();

    // Set variant if available
    if let Some(variant) = tcode_config.get("variant") {
        params.sap_variant_name = Some(variant.clone());
    }

    // Set layout if available
    if let Some(layout) = tcode_config.get("layout") {
        params.layout_row = Some(layout.clone());
    }

    // Set date range if available
    if let Some(start_date) = tcode_config.get("date_range_start") {
        if let Ok(date) = parse_date(start_date) {
            params.start_date = date;
        }
    }

    if let Some(end_date) = tcode_config.get("date_range_end") {
        if let Ok(date) = parse_date(end_date) {
            params.end_date = date;
        }
    }

    // Set by_date if available
    if let Some(by_date) = tcode_config.get("by_date") {
        params.by_date = by_date.to_lowercase() == "true";
    }

    // Set column_name if available
    if let Some(column_name) = tcode_config.get("column_name") {
        params.column_name = Some(column_name.clone());
    }

    Ok(params)
}

fn mock_auto_run_vt11(config_path: &str) -> Result<MockVT11Params, String> {
    // Load the config
    let config = match SapConfig::load() {
        Ok(cfg) => cfg,
        Err(e) => return Err(format!("Failed to load config: {}", e)),
    };

    // Get VT11 specific configuration
    let tcode_config = match config.get_tcode_config("VT11") {
        Some(cfg) => cfg,
        None => return Err("No configuration found for VT11".to_string()),
    };

    // Create params from config
    let mut params = MockVT11Params::default();

    // Set variant if available
    if let Some(variant) = tcode_config.get("variant") {
        params.sap_variant_name = Some(variant.clone());
    }

    // Set layout if available
    if let Some(layout) = tcode_config.get("layout") {
        params.layout_row = Some(layout.clone());
    }

    // Set date range if available
    if let Some(start_date) = tcode_config.get("date_range_start") {
        if let Ok(date) = parse_date(start_date) {
            params.start_date = date;
        }
    }

    if let Some(end_date) = tcode_config.get("date_range_end") {
        if let Ok(date) = parse_date(end_date) {
            params.end_date = date;
        }
    }

    // Set column_name if available
    if let Some(column_name) = tcode_config.get("column_name") {
        params.column_name = Some(column_name.clone());
    }

    Ok(params)
}

fn mock_auto_run_zmdesnr(config_path: &str) -> Result<MockZMDESNRParams, String> {
    // Load the config
    let config = match SapConfig::load() {
        Ok(cfg) => cfg,
        Err(e) => return Err(format!("Failed to load config: {}", e)),
    };

    // Get ZMDESNR specific configuration
    let tcode_config = match config.get_tcode_config("ZMDESNR") {
        Some(cfg) => cfg,
        None => return Err("No configuration found for ZMDESNR".to_string()),
    };

    // Create params from config
    let mut params = MockZMDESNRParams::default();

    // Set variant if available
    if let Some(variant) = tcode_config.get("variant") {
        params.sap_variant_name = Some(variant.clone());
    }

    // Set layout if available
    if let Some(layout) = tcode_config.get("layout") {
        params.layout_row = Some(layout.clone());
    }

    // Set serial_number if available
    if let Some(serial_number) = tcode_config.get("serial_number") {
        params.serial_number = serial_number.clone();
    }

    Ok(params)
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

// Create a test config file and set it as the current config
fn setup_test_config(content: &str) -> (String, bool) {
    let temp_dir = std::env::temp_dir();
    let config_path = temp_dir.join("test_config.toml");
    fs::write(&config_path, content).expect("Failed to write test config file");
    
    // Create a backup of the current config.toml if it exists
    let backup_path = "config.toml.bak";
    let config_exists = Path::new("config.toml").exists();
    if config_exists {
        fs::copy("config.toml", backup_path).expect("Failed to backup config.toml");
    }
    
    // Copy our test config to the project directory
    fs::copy(&config_path, "config.toml").expect("Failed to copy test config to project directory");
    
    (backup_path.to_string(), config_exists)
}

// Restore the original config.toml
fn teardown_test_config(backup_path: &str, config_existed: bool) {
    if config_existed {
        fs::copy(backup_path, "config.toml").expect("Failed to restore config.toml");
        fs::remove_file(backup_path).expect("Failed to remove backup file");
    } else {
        fs::remove_file("config.toml").expect("Failed to remove test config file");
    }
}

#[test]
fn test_vl06o_integration() {
    // Create a test config
    let config_content = r#"
[sap_config]
reports_dir = "C:\\Test\\Reports"
tcode = "VL06O"
variant = "TEST_VARIANT"
layout = "TEST_LAYOUT"
column_name = "Test Column"
date_range_start = "04/01/2025"
date_range_end = "04/15/2025"
by_date = "true"
"#;
    
    let (backup_path, config_existed) = setup_test_config(config_content);
    
    // Run the mock auto run
    let result = mock_auto_run_vl06o("config.toml");
    
    // Verify the result
    assert!(result.is_ok(), "VL06O auto run failed: {:?}", result.err());
    let params = result.unwrap();
    
    // Check the parameters
    assert_eq!(params.sap_variant_name, Some("TEST_VARIANT".to_string()));
    assert_eq!(params.layout_row, Some("TEST_LAYOUT".to_string()));
    assert_eq!(params.column_name, Some("Test Column".to_string()));
    assert_eq!(params.by_date, true);
    
    // Check the dates
    let expected_start = NaiveDate::parse_from_str("04/01/2025", "%m/%d/%Y").unwrap();
    let expected_end = NaiveDate::parse_from_str("04/15/2025", "%m/%d/%Y").unwrap();
    assert_eq!(params.start_date, expected_start);
    assert_eq!(params.end_date, expected_end);
    
    // Restore the original config
    teardown_test_config(&backup_path, config_existed);
}

#[test]
fn test_vt11_integration() {
    // Create a test config
    let config_content = r#"
[sap_config]
reports_dir = "C:\\Test\\Reports"
tcode = "VT11"
variant = "TEST_VARIANT"
layout = "TEST_LAYOUT"
column_name = "Test Column"
date_range_start = "04/01/2025"
date_range_end = "04/15/2025"
"#;
    
    let (backup_path, config_existed) = setup_test_config(config_content);
    
    // Run the mock auto run
    let result = mock_auto_run_vt11("config.toml");
    
    // Verify the result
    assert!(result.is_ok(), "VT11 auto run failed: {:?}", result.err());
    let params = result.unwrap();
    
    // Check the parameters
    assert_eq!(params.sap_variant_name, Some("TEST_VARIANT".to_string()));
    assert_eq!(params.layout_row, Some("TEST_LAYOUT".to_string()));
    assert_eq!(params.column_name, Some("Test Column".to_string()));
    
    // Check the dates
    let expected_start = NaiveDate::parse_from_str("04/01/2025", "%m/%d/%Y").unwrap();
    let expected_end = NaiveDate::parse_from_str("04/15/2025", "%m/%d/%Y").unwrap();
    assert_eq!(params.start_date, expected_start);
    assert_eq!(params.end_date, expected_end);
    
    // Restore the original config
    teardown_test_config(&backup_path, config_existed);
}

#[test]
fn test_zmdesnr_integration() {
    // Create a test config
    let config_content = r#"
[sap_config]
reports_dir = "C:\\Test\\Reports"
tcode = "ZMDESNR"
variant = "TEST_VARIANT"
layout = "TEST_LAYOUT"
serial_number = "123456789"
"#;
    
    let (backup_path, config_existed) = setup_test_config(config_content);
    
    // Run the mock auto run
    let result = mock_auto_run_zmdesnr("config.toml");
    
    // Verify the result
    assert!(result.is_ok(), "ZMDESNR auto run failed: {:?}", result.err());
    let params = result.unwrap();
    
    // Check the parameters
    assert_eq!(params.sap_variant_name, Some("TEST_VARIANT".to_string()));
    assert_eq!(params.layout_row, Some("TEST_LAYOUT".to_string()));
    assert_eq!(params.serial_number, "123456789".to_string());
    
    // Restore the original config
    teardown_test_config(&backup_path, config_existed);
}

#[test]
fn test_tcode_specific_integration() {
    // Create a test config with tcode-specific parameters
    let config_content = r#"
[sap_config]
reports_dir = "C:\\Test\\Reports"
variant = "COMMON_VARIANT"
layout = "COMMON_LAYOUT"
date_range_start = "04/01/2025"
date_range_end = "04/15/2025"
VL06O_variant = "VL06O_VARIANT"
VL06O_by_date = "true"
VT11_column_name = "VT11 Column"
ZMDESNR_serial_number = "123456789"
"#;
    
    let (backup_path, config_existed) = setup_test_config(config_content);
    
    // Run the mock auto runs
    let vl06o_result = mock_auto_run_vl06o("config.toml");
    let vt11_result = mock_auto_run_vt11("config.toml");
    let zmdesnr_result = mock_auto_run_zmdesnr("config.toml");
    
    // Verify the VL06O result
    assert!(vl06o_result.is_ok(), "VL06O auto run failed: {:?}", vl06o_result.err());
    let vl06o_params = vl06o_result.unwrap();
    
    // Check the VL06O parameters (should use VL06O-specific variant)
    assert_eq!(vl06o_params.sap_variant_name, Some("VL06O_VARIANT".to_string()));
    assert_eq!(vl06o_params.layout_row, Some("COMMON_LAYOUT".to_string()));
    assert_eq!(vl06o_params.by_date, true);
    
    // Verify the VT11 result
    assert!(vt11_result.is_ok(), "VT11 auto run failed: {:?}", vt11_result.err());
    let vt11_params = vt11_result.unwrap();
    
    // Check the VT11 parameters (should use common variant but VT11-specific column name)
    assert_eq!(vt11_params.sap_variant_name, Some("COMMON_VARIANT".to_string()));
    assert_eq!(vt11_params.layout_row, Some("COMMON_LAYOUT".to_string()));
    assert_eq!(vt11_params.column_name, Some("VT11 Column".to_string()));
    
    // Verify the ZMDESNR result
    assert!(zmdesnr_result.is_ok(), "ZMDESNR auto run failed: {:?}", zmdesnr_result.err());
    let zmdesnr_params = zmdesnr_result.unwrap();
    
    // Check the ZMDESNR parameters (should use common variant and ZMDESNR-specific serial number)
    assert_eq!(zmdesnr_params.sap_variant_name, Some("COMMON_VARIANT".to_string()));
    assert_eq!(zmdesnr_params.layout_row, Some("COMMON_LAYOUT".to_string()));
    assert_eq!(zmdesnr_params.serial_number, "123456789".to_string());
    
    // Restore the original config
    teardown_test_config(&backup_path, config_existed);
}

#[test]
fn test_missing_config_integration() {
    // Create a test config with missing parameters
    let config_content = r#"
[sap_config]
reports_dir = "C:\\Test\\Reports"
"#;
    
    let (backup_path, config_existed) = setup_test_config(config_content);
    
    // Run the mock auto runs
    let vl06o_result = mock_auto_run_vl06o("config.toml");
    let vt11_result = mock_auto_run_vt11("config.toml");
    let zmdesnr_result = mock_auto_run_zmdesnr("config.toml");
    
    // All should fail due to missing configurations
    assert!(vl06o_result.is_err(), "VL06O auto run should have failed with missing config");
    assert!(vt11_result.is_err(), "VT11 auto run should have failed with missing config");
    assert!(zmdesnr_result.is_err(), "ZMDESNR auto run should have failed with missing config");
    
    // Verify the error messages
    let vl06o_error = vl06o_result.err().unwrap();
    assert!(vl06o_error.contains("No configuration found for VL06O"), 
            "Unexpected error for VL06O: {}", vl06o_error);
    let vt11_error = vt11_result.err().unwrap();
    assert!(vt11_error.contains("No configuration found for VT11"), 
            "Unexpected error for VT11: {}", vt11_error);
    let zmdesnr_error = zmdesnr_result.err().unwrap();
    assert!(zmdesnr_error.contains("No configuration found for ZMDESNR"), 
            "Unexpected error for ZMDESNR: {}", zmdesnr_error);
    
    // Restore the original config
    teardown_test_config(&backup_path, config_existed);
}

#[test]
fn test_date_format_integration() {
    // Create a test config with different date formats
    let config_content = r#"
[sap_config]
reports_dir = "C:\\Test\\Reports"
tcode = "VL06O"
variant = "TEST_VARIANT"
layout = "TEST_LAYOUT"
date_range_start = "2025-04-01"  # YYYY-MM-DD format
date_range_end = "04-15-2025"    # MM-DD-YYYY format
"#;
    
    let (backup_path, config_existed) = setup_test_config(config_content);
    
    // Run the mock auto run
    let result = mock_auto_run_vl06o("config.toml");
    
    // Verify the result
    assert!(result.is_ok(), "VL06O auto run failed: {:?}", result.err());
    let params = result.unwrap();
    
    // Check the dates
    let expected_start = NaiveDate::parse_from_str("2025-04-01", "%Y-%m-%d").unwrap();
    let expected_end = NaiveDate::parse_from_str("04-15-2025", "%m-%d-%Y").unwrap();
    assert_eq!(params.start_date, expected_start);
    assert_eq!(params.end_date, expected_end);
    
    // Restore the original config
    teardown_test_config(&backup_path, config_existed);
}
