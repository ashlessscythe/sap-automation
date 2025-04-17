use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use toml::Value;

/// Configuration structure for SAP automation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SapConfig {
    #[serde(skip)]
    pub config_path: String,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub global: Option<GlobalConfig>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub build: Option<BuildConfig>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tcode: Option<HashMap<String, TcodeConfig>>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub loop_config: Option<LoopConfig>,
    
    #[serde(skip)]
    pub raw_config: Option<toml::Value>,
}

/// Global configuration settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalConfig {
    #[serde(default = "default_instance_id")]
    pub instance_id: String,
    
    #[serde(default = "get_default_reports_dir")]
    pub reports_dir: String,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_tcode: Option<String>,
    
    #[serde(flatten)]
    pub additional_params: HashMap<String, String>,
}

/// Build configuration settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildConfig {
    pub target: String,
    
    #[serde(flatten)]
    pub additional_params: HashMap<String, String>,
}

/// TCode-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TcodeConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variant: Option<String>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layout: Option<String>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column_name: Option<String>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date_range_start: Option<String>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date_range_end: Option<String>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub by_date: Option<String>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub serial_number: Option<String>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tab_number: Option<String>,
    
    #[serde(flatten)]
    pub additional_params: HashMap<String, String>,
}

/// Loop configuration settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoopConfig {
    pub tcode: String,
    
    #[serde(default = "default_iterations")]
    pub iterations: String,
    
    #[serde(default = "default_delay_seconds")]
    pub delay_seconds: String,
    
    #[serde(flatten)]
    pub params: HashMap<String, String>,
}

/// Gets the default reports directory path
pub fn get_default_reports_dir() -> String {
    match env::var("USERPROFILE") {
        Ok(profile) => format!("{}\\Documents\\Reports", profile),
        Err(_) => {
            eprintln!("Could not determine user profile directory");
            String::from(".\\Reports")
        }
    }
}

/// Default instance ID
pub fn default_instance_id() -> String {
    "rs".to_string()
}

/// Default iterations
pub fn default_iterations() -> String {
    "1".to_string()
}

/// Default delay seconds
pub fn default_delay_seconds() -> String {
    "60".to_string()
}
