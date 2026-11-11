//! MM60 unattended flow: `--tcode mm60 [--layout L] [--variant V] [--export-type N]
//! [--lx03-file F] [--rname-dir D]`. Reads the material list of the newest LX03 export, runs
//! MM60 and exports it. Errors are returned (non-zero exit), nothing is prompted.
#![allow(dead_code)]

use anyhow::{anyhow, Result};
use sap_scripting::*;
use std::collections::HashMap;

use crate::mm60::{paste_list, read_lx03_file, resolve_lx03_file, run_export_outcome, MM60Params};
use crate::utils::cli_overrides::cli_overrides;
use crate::utils::config_types::SapConfig;
use crate::utils::export_outcome::ExportOutcome;

/// Build the SAP-side parameters from the merged `[tcode.MM60]` settings (CLI already merged
/// in by `SapConfig::load`). `materials` is filled by the caller.
pub fn create_mm60_params_from_config(config: &HashMap<String, String>) -> MM60Params {
    let mut p = MM60Params::default();
    let get = |k: &str| {
        config
            .get(k)
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
    };
    p.variant = get("variant");
    p.layout = get("layout");
    p.export_type = get("export_type").and_then(|v| v.parse::<u8>().ok());
    p.ignore_display_error =
        get("ignore_display_error").is_some_and(|v| v.eq_ignore_ascii_case("true"));
    p
}

/// LX03 source: CLI `--lx03-file` wins over `[tcode.MM60].lx03_file`.
pub fn lx03_source(config: &HashMap<String, String>) -> Option<String> {
    config
        .get("cli_lx03_file")
        .or_else(|| config.get("lx03_file"))
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

pub fn run_mm60_auto(session: &GuiSession) -> Result<()> {
    println!("MM60 - Auto Run from Configuration");
    println!("==================================");

    let config = SapConfig::load().map_err(|e| anyhow!("Error loading configuration: {e}"))?;
    let tcode_config = config
        .get_tcode_config("MM60", Some(true))
        .unwrap_or_default();

    let mut params = create_mm60_params_from_config(&tcode_config);
    if params.export_type.is_none() {
        params.export_type = config.get_effective_export_type("MM60");
    }

    let file = resolve_lx03_file(
        lx03_source(&tcode_config).as_deref(),
        &config.get_reports_dir(),
        cli_overrides().rname_dir.as_deref(),
    )?;
    println!("Reading materials from {}", file.display());
    let rows = read_lx03_file(&file)?;
    params.materials = paste_list(rows);
    if params.materials.is_empty() {
        return Err(anyhow!(
            "MM60: no materials in {}; nothing to run",
            file.display()
        ));
    }

    println!("Running MM60 with the following parameters:");
    println!("-------------------------------------------");
    println!("Materials: {}", params.materials.len());
    println!("Variant: {:?}", params.variant);
    println!("Layout: {:?}", params.layout);
    println!("Export Type: {:?}", params.export_type);
    println!("-------------------------------------------");

    match run_export_outcome(session, &params).map_err(|e| anyhow!("MM60 failed: {e}"))? {
        ExportOutcome::File(p) => {
            println!("MM60 export completed: {}", p.display());
            Ok(())
        }
        ExportOutcome::NoData(m) => Err(anyhow!("MM60: no data ({m})")),
        ExportOutcome::Error(m) => Err(anyhow!("{m}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn params_from_config() {
        let mut c = HashMap::new();
        c.insert("variant".to_string(), " V1 ".to_string());
        c.insert("layout".to_string(), "".to_string());
        c.insert("export_type".to_string(), "1".to_string());
        let p = create_mm60_params_from_config(&c);
        assert_eq!(p.variant.as_deref(), Some("V1"));
        assert_eq!(p.layout, None);
        assert_eq!(p.export_type, Some(1));
    }

    #[test]
    fn cli_source_wins() {
        let mut c = HashMap::new();
        c.insert("lx03_file".to_string(), "a.txt".to_string());
        assert_eq!(lx03_source(&c).as_deref(), Some("a.txt"));
        c.insert("cli_lx03_file".to_string(), "b.txt".to_string());
        assert_eq!(lx03_source(&c).as_deref(), Some("b.txt"));
    }
}
