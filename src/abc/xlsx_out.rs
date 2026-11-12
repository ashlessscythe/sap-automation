//! xlsx output for the ABC reports: one worksheet per table (bold, frozen header row).
//! TSV stays the canonical output; the workbook is an additive convenience (plan decision 6).
#![allow(dead_code)]

use super::registry::{self, ReportDef};
use super::run::RunDate;
use super::table::Table;
use crate::utils::cli_overrides::{self, OutputFormat};
use anyhow::{anyhow, Context, Result};
use rust_xlsxwriter::{Format, Workbook};
use std::path::{Path, PathBuf};

/// Excel sheet-name limit.
const MAX_SHEET_NAME: usize = 31;

/// Excel-safe sheet name: forbidden `[]:*?/\` removed, leading/trailing `'` trimmed, at most
/// 31 characters, never empty.
pub fn sanitize_sheet_name(raw: &str) -> String {
    let cleaned: String = raw
        .chars()
        .filter(|c| !matches!(c, '[' | ']' | ':' | '*' | '?' | '/' | '\\') && !c.is_control())
        .collect();
    let trimmed = cleaned.trim().trim_matches('\'').trim();
    let cut: String = trimmed.chars().take(MAX_SHEET_NAME).collect();
    let cut = cut.trim_end().to_string();
    if cut.is_empty() {
        "Sheet".to_string()
    } else {
        cut
    }
}

/// Sanitise all names and make them unique (case-insensitive) after truncation by replacing
/// the tail with `_2`, `_3`, ...
pub fn unique_sheet_names(names: &[&str]) -> Vec<String> {
    let mut out: Vec<String> = Vec::with_capacity(names.len());
    for raw in names {
        let base = sanitize_sheet_name(raw);
        let mut cand = base.clone();
        let mut n = 2;
        while out.iter().any(|o| o.eq_ignore_ascii_case(&cand)) {
            let suffix = format!("_{n}");
            let keep = MAX_SHEET_NAME - suffix.chars().count();
            cand = format!("{}{suffix}", base.chars().take(keep).collect::<String>());
            n += 1;
        }
        out.push(cand);
    }
    out
}

/// Is `s` a plain number that survives a text -> number -> text round trip? No leading zeros
/// (except `0` and `0.x`), no `+`, no trailing fraction zeros, no exponent, at most 15
/// significant digits. Identifiers such as `00123` therefore stay text.
pub fn is_plain_number(s: &str) -> bool {
    let body = s.strip_prefix('-').unwrap_or(s);
    let (int, frac) = match body.split_once('.') {
        Some((i, f)) => (i, Some(f)),
        None => (body, None),
    };
    if int.is_empty() || !int.bytes().all(|b| b.is_ascii_digit()) {
        return false;
    }
    if int.len() > 1 && int.starts_with('0') {
        return false;
    }
    if let Some(f) = frac {
        if f.is_empty() || !f.bytes().all(|b| b.is_ascii_digit()) || f.ends_with('0') {
            return false;
        }
    }
    let digits = int.len() + frac.map_or(0, str::len);
    if digits > 15 {
        return false;
    }
    // "-0" would come back as "0".
    s != "-0"
}

/// A column is numeric when it has at least one non-empty cell and every non-empty cell is a
/// plain number.
fn numeric_columns(t: &Table) -> Vec<bool> {
    (0..t.headers.len())
        .map(|c| {
            let mut any = false;
            for r in &t.rows {
                let v = r[c].as_str();
                if v.is_empty() {
                    continue;
                }
                if !is_plain_number(v) {
                    return false;
                }
                any = true;
            }
            any
        })
        .collect()
}

fn build(sheets: &[(&str, &Table)]) -> Result<Workbook> {
    let mut wb = Workbook::new();
    let bold = Format::new().set_bold();
    let names: Vec<&str> = sheets.iter().map(|(n, _)| *n).collect();
    let names = unique_sheet_names(&names);
    for ((_, t), name) in sheets.iter().zip(&names) {
        let ws = wb.add_worksheet();
        ws.set_name(name)
            .map_err(|e| anyhow!("sheet name '{name}': {e}"))?;
        let numeric = numeric_columns(t);
        for (c, h) in t.headers.iter().enumerate() {
            ws.write_string_with_format(0, c as u16, h, &bold)
                .map_err(|e| anyhow!("sheet '{name}': {e}"))?;
        }
        for (i, row) in t.rows.iter().enumerate() {
            let r = (i + 1) as u32;
            for (c, v) in row.iter().enumerate() {
                if v.is_empty() {
                    continue;
                }
                let res = if numeric[c] {
                    // is_plain_number guarantees a valid f64.
                    ws.write_number(r, c as u16, v.parse::<f64>().unwrap_or(0.0))
                } else {
                    ws.write_string(r, c as u16, v)
                };
                res.map_err(|e| anyhow!("sheet '{name}' row {r}: {e}"))?;
            }
        }
        ws.set_freeze_panes(1, 0)
            .map_err(|e| anyhow!("sheet '{name}': {e}"))?;
    }
    Ok(wb)
}

/// Write one worksheet per table. Parent directories are created; the file is written to a
/// temporary sibling and renamed, so a failure leaves no partial file.
pub fn write_workbook(path: &Path, sheets: &[(&str, &Table)]) -> Result<()> {
    if sheets.is_empty() {
        return Err(anyhow!("write_workbook: no sheets for {}", path.display()));
    }
    let mut wb = build(sheets)?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).with_context(|| format!("create {}", dir.display()))?;
    }
    let mut tmp_name = path
        .file_name()
        .ok_or_else(|| anyhow!("no file name in {}", path.display()))?
        .to_os_string();
    tmp_name.push(".tmp");
    let tmp = path.with_file_name(tmp_name);
    if let Err(e) = wb.save(&tmp) {
        let _ = std::fs::remove_file(&tmp);
        return Err(anyhow!("write {}: {e}", path.display()));
    }
    if let Err(e) = std::fs::rename(&tmp, path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(anyhow!("write {}: {e}", path.display()));
    }
    Ok(())
}

/// If `--output-format=xlsx` is active, also write `<DATE>_<RNAME>.xlsx` (every table as a
/// sheet, named after the table) next to the report's TSV files and return its path. With the
/// default format nothing is written and `None` is returned (legacy behaviour).
pub fn write_report_workbook(
    def: &ReportDef,
    reports_dir: &Path,
    today: chrono::NaiveDate,
    run_date: &RunDate,
    tables: &[(&str, &Table)],
) -> Result<Option<PathBuf>> {
    if cli_overrides::output_format() != OutputFormat::Xlsx {
        return Ok(None);
    }
    let path = registry::workbook_path(reports_dir, def, today, run_date);
    write_workbook(&path, tables)?;
    Ok(Some(path))
}

#[cfg(test)]
mod tests {
    use super::*;
    use calamine::{open_workbook, Data, Reader, Xlsx};

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn plain_number_rules() {
        for ok in ["0", "7", "-3", "0.5", "12.25", "123456789012345"] {
            assert!(is_plain_number(ok), "{ok}");
        }
        for bad in [
            "",
            "-",
            "007",
            "+5",
            "1.50",
            "1.",
            ".5",
            "1e5",
            "1,5",
            "-0",
            "1234567890123456",
            "0.1234567890123456",
            "abc",
            "2026-11-05",
        ] {
            assert!(!is_plain_number(bad), "{bad}");
        }
    }

    #[test]
    fn sheet_names_sanitised_and_unique() {
        assert_eq!(sanitize_sheet_name("a[b]:c*d?e/f\\g"), "abcdefg");
        assert_eq!(sanitize_sheet_name("'x'"), "x");
        assert_eq!(sanitize_sheet_name(""), "Sheet");
        let long = "a".repeat(40);
        let names = unique_sheet_names(&[&long, &long, "PIVOT", "pivot"]);
        assert_eq!(names[0].len(), 31);
        assert_eq!(names[1].len(), 31);
        assert!(names[1].ends_with("_2"));
        assert_eq!(names[3], "pivot_2");
    }

    #[test]
    fn round_trip_with_calamine() {
        let dir = std::env::temp_dir().join(format!("xlsx_out_rt_{}", std::process::id()));
        let path = dir.join("sub").join("book.xlsx");
        let t1 = Table::new(
            "t1",
            s(&["id", "qty", "note"]),
            vec![s(&["00123", "5", "a"]), s(&["00456", "2.5", ""])],
        );
        let t2 = Table::new("t2", s(&["x"]), vec![s(&["1"])]);
        let long = "L".repeat(45);
        write_workbook(&path, &[("first", &t1), (long.as_str(), &t2)]).unwrap();
        assert!(!dir.join("sub").join("book.xlsx.tmp").exists());

        let mut wb: Xlsx<_> = open_workbook(&path).unwrap();
        let names = wb.sheet_names();
        assert_eq!(names[0], "first");
        assert_eq!(names[1], "L".repeat(31));
        let r = wb.worksheet_range("first").unwrap();
        let cell = |row: usize, col: usize| r.get((row, col)).cloned();
        assert_eq!(cell(0, 0), Some(Data::String("id".into())));
        assert_eq!(cell(1, 0), Some(Data::String("00123".into())));
        match cell(1, 1) {
            Some(Data::Float(f)) => assert_eq!(f, 5.0),
            Some(Data::Int(i)) => assert_eq!(i, 5),
            other => panic!("expected number, got {other:?}"),
        }
        match cell(2, 1) {
            Some(Data::Float(f)) => assert!((f - 2.5).abs() < 1e-12),
            other => panic!("expected number, got {other:?}"),
        }
        assert_eq!(cell(1, 2), Some(Data::String("a".into())));
        let r2 = wb.worksheet_range(&"L".repeat(31)).unwrap();
        assert!(matches!(
            r2.get((1, 0)),
            Some(Data::Float(_)) | Some(Data::Int(_))
        ));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn no_sheets_is_an_error() {
        assert!(write_workbook(Path::new("never.xlsx"), &[]).is_err());
    }
}
