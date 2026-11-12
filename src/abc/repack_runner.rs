//! Runner for the `repack` report (spec 5.7, verified in 9.5), parameterised by a SAP-step seam of
//! its own ([`RepackSteps`]); the other seams are untouched. The pure stages are in
//! [`super::repack`].
//!
//! Flow (spec 5.7 rows 68-71 and 9.5):
//! 1. the delivery list = the `delivery` column of the **k2xx block's `vl06o` TSV of the same
//!    run**, and the exclusion list = the `lm_serial_number` of the rows with `yellow = 1` in
//!    **relabel's `combined` TSV of the same run** (owner decisions 15 and 35), both re-read from
//!    disk at the paths [`registry::output_path`] gives (same run date, today's folder). A
//!    missing or empty file stops the run with an error that says to run `k2xx` / `relabel`
//!    first;
//! 2. ZMDESNR by deliveries excluding parents (variant `ZMDESNR_REPACK_VARIANT`, layout
//!    `ZMDESNR_REPACK_LAYOUT`) -> `repack_sn`. The exclusion list is only sent when it has at least two
//!    serials (spec 5.7 row 68; with fewer the export runs without exclusion and a line says so);
//! 3. if block R2 has serials: ZMDESNR by serial / History for them (same variant and layout);
//! 4. the pure stages (alias plant as an effective plant, blocks R1-R4, KC/SC deletion,
//!    `export_date`, pivot customer x plant counting `lm_serial_number`) and the save step.
//!
//! The run date must be a single date: relabel (whose output is read) needs one. Error semantics
//! are those of the other reports: the first no-data / error / empty list stops the run and
//! nothing is written (outputs are written only after the last step).
//!
//! Outputs (TSV, `registry::output_path`, folder `K2XX`):
//! `<reports_dir>\K2XX\<today>\repack\<date>_REPACK_{SNR_REPACK,REPACK_SN,PIVOT}.tsv`.
//! `SNR_REPACK` has the columns of the export plus `export_date`; `REPACK_SN` is the export as
//! loaded (the macro sheet; its temporary alias plant marks and extra last row are not
//! reproduced); `PIVOT` is the macro cross-tab (header `customer`, one column per plant,
//! `Grand Total`; one row per customer, then a `Grand Total` row).
//!
//! Canonical columns each step must deliver (a missing one stops the run naming it):
//! * vl06o TSV of k2xx: `delivery`
//! * combined TSV of relabel: [`repack::COMBINED_COLUMNS`]
//! * zmdesnr by deliveries: [`repack::REPACK_SN_COLUMNS`]
//! * zmdesnr history: [`repack::HISTORY_COLUMNS`] (and every column of the by-deliveries export)
#![allow(dead_code)]

use super::inv_runner::{log, no_data, stage, take, write_outputs};
use super::registry::{self, RELABEL, REPACK};
use super::relabel;
use super::relabel_runner::read_k2xx_deliveries_for;
use super::repack;
use super::run::RunDate;
use super::runner::{export_date_label, OutputSpec, StepOutput};
use super::table::Table;
use anyhow::{anyhow, Context, Result};
use std::path::PathBuf;

/// Names of the SAP variant/layout both ZMDESNR calls use, and the stage rules (resolved by the
/// caller).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepackSelection {
    pub zmdesnr_variant: String,
    pub zmdesnr_layout: String,
    /// Plant codes, customer patterns and the History warehouse (`[abc.repack]`).
    pub rules: repack::RepackRules,
}

impl Default for RepackSelection {
    /// Defaults of spec 5.7 (row 68 and the History call).
    fn default() -> Self {
        Self {
            zmdesnr_variant: "ZMDESNR_REPACK_VARIANT".to_string(),
            zmdesnr_layout: "ZMDESNR_REPACK_LAYOUT".to_string(),
            rules: repack::RepackRules::default(),
        }
    }
}

/// One method per SAP call of `repack`. Inputs are plain, distinct, blank-free lists.
pub trait RepackSteps {
    /// ZMDESNR by deliveries with `variant` and `layout`, the parent serials in `exclude`
    /// excluded (empty = no exclusion), -> `repack_sn`.
    fn zmdesnr_by_deliveries_excluding(
        &mut self,
        deliveries: &[String],
        exclude: &[String],
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
pub struct RepackSummary {
    /// `snr_repack`, `repack_sn`, `pivot` files.
    pub outputs: Vec<PathBuf>,
    pub export_date: String,
    /// Distinct deliveries read from the k2xx `vl06o` file and sent to ZMDESNR.
    pub deliveries: usize,
    /// Yellow rows of relabel's `combined` and serials actually excluded (0 = below the minimum).
    pub yellow_rows: usize,
    pub excluded: usize,
    /// Rows of the by-deliveries export (`repack_sn`).
    pub repack_sn_rows: usize,
    /// Rows treated as the alias plant during the stage.
    pub alias_rows: usize,
    /// Rows of R1, R2 (not appended), history rows appended, R3, rows deleted (KC/SC), R4.
    pub blocks: [usize; 6],
    /// Serials sent to the History call (0 = no call) and rows it returned.
    pub history_serials: usize,
    pub history_rows: usize,
    /// Rows of `snr_repack`.
    pub snr_rows: usize,
    /// Sum of the pivot (= `snr_rows`) and its rows.
    pub total: String,
    pub pivot_rows: usize,
}

/// Path of relabel's `combined` TSV of this run (same run date, today's folder).
pub fn relabel_combined_path(out: &OutputSpec, date: &RunDate) -> PathBuf {
    registry::output_path(&out.reports_dir, &RELABEL, out.today, date, "combined")
}

/// Read relabel's `combined` table of the same run. A missing, unreadable or empty file is an
/// error that tells the user to run `relabel` first; so is one without the columns `repack`
/// needs (an output of an older relabel has no `yellow` column).
pub fn read_relabel_combined(out: &OutputSpec, date: &RunDate) -> Result<Table> {
    let path = relabel_combined_path(out, date);
    let first = |why: &str| {
        anyhow!(
            "repack needs the relabel output of the same run ({why}: {}). Run relabel first \
             (after k2xx): sap_automation --rname=relabel --run-date={}",
            path.display(),
            export_date_label(date)
        )
    };
    if !path.is_file() {
        return Err(first("file not found"));
    }
    let bytes = std::fs::read(&path).with_context(|| format!("read {}", path.display()))?;
    let text = String::from_utf8(bytes).map_err(|_| {
        anyhow!(
            "{} is not valid UTF-8 (relabel writes UTF-8)",
            path.display()
        )
    })?;
    if text.trim().is_empty() {
        return Err(first("file is empty"));
    }
    let t = Table::from_tsv("relabel_combined", &text)
        .with_context(|| format!("read {}", path.display()))?;
    for c in repack::COMBINED_COLUMNS {
        if t.col(c).is_err() {
            return Err(anyhow!(
                "{} has no column '{c}' (found: {}); run relabel again",
                path.display(),
                t.headers.join(", ")
            ));
        }
    }
    if t.nrows() == 0 {
        return Err(first("file has no rows"));
    }
    Ok(t)
}

/// Run `repack`: k2xx deliveries + relabel yellow rows -> ZMDESNR (excluding parents) ->
/// (History) -> snr_repack -> pivot -> outputs (`snr_repack`, `repack_sn`, `pivot`).
pub fn run_repack(
    steps: &mut impl RepackSteps,
    date: &RunDate,
    sel: &RepackSelection,
    out: &OutputSpec,
) -> Result<RepackSummary> {
    let rep = REPACK.name;
    if date.date2.is_some_and(|e| e != date.date1) {
        return Err(anyhow!(
            "repack needs a single run date (it reads the relabel output of that date); got {}",
            export_date_label(date)
        ));
    }

    // inputs of the same run: deliveries (k2xx) and exclusion list (relabel)
    let deliveries = read_k2xx_deliveries_for(rep, out, date)?;
    log(
        rep,
        &format!(
            "{} deliveries from {}",
            deliveries.len(),
            super::relabel_runner::k2xx_vl06o_path(out, date).display()
        ),
    );
    let combined = read_relabel_combined(out, date)?;
    let (exclude, yellow_rows) =
        stage(rep, "exclusion list", repack::exclusion_serials(&combined))?;
    if exclude.is_empty() {
        log(
            rep,
            &format!(
                "{yellow_rows} yellow row(s) in combined (fewer than {}): ZMDESNR runs without the parent exclusion",
                repack::MIN_EXCLUSIONS
            ),
        );
    } else {
        log(
            rep,
            &format!(
                "{} serials of {} yellow rows excluded as parents (from {})",
                exclude.len(),
                yellow_rows,
                relabel_combined_path(out, date).display()
            ),
        );
    }

    // row 68: ZMDESNR by deliveries excluding parents
    let repack_sn = take(
        rep,
        "zmdesnr_by_deliveries",
        steps.zmdesnr_by_deliveries_excluding(
            &deliveries,
            &exclude,
            &sel.zmdesnr_variant,
            &sel.zmdesnr_layout,
        ),
        &repack::REPACK_SN_COLUMNS,
    )?;
    if repack_sn.nrows() == 0 {
        return Err(no_data("zmdesnr_by_deliveries", "empty list"));
    }

    // alias plant marks (kept in a side vector, nothing is written back) and the blocks
    let eff = stage(
        rep,
        "alias plant",
        relabel::effective_plants(&repack_sn, &sel.rules.plants),
    )?;
    let alias_rows = eff
        .iter()
        .filter(|p| p.as_str() == sel.rules.plants.alias_plant)
        .count();
    let blocks = stage(
        rep,
        "blocks",
        repack::select_blocks(&repack_sn, &eff, &sel.rules),
    )?;

    // R2 -> History
    let serials = stage(
        rep,
        "history serials",
        repack::history_serials(&repack_sn, &blocks),
    )?;
    let history: Option<(Table, Vec<usize>)> = if serials.is_empty() {
        log(rep, "no heavy rows (R2): no history call");
        None
    } else {
        let h = take(
            rep,
            "zmdesnr_history",
            steps.zmdesnr_history(&serials, &sel.zmdesnr_variant, &sel.zmdesnr_layout),
            &repack::HISTORY_COLUMNS,
        )?;
        if h.nrows() == 0 {
            return Err(no_data("zmdesnr_history", "empty list"));
        }
        let idx = stage(rep, "history filter", repack::history_rows(&h, &sel.rules))?;
        log(
            rep,
            &format!("{} of {} history rows kept (UNP)", idx.len(), h.nrows()),
        );
        Some((h, idx))
    };
    let history_rows = history.as_ref().map_or(0, |(h, _)| h.nrows());

    // spec 9.5 steps 2-3
    let export_date = export_date_label(date);
    let snr = stage(
        rep,
        "snr_repack",
        repack::build_snr_repack(
            &repack_sn,
            &blocks,
            history.as_ref().map(|(h, i)| (h, i.as_slice())),
            &export_date,
        ),
    )?;
    if snr.table.nrows() == 0 {
        return Err(no_data("snr_repack", "no rows in any block"));
    }
    let pivot = stage(rep, "pivot", repack::pivot_customer_plant(&snr.table))?;
    let total = stage(rep, "total", relabel::pivot_total(&pivot))?;

    // row 71: save (only now, so a failed run leaves no output)
    let tables: Vec<(&str, &Table)> = vec![
        ("snr_repack", &snr.table),
        ("repack_sn", &repack_sn),
        ("pivot", &pivot),
    ];
    let outputs = stage(rep, "save", write_outputs(&REPACK, out, date, &tables))?;
    for p in &outputs {
        log(rep, &format!("wrote {}", p.display()));
    }
    Ok(RepackSummary {
        outputs,
        export_date,
        deliveries: deliveries.len(),
        yellow_rows,
        excluded: exclude.len(),
        repack_sn_rows: repack_sn.nrows(),
        alias_rows,
        blocks: [
            snr.r1,
            blocks.r2.len(),
            snr.history,
            snr.r3,
            snr.deleted,
            snr.r4,
        ],
        history_serials: serials.len(),
        history_rows,
        snr_rows: snr.table.nrows(),
        total: total.to_string(),
        pivot_rows: pivot.nrows(),
    })
}
