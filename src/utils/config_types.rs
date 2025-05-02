use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use toml::Value;
use std::vec::Vec;

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
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sequence: Option<SequenceConfig>,
    
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
    
    #[serde(default = "default_date_format")]
    pub date_format: String,
    
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

    #[serde(skip_serializing_if = "Option::is_none")]
    pub subdir: Option<String>,
    
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

/// Sequence configuration settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SequenceConfig {
    #[serde(default = "default_sequence_options")]
    pub options: Vec<String>,
    
    #[serde(default = "default_iterations")]
    pub iterations: String,
    
    #[serde(default = "default_delay_seconds")]
    pub delay_seconds: String,
    
    #[serde(default = "default_interval_seconds")]
    pub interval_seconds: String,
    
    #[serde(flatten)]
    pub params: HashMap<String, String>,
}

/// Gets the default reports directory path with \\\\
// return double backslash based on userprofile
pub fn get_default_reports_dir() -> String {
    let user_profile = env::var("USERPROFILE").unwrap_or_else(|_| ".".to_string());
    let formatted_path = format!("{}\\Documents\\Reports", user_profile); // Ensure double backslashes

    formatted_path.replace("\\", "\\\\") // Replace single backslashes with double backslashes
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

/// Default interval seconds between sequence steps
pub fn default_interval_seconds() -> String {
    "10".to_string()
}

/// Default sequence options
pub fn default_sequence_options() -> Vec<String> {
    vec![]
}

/// Default date format (mm/dd/yyyy)
pub fn default_date_format() -> String {
    "mm/dd/yyyy".to_string()
}
