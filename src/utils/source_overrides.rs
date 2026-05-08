//! CLI-driven delivery / shipment source overrides.
//!
//! When the user passes `--delivery-file=<slug-or-path>` (and optionally
//! `--delivery-col=<header>`), the existing `by_delivery` branches in the
//! report modules should consume that source instead of the default
//! ZMDESNR + VT11 ListCheck merge. Same idea for `--shipment-file` /
//! `--shipment-col` for VL06O.
//!
//! These helpers return:
//! - `Ok(Some(nums))` when CLI supplied a source AND it produced at least one
//!   number → call site uses these as-is.
//! - `Ok(Some(vec![]))` when CLI supplied a source but produced no numbers →
//!   call site should still use the empty vec (don't fall back), so the user
//!   notices the empty source rather than silently consuming legacy files.
//! - `Ok(None)` when no CLI source was set → call site keeps existing
//!   behavior (legacy merge / default Excel pickup).

use std::path::Path;

use crate::utils::cli_overrides::{cli_overrides, CliOverrides};
use crate::utils::config_ops::get_reports_dir;
use crate::utils::excel_file_ops::read_excel_column;
use crate::vl06o_delivery_module::read_tab_delimited_column;

/// Resolve a `--delivery-file` / `--shipment-file` value into a concrete
/// file path. Returns the newest matching file in the resolved directory
/// when the value is a slug, or the value itself when it looks like a path.
fn resolve_source_path(value: &str) -> std::io::Result<Option<String>> {
    if CliOverrides::looks_like_path(value) {
        // Literal path — caller will confirm it exists.
        let p = Path::new(value);
        if !p.exists() {
            println!("Source file does not exist: {}", value);
            return Ok(None);
        }
        return Ok(Some(value.to_string()));
    }

    // Slug → <reports_dir>\<slug-lowercased>\
    let reports_dir = get_reports_dir();
    let dir = format!("{}\\{}", reports_dir, value.to_lowercase());
    let dir_path = Path::new(&dir);
    if !dir_path.exists() {
        println!("Source directory does not exist: {}", dir);
        return Ok(None);
    }

    // Find newest file in that directory (any extension; we'll dispatch on ext below).
    let mut newest_path = String::new();
    let mut newest_time: Option<std::time::SystemTime> = None;
    for entry in std::fs::read_dir(&dir)?.flatten() {
        let p = entry.path();
        if !p.is_file() {
            continue;
        }
        if let Some(name) = p.file_name().and_then(|s| s.to_str()) {
            // Skip already-marked files (e.g. VT11 ListCheck "_.csv" sentinel).
            if name.ends_with("_.csv") {
                continue;
            }
        }
        if let Ok(meta) = entry.metadata() {
            if let Ok(modified) = meta.modified() {
                if newest_time.map(|t| modified > t).unwrap_or(true) {
                    newest_time = Some(modified);
                    newest_path = p.to_string_lossy().to_string();
                }
            }
        }
    }
    if newest_path.is_empty() {
        println!("No usable files found in {}", dir);
        return Ok(None);
    }
    Ok(Some(newest_path))
}

/// Read the named column from a comma-CSV file (header on row 1).
fn read_csv_column(file_path: &str, header: &str) -> std::io::Result<Vec<String>> {
    use std::fs::File;
    use std::io::{BufRead, BufReader};

    let file = File::open(file_path)?;
    let reader = BufReader::new(file);

    let mut header_idx: Option<usize> = None;
    let mut values: Vec<String> = Vec::new();
    for (line_no, line) in reader.lines().enumerate() {
        let line = line?;
        let cols: Vec<&str> = line.split(',').collect();
        if line_no == 0 {
            header_idx = cols.iter().position(|c| c.trim() == header);
            if header_idx.is_none() {
                println!(
                    "CSV header '{}' not found in {}. Headers: {:?}",
                    header, file_path, cols
                );
                return Ok(Vec::new());
            }
            continue;
        }
        let idx = header_idx.unwrap();
        if let Some(v) = cols.get(idx) {
            let v = v.trim();
            if !v.is_empty() {
                values.push(v.to_string());
            }
        }
    }
    Ok(values)
}

/// Dispatch on file extension to extract the named column.
fn read_column_from_file(path: &str, column: &str) -> std::io::Result<Vec<String>> {
    let ext = Path::new(path)
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "csv" => read_csv_column(path, column),
        "tsv" | "txt" | "rtf" | "html" => read_tab_delimited_column(path, column),
        "xlsx" | "xls" => match read_excel_column(path, "Sheet1", column) {
            Ok(v) => Ok(v),
            Err(e) => Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Excel read error: {}", e),
            )),
        },
        other => {
            println!(
                "Unsupported source file extension '.{}' for {} (expected csv/tsv/txt/xlsx)",
                other, path
            );
            Ok(Vec::new())
        }
    }
}

/// CLI-driven delivery numbers. See module-level docs for return semantics.
pub fn cli_delivery_numbers_override() -> std::io::Result<Option<Vec<String>>> {
    let o = cli_overrides();
    let Some(file_value) = &o.delivery_file else {
        return Ok(None);
    };
    let column = o
        .delivery_col
        .clone()
        .unwrap_or_else(|| "Delivery".to_string());

    let Some(resolved) = resolve_source_path(file_value)? else {
        return Ok(Some(Vec::new()));
    };
    println!(
        "[CLI override] Reading delivery numbers from {} (column '{}')",
        resolved, column
    );
    let nums = read_column_from_file(&resolved, &column)?;
    let nums = dedup_and_sanitize(nums);
    println!(
        "[CLI override] Loaded {} unique delivery numbers from CLI source",
        nums.len()
    );
    Ok(Some(nums))
}

/// CLI-driven shipment numbers (VL06O only). See module-level docs.
pub fn cli_shipment_numbers_override() -> std::io::Result<Option<Vec<String>>> {
    let o = cli_overrides();
    let Some(file_value) = &o.shipment_file else {
        return Ok(None);
    };
    let column = o
        .shipment_col
        .clone()
        .unwrap_or_else(|| "Shipment Number".to_string());

    let Some(resolved) = resolve_source_path(file_value)? else {
        return Ok(Some(Vec::new()));
    };
    println!(
        "[CLI override] Reading shipment numbers from {} (column '{}')",
        resolved, column
    );
    let nums = read_column_from_file(&resolved, &column)?;
    let nums = dedup_and_sanitize(nums);
    println!(
        "[CLI override] Loaded {} unique shipment numbers from CLI source",
        nums.len()
    );
    Ok(Some(nums))
}

fn dedup_and_sanitize(input: Vec<String>) -> Vec<String> {
    let mut nums: Vec<String> = input
        .into_iter()
        .filter(|s| !s.trim().is_empty())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    nums.sort();
    nums
}
