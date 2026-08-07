//! Inbond shipment workflow: VL06O by shipment → 149 by delivery → paste-ready file.

use sap_scripting::*;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use windows::core::Result;

use crate::utils::config_ops::get_reports_dir;
use crate::utils::sap_ctrl_utils::{exist_ctrl, hit_ctrl};
use crate::utils::sap_export_utils::export_local_file;
use crate::utils::sap_tcode_utils::{assert_tcode, variant_select};
use crate::y_149::{run_export_by_delivery, Report149ByDeliveryParams};

/// Resolved inbond settings (config defaults applied).
#[derive(Debug, Clone)]
pub struct InbondSettings {
    pub vl06o_variant: String,
    pub variant_149: String,
    pub layout_149: String,
    pub layout_columns: Vec<String>,
    pub export_type: u8,
    pub open_notepad: bool,
}

impl Default for InbondSettings {
    fn default() -> Self {
        Self {
            vl06o_variant: "blank_".to_string(),
            variant_149: "inb_ship".to_string(),
            layout_149: "inb_ship".to_string(),
            layout_columns: crate::utils::choose_layout_utils::inbond_default_layout_columns(),
            export_type: 1,
            open_notepad: true,
        }
    }
}

impl InbondSettings {
    /// Load `[inbond]` from config.toml with defaults.
    pub fn load() -> Self {
        let mut settings = Self::default();
        let Ok(cfg) = crate::utils::config_types::SapConfig::load() else {
            return settings;
        };
        let Some(raw) = cfg.raw_config.as_ref() else {
            return settings;
        };
        let Some(table) = raw.get("inbond").and_then(|v| v.as_table()) else {
            return settings;
        };

        if let Some(v) = table.get("vl06o_variant").and_then(|v| v.as_str()) {
            if !v.is_empty() {
                settings.vl06o_variant = v.to_string();
            }
        }
        if let Some(v) = table.get("variant_149").and_then(|v| v.as_str()) {
            if !v.is_empty() {
                settings.variant_149 = v.to_string();
            }
        }
        if let Some(v) = table.get("layout_149").and_then(|v| v.as_str()) {
            // Allow empty string to mean "always set up columns"
            settings.layout_149 = v.to_string();
        }
        if let Some(arr) = table.get("layout_columns").and_then(|v| v.as_array()) {
            let cols: Vec<String> = arr
                .iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .filter(|s| !s.is_empty())
                .collect();
            if !cols.is_empty() {
                settings.layout_columns = cols;
            }
        }
        match table.get("export_type") {
            Some(toml::Value::Integer(i)) if (0..=4).contains(i) => {
                settings.export_type = *i as u8;
            }
            Some(toml::Value::String(s)) => {
                if let Ok(i) = s.parse::<u8>() {
                    if i <= 4 {
                        settings.export_type = i;
                    }
                }
            }
            _ => {}
        }
        match table.get("open_notepad") {
            Some(toml::Value::Boolean(b)) => settings.open_notepad = *b,
            Some(toml::Value::String(s)) => {
                settings.open_notepad = matches!(s.to_lowercase().as_str(), "true" | "1" | "yes");
            }
            _ => {}
        }

        settings
    }
}

/// Run VL06O for a single shipment, export list, return export path.
pub fn run_vl06o_by_shipment(
    session: &GuiSession,
    shipment: &str,
    variant: &str,
    export_type: u8,
) -> Result<Option<String>> {
    println!(
        "Inbond VL06O: shipment={}, variant={}",
        shipment, variant
    );

    if !assert_tcode(session, "VL06O", Some(0))? {
        println!("Failed to activate VL06O");
        return Ok(None);
    }

    if let Ok(btn) = session.find_by_id("wnd[0]/usr/btnBUTTON6".to_string()) {
        if let Some(button) = btn.downcast::<GuiButton>() {
            button.press()?;
        }
    }

    if !variant.is_empty() && !variant_select(session, "VL06O", variant)? {
        println!("Failed to select VL06O variant '{}'", variant);
        // continue — blank selection screen may still work
    }

    // Clear date fields
    if let Ok(txt) = session.find_by_id("wnd[0]/usr/ctxtIT_WADAT-LOW".to_string()) {
        if let Some(text_field) = txt.downcast::<GuiCTextField>() {
            text_field.set_text("".to_string())?;
        }
    }
    if let Ok(txt) = session.find_by_id("wnd[0]/usr/ctxtIT_WADAT-HIGH".to_string()) {
        if let Some(text_field) = txt.downcast::<GuiCTextField>() {
            text_field.set_text("".to_string())?;
        }
    }

    // Single shipment → LOW field (prefer over multi)
    if let Ok(txt) = session.find_by_id("wnd[0]/usr/ctxtIT_TKNUM-LOW".to_string()) {
        if let Some(text_field) = txt.downcast::<GuiCTextField>() {
            text_field.set_text(shipment.trim().to_string())?;
        } else {
            println!("Shipment LOW field found but could not set text");
            return Ok(None);
        }
    } else {
        println!("Shipment LOW field not found");
        return Ok(None);
    }

    // Execute
    if let Ok(wnd) = session.find_by_id("wnd[0]".to_string()) {
        if let Some(gui) = wnd.downcast::<GuiMainWindow>() {
            gui.send_v_key(8)?;
        }
    }

    let sbar = hit_ctrl(session, 0, "/sbar", "Text", "Get", "")?;
    if !sbar.is_empty() {
        eprintln!("VL06O status bar: {}", sbar);
        let lower = sbar.to_lowercase();
        if lower.contains("no shipment")
            || lower.contains("no data")
            || lower.contains("not found")
            || lower.contains("no items")
        {
            println!("VL06O returned no data for shipment {}", shipment);
            return Ok(None);
        }
    }

    // Item View
    if let Ok(btn) = session.find_by_id("wnd[0]/tbar[1]/btn[18]".to_string()) {
        if let Some(button) = btn.downcast::<GuiButton>() {
            button.press()?;
        }
    }

    if !try_open_local_file_export(session) {
        println!("Could not open VL06O local file export dialog");
        return Ok(None);
    }

    let suffix = format!("inbond-{}", shipment.trim());
    match export_local_file(session, "VL06O", export_type, Some(&suffix)) {
        Ok(path) if !path.is_empty() => Ok(Some(path)),
        Ok(_) => Ok(None),
        Err(e) => {
            println!("VL06O export failed: {}", e);
            Ok(None)
        }
    }
}

fn try_open_local_file_export(session: &GuiSession) -> bool {
    let candidates = [
        "wnd[0]/mbar/menu[0]/menu[5]/menu[2]",
        "wnd[0]/mbar/menu[0]/menu[5]/menu[0]",
        "wnd[0]/mbar/menu[0]/menu[3]/menu[2]",
        "wnd[0]/mbar/menu[0]/menu[3]/menu[0]",
    ];

    for path in candidates.iter() {
        if let Ok(menu) = session.find_by_id((*path).to_string()) {
            if let Some(menu_item) = menu.downcast::<GuiMenu>() {
                if menu_item.select().is_ok() {
                    if let Ok(err_wnd) = exist_ctrl(session, 1, "", true) {
                        if err_wnd.cband {
                            return true;
                        }
                    }
                }
            }
        }
    }
    false
}

/// Parse unique delivery numbers from a VL06O (or similar) export, preserving first-seen order.
pub fn parse_delivery_numbers_deduped(file_path: &str) -> std::io::Result<Vec<String>> {
    let content = fs::read_to_string(file_path)?;
    let lines: Vec<&str> = content.lines().collect();

    let mut header_idx = None;
    let mut delivery_col = None;

    for (i, line) in lines.iter().enumerate() {
        let cols = split_report_columns(line);
        for (j, col) in cols.iter().enumerate() {
            let lower = col.to_lowercase();
            if lower.contains("delivery") {
                header_idx = Some(i);
                delivery_col = Some(j);
                break;
            }
        }
        if header_idx.is_some() {
            break;
        }
    }

    let (Some(h_idx), Some(col_idx)) = (header_idx, delivery_col) else {
        println!(
            "Could not find a Delivery Number column in {}",
            file_path
        );
        return Ok(Vec::new());
    };

    let mut seen = HashSet::new();
    let mut out = Vec::new();

    for line in lines.iter().skip(h_idx + 1) {
        let cols = split_report_columns(line);
        if cols.is_empty() {
            continue;
        }
        if col_idx >= cols.len() {
            continue;
        }
        let raw = cols[col_idx].trim();
        if raw.is_empty() {
            continue;
        }
        // Skip lines that look like continued headers / totals
        let lower = raw.to_lowercase();
        if lower.contains("delivery") || lower == "sum" || lower.starts_with('*') {
            continue;
        }
        // Prefer numeric-looking delivery ids
        if !raw.chars().any(|c| c.is_ascii_digit()) {
            continue;
        }
        if seen.insert(raw.to_string()) {
            out.push(raw.to_string());
        }
    }

    println!(
        "Parsed {} unique delivery number(s) from {}",
        out.len(),
        file_path
    );
    Ok(out)
}

fn split_report_columns(line: &str) -> Vec<String> {
    if line.contains('\t') {
        line.split('\t').map(|s| s.trim().to_string()).collect()
    } else {
        line.split_whitespace().map(|s| s.to_string()).collect()
    }
}

/// Normalize one TSV data row: trim cells, drop empty fields (collapses `\t\t` from blank SAP columns).
fn normalize_tsv_row(line: &str) -> Option<String> {
    let cells: Vec<&str> = line
        .split('\t')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();
    if cells.is_empty() {
        return None;
    }
    Some(cells.join("\t"))
}

/// Convert a 149 tab/text export into a paste-ready file: strip header/metadata, keep tab separators.
/// Empty columns (extra tabs with no data) are removed so Plant/Delivery/Material stay aligned.
pub fn normalize_paste_ready(source_path: &str, shipment: &str) -> std::io::Result<PathBuf> {
    let content = fs::read_to_string(source_path)?;
    let lines: Vec<&str> = content.lines().collect();

    let mut header_idx = None;
    for (i, line) in lines.iter().enumerate() {
        let lower = line.to_lowercase();
        if lower.contains("plant") && lower.contains("delivery") && lower.contains("material") {
            header_idx = Some(i);
            break;
        }
        // Softer match: plant + delivery
        if lower.contains("plant") && lower.contains("delivery") {
            header_idx = Some(i);
            break;
        }
    }

    let mut paste_lines: Vec<String> = Vec::new();
    let data_iter: Box<dyn Iterator<Item = &&str>> = if let Some(h_idx) = header_idx {
        Box::new(lines.iter().skip(h_idx + 1))
    } else {
        Box::new(lines.iter())
    };

    for line in data_iter {
        let trimmed = line.trim_end_matches('\r').trim();
        if trimmed.is_empty() {
            continue;
        }

        // Skip footer/total-ish rows
        let lower = trimmed.to_lowercase();
        if lower.contains("sum") || lower.starts_with('*') {
            continue;
        }

        if trimmed.contains('\t') {
            if let Some(row) = normalize_tsv_row(trimmed) {
                paste_lines.push(row);
            }
        } else {
            // No tabs in source row — keep as-is (whitespace already separates fields)
            paste_lines.push(trimmed.to_string());
        }
    }

    let reports_dir = get_reports_dir().replace("\\\\", "\\");
    let out_dir = PathBuf::from(&reports_dir).join("y_149");
    fs::create_dir_all(&out_dir)?;

    let ts = crate::utils::utils::generate_timestamp();
    let out_name = format!("{}_y_149-inbond-paste-{}.txt", ts, shipment.trim());
    let out_path = out_dir.join(&out_name);

    let mut body = paste_lines.join("\r\n");
    if !body.is_empty() {
        body.push_str("\r\n");
    }
    fs::write(&out_path, body)?;
    println!("Wrote paste-ready file (tabs preserved): {}", out_path.display());
    Ok(out_path)
}

/// Open a file in Notepad (Windows).
pub fn open_in_notepad(path: &Path) -> std::io::Result<()> {
    Command::new("notepad.exe").arg(path).spawn()?;
    Ok(())
}

/// Full inbond orchestration. Returns the paste-ready file path on success.
pub fn run_inbond_shipment_flow(
    session: &GuiSession,
    shipment: &str,
    settings: &InbondSettings,
) -> Result<Option<PathBuf>> {
    let shipment = shipment.trim();
    if shipment.is_empty() {
        println!("Shipment number is empty");
        return Ok(None);
    }

    let vl06o_path = match run_vl06o_by_shipment(
        session,
        shipment,
        &settings.vl06o_variant,
        settings.export_type,
    )? {
        Some(p) => p,
        None => {
            println!("VL06O step failed; aborting inbond flow");
            return Ok(None);
        }
    };

    let deliveries = match parse_delivery_numbers_deduped(&vl06o_path) {
        Ok(d) => d,
        Err(e) => {
            println!("Failed to parse VL06O export: {}", e);
            return Ok(None);
        }
    };
    if deliveries.is_empty() {
        println!("No delivery numbers found in VL06O export");
        return Ok(None);
    }

    let params = Report149ByDeliveryParams {
        variant: settings.variant_149.clone(),
        layout: settings.layout_149.clone(),
        layout_columns: settings.layout_columns.clone(),
        delivery_numbers: deliveries,
        export_type: settings.export_type,
        filename_suffix: Some(format!("inbond-{}", shipment)),
    };

    let export_149 = match run_export_by_delivery(session, &params)? {
        Some(p) => p,
        None => {
            println!("149 by-delivery step failed; aborting inbond flow");
            return Ok(None);
        }
    };

    let paste_path = match normalize_paste_ready(&export_149, shipment) {
        Ok(p) => p,
        Err(e) => {
            println!("Failed to normalize paste-ready file: {}", e);
            // Fall back to raw 149 export
            PathBuf::from(&export_149)
        }
    };

    println!("========================================");
    println!("Inbond paste file: {}", paste_path.display());
    println!("Shipment for #shipmentInput: {}", shipment);
    println!("Paste file contents into material.php #clipboard,");
    println!("enter the shipment number, then click splitBtn.");
    println!("========================================");

    if settings.open_notepad {
        if let Err(e) = open_in_notepad(&paste_path) {
            println!("Could not open Notepad: {}", e);
        }
    }

    Ok(Some(paste_path))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn dedupes_delivery_numbers_preserving_order() {
        let dir = std::env::temp_dir();
        let path = dir.join("inbond_vl06o_parse_test.txt");
        let mut f = fs::File::create(&path).unwrap();
        writeln!(
            f,
            "Plant\tDelivery Number\tMaterial\nFV52\t1001\tMAT1\nFV52\t1002\tMAT2\nFV52\t1001\tMAT1\nFV52\t1003\tMAT3"
        )
        .unwrap();

        let got = parse_delivery_numbers_deduped(path.to_str().unwrap()).unwrap();
        assert_eq!(got, vec!["1001", "1002", "1003"]);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn drops_empty_tab_fields_between_columns() {
        // Extra blank column between delivery and material (SAP layout fluff)
        let row = normalize_tsv_row("FV55\t8012178144\t\tM22830X3\t376,164.000\tFT").unwrap();
        assert_eq!(row, "FV55\t8012178144\tM22830X3\t376,164.000\tFT");

        // Already clean row unchanged
        let clean =
            normalize_tsv_row("FV55\t8012178164\t33129011\t2,000.000\tPC\tQP01430157744").unwrap();
        assert_eq!(
            clean,
            "FV55\t8012178164\t33129011\t2,000.000\tPC\tQP01430157744"
        );
    }
}
