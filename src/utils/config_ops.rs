use anyhow::{anyhow, Result};
use dialoguer::{Input, Select};
use std::collections::HashMap;
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
                default_menu_option: Some(get_default_menu_option()),
                date_format: default_date_format(),
                timezone: default_timezone(),
                default_export_type: None,
                additional_params: HashMap::new(),
            }),
            build: None,
            tcode: Some(HashMap::new()),
            loop_config: None,
            sequence: None,
            raw_config: None,
        }
    }
}

impl SapConfig {
    /// Create a new configuration with default values
    #[allow(dead_code)]
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
                            target: build
                                .get("target")
                                .and_then(|v| v.as_str())
                                .unwrap_or("i686-pc-windows-msvc")
                                .to_string(),
                            additional_params: HashMap::new(),
                        };

                        // Extract additional build parameters
                        for (key, value) in build {
                            if key != "target" {
                                if let Some(val_str) = value.as_str() {
                                    build_config
                                        .additional_params
                                        .insert(key.clone(), val_str.to_string());
                                }
                            }
                        }

                        config.build = Some(build_config);
                    }

                    // Check for new format (with global and tcode sections)
                    let is_new_format =
                        parsed.get("global").is_some() || parsed.get("tcode").is_some();

                    if is_new_format {
                        // Extract global section
                        if let Some(global) = parsed.get("global").and_then(|v| v.as_table()) {
                            let mut global_config = GlobalConfig {
                                instance_id: global
                                    .get("instance_id")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or(&default_instance_id())
                                    .to_string(),
                                reports_dir: global
                                    .get("reports_dir")
                                    .and_then(|v| v.as_str())
                                    .map(|s| s.replace("\\", "\\\\"))
                                    .unwrap_or_else(get_default_reports_dir),
                                default_tcode: global
                                    .get("default_tcode")
                                    .and_then(|v| v.as_str())
                                    .map(|s| s.to_string()),
                                default_menu_option: global.get("default_menu_option").and_then(
                                    |v| match v {
                                        toml::Value::Integer(i) => Some(*i as usize),
                                        _ => None,
                                    },
                                ),
                                date_format: global
                                    .get("date_format")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or(&default_date_format())
                                    .to_string(),
                                timezone: global
                                    .get("timezone")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or(&default_timezone())
                                    .to_string(),
                                default_export_type: match global.get("default_export_type") {
                                    Some(toml::Value::Integer(i)) => Some(*i as u8),
                                    Some(toml::Value::String(s)) => s.parse::<u8>().ok(),
                                    _ => None,
                                },
                                additional_params: HashMap::new(),
                            };

                            // Extract additional global parameters
                            for (key, value) in global {
                                if ![
                                    "instance_id",
                                    "reports_dir",
                                    "default_tcode",
                                    "default_menu_option",
                                    "date_format",
                                    "timezone",
                                    "default_export_type",
                                ]
                                .contains(&key.as_str())
                                {
                                    if let Some(val_str) = value.as_str() {
                                        global_config
                                            .additional_params
                                            .insert(key.clone(), val_str.to_string());
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
                                    tcode_config.variant = tcode_table
                                        .get("variant")
                                        .and_then(|v| v.as_str())
                                        .map(|s| s.to_string());

                                    tcode_config.layout = tcode_table
                                        .get("layout")
                                        .and_then(|v| v.as_str())
                                        .map(|s| s.to_string());

                                    tcode_config.column_name = tcode_table
                                        .get("column_name")
                                        .and_then(|v| v.as_str())
                                        .map(|s| s.to_string());

                                    tcode_config.date_range_start = tcode_table
                                        .get("date_range_start")
                                        .and_then(|v| v.as_str())
                                        .map(|s| s.to_string());

                                    tcode_config.date_range_end = tcode_table
                                        .get("date_range_end")
                                        .and_then(|v| v.as_str())
                                        .map(|s| s.to_string());

                                    tcode_config.by_date = tcode_table
                                        .get("by_date")
                                        .and_then(|v| v.as_str())
                                        .map(|s| s.to_string());

                                    // First-class so CLI `--by-delivery=<bool>` overrides
                                    // are not clobbered by the additional_params catch-all
                                    // below when [tcode.X] also sets by_delivery.
                                    tcode_config.by_delivery = tcode_table
                                        .get("by_delivery")
                                        .and_then(|v| v.as_str())
                                        .map(|s| s.to_string());

                                    tcode_config.serial_number = tcode_table
                                        .get("serial_number")
                                        .and_then(|v| v.as_str())
                                        .map(|s| s.to_string());

                                    tcode_config.tab_number = tcode_table
                                        .get("tab_number")
                                        .and_then(|v| v.as_str())
                                        .map(|s| s.to_string());

                                    // Parse export_type as u8 if present
                                    tcode_config.export_type =
                                        tcode_table.get("export_type").and_then(|v| match v {
                                            toml::Value::Integer(i) => Some(*i as u8),
                                            toml::Value::String(s) => s.parse::<u8>().ok(),
                                            _ => None,
                                        });

                                    // Parse layout_columns as Vec<String> if present
                                    tcode_config.layout_columns = tcode_table
                                        .get("layout_columns")
                                        .and_then(|v| v.as_array())
                                        .map(|arr| {
                                            arr.iter()
                                                .filter_map(|val| {
                                                    val.as_str().map(|s| s.to_string())
                                                })
                                                .collect()
                                        });

                                    // Extract additional parameters
                                    for (key, value) in tcode_table {
                                        if ![
                                            "variant",
                                            "layout",
                                            "column_name",
                                            "date_range_start",
                                            "date_range_end",
                                            "by_date",
                                            "by_delivery",
                                            "serial_number",
                                            "tab_number",
                                            "export_type",
                                            "add_layout_columns",
                                            "layout_columns",
                                        ]
                                        .contains(&key.as_str())
                                        {
                                            if let Some(val_str) = value.as_str() {
                                                tcode_config
                                                    .additional_params
                                                    .insert(key.clone(), val_str.to_string());
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
                                tcode: loop_table
                                    .get("tcode")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    .to_string(),
                                tcode_run_type: loop_table
                                    .get("tcode_run_type")
                                    .and_then(|v| v.as_str())
                                    .map(|s| s.to_string()),
                                iterations: loop_table
                                    .get("iterations")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or(&default_iterations())
                                    .to_string(),
                                delay_seconds: loop_table
                                    .get("delay_seconds")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or(&default_delay_seconds())
                                    .to_string(),
                                params: HashMap::new(),
                            };

                            // Add additional loop parameters
                            for (key, value) in loop_table {
                                if !["tcode", "iterations", "delay_seconds"].contains(&key.as_str())
                                {
                                    if let Some(val_str) = value.as_str() {
                                        loop_config.params.insert(key.clone(), val_str.to_string());
                                    }
                                }
                            }

                            config.loop_config = Some(loop_config);
                        }

                        // Extract sequence section
                        if let Some(seq_table) = parsed.get("sequence").and_then(|v| v.as_table()) {
                            let mut sequence_config = SequenceConfig {
                                options: seq_table
                                    .get("options")
                                    .and_then(|v| v.as_array())
                                    .map(|arr| {
                                        arr.iter()
                                            .filter_map(|val| val.as_str().map(|s| s.to_string()))
                                            .collect::<Vec<String>>()
                                    })
                                    .unwrap_or_default(),
                                iterations: seq_table
                                    .get("iterations")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or(&default_iterations())
                                    .to_string(),
                                delay_seconds: seq_table
                                    .get("delay_seconds")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or(&default_delay_seconds())
                                    .to_string(),
                                interval_seconds: seq_table
                                    .get("interval_seconds")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or(&default_interval_seconds())
                                    .to_string(),
                                params: HashMap::new(),
                            };

                            // Add additional sequence parameters
                            for (key, value) in seq_table {
                                if !["options", "iterations", "delay_seconds", "interval_seconds"]
                                    .contains(&key.as_str())
                                {
                                    if let Some(val_str) = value.as_str() {
                                        sequence_config
                                            .params
                                            .insert(key.clone(), val_str.to_string());
                                    }
                                }
                            }

                            config.sequence = Some(sequence_config);
                        }
                    }
                }
                Err(e) => return Err(anyhow!("Failed to parse config: {}", e)),
            }
        }

        // Apply CLI flag overrides last so they always win over file values
        // and so they take effect even when config.toml is missing.
        config.apply_cli_overrides();

        Ok(config)
    }

    /// Merge CLI flag overrides (installed via `cli_overrides::set_cli_overrides`)
    /// into this config. Called at the end of every load path so every consumer
    /// sees a merged view.
    fn apply_cli_overrides(&mut self) {
        let o = crate::utils::cli_overrides::cli_overrides();

        // ----- [global] overrides -----
        let touches_global = o.reports_dir.is_some()
            || o.date_format.is_some()
            || o.timezone.is_some();

        if touches_global {
            let global = self.global.get_or_insert_with(|| GlobalConfig {
                instance_id: default_instance_id(),
                reports_dir: get_default_reports_dir(),
                default_tcode: None,
                default_menu_option: Some(get_default_menu_option()),
                date_format: default_date_format(),
                timezone: default_timezone(),
                default_export_type: None,
                additional_params: HashMap::new(),
            });

            if let Some(dir) = &o.reports_dir {
                // Match the file-load path which doubles backslashes for SAP paths.
                global.reports_dir = dir.replace("\\", "\\\\");
            }
            if let Some(fmt) = &o.date_format {
                global.date_format = fmt.clone();
            }
            if let Some(tz) = &o.timezone {
                global.timezone = tz.clone();
            }
        }

        // ----- [tcode.X] overrides (per --tcode <X>) -----
        if let Some(tc_name) = &o.tcode {
            let key = tc_name.to_uppercase();
            let map = self.tcode.get_or_insert_with(HashMap::new);
            let entry = map.entry(key.clone()).or_insert_with(TcodeConfig::default);

            if let Some(layout) = &o.layout {
                entry.layout = Some(layout.clone());
            }
            if let Some(variant) = &o.variant {
                entry.variant = Some(variant.clone());
            }
            if let Some(et) = o.export_type {
                entry.export_type = Some(et);
            }

            // Filter toggles. Stored as `"true"`/`"false"` strings to match the
            // existing on-disk + downstream-string representation.
            if let Some(b) = o.by_date {
                entry.by_date = Some(b.to_string());
            }
            if let Some(b) = o.by_delivery {
                entry.by_delivery = Some(b.to_string());
            }
            if let Some(b) = o.by_shipment {
                // No first-class field for `by_shipment`, lives in additional_params.
                entry
                    .additional_params
                    .insert("by_shipment".to_string(), b.to_string());

                // VL06O activates the shipment path when `column_name` is set, so
                // pre-fill it with a sensible default when --by-shipment=true and
                // no explicit --shipment-col was provided.
                if b && key == "VL06O" && entry.column_name.is_none() {
                    let default_col = o
                        .shipment_col
                        .clone()
                        .unwrap_or_else(|| "Shipment Number".to_string());
                    entry.column_name = Some(default_col);
                }
            }

            // VT11 / ZVT11 limiter. No first-class field on TcodeConfig.
            if let Some(lim) = &o.limiter {
                entry
                    .additional_params
                    .insert("limiter".to_string(), lim.clone());
            }

            // Date range. parse_date() in the report modules already accepts ISO,
            // so write through as ISO YYYY-MM-DD.
            if let Some(d) = o.date_start {
                entry.date_range_start = Some(d.format("%Y-%m-%d").to_string());
            }
            if let Some(d) = o.date_end {
                entry.date_range_end = Some(d.format("%Y-%m-%d").to_string());
            }

            // Delivery / shipment source overrides — consumed by
            // utils::source_overrides at delivery/shipment load time.
            if let Some(v) = &o.delivery_file {
                entry
                    .additional_params
                    .insert("cli_delivery_file".to_string(), v.clone());
            }
            if let Some(v) = &o.delivery_col {
                entry
                    .additional_params
                    .insert("cli_delivery_col".to_string(), v.clone());
            }
            if let Some(v) = &o.shipment_file {
                entry
                    .additional_params
                    .insert("cli_shipment_file".to_string(), v.clone());
            }
            if let Some(v) = &o.shipment_col {
                // shipment_col also feeds VL06O's existing column_name path so
                // the legacy shipment-from-Excel branch picks it up directly.
                entry.column_name = Some(v.clone());
                entry
                    .additional_params
                    .insert("cli_shipment_col".to_string(), v.clone());
            }

            // ZMDESNR-only knobs.
            if key == "ZMDESNR" {
                // pre_export_back: read by `create_zmdesnr_params_from_config`
                // as a string and compared to "true"; mirror the on-disk format.
                if let Some(b) = o.pre_export_back {
                    entry
                        .additional_params
                        .insert("pre_export_back".to_string(), b.to_string());
                }

                // tab_number: stored as String on TcodeConfig (parsed back to i32
                // by the consumer). When neither config nor CLI sets it, the
                // ZMDESNR module falls back to 2 via `unwrap_or(2)`.
                if let Some(n) = o.tab_number {
                    entry.tab_number = Some(n.to_string());
                }
            }
        }
    }

    /// Load configuration from legacy format
    #[allow(dead_code)]
    fn load_legacy_format(parsed: toml::Value, mut config: SapConfig) -> Result<SapConfig> {
        // Extract sap_config section
        if let Some(sap_config) = parsed.get("sap_config").and_then(|v| v.as_table()) {
            // Create global config
            let mut global_config = GlobalConfig {
                instance_id: sap_config
                    .get("instance_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or(&default_instance_id())
                    .to_string(),
                reports_dir: sap_config
                    .get("reports_dir")
                    .and_then(|v| v.as_str())
                    .map(|s| s.replace("\\", "\\\\"))
                    .unwrap_or_else(get_default_reports_dir),
                default_tcode: sap_config
                    .get("tcode")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                default_menu_option: sap_config.get("menu_option").and_then(|v| match v {
                    toml::Value::Integer(i) => Some(*i as usize),
                    toml::Value::String(s) => s.parse::<usize>().ok(),
                    _ => None,
                }),
                date_format: sap_config
                    .get("date_format")
                    .and_then(|v| v.as_str())
                    .unwrap_or(&default_date_format())
                    .to_string(),
                timezone: sap_config
                    .get("timezone")
                    .and_then(|v| v.as_str())
                    .unwrap_or(&default_timezone())
                    .to_string(),
                default_export_type: None, // Legacy format doesn't have this
                additional_params: HashMap::new(),
            };

            // Get the default tcode from the config
            let default_tcode = global_config.default_tcode.clone().unwrap_or_default();

            // Create tcode config for the default tcode
            let mut tcode_config = TcodeConfig::default();

            // Extract standard fields
            tcode_config.variant = sap_config
                .get("variant")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            tcode_config.layout = sap_config
                .get("layout")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            tcode_config.column_name = sap_config
                .get("column_name")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            tcode_config.date_range_start = sap_config
                .get("date_range_start")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            tcode_config.date_range_end = sap_config
                .get("date_range_end")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            // Create loop config
            let mut loop_config = LoopConfig {
                tcode: sap_config
                    .get("loop_tcode")
                    .and_then(|v| v.as_str())
                    .unwrap_or(&default_tcode)
                    .to_string(),
                tcode_run_type: sap_config
                    .get("loop_tcode_run_type")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                iterations: sap_config
                    .get("loop_iterations")
                    .and_then(|v| v.as_str())
                    .unwrap_or(&default_iterations())
                    .to_string(),
                delay_seconds: sap_config
                    .get("loop_delay_seconds")
                    .and_then(|v| v.as_str())
                    .unwrap_or(&default_delay_seconds())
                    .to_string(),
                params: HashMap::new(),
            };

            // Extract additional parameters
            for (key, value) in sap_config {
                if ![
                    "instance_id",
                    "reports_dir",
                    "tcode",
                    "variant",
                    "layout",
                    "column_name",
                    "date_range_start",
                    "date_range_end",
                    "loop_tcode",
                    "loop_iterations",
                    "loop_delay_seconds",
                    "date_format",
                ]
                .contains(&key.as_str())
                {
                    if let Some(val_str) = value.as_str() {
                        // Check if it's a loop parameter
                        if key.starts_with("loop_param_") {
                            let param_name = key.replacen("loop_param_", "", 1);
                            loop_config.params.insert(param_name, val_str.to_string());
                        } else if key.starts_with("loop_") {
                            // Other loop-related parameters
                            let param_name = key.replacen("loop_", "", 1);
                            loop_config.params.insert(param_name, val_str.to_string());
                        } else if !default_tcode.is_empty()
                            && key.starts_with(&format!("{}_", default_tcode))
                        {
                            // TCode-specific parameters
                            let param_name = key.replacen(&format!("{}_", default_tcode), "", 1);
                            tcode_config
                                .additional_params
                                .insert(param_name, val_str.to_string());
                        } else {
                            // Global parameters
                            global_config
                                .additional_params
                                .insert(key.clone(), val_str.to_string());
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
            // Get all top-level keys that aren't "build", "global", "tcode", "loop", "sequence", or "sap_config"
            let preserved_keys: Vec<&String> = raw_config
                .as_table()
                .map(|table| {
                    table
                        .keys()
                        .filter(|key| {
                            !["build", "global", "tcode", "loop", "sequence", "sap_config"]
                                .contains(&key.as_str())
                        })
                        .collect()
                })
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
            content.push_str(&format!("date_format = \"{}\"\n", global.date_format));
            content.push_str(&format!("timezone = \"{}\"\n", global.timezone));

            if let Some(default_tcode) = &global.default_tcode {
                content.push_str(&format!("default_tcode = \"{}\"\n", default_tcode));
            }

            if let Some(default_menu_option) = &global.default_menu_option {
                content.push_str(&format!("default_menu_option = {}\n", default_menu_option));
            }

            if let Some(default_export_type) = &global.default_export_type {
                content.push_str(&format!("default_export_type = {}\n", default_export_type));
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

                if let Some(export_type) = &tcode_config.export_type {
                    content.push_str(&format!("export_type = {}\n", export_type));
                }

                if let Some(layout_columns) = &tcode_config.layout_columns {
                    content.push_str("layout_columns = [\n");
                    for (i, col) in layout_columns.iter().enumerate() {
                        if i > 0 {
                            content.push_str(",\n");
                        }
                        content.push_str(&format!("  \"{}\"", col));
                    }
                    content.push_str("]\n");
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
            content.push_str(&format!(
                "delay_seconds = \"{}\"\n",
                loop_config.delay_seconds
            ));

            // Add additional loop parameters
            for (key, value) in &loop_config.params {
                content.push_str(&format!("param_{} = \"{}\"\n", key, value));
            }

            content.push('\n');
        }

        // Add sequence section
        if let Some(sequence_config) = &self.sequence {
            content.push_str("[sequence]\n");

            // Add options as an array
            if !sequence_config.options.is_empty() {
                content.push_str("options = [");
                for (i, option) in sequence_config.options.iter().enumerate() {
                    if i > 0 {
                        content.push_str(", ");
                    }
                    content.push_str(&format!("\"{}\"", option));
                }
                content.push_str("]\n");
            }

            content.push_str(&format!(
                "iterations = \"{}\"\n",
                sequence_config.iterations
            ));
            content.push_str(&format!(
                "delay_seconds = \"{}\"\n",
                sequence_config.delay_seconds
            ));
            content.push_str(&format!(
                "interval_seconds = \"{}\"\n",
                sequence_config.interval_seconds
            ));

            // Add additional sequence parameters
            for (key, value) in &sequence_config.params {
                content.push_str(&format!("param_{} = \"{}\"\n", key, value));
            }

            content.push('\n');
        }

        // Write updated config
        fs::write(path, content)?;

        Ok(())
    }

    /// Get configuration for a specific tcode
    pub fn get_tcode_config(
        &self,
        tcode: &str,
        is_loop_run: Option<bool>,
    ) -> Option<HashMap<String, String>> {
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

                if let Some(export_type) = &tcode_config.export_type {
                    config.insert("export_type".to_string(), export_type.to_string());
                }

                if let Some(layout_columns) = &tcode_config.layout_columns {
                    config.insert(
                        "layout_columns".to_string(),
                        serde_json::to_string(layout_columns).ok()?,
                    );
                }
                if let Some(by_delivery) = &tcode_config.by_delivery {
                    config.insert("by_delivery".to_string(), by_delivery.clone());
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

    /// Determine effective export type for a tcode: prefer tcode.export_type, else global.default_export_type
    pub fn get_effective_export_type(&self, tcode: &str) -> Option<u8> {
        // Prefer tcode-specific value
        if let Some(tcode_map) = self.get_tcode_config(tcode, Some(true)) {
            if let Some(val) = tcode_map.get("export_type") {
                if let Ok(num) = val.parse::<u8>() {
                    return Some(num);
                }
            }
        }
        // Fall back to global
        self.global.as_ref().and_then(|g| g.default_export_type)
    }

    /// Get the instance ID
    pub fn get_instance_id(&self) -> String {
        self.global
            .as_ref()
            .map(|g| g.instance_id.clone())
            .unwrap_or_else(default_instance_id)
    }

    /// Get the reports directory
    pub fn get_reports_dir(&self) -> String {
        self.global
            .as_ref()
            .map(|g| g.reports_dir.clone())
            .unwrap_or_else(get_default_reports_dir)
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
                default_menu_option: Some(get_default_menu_option()),
                date_format: default_date_format(),
                timezone: default_timezone(),
                default_export_type: None,
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
                default_menu_option: Some(get_default_menu_option()),
                date_format: default_date_format(),
                timezone: default_timezone(),
                default_export_type: None,
                additional_params: HashMap::new(),
            });
        }
    }

    /// Get the date format from configuration
    pub fn get_date_format(&self) -> String {
        self.global
            .as_ref()
            .map(|g| g.date_format.clone())
            .unwrap_or_else(default_date_format)
    }

    /// Format a date using the configured date format
    pub fn format_date(&self, date: chrono::NaiveDate) -> String {
        let date_format = self.get_date_format();
        match date_format.as_str() {
            "mm/dd/yyyy" => date.format("%m/%d/%Y").to_string(),
            "yyyy-mm-dd" => date.format("%Y-%m-%d").to_string(),
            "dd-mm-yy" => date.format("%d-%m-%y").to_string(),
            "dd-mm-yyyy" => date.format("%d-%m-%Y").to_string(),
            _ => {
                // Default to ISO format if unknown format
                date.format("%Y-%m-%d").to_string()
            }
        }
    }
}

/// Check if a status bar message indicates a date format error
pub fn is_date_format_error(status_text: &str) -> bool {
    let status_lower = status_text.to_lowercase();
    status_lower.contains("enter date in the format")
        || status_lower.contains("date format")
        || status_lower.contains("invalid date")
        || status_lower.contains("date not valid")
        || status_lower.contains("incorrect date format")
        || status_lower.contains("wrong date format")
}

/// Get the SAP-expected date format based on the error message
pub fn get_sap_expected_date_format(status_text: &str) -> Option<String> {
    let status_lower = status_text.to_lowercase();

    // Check for common SAP date format patterns in error messages
    // Check longer patterns first to avoid substring matches
    if status_lower.contains("____-__-__") || status_lower.contains("yyyy-mm-dd") {
        Some("YYYY-MM-DD".to_string())
    } else if status_lower.contains("__-__-__") || status_lower.contains("dd-mm-yy") {
        Some("DD-MM-YY".to_string())
    } else if status_lower.contains("__/__/__") || status_lower.contains("mm/dd/yyyy") {
        Some("MM/DD/YYYY".to_string())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_date_format_error() {
        assert!(is_date_format_error("Enter date in the format __-__-__"));
        assert!(is_date_format_error("Date format error"));
        assert!(is_date_format_error("Invalid date"));
        assert!(is_date_format_error("Date not valid"));
        assert!(is_date_format_error("Incorrect date format"));
        assert!(is_date_format_error("Wrong date format"));
        assert!(!is_date_format_error("No data found"));
        assert!(!is_date_format_error("Success"));
    }

    #[test]
    fn test_get_sap_expected_date_format() {
        assert_eq!(
            get_sap_expected_date_format("Enter date in the format __-__-__"),
            Some("DD-MM-YY".to_string())
        );
        assert_eq!(
            get_sap_expected_date_format("Date format: dd-mm-yy"),
            Some("DD-MM-YY".to_string())
        );
        assert_eq!(
            get_sap_expected_date_format("Enter date in the format __/__/__"),
            Some("MM/DD/YYYY".to_string())
        );
        assert_eq!(
            get_sap_expected_date_format("Date format: mm/dd/yyyy"),
            Some("MM/DD/YYYY".to_string())
        );
        assert_eq!(
            get_sap_expected_date_format("Enter date in the format ____-__-__"),
            Some("YYYY-MM-DD".to_string())
        );
        assert_eq!(
            get_sap_expected_date_format("Date format: yyyy-mm-dd"),
            Some("YYYY-MM-DD".to_string())
        );
        assert_eq!(get_sap_expected_date_format("Some other error"), None);
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

    let mut new_dir;

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
