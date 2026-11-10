//! Runner for the `relabel` report (spec 5.6, verified in 9.5), parameterised by a SAP-step seam
//! of its own ([`RelabelSteps`]); the other seams are untouched. The pure stages are in
//! [`super::relabel`].
//!
//! Flow (spec 5.6 rows 61-65 and 9.5):
//! 1. the delivery list = the `delivery` column of the **k2xx block's `vl06o` TSV of the same
//!    run** (owner decision; spec 7.0 item 15), re-read from disk at the path
//!    [`registry::output_path`] gives for `k2xx` / `vl06o`, the same run date and today's
//!    folder. A missing or empty file stops the run with an error that says to run `k2xx` first;
//! 2. ZMDESNR by deliveries (variant `relabel_k2xx`, layout `site_relabel`) -> `sn_del`;
//! 3. if block B4 has serials: ZMDESNR by serial / History for them (same variant and layout);
//! 4. the pure stages (AL55 as an effective plant, blocks B1-B7, `_of_labels`, `export_date`,
//!    pivot customer x plant) and the save step.
//!
//! The run date must be a single date (spec 7.0 item 16: the window is `D`..`D+1`). Error
//! semantics are those of the other reports: the first no-data / error / empty list stops the
//! run and nothing is written (outputs are written only after the last step).
//!
//! Outputs (TSV, `registry::output_path`, folder `K2XX`):
//! `<reports_dir>\K2XX\<today>\relabel\<date>_RELABEL_{COMBINED,SN_DEL,SNR_ORIG,PIVOT}.tsv`.
//! `COMBINED` has the columns of the export plus `_of_labels`, `export_date`, `yellow`;
//! `SN_DEL` and `SNR_ORIG` are the export as loaded (the macro's two sheets; the temporary AL55
//! marks and the extra last row of the macro sheet are not reproduced); `PIVOT` is the long table
//! `customer`, `plant`, `_of_labels`. `save full_label` (A!62) is not implemented (it was not
//! ticked in the verified run).
//!
//! Canonical columns each step must deliver (a missing one stops the run naming it):
//! * vl06o TSV of k2xx: `delivery`
//! * zmdesnr by deliveries: [`relabel::SN_DEL_COLUMNS`]
//! * zmdesnr history: [`relabel::HISTORY_COLUMNS`]
#![allow(dead_code)]

use super::inv;
use super::inv_runner::{log, no_data, stage, take, write_outputs};
use super::registry::{self, K2XX, RELABEL};
use super::relabel;
use super::run::RunDate;
use super::runner::{export_date_label, OutputSpec, StepOutput};
use super::table::Table;
use anyhow::{anyhow, Context, Result};
use std::path::PathBuf;

/// Names of the SAP variant/layout both ZMDESNR calls use (resolved by the caller).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelabelSelection {
    pub zmdesnr_variant: String,
    pub zmdesnr_layout: String,
}

impl Default for RelabelSelection {
    /// Defaults of spec 5.6 (rows 61 and the PL55 history call).
    fn default() -> Self {
        Self {
            zmdesnr_variant: "relabel_k2xx".to_string(),
            zmdesnr_layout: "site_relabel".to_string(),
        }
    }
}

/// One method per SAP call of `relabel`. Inputs are plain, distinct, blank-free lists.
pub trait RelabelSteps {
    /// ZMDESNR by deliveries with `variant` and `layout` (`sn_del`).
    fn zmdesnr_by_deliveries(
        &mut self,
        deliveries: &[String],
        variant: &str,
        layout: &str,
    ) -> Result<StepOutput>;
    /// ZMDESNR by serial / History (tab 1, radio "History") for the serials.
    fn zmdesnr_history(
        &mut self,
        serials: &[String],
        variant: &str,
        layout: &str,
    ) -> Result<StepOutput>;
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RelabelSummary {
    /// `combined`, `sn_del`, `snr_orig`, `pivot` files.
    pub outputs: Vec<PathBuf>,
    pub export_date: String,
    /// Distinct deliveries read from the k2xx `vl06o` file and sent to ZMDESNR.
    pub deliveries: usize,
    /// Rows of the delivery export (`sn_del`).
    pub sn_del_rows: usize,
    /// Rows treated as plant `AL55` during the stage.
    pub al55_rows: usize,
    /// Rows of blocks B1, B2, B3, B4 (not appended), B5, B6, B7.
    pub blocks: [usize; 7],
    /// Serials sent to the History call (0 = no call) and rows it returned.
    pub history_serials: usize,
    pub history_rows: usize,
    /// Rows of `combined`, of which yellow and of which with 4 labels.
    pub combined_rows: usize,
    pub yellow_rows: usize,
    pub rows_with_4: usize,
    /// Sum of `_of_labels` (= sum of the pivot).
    pub labels: String,
    pub pivot_rows: usize,
}

/// Path of the k2xx block's `vl06o` TSV of this run (same run date, today's folder).
pub fn k2xx_vl06o_path(out: &OutputSpec, date: &RunDate) -> PathBuf {
    registry::output_path(&out.reports_dir, &K2XX, out.today, date, "vl06o")
}

/// Read the delivery list of the k2xx block of the same run. A missing, unreadable or empty
/// file, or one without deliveries, is an error that tells the user to run `k2xx` first.
pub fn read_k2xx_deliveries(out: &OutputSpec, date: &RunDate) -> Result<Vec<String>> {
    let path = k2xx_vl06o_path(out, date);
    let first = |why: &str| {
        anyhow!(
            "relabel needs the k2xx output of the same run ({why}: {}). Run k2xx first: \
             sap_automation --rname=k2xx --run-date={}",
            path.display(),
            export_date_label(date)
        )
    };
    if !path.is_file() {
        return Err(first("file not found"));
    }
    let bytes = std::fs::read(&path).with_context(|| format!("read {}", path.display()))?;
    let text = String::from_utf8(bytes)
        .map_err(|_| anyhow!("{} is not valid UTF-8 (k2xx writes UTF-8)", path.display()))?;
    if text.trim().is_empty() {
        return Err(first("file is empty"));
    }
    let t =
        Table::from_tsv("k2xx_vl06o", &text).with_context(|| format!("read {}", path.display()))?;
    if t.col("delivery").is_err() {
        return Err(anyhow!(
            "{} has no column 'delivery' (found: {}); run k2xx again",
            path.display(),
            t.headers.join(", ")
        ));
    }
    inv::delivery_values(&t).map_err(|_| first("file has no deliveries"))
}

/// Run `relabel`: k2xx deliveries -> ZMDESNR -> (History) -> combined -> pivot -> outputs
/// (`combined`, `sn_del`, `snr_orig`, `pivot`).
pub fn run_relabel(
    steps: &mut impl RelabelSteps,
    date: &RunDate,
    sel: &RelabelSelection,
    out: &OutputSpec,
) -> Result<RelabelSummary> {
    let rep = RELABEL.name;
    // spec 7.0 item 16: the window is D..D+1, so a range or week cannot be used
    if date.date2.is_some_and(|e| e != date.date1) {
        return Err(anyhow!(
            "relabel needs a single run date (the window is the run date and the day after); \
             got {}",
            export_date_label(date)
        ));
    }
    let d = date.date1;

    // delivery list of the k2xx block
    let deliveries = read_k2xx_deliveries(out, date)?;
    log(
        rep,
        &format!(
            "{} deliveries from {}",
            deliveries.len(),
            k2xx_vl06o_path(out, date).display()
        ),
    );

    // row 61: ZMDESNR by deliveries
    let sn_del = take(
        rep,
        "zmdesnr_by_deliveries",
        steps.zmdesnr_by_deliveries(&deliveries, &sel.zmdesnr_variant, &sel.zmdesnr_layout),
        &relabel::SN_DEL_COLUMNS,
    )?;
    if sn_del.nrows() == 0 {
        return Err(no_data("zmdesnr_by_deliveries", "empty list"));
    }

    // AL55 marks (kept in a side vector, nothing is written back) and the blocks
    let eff = stage(rep, "al55", relabel::effective_plants(&sn_del))?;
    let al55_rows = eff.iter().filter(|p| p.as_str() == relabel::AL55).count();
    let blocks = stage(rep, "blocks", relabel::select_blocks(&sn_del, &eff, d))?;

    // PL55 history -> yellow
    let serials = stage(
        rep,
        "history serials",
        relabel::history_serials(&sn_del, &blocks),
    )?;
    let (yellow, history_rows) = if serials.is_empty() {
        log(rep, "no PL55 pallet 1 rows: no history call");
        (Default::default(), 0)
    } else {
        let h = take(
            rep,
            "zmdesnr_history",
            steps.zmdesnr_history(&serials, &sel.zmdesnr_variant, &sel.zmdesnr_layout),
            &relabel::HISTORY_COLUMNS,
        )?;
        if h.nrows() == 0 {
            return Err(no_data("zmdesnr_history", "empty list"));
        }
        let y = stage(rep, "history filter", relabel::yellow_serials(&h))?;
        log(rep, &format!("{} history serials marked yellow", y.len()));
        (y, h.nrows())
    };

    // steps 2-4 of spec 9.5
    let export_date = export_date_label(date);
    let combined = stage(
        rep,
        "combined",
        relabel::build_combined(&sn_del, &eff, &blocks, &yellow, &export_date),
    )?;
    if combined.table.nrows() == 0 {
        return Err(no_data("combined", "no rows in any block"));
    }
    let pivot = stage(rep, "pivot", relabel::pivot_customer_plant(&combined.table))?;
    let labels = stage(rep, "labels", relabel::pivot_total(&pivot))?;

    // row 65: save (only now, so a failed run leaves no output)
    let mut snr_orig = sn_del.clone();
    snr_orig.name = "snr_orig".to_string();
    let tables: Vec<(&str, &Table)> = vec![
        ("combined", &combined.table),
        ("sn_del", &sn_del),
        ("snr_orig", &snr_orig),
        ("pivot", &pivot),
    ];
    let outputs = stage(rep, "save", write_outputs(&RELABEL, out, date, &tables))?;
    for p in &outputs {
        log(rep, &format!("wrote {}", p.display()));
    }
    Ok(RelabelSummary {
        outputs,
        export_date,
        deliveries: deliveries.len(),
        sn_del_rows: sn_del.nrows(),
        al55_rows,
        blocks: [
            blocks.b1.len(),
            blocks.b2.len(),
            blocks.b3.len(),
            blocks.b4.len(),
            blocks.b5.len(),
            blocks.b6.len(),
            blocks.b7.len(),
        ],
        history_serials: serials.len(),
        history_rows,
        combined_rows: combined.table.nrows(),
        yellow_rows: combined.yellow_rows,
        rows_with_4: combined.rows_with_4,
        labels: labels.to_string(),
        pivot_rows: pivot.nrows(),
    })
}
