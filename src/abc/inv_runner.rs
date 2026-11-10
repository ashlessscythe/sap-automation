//! Runner for the `INV` reports, so far `xdock` (spec 5.3, verified in 9.6), parameterised by a
//! SAP-step seam of its own ([`InvSteps`]); the `mustgo` seam ([`super::runner::SapSteps`]) is
//! untouched. The pure stages are in [`super::inv`].
//!
//! Flow (spec 5.3 = toyota rows 25-30): VT11 (variant `VT11_XDOCK_VARIANT`, layout `scorecard`) ->
//! VL06O for all shipments (layout `xdock`) -> one row per delivery + `date` column -> ZMDETPC
//! summarised for the deliveries (variant `ZMDETPC_VARIANT`) -> merge (spec 4.12) -> pivot plant /
//! sum of `total`. The macro's ZVT11 Closed step is dropped (its output is never used, item 10).
//!
//! Error semantics are those of `mustgo`: the first no-data / error / empty list stops the run
//! and nothing is written (outputs are written only after the last step).
//!
//! Outputs (TSV, `registry::output_path`, folder `INV`):
//! `<reports_dir>\INV\<today>\xdock\<date>_XDOCK_{VT11,VL06O,PIVOT}.tsv`. `VL06O` is the merged
//! table (the macro's `XDOCK_VL06O` sheet, without the macro's three stray trailing columns),
//! `PIVOT` the long table `plant`, `total` (no grand-total row, `(blank)` last).
//!
//! Canonical columns each step must deliver (a missing one stops the run naming it):
//! * vt11: `shipment_number`
//! * vl06o: `delivery`, `number_of_packages`, `plant` (the `xdock` layout also has
//!   `shipto_party`, `name_of_the_shipto_party`, `external_delivery_id`, `total_gds_mvt_stat`)
//! * zmdetpc: `delivery`, `pallet`, `child`
#![allow(dead_code)]

use super::inv;
use super::registry::{self, XDOCK};
use super::run::RunDate;
use super::runner::{export_date_label, NoDataError, OutputSpec, StepOutput};
use super::table::Table;
use crate::utils::export_outcome::paste_value_list;
use anyhow::{anyhow, Context, Result};
use std::path::PathBuf;

/// Names of the SAP variants/layouts a run uses (resolved by the caller).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvSelection {
    pub vt11_variant: String,
    pub vt11_layout: String,
    pub vl06o_layout: String,
    pub zmdetpc_variant: String,
}

impl InvSelection {
    /// Defaults of `xdock` (spec 5.3 and 5.2 row 28).
    pub fn xdock() -> Self {
        Self {
            vt11_variant: "VT11_XDOCK_VARIANT".to_string(),
            vt11_layout: "scorecard".to_string(),
            vl06o_layout: "xdock".to_string(),
            zmdetpc_variant: "ZMDETPC_VARIANT".to_string(),
        }
    }
}

/// One method per SAP call of the INV reports. Inputs are plain, distinct, blank-free lists.
pub trait InvSteps {
    /// VT11 for the run date (range for week runs) with `variant` and `layout`.
    fn vt11(&mut self, date: &RunDate, variant: &str, layout: &str) -> Result<StepOutput>;
    /// VL06O (item view) for the shipments with `layout`.
    fn vl06o_by_shipments(&mut self, shipments: &[String], layout: &str) -> Result<StepOutput>;
    /// ZMDETPC summarised for the deliveries and the run date with `variant`.
    fn zmdetpc_summarised(
        &mut self,
        deliveries: &[String],
        date: &RunDate,
        variant: &str,
    ) -> Result<StepOutput>;
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InvSummary {
    /// `vt11`, `vl06o`, `pivot` files.
    pub outputs: Vec<PathBuf>,
    pub export_date: String,
    pub shipments: usize,
    /// Distinct deliveries sent to ZMDETPC (= rows of the merged table minus ZMDETPC-only rows).
    pub deliveries: usize,
    pub zmdetpc_rows: usize,
    /// Deliveries without a ZMDETPC row (`total` falls back to `number_of_packages`).
    pub without_zmdetpc: usize,
    pub vl06o_rows: usize,
    pub pivot_rows: usize,
    /// Sum of `total` over all plants.
    pub total: String,
}

fn log(msg: &str) {
    println!("[xdock] {msg}");
}

fn take(step: &str, out: Result<StepOutput>, required: &[&str]) -> Result<Table> {
    let out = out.with_context(|| format!("xdock step '{step}' failed"))?;
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
            "xdock step '{step}': missing column(s) {} (found: {})",
            missing.join(", "),
            t.headers.join(", ")
        ));
    }
    log(&format!("{step}: {} rows", t.nrows()));
    Ok(t)
}

fn stage<T>(step: &str, r: Result<T>) -> Result<T> {
    r.with_context(|| format!("xdock step '{step}'"))
}

fn no_data(step: &str, message: &str) -> anyhow::Error {
    NoDataError {
        step: step.to_string(),
        message: message.to_string(),
    }
    .into()
}

/// Distinct, normalised, blank-free shipment numbers of the VT11 table (table order).
pub fn shipment_list(vt11: &Table) -> Result<Vec<String>> {
    let c = vt11.col("shipment_number")?;
    let raw: Vec<&str> = vt11.rows.iter().map(|r| r[c].as_str()).collect();
    Ok(paste_value_list(&raw, false))
}

/// Number of distinct deliveries of `vl06o` (by [`inv::delivery_key`]) that have no ZMDETPC row.
pub fn count_without_zmdetpc(vl06o: &Table, zmdetpc: &Table) -> Result<usize> {
    let zc = zmdetpc.col("delivery")?;
    let have: std::collections::HashSet<String> = zmdetpc
        .rows
        .iter()
        .map(|r| inv::delivery_key(&r[zc]))
        .collect();
    let c = vl06o.col("delivery")?;
    Ok(vl06o
        .rows
        .iter()
        .filter(|r| !have.contains(&inv::delivery_key(&r[c])))
        .count())
}

/// Write `vt11`, `vl06o` and `pivot` as TSV; on a write error the files already written are
/// removed again. Returns the paths in that order.
pub fn write_outputs(
    out: &OutputSpec,
    run_date: &RunDate,
    vt11: &Table,
    vl06o: &Table,
    pivot: &Table,
) -> Result<Vec<PathBuf>> {
    let mut paths: Vec<PathBuf> = Vec::new();
    for (name, t) in [("vt11", vt11), ("vl06o", vl06o), ("pivot", pivot)] {
        let path = registry::output_path(&out.reports_dir, &XDOCK, out.today, run_date, name);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).with_context(|| format!("create {}", dir.display()))?;
        }
        if let Err(e) = std::fs::write(&path, t.to_tsv()) {
            for done in &paths {
                let _ = std::fs::remove_file(done);
            }
            return Err(anyhow!("write {}: {e}", path.display()));
        }
        paths.push(path);
    }
    Ok(paths)
}

/// Run `xdock`: VT11 -> VL06O -> ZMDETPC -> merge -> pivot -> outputs.
pub fn run_xdock(
    steps: &mut impl InvSteps,
    date: &RunDate,
    sel: &InvSelection,
    out: &OutputSpec,
) -> Result<InvSummary> {
    // VT11
    let vt11 = take(
        "vt11",
        steps.vt11(date, &sel.vt11_variant, &sel.vt11_layout),
        &["shipment_number"],
    )?;
    let shipments = stage("vt11 shipments", shipment_list(&vt11))?;
    if shipments.is_empty() {
        return Err(no_data("vt11", "no shipments"));
    }
    log(&format!("{} shipments", shipments.len()));

    // VL06O
    let mut vl06o = take(
        "vl06o",
        steps.vl06o_by_shipments(&shipments, &sel.vl06o_layout),
        &["delivery", "number_of_packages", "plant"],
    )?;
    let export_date = export_date_label(date);
    stage(
        "vl06o prepare",
        inv::prepare_vl06o(&mut vl06o, &export_date),
    )?;
    let deliveries = match inv::delivery_values(&vl06o) {
        Ok(d) => d,
        Err(_) => return Err(no_data("vl06o", "no deliveries")),
    };
    log(&format!("{} deliveries", deliveries.len()));

    // ZMDETPC
    let zmdetpc = take(
        "zmdetpc",
        steps.zmdetpc_summarised(&deliveries, date, &sel.zmdetpc_variant),
        &["delivery", "pallet", "child"],
    )?;
    let without = stage("zmdetpc coverage", count_without_zmdetpc(&vl06o, &zmdetpc))?;
    if without > 0 {
        log(&format!(
            "{without} deliveries have no ZMDETPC row (total = number_of_packages)"
        ));
    }

    // merge + pivot
    let merged = stage("merge", inv::merge_zmdetpc(&vl06o, &zmdetpc))?;
    let pivot = stage("pivot", inv::pivot_sum(&merged, "plant", "total"))?;
    let tc = pivot.col("total")?;
    let total: f64 = pivot
        .rows
        .iter()
        .filter_map(|r| r[tc].parse::<f64>().ok())
        .sum();

    // save (only now, so a failed run leaves no output)
    let outputs = stage("save", write_outputs(out, date, &vt11, &merged, &pivot))?;
    for p in &outputs {
        log(&format!("wrote {}", p.display()));
    }
    Ok(InvSummary {
        outputs,
        export_date,
        shipments: shipments.len(),
        deliveries: deliveries.len(),
        zmdetpc_rows: zmdetpc.nrows(),
        without_zmdetpc: without,
        vl06o_rows: merged.nrows(),
        pivot_rows: pivot.nrows(),
        total: format!("{total}"),
    })
}
