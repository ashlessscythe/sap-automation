//! Runner for the `k2xx` report (spec 5.5, verified in 9.5), parameterised by a SAP-step seam
//! of its own ([`K2xxSteps`]); the `mustgo` and INV seams are untouched. The pure stages are in
//! [`super::k2xx`].
//!
//! Flow (spec 5.5 rows 52-58): VT11 (variant `VT11_K2XX_VARIANT`, layout `scorecard`,
//! `monday_include_sunday`) -> VL06O for all shipments (layout `pallets`) -> one row per
//! delivery + `date` column -> ZVT11 Closed for the deliveries (variant `k2xx_`, layout
//! `add_del`, run date +- 100 days) -> ZMDETPC summarised for the deliveries (variant
//! `ZMDETPC_VARIANT`) -> merge (spec 4.12, plain rules) -> save. No pivot (spec 7.0 item 1).
//!
//! Error semantics are those of the other reports: the first no-data / error / empty list
//! stops the run and nothing is written (outputs are written only after the last step).
//!
//! Outputs (TSV, `registry::output_path`, folder `K2XX`), in the macro's sheet order:
//! `<reports_dir>\K2XX\<today>\k2xx\<date>_K2XX_{ZVT11,VT11,VL06O}.tsv`. `VL06O` is the merged
//! table; `relabel` and `repack` will read its `delivery` column (spec 7.0 item 15).
//!
//! Canonical columns each step must deliver (a missing one stops the run naming it):
//! * vt11: `shipment_number`
//! * vl06o: `delivery` (the `pallets` layout also has `shipto_party`,
//!   `name_of_the_shipto_party`, `deliv_datefromto`, `total_gds_mvt_stat`, `plant`,
//!   `external_delivery_id`, `number_of_packages`)
//! * zvt11: none by name (the table is written as exported and the header titles of the real
//!   text export are unknown); it must have at least two columns and one row
//! * zmdetpc: `delivery`, `pallet`, `child`
#![allow(dead_code)]

use super::inv;
use super::inv_runner::{
    count_without_zmdetpc, log, no_data, shipment_list, stage, take, write_outputs,
};
use super::k2xx;
use super::registry::K2XX;
use super::run::RunDate;
use super::runner::{export_date_label, OutputSpec, StepOutput};
use super::table::Table;
use anyhow::Result;
use chrono::NaiveDate;
use std::path::PathBuf;

/// Names of the SAP variants/layouts a `k2xx` run uses (resolved by the caller).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct K2xxSelection {
    pub vt11_variant: String,
    pub vt11_layout: String,
    /// Spec 5.5 row 52 `arg:monday_include_sunday`: a Monday run date starts on the Sunday.
    pub monday_include_sunday: bool,
    pub vl06o_layout: String,
    pub zvt11_variant: String,
    pub zvt11_layout: String,
    /// Days on either side of the run date for ZVT11 Closed.
    pub zvt11_days: i64,
    pub zmdetpc_variant: String,
}

impl Default for K2xxSelection {
    /// Defaults of spec 5.5.
    fn default() -> Self {
        Self {
            vt11_variant: "VT11_K2XX_VARIANT".to_string(),
            vt11_layout: "scorecard".to_string(),
            monday_include_sunday: true,
            vl06o_layout: "pallets".to_string(),
            zvt11_variant: "k2xx_".to_string(),
            zvt11_layout: "add_del".to_string(),
            zvt11_days: k2xx::ZVT11_WINDOW_DAYS,
            zmdetpc_variant: "ZMDETPC_VARIANT".to_string(),
        }
    }
}

/// One method per SAP call of `k2xx`. Inputs are plain, distinct, blank-free lists.
pub trait K2xxSteps {
    /// VT11 for the run date (range for week runs) with `variant` and `layout`;
    /// `monday_include_sunday` moves a Monday start to the previous Sunday.
    fn vt11(
        &mut self,
        date: &RunDate,
        variant: &str,
        layout: &str,
        monday_include_sunday: bool,
    ) -> Result<StepOutput>;
    /// VL06O (item view) for the shipments with `layout` (`pallets`).
    fn vl06o_by_shipments(&mut self, shipments: &[String], layout: &str) -> Result<StepOutput>;
    /// ZVT11 Closed for the deliveries, selection dates `from`..`to`, with `variant` and
    /// `layout`.
    fn zvt11_closed(
        &mut self,
        deliveries: &[String],
        from: NaiveDate,
        to: NaiveDate,
        variant: &str,
        layout: &str,
    ) -> Result<StepOutput>;
    /// ZMDETPC summarised for the deliveries and the run date with `variant`.
    fn zmdetpc_summarised(
        &mut self,
        deliveries: &[String],
        date: &RunDate,
        variant: &str,
    ) -> Result<StepOutput>;
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct K2xxSummary {
    /// `zvt11`, `vt11`, `vl06o` files.
    pub outputs: Vec<PathBuf>,
    pub export_date: String,
    pub shipments: usize,
    /// Distinct deliveries sent to ZVT11 and ZMDETPC.
    pub deliveries: usize,
    pub zvt11_rows: usize,
    pub zmdetpc_rows: usize,
    /// Deliveries without a ZMDETPC row (their `total` stays blank).
    pub without_zmdetpc: usize,
    /// Rows of the merged `vl06o` table.
    pub vl06o_rows: usize,
    /// Sum of `total` over the merged table.
    pub total: String,
}

/// Run `k2xx`: VT11 -> VL06O -> ZVT11 Closed -> ZMDETPC -> merge -> outputs (`zvt11`, `vt11`,
/// `vl06o`).
pub fn run_k2xx(
    steps: &mut impl K2xxSteps,
    date: &RunDate,
    sel: &K2xxSelection,
    out: &OutputSpec,
) -> Result<K2xxSummary> {
    let rep = K2XX.name;
    // row 52: VT11
    let vt11 = take(
        rep,
        "vt11",
        steps.vt11(
            date,
            &sel.vt11_variant,
            &sel.vt11_layout,
            sel.monday_include_sunday,
        ),
        &["shipment_number"],
    )?;
    let shipments = stage(rep, "vt11 shipments", shipment_list(&vt11))?;
    if shipments.is_empty() {
        return Err(no_data("vt11", "no shipments"));
    }
    log(rep, &format!("{} shipments", shipments.len()));

    // rows 53-54: VL06O, one row per delivery, `date`
    let mut vl06o = take(
        rep,
        "vl06o",
        steps.vl06o_by_shipments(&shipments, &sel.vl06o_layout),
        &["delivery"],
    )?;
    let export_date = export_date_label(date);
    stage(
        rep,
        "vl06o prepare",
        k2xx::prepare_vl06o(&mut vl06o, &export_date),
    )?;
    let deliveries = match inv::delivery_values(&vl06o) {
        Ok(d) => d,
        Err(_) => return Err(no_data("vl06o", "no deliveries")),
    };
    log(rep, &format!("{} deliveries", deliveries.len()));

    // row 55: ZVT11 Closed (written as exported)
    let (from, to) = k2xx::zvt11_window(date, sel.zvt11_days);
    let zvt11 = take(
        rep,
        "zvt11",
        steps.zvt11_closed(&deliveries, from, to, &sel.zvt11_variant, &sel.zvt11_layout),
        &[],
    )?;
    if zvt11.is_empty_import() || zvt11.nrows() == 0 {
        return Err(no_data("zvt11", "empty list"));
    }

    // row 56: ZMDETPC + merge
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
            &format!("{without} deliveries have no ZMDETPC row (total stays blank)"),
        );
    }
    let merged = stage(rep, "merge", k2xx::merge_zmdetpc(&vl06o, &zmdetpc))?;
    let total = stage(rep, "total", k2xx::total_sum(&merged))?;

    // row 58: save (only now, so a failed run leaves no output); no pivot
    let tables: Vec<(&str, &Table)> = vec![("zvt11", &zvt11), ("vt11", &vt11), ("vl06o", &merged)];
    let outputs = stage(rep, "save", write_outputs(&K2XX, out, date, &tables))?;
    for p in &outputs {
        log(rep, &format!("wrote {}", p.display()));
    }
    Ok(K2xxSummary {
        outputs,
        export_date,
        shipments: shipments.len(),
        deliveries: deliveries.len(),
        zvt11_rows: zvt11.nrows(),
        zmdetpc_rows: zmdetpc.nrows(),
        without_zmdetpc: without,
        vl06o_rows: merged.nrows(),
        total: format!("{total}"),
    })
}
