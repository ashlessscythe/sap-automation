//! Runner for the `mustgo` flow (spec 5.1 steps 9-22), parameterised by a SAP-step seam.
//!
//! The runner only orchestrates: every SAP call goes through [`SapSteps`] (plain inputs, an
//! already loaded [`Table`] or "no data" back) and every table transformation is a pure stage
//! of [`super::mustgo`]. The ZMDESNR history call of step 19 (`Filter_Copy_MG`) is wasted work
//! in the macro (its result never reaches the output), so the trait has no method for it.
//!
//! Error semantics: the first no-data / error / empty list stops the run, the remaining steps
//! are skipped and nothing is written (outputs are only written after the last step).
//!
//! `export_date` (spec 7.0 item 2 open point, decided): the run date `yyyy-mm-dd` for a single
//! date, the `WK_<n>` label (registry::date_label) for a range. There is one run per call, no
//! per-day loop.
//!
//! Canonical columns each step must deliver (checked after loading, a missing one stops the
//! run with an error listing the columns found; extra columns are ignored; the short SAP list
//! titles are mapped by `raw::*_ALIASES`, canonical names are accepted as they are):
//! * vt11: `shipment_number`, `transportplanningpt`
//! * vl06o: `delivery`
//! * zmdesnr by deliveries: `delivery`, `transfer_order_number`, `parent_serial_number`, `status`
//! * lt23: `delivery`, `transfer_order_number`, `creation_date`, `creation_time`,
//!   `source_storage_bin`
//! * zmdesnr by deliveries + TOs: `lm_serial_number`, `parent_serial_number`, `plant`,
//!   `created_on` (the `week` column is inserted at position 19, so the layout should have the
//!   19 columns of `mg_view`: `created_on` is column 18)
#![allow(dead_code)]

use super::mustgo;
use super::raw::{load_export_canonical, RawOptions};
use super::registry;
use super::run::RunDate;
use super::table::Table;
use crate::utils::export_outcome::ExportOutcome;
use anyhow::{anyhow, Context, Result};
use chrono::NaiveDate;
use std::fmt;
use std::path::PathBuf;

/// Result of one SAP step.
#[derive(Debug, Clone)]
pub enum StepOutput {
    /// Export loaded into a table (canonical headers).
    Table(Table),
    /// SAP reported that the selection returned nothing.
    NoData(String),
}

/// One method per SAP call of `mustgo`. Inputs are plain, already distinct/blank-free lists.
pub trait SapSteps {
    /// Step 9: VT11 for the run date (range for week runs).
    fn vt11(&mut self, date: &RunDate) -> Result<StepOutput>;
    /// Step 11: VL06O for the TPDC shipments.
    fn vl06o(&mut self, shipments: &[String]) -> Result<StepOutput>;
    /// Step 12: ZMDESNR by deliveries.
    fn zmdesnr_by_deliveries(&mut self, deliveries: &[String]) -> Result<StepOutput>;
    /// Step 16: LT23 for transfer orders.
    fn lt23(&mut self, transfer_orders: &[String]) -> Result<StepOutput>;
    /// Step 19: ZMDESNR by deliveries + transfer orders.
    fn zmdesnr_by_deliveries_and_tos(
        &mut self,
        deliveries: &[String],
        transfer_orders: &[String],
    ) -> Result<StepOutput>;
}

/// Where outputs go.
#[derive(Debug, Clone)]
pub struct OutputSpec {
    pub reports_dir: PathBuf,
    pub today: NaiveDate,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RunSummary {
    /// `lt23`, `snr_copy`, `pivot` files.
    pub outputs: Vec<PathBuf>,
    pub export_date: String,
    pub shipments: usize,
    pub deliveries: usize,
    pub transfer_orders: usize,
    pub lt23_rows: usize,
    pub snr_copy_rows: usize,
    pub pivot_rows: usize,
}

/// SAP returned no data at `step` (distinguishable from other failures via `downcast_ref`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoDataError {
    pub step: String,
    pub message: String,
}

impl fmt::Display for NoDataError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "no data at step '{}': {}", self.step, self.message)
    }
}

impl std::error::Error for NoDataError {}

/// Turn an [`ExportOutcome`] into a [`StepOutput`]: a file is read and loaded for `tcode`
/// (`vt11`, `vl06o`, `zmdesnr`, `lt23`); no-data stays no-data; an error (or a file outcome
/// without a path, i.e. a clipboard export) is an `Err`.
pub fn outcome_to_output(
    tcode: &str,
    outcome: ExportOutcome,
    opts: &RawOptions,
) -> Result<StepOutput> {
    match outcome {
        ExportOutcome::File(path) => {
            if path.as_os_str().is_empty() {
                return Err(anyhow!(
                    "{tcode}: export produced no file (clipboard export)"
                ));
            }
            let bytes = std::fs::read(&path)
                .with_context(|| format!("{tcode}: cannot read export {}", path.display()))?;
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| tcode.to_string());
            let t = load_export_canonical(tcode, &name, &bytes, opts)
                .with_context(|| format!("{tcode}: cannot load {}", path.display()))?;
            Ok(StepOutput::Table(t))
        }
        ExportOutcome::NoData(m) => Ok(StepOutput::NoData(m)),
        ExportOutcome::Error(m) => Err(anyhow!("{tcode}: export failed: {m}")),
    }
}

fn log(msg: &str) {
    println!("[mustgo] {msg}");
}

/// Resolve a step's output into a table or the run-stopping error.
fn take(step: &str, out: Result<StepOutput>, required: &[&str]) -> Result<Table> {
    let out = out.with_context(|| format!("mustgo step '{step}' failed"))?;
    let t = match out {
        StepOutput::Table(t) => t,
        StepOutput::NoData(message) => {
            return Err(NoDataError {
                step: step.to_string(),
                message,
            }
            .into())
        }
    };
    let missing: Vec<&str> = required
        .iter()
        .copied()
        .filter(|c| t.col(c).is_err())
        .collect();
    if !missing.is_empty() {
        return Err(anyhow!(
            "mustgo step '{step}': missing column(s) {} (found: {})",
            missing.join(", "),
            t.headers.join(", ")
        ));
    }
    log(&format!("{step}: {} rows", t.nrows()));
    Ok(t)
}

fn stage<T>(step: &str, r: Result<T>) -> Result<T> {
    r.with_context(|| format!("mustgo step '{step}'"))
}

/// Export-date text for a run: `yyyy-mm-dd` (single date) or `WK_<n>` (range).
pub fn export_date_label(date: &RunDate) -> String {
    match date.date2 {
        Some(end) if end != date.date1 => registry::date_label(date),
        _ => date.date1.format("%Y-%m-%d").to_string(),
    }
}

/// Run `mustgo`: VT11 -> VL06O -> ZMDESNR -> LT23 -> ZMDESNR (deliveries + TOs) -> outputs.
pub fn run_mustgo(
    steps: &mut impl SapSteps,
    date: &RunDate,
    out: &OutputSpec,
) -> Result<RunSummary> {
    // 9-10 VT11, TPDC selection
    let mut vt11 = take(
        "vt11",
        steps.vt11(date),
        &["shipment_number", "transportplanningpt"],
    )?;
    let shipments = stage(
        "vt11 TPDC selection",
        mustgo::select_tpdc_shipments(&mut vt11),
    )?;
    log(&format!("{} TPDC shipments", shipments.len()));

    // 11 VL06O
    let vl06o = take("vl06o", steps.vl06o(&shipments), &["delivery"])?;
    let deliveries = stage("vl06o deliveries", mustgo::delivery_list(&vl06o))?;

    // 12-15 ZMDESNR by deliveries, selection and dedup
    let mut sn_del = take(
        "zmdesnr_by_deliveries",
        steps.zmdesnr_by_deliveries(&deliveries),
        &[
            "delivery",
            "transfer_order_number",
            "parent_serial_number",
            "status",
        ],
    )?;
    stage("sn_del selection", mustgo::filter_sn_del(&mut sn_del))?;
    let tos = stage(
        "sn_del transfer orders",
        mustgo::transfer_order_list(&sn_del),
    )?;
    if tos.is_empty() {
        return Err(NoDataError {
            step: "sn_del selection".to_string(),
            message: "no serial rows with a parent and status other than PAL".to_string(),
        }
        .into());
    }
    log(&format!(
        "{} deliveries, {} transfer orders",
        deliveries.len(),
        tos.len()
    ));

    // 16 LT23 and post-process
    let mut lt23 = take(
        "lt23",
        steps.lt23(&tos),
        &[
            "delivery",
            "transfer_order_number",
            "creation_date",
            "creation_time",
            "source_storage_bin",
        ],
    )?;
    stage("lt23 post-process", mustgo::lt23_postprocess(&mut lt23))?;
    if lt23.nrows() == 0 {
        return Err(NoDataError {
            step: "lt23 post-process".to_string(),
            message: "no delivery with more than one transfer order".to_string(),
        }
        .into());
    }

    // 17-18 lookup + delete, giving the lists for step 19
    stage(
        "sn_del lookup",
        mustgo::mark_sn_del_by_lt23(&mut sn_del, &lt23),
    )?;
    let deliveries2 = stage("sn_del deliveries", mustgo::delivery_list(&sn_del))?;
    let tos2 = stage(
        "sn_del transfer orders",
        mustgo::transfer_order_list(&sn_del),
    )?;
    if tos2.is_empty() {
        return Err(NoDataError {
            step: "sn_del lookup".to_string(),
            message: "no serial rows left".to_string(),
        }
        .into());
    }

    // 19 ZMDESNR by deliveries + TOs (no history call)
    let sn_to = take(
        "zmdesnr_by_deliveries_and_tos",
        steps.zmdesnr_by_deliveries_and_tos(&deliveries2, &tos2),
        &[
            "lm_serial_number",
            "parent_serial_number",
            "plant",
            "created_on",
        ],
    )?;

    // 19b-21 snr_copy, 22 pivot
    let export_date = export_date_label(date);
    let snr_copy = stage("snr_copy", mustgo::snr_copy_labeled(&sn_to, &export_date))?;
    if snr_copy.nrows() == 0 {
        return Err(NoDataError {
            step: "snr_copy".to_string(),
            message: "no rows".to_string(),
        }
        .into());
    }
    let pivot = stage("pivot", mustgo::pivot_plant_by_date(&snr_copy))?;

    // 22 save (only now, so a failed run leaves no output)
    let outputs = stage(
        "save",
        mustgo::write_outputs(&out.reports_dir, out.today, date, &lt23, &snr_copy, &pivot),
    )?;
    for p in &outputs {
        log(&format!("wrote {}", p.display()));
    }
    Ok(RunSummary {
        outputs,
        export_date,
        shipments: shipments.len(),
        deliveries: deliveries.len(),
        transfer_orders: tos.len(),
        lt23_rows: lt23.nrows(),
        snr_copy_rows: snr_copy.nrows(),
        pivot_rows: pivot.nrows(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_date_labels() {
        let d = |m, day| NaiveDate::from_ymd_opt(2026, m, day).unwrap();
        let single = RunDate {
            date1: d(11, 5),
            date2: None,
        };
        assert_eq!(export_date_label(&single), "2026-11-05");
        let same = RunDate {
            date1: d(11, 5),
            date2: Some(d(11, 5)),
        };
        assert_eq!(export_date_label(&same), "2026-11-05");
        let wk = RunDate {
            date1: d(11, 1),
            date2: Some(d(11, 7)),
        };
        assert_eq!(export_date_label(&wk), "WK_45");
    }

    #[test]
    fn outcome_mapping() {
        let o = RawOptions::default();
        assert!(matches!(
            outcome_to_output("lt23", ExportOutcome::NoData("x".into()), &o).unwrap(),
            StepOutput::NoData(m) if m == "x"
        ));
        assert!(outcome_to_output("lt23", ExportOutcome::Error("boom".into()), &o).is_err());
        assert!(outcome_to_output("lt23", ExportOutcome::File(PathBuf::new()), &o).is_err());
        let e = outcome_to_output(
            "lt23",
            ExportOutcome::File(PathBuf::from("does/not/exist.txt")),
            &o,
        )
        .unwrap_err();
        assert!(e.to_string().contains("cannot read"), "{e}");
    }
}
