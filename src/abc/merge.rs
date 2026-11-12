//! Merge per-day report outputs of a range (e.g. a week) into one workbook per report.
//!
//! Per day the runner writes `<DATE>_<RNAME>_<table>.tsv` files (see `registry::output_path`).
//! `merge_report` reads those back (plain UTF-8, as the runners write them), concatenates each
//! table across the days (a `run_date` column is prepended, see `Table::concat`) and writes ONE
//! workbook (`registry::merged_path`) with one sheet per table, in `registry::tables` order.
#![allow(dead_code)]

use anyhow::{anyhow, Context, Result};
use chrono::NaiveDate;
use std::path::{Path, PathBuf};

use super::registry::{self, ReportDef};
use super::run::RunDate;
use super::table::Table;
use super::xlsx_out;

/// What a merge produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergeOutcome {
    /// Report (rName) this outcome belongs to.
    pub report: String,
    /// The merged workbook.
    pub path: PathBuf,
    /// Days that had output and were merged (ascending, as given).
    pub days_merged: Vec<NaiveDate>,
    /// Days without any output file (e.g. skipped for no data).
    pub days_missing: Vec<NaiveDate>,
    /// Sheets written, in order, with their row counts (header excluded, `run_date` included
    /// as a column, so the count is the sum over the days).
    pub tables: Vec<(String, usize)>,
}

/// Merge the per-day outputs of `def` for `days` into one workbook at
/// `registry::merged_path(reports_dir, def, today, range)`.
///
/// - A day with none of the report's table files is recorded in `days_missing` and skipped.
/// - A day with some but not all table files is an error naming the day and the missing tables.
/// - A table whose columns differ between days is an error naming the table and the days.
/// - No day with output at all: error "nothing to merge", nothing is written.
/// - Nothing is written on any error.
pub fn merge_report(
    reports_dir: &Path,
    def: &ReportDef,
    today: NaiveDate,
    range: &RunDate,
    days: &[NaiveDate],
) -> Result<MergeOutcome> {
    let names = registry::tables(def.name);
    let mut days_merged = Vec::new();
    let mut days_missing = Vec::new();
    // per table: (ISO run_date, table) of every merged day
    let mut per_table: Vec<Vec<(String, Table)>> = vec![Vec::new(); names.len()];

    for &day in days {
        let single = RunDate::single(day);
        let label = registry::date_label(&single);
        // the merged `run_date` column is ISO 8601; file names / messages keep `label`
        let iso = day.format("%Y-%m-%d").to_string();
        let paths: Vec<PathBuf> = names
            .iter()
            .map(|t| registry::output_path(reports_dir, def, today, &single, t))
            .collect();
        let missing: Vec<&str> = names
            .iter()
            .zip(&paths)
            .filter(|(_, p)| !p.is_file())
            .map(|(t, _)| *t)
            .collect();
        if missing.len() == names.len() {
            days_missing.push(day);
            continue;
        }
        if !missing.is_empty() {
            return Err(anyhow!(
                "merge {}: {label} has incomplete output, missing table(s): {}",
                def.name,
                missing.join(", ")
            ));
        }
        for (i, p) in paths.iter().enumerate() {
            let text =
                std::fs::read_to_string(p).with_context(|| format!("read {}", p.display()))?;
            let t = Table::from_tsv(names[i], &text)
                .with_context(|| format!("merge {}: {label} table {}", def.name, names[i]))?;
            per_table[i].push((iso.clone(), t));
        }
        days_merged.push(day);
    }
    if days_merged.is_empty() {
        return Err(anyhow!(
            "merge {}: nothing to merge, no output found for any of the {} day(s)",
            def.name,
            days.len()
        ));
    }

    let mut merged: Vec<Table> = Vec::with_capacity(names.len());
    for (i, parts) in per_table.iter().enumerate() {
        merged.push(
            Table::concat(parts)
                .with_context(|| format!("merge {}: table {}", def.name, names[i]))?,
        );
    }
    let sheets: Vec<(&str, &Table)> = names.iter().copied().zip(merged.iter()).collect();
    let path = registry::merged_path(reports_dir, def, today, range);
    xlsx_out::write_workbook(&path, &sheets)?;
    Ok(MergeOutcome {
        report: def.name.to_string(),
        path,
        days_merged,
        days_missing,
        tables: names
            .iter()
            .zip(&merged)
            .map(|(n, t)| (n.to_string(), t.nrows()))
            .collect(),
    })
}

/// Run `merge_report` for every report of `registry::chain(rname)` (prerequisites first, then
/// `rname`). One xlsx per report. A report that fails (e.g. a prerequisite without any output)
/// does not abort the others: every report gets its own `Result`, in chain order. An unknown
/// rName is an error.
pub fn merge_chain(
    reports_dir: &Path,
    rname: &str,
    today: NaiveDate,
    range: &RunDate,
    days: &[NaiveDate],
) -> Result<Vec<(String, Result<MergeOutcome>)>> {
    let chain = registry::chain(rname);
    if chain.is_empty() {
        return Err(anyhow!("merge: unknown report '{rname}'"));
    }
    Ok(chain
        .into_iter()
        .map(|name| {
            let def = registry::find_report(name).expect("chain names are registry names");
            (
                name.to_string(),
                merge_report(reports_dir, def, today, range, days),
            )
        })
        .collect())
}
