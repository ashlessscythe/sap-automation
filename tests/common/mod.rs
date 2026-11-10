//! Shared helpers for the ABC golden tests.
#![allow(dead_code)]

use sap_automation::abc::table::Table;
use std::path::PathBuf;

/// Load `tests/fixtures/<dir>/<file>` as a [`Table`] (headers as in the file).
pub fn load_fixture(dir: &str, file: &str) -> Table {
    let path: PathBuf = [env!("CARGO_MANIFEST_DIR"), "tests", "fixtures", dir, file]
        .iter()
        .collect();
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read fixture {}: {e}", path.display()));
    Table::from_tsv(file, &text).unwrap_or_else(|e| panic!("bad fixture {file}: {e}"))
}

/// Compare two tables row by row; `Err(report)` lists the first `max_rows` differing rows.
pub fn diff_tables(expected: &Table, actual: &Table, max_rows: usize) -> Result<(), String> {
    let mut report = String::new();
    if expected.headers != actual.headers {
        report.push_str(&format!(
            "headers differ:\n  expected: {:?}\n  actual:   {:?}\n",
            expected.headers, actual.headers
        ));
    }
    if expected.nrows() != actual.nrows() {
        report.push_str(&format!(
            "row count differs: expected {}, actual {}\n",
            expected.nrows(),
            actual.nrows()
        ));
    }
    let mut bad = 0usize;
    for i in 0..expected.nrows().min(actual.nrows()) {
        let (e, a) = (&expected.rows[i], &actual.rows[i]);
        if e == a {
            continue;
        }
        bad += 1;
        if bad <= max_rows {
            report.push_str(&format!("row {i} (0-based data row) differs:\n"));
            for c in 0..e.len().max(a.len()) {
                let ev = e.get(c).map(String::as_str).unwrap_or("<missing>");
                let av = a.get(c).map(String::as_str).unwrap_or("<missing>");
                if ev != av {
                    let name = expected
                        .headers
                        .get(c)
                        .or_else(|| actual.headers.get(c))
                        .map(String::as_str)
                        .unwrap_or("?");
                    report.push_str(&format!(
                        "    col {c} '{name}': expected {ev:?}, actual {av:?}\n"
                    ));
                }
            }
        }
    }
    if bad > 0 {
        report.push_str(&format!(
            "{bad} differing row(s) in total (showing at most {max_rows})\n"
        ));
    }
    if report.is_empty() {
        Ok(())
    } else {
        Err(report)
    }
}

/// Panic with a readable report unless the tables are identical.
pub fn assert_tables_equal(expected: &Table, actual: &Table) {
    if let Err(r) = diff_tables(expected, actual, 10) {
        panic!(
            "tables '{}' vs '{}' differ:\n{r}",
            expected.name, actual.name
        );
    }
}
