//! In-memory table for the headless ABC runner (spec 1.3, 4 intro, 7.0 items 25-26).
//!
//! All cells are strings. Rows carry a visibility flag: `Sel(T, pred)` in the spec hides rows
//! instead of removing them, and later steps read "visible rows" until the selection is reset.
#![allow(dead_code)]

use crate::utils::export_outcome::normalize_value;
use anyhow::{anyhow, Result};

/// Header used to fill a blank header cell (`DEFAULT_BLANK_FILL_HEADER`).
pub const BLANK_HEADER_FILL: &str = "customer";

/// Keep only `A-Za-z0-9`, space and `_`; spaces become `_`; lowercase. `Serial #` -> `serial_`.
pub fn sanitize_header(raw: &str) -> String {
    raw.trim()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == ' ' || *c == '_')
        .map(|c| {
            if c == ' ' {
                '_'
            } else {
                c.to_ascii_lowercase()
            }
        })
        .collect()
}

/// Normalise an imported cell: trim, strip leading zeros of integers, drop a trailing `.0`
/// (any all-zero fraction) from decimals. Everything else is left as is.
pub fn normalize_cell(raw: &str) -> String {
    let v = raw.trim();
    if let Some((int, frac)) = v.split_once('.') {
        let int_digits = int.strip_prefix('-').unwrap_or(int);
        let numeric = !int_digits.is_empty()
            && int_digits.chars().all(|c| c.is_ascii_digit())
            && !frac.is_empty()
            && frac.chars().all(|c| c.is_ascii_digit());
        if numeric {
            let frac = frac.trim_end_matches('0');
            let sign = if int.starts_with('-') { "-" } else { "" };
            let int = normalize_value(int_digits);
            return if frac.is_empty() {
                format!("{sign}{int}")
            } else {
                format!("{sign}{int}.{frac}")
            };
        }
        return v.to_string();
    }
    normalize_value(v)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Table {
    pub name: String,
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
    /// `true` = visible/selected. Same length as `rows`.
    pub visible: Vec<bool>,
}

/// Where to insert a new column.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ColPos {
    First,
    Last,
    /// 1-based position the new column will occupy.
    Index(usize),
    Before(String),
    After(String),
}

impl Table {
    /// Build from already-clean headers and rows (all rows visible). Rows are padded or
    /// truncated to the header width.
    pub fn new(name: &str, headers: Vec<String>, rows: Vec<Vec<String>>) -> Self {
        let w = headers.len();
        let rows: Vec<Vec<String>> = rows
            .into_iter()
            .map(|mut r| {
                r.resize(w, String::new());
                r
            })
            .collect();
        let visible = vec![true; rows.len()];
        Table {
            name: name.to_string(),
            headers,
            rows,
            visible,
        }
    }

    /// Import raw SAP data: sanitise headers, blank header -> `customer`, normalise cells.
    pub fn from_raw(name: &str, raw_headers: &[String], raw_rows: Vec<Vec<String>>) -> Self {
        let headers = raw_headers
            .iter()
            .map(|h| {
                let s = sanitize_header(h);
                if s.is_empty() {
                    BLANK_HEADER_FILL.to_string()
                } else {
                    s
                }
            })
            .collect();
        let rows = raw_rows
            .into_iter()
            .map(|r| r.iter().map(|c| normalize_cell(c)).collect())
            .collect();
        Table::new(name, headers, rows)
    }

    pub fn nrows(&self) -> usize {
        self.rows.len()
    }

    /// Fewer than 2 columns after import counts as empty (spec 1.3).
    pub fn is_empty_import(&self) -> bool {
        self.headers.len() < 2
    }

    pub fn col(&self, name: &str) -> Result<usize> {
        self.headers
            .iter()
            .position(|h| h.eq_ignore_ascii_case(name))
            .ok_or_else(|| {
                anyhow!(
                    "table '{}' has no column '{name}' (have: {})",
                    self.name,
                    self.headers.join(", ")
                )
            })
    }

    pub fn visible_indices(&self) -> Vec<usize> {
        (0..self.rows.len()).filter(|&i| self.visible[i]).collect()
    }

    pub fn visible_count(&self) -> usize {
        self.visible.iter().filter(|v| **v).count()
    }

    /// Clear the selection: every row visible again.
    pub fn reset_selection(&mut self) {
        self.visible.iter_mut().for_each(|v| *v = true);
    }

    /// Remove hidden rows for good; everything left is visible.
    pub fn materialize(&mut self) {
        let mut i = 0;
        self.rows.retain(|_| {
            let keep = self.visible[i];
            i += 1;
            keep
        });
        self.visible = vec![true; self.rows.len()];
    }

    /// Values of a column over visible rows only (no header).
    pub fn visible_column(&self, col: &str) -> Result<Vec<String>> {
        let c = self.col(col)?;
        Ok(self
            .visible_indices()
            .into_iter()
            .map(|i| self.rows[i][c].clone())
            .collect())
    }

    /// Insert a column filled with `fill` (one value per row). Returns its index.
    pub fn insert_column(&mut self, name: &str, pos: &ColPos, fill: Vec<String>) -> Result<usize> {
        if fill.len() != self.rows.len() {
            return Err(anyhow!(
                "insert_column '{name}': {} values for {} rows",
                fill.len(),
                self.rows.len()
            ));
        }
        let idx = match pos {
            ColPos::First => 0,
            ColPos::Last => self.headers.len(),
            ColPos::Index(n) => n.saturating_sub(1).min(self.headers.len()),
            ColPos::Before(c) => self.col(c)?,
            ColPos::After(c) => self.col(c)? + 1,
        };
        self.headers.insert(idx, name.to_string());
        for (row, v) in self.rows.iter_mut().zip(fill) {
            row.insert(idx, v);
        }
        Ok(idx)
    }

    /// Serialise as TSV (header + all rows, hidden included). `\`, tab, CR and LF in cells
    /// are escaped as `\\`, `\t`, `\r`, `\n`.
    pub fn to_tsv(&self) -> String {
        let mut out = String::new();
        let line = |cells: &[String], out: &mut String| {
            let esc: Vec<String> = cells.iter().map(|c| escape_cell(c)).collect();
            out.push_str(&esc.join("\t"));
            out.push('\n');
        };
        line(&self.headers, &mut out);
        for r in &self.rows {
            line(r, &mut out);
        }
        out
    }

    /// Parse TSV written by [`Table::to_tsv`] (headers are taken as is, not re-sanitised).
    pub fn from_tsv(name: &str, text: &str) -> Result<Self> {
        let mut lines = text.lines();
        let head = lines
            .next()
            .ok_or_else(|| anyhow!("empty TSV for '{name}'"))?;
        let headers: Vec<String> = head.split('\t').map(unescape_cell).collect();
        let rows = lines
            .filter(|l| !l.is_empty())
            .map(|l| l.split('\t').map(unescape_cell).collect())
            .collect();
        Ok(Table::new(name, headers, rows))
    }
}

fn escape_cell(c: &str) -> String {
    let mut s = String::with_capacity(c.len());
    for ch in c.chars() {
        match ch {
            '\\' => s.push_str("\\\\"),
            '\t' => s.push_str("\\t"),
            '\r' => s.push_str("\\r"),
            '\n' => s.push_str("\\n"),
            _ => s.push(ch),
        }
    }
    s
}

fn unescape_cell(c: &str) -> String {
    let mut s = String::with_capacity(c.len());
    let mut it = c.chars();
    while let Some(ch) = it.next() {
        if ch == '\\' {
            match it.next() {
                Some('t') => s.push('\t'),
                Some('r') => s.push('\r'),
                Some('n') => s.push('\n'),
                Some('\\') => s.push('\\'),
                Some(o) => {
                    s.push('\\');
                    s.push(o);
                }
                None => s.push('\\'),
            }
        } else {
            s.push(ch);
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn header_sanitising() {
        assert_eq!(
            sanitize_header("Name of the ship-to party"),
            "name_of_the_shipto_party"
        );
        assert_eq!(sanitize_header("Serial #"), "serial_");
        assert_eq!(sanitize_header("Deliv. date(From/to)"), "deliv_datefromto");
        assert_eq!(sanitize_header("Ship-to party"), "shipto_party");
        assert_eq!(sanitize_header("  "), "");
    }

    #[test]
    fn blank_header_filled_with_customer() {
        let t = Table::from_raw("t", &s(&["Plant", ""]), vec![s(&["a", "b"])]);
        assert_eq!(t.headers, s(&["plant", "customer"]));
    }

    #[test]
    fn cell_normalisation() {
        assert_eq!(normalize_cell("144.0"), "144");
        assert_eq!(normalize_cell("0012"), "12");
        assert_eq!(normalize_cell("0.50"), "0.5");
        assert_eq!(normalize_cell("-3.00"), "-3");
        assert_eq!(normalize_cell(" abc "), "abc");
        assert_eq!(normalize_cell("1.2.3"), "1.2.3");
        assert_eq!(normalize_cell("2026-11-05"), "2026-11-05");
    }

    #[test]
    fn rows_padded_to_header_width() {
        let t = Table::new(
            "t",
            s(&["a", "b", "c"]),
            vec![s(&["1"]), s(&["1", "2", "3", "4"])],
        );
        assert!(t.rows.iter().all(|r| r.len() == 3));
    }

    #[test]
    fn selection_mask_and_materialize() {
        let mut t = Table::new("t", s(&["a"]), vec![s(&["1"]), s(&["2"]), s(&["3"])]);
        t.visible[1] = false;
        assert_eq!(t.visible_column("a").unwrap(), s(&["1", "3"]));
        t.reset_selection();
        assert_eq!(t.visible_count(), 3);
        t.visible[0] = false;
        t.materialize();
        assert_eq!(t.rows, vec![s(&["2"]), s(&["3"])]);
        assert_eq!(t.visible, vec![true, true]);
    }

    #[test]
    fn insert_column_positions() {
        let mut t = Table::new("t", s(&["a", "b"]), vec![s(&["1", "2"])]);
        t.insert_column("x", &ColPos::Before("b".into()), s(&["X"]))
            .unwrap();
        t.insert_column("y", &ColPos::Last, s(&["Y"])).unwrap();
        t.insert_column("z", &ColPos::First, s(&["Z"])).unwrap();
        t.insert_column("w", &ColPos::Index(3), s(&["W"])).unwrap();
        assert_eq!(t.headers, s(&["z", "a", "w", "x", "b", "y"]));
        assert_eq!(t.rows[0], s(&["Z", "1", "W", "X", "2", "Y"]));
        assert!(t.insert_column("q", &ColPos::Last, vec![]).is_err());
    }

    #[test]
    fn tsv_round_trip_with_escapes() {
        let t = Table::new(
            "t",
            s(&["a", "b"]),
            vec![s(&["x\ty", "line1\nline2"]), s(&["back\\slash", ""])],
        );
        let text = t.to_tsv();
        assert_eq!(text.lines().count(), 3);
        let back = Table::from_tsv("t", &text).unwrap();
        assert_eq!(back.rows, t.rows);
        assert_eq!(back.headers, t.headers);
    }

    #[test]
    fn missing_column_error_lists_columns() {
        let t = Table::new("t", s(&["a"]), vec![]);
        let e = t.col("nope").unwrap_err().to_string();
        assert!(e.contains("nope") && e.contains("a"));
    }
}
