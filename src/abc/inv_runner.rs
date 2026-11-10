//! Runner for the `INV` reports, so far `xdock` (spec 5.3, verified in 9.6) and `toyota` (5.2), parameterised by a
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
use super::registry::{self, ReportDef, MISC, TOYOTA, XDOCK};
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
    /// Defaults of `toyota` (spec 5.2): VT11 variant `VT11_TOYOTA_VARIANT`, the rest as `xdock`.
    pub fn toyota() -> Self {
        Self {
            vt11_variant: "VT11_TOYOTA_VARIANT".to_string(),
            ..Self::xdock()
        }
    }

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

/// The two SAP calls of `misc` (no ZMDETPC); [`InvSteps`] stays as it is for xdock/toyota.
pub trait MiscSteps {
    /// VT11 for the run date with `variant` and `layout`.
    fn vt11(&mut self, date: &RunDate, variant: &str, layout: &str) -> Result<StepOutput>;
    /// VL06O (item view) for the shipments with `layout` (`inv`).
    fn vl06o_by_shipments(&mut self, shipments: &[String], layout: &str) -> Result<StepOutput>;
}

impl InvSelection {
    /// Defaults of `misc` (spec 5.4): VT11 variant `VT11_MISC_VARIANT`, layout `scorecard`, VL06O
    /// layout `inv`. `zmdetpc_variant` is not used.
    pub fn misc() -> Self {
        Self {
            vt11_variant: "VT11_MISC_VARIANT".to_string(),
            vl06o_layout: "inv".to_string(),
            ..Self::xdock()
        }
    }
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
    /// `misc` only: rows whose `PLACEHOLDER_SP` shipping point was replaced by the ship-to name part.
    pub regex_replaced: usize,
    /// `toyota` only: `(plant, trailers)` rows of the trailers table (empty for `xdock`).
    pub trailers: Vec<(String, String)>,
}

fn log(rep: &str, msg: &str) {
    println!("[{rep}] {msg}");
}

fn take(rep: &str, step: &str, out: Result<StepOutput>, required: &[&str]) -> Result<Table> {
    let out = out.with_context(|| format!("{rep} step '{step}' failed"))?;
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
            "{rep} step '{step}': missing column(s) {} (found: {})",
            missing.join(", "),
            t.headers.join(", ")
        ));
    }
    log(rep, &format!("{step}: {} rows", t.nrows()));
    Ok(t)
}

fn stage<T>(rep: &str, step: &str, r: Result<T>) -> Result<T> {
    r.with_context(|| format!("{rep} step '{step}'"))
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

/// Write the named tables as TSV under the report's output paths; on a write error the files
/// already written are removed again. Returns the paths in the order given.
pub fn write_outputs(
    def: &ReportDef,
    out: &OutputSpec,
    run_date: &RunDate,
    tables: &[(&str, &Table)],
) -> Result<Vec<PathBuf>> {
    let mut paths: Vec<PathBuf> = Vec::new();
    for (name, t) in tables {
        let path = registry::output_path(&out.reports_dir, def, out.today, run_date, name);
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

/// Run `xdock`: VT11 -> VL06O -> ZMDETPC -> merge -> pivot -> outputs (`vt11`, `vl06o`, `pivot`).
pub fn run_xdock(
    steps: &mut impl InvSteps,
    date: &RunDate,
    sel: &InvSelection,
    out: &OutputSpec,
) -> Result<InvSummary> {
    run_inv(steps, &XDOCK, false, date, sel, out)
}

/// Run `toyota` (spec 5.2): the `xdock` flow with the toyota VT11 variant, plus the `trailers`
/// table (`ROUND(total / 14.5)` per pivot row). Outputs `vt11`, `vl06o`, `pivot`, `trailers`.
pub fn run_toyota(
    steps: &mut impl InvSteps,
    date: &RunDate,
    sel: &InvSelection,
    out: &OutputSpec,
) -> Result<InvSummary> {
    run_inv(steps, &TOYOTA, true, date, sel, out)
}

/// Shared flow of the INV reports. `with_trailers` adds the trailers table to the outputs.
fn run_inv(
    steps: &mut impl InvSteps,
    def: &ReportDef,
    with_trailers: bool,
    date: &RunDate,
    sel: &InvSelection,
    out: &OutputSpec,
) -> Result<InvSummary> {
    let rep = def.name;
    // VT11
    let vt11 = take(
        rep,
        "vt11",
        steps.vt11(date, &sel.vt11_variant, &sel.vt11_layout),
        &["shipment_number"],
    )?;
    let shipments = stage(rep, "vt11 shipments", shipment_list(&vt11))?;
    if shipments.is_empty() {
        return Err(no_data("vt11", "no shipments"));
    }
    log(rep, &format!("{} shipments", shipments.len()));

    // VL06O
    let mut vl06o = take(
        rep,
        "vl06o",
        steps.vl06o_by_shipments(&shipments, &sel.vl06o_layout),
        &["delivery", "number_of_packages", "plant"],
    )?;
    let export_date = export_date_label(date);
    stage(
        rep,
        "vl06o prepare",
        inv::prepare_vl06o(&mut vl06o, &export_date),
    )?;
    let deliveries = match inv::delivery_values(&vl06o) {
        Ok(d) => d,
        Err(_) => return Err(no_data("vl06o", "no deliveries")),
    };
    log(rep, &format!("{} deliveries", deliveries.len()));

    // ZMDETPC
    let zmdetpc = take(
        rep,
        "zmdetpc",
        steps.zmdetpc_summarised(&deliveries, date, &sel.zmdetpc_variant),
        &["delivery", "pallet", "child"],
    )?;
    let without = stage(
        rep,
        "zmdetpc coverage",
        count_without_zmdetpc(&vl06o, &zmdetpc),
    )?;
    if without > 0 {
        log(
            rep,
            &format!("{without} deliveries have no ZMDETPC row (total = number_of_packages)"),
        );
    }

    // merge + pivot (+ trailers)
    let merged = stage(rep, "merge", inv::merge_zmdetpc(&vl06o, &zmdetpc))?;
    let pivot = stage(rep, "pivot", inv::pivot_sum(&merged, "plant", "total"))?;
    let tc = pivot.col("total")?;
    let total: f64 = pivot
        .rows
        .iter()
        .filter_map(|r| r[tc].parse::<f64>().ok())
        .sum();
    // `pivot_sum` names its columns after the arguments (`plant`, `total`), as `trailers` expects
    let trailers = if with_trailers {
        Some(stage(rep, "trailers", inv::trailers(&pivot))?)
    } else {
        None
    };

    // save (only now, so a failed run leaves no output)
    let mut tables: Vec<(&str, &Table)> =
        vec![("vt11", &vt11), ("vl06o", &merged), ("pivot", &pivot)];
    if let Some(t) = &trailers {
        tables.push(("trailers", t));
    }
    let outputs = stage(rep, "save", write_outputs(def, out, date, &tables))?;
    for p in &outputs {
        log(rep, &format!("wrote {}", p.display()));
    }
    let trailer_rows = trailers
        .as_ref()
        .map(|t| {
            t.rows
                .iter()
                .map(|r| (r[0].clone(), r[1].clone()))
                .collect()
        })
        .unwrap_or_default();
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
        regex_replaced: 0,
        trailers: trailer_rows,
    })
}

/// Run `misc` (spec 5.4): VT11 -> VL06O -> one row per delivery + `date` -> regex on
/// `shipping_pointreceiving_pt` -> pivot of `number_of_packages` by `shipping_pointreceiving_pt`
/// (long table, same shape as the xdock pivot) -> outputs `vt11`, `vl06o`, `pivot`.
pub fn run_misc(
    steps: &mut impl MiscSteps,
    date: &RunDate,
    sel: &InvSelection,
    out: &OutputSpec,
) -> Result<InvSummary> {
    let rep = MISC.name;
    let vt11 = take(
        rep,
        "vt11",
        steps.vt11(date, &sel.vt11_variant, &sel.vt11_layout),
        &["shipment_number"],
    )?;
    let shipments = stage(rep, "vt11 shipments", shipment_list(&vt11))?;
    if shipments.is_empty() {
        return Err(no_data("vt11", "no shipments"));
    }
    log(rep, &format!("{} shipments", shipments.len()));

    let mut vl06o = take(
        rep,
        "vl06o",
        steps.vl06o_by_shipments(&shipments, &sel.vl06o_layout),
        &[
            "delivery",
            "name_of_the_shipto_party",
            "shipping_pointreceiving_pt",
            "number_of_packages",
        ],
    )?;
    let export_date = export_date_label(date);
    stage(
        rep,
        "vl06o prepare",
        inv::prepare_vl06o(&mut vl06o, &export_date),
    )?;
    let deliveries = match inv::delivery_values(&vl06o) {
        Ok(d) => d,
        Err(_) => return Err(no_data("vl06o", "no deliveries")),
    };
    log(rep, &format!("{} deliveries", deliveries.len()));

    let pc = vl06o.col("shipping_pointreceiving_pt")?;
    let before: Vec<String> = vl06o.rows.iter().map(|r| r[pc].clone()).collect();
    stage(rep, "regex", inv::misc_ship_to_regex(&mut vl06o))?;
    let replaced = vl06o
        .rows
        .iter()
        .zip(&before)
        .filter(|(r, b)| &r[pc] != *b)
        .count();
    if replaced > 0 {
        log(rep, &format!("{replaced} PLACEHOLDER_SP shipping points replaced"));
    }

    let pivot = stage(
        rep,
        "pivot",
        inv::pivot_sum(&vl06o, "shipping_pointreceiving_pt", "number_of_packages"),
    )?;
    let total: f64 = pivot
        .rows
        .iter()
        .filter_map(|r| r[1].parse::<f64>().ok())
        .sum();

    let tables: Vec<(&str, &Table)> = vec![("vt11", &vt11), ("vl06o", &vl06o), ("pivot", &pivot)];
    let outputs = stage(rep, "save", write_outputs(&MISC, out, date, &tables))?;
    for p in &outputs {
        log(rep, &format!("wrote {}", p.display()));
    }
    Ok(InvSummary {
        outputs,
        export_date,
        shipments: shipments.len(),
        deliveries: deliveries.len(),
        vl06o_rows: vl06o.nrows(),
        pivot_rows: pivot.nrows(),
        total: format!("{total}"),
        regex_replaced: replaced,
        ..InvSummary::default()
    })
}
