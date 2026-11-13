//! Per-day iteration of a headless report (`--rname=<r> --run-date=<A..B|WKnn> --loop [--merge]`).
//!
//! For every day of the run date the report chain (`registry::chain`: prerequisites first, e.g.
//! `repack` -> k2xx, relabel, repack) runs as its own single-day run. Policy (owner decisions):
//! - [`NoDataError`] for a day: logged, the day is recorded as skipped and the rest of the chain
//!   of that day is not run (later reports depend on the earlier ones);
//! - any other error stops the whole iteration at once: no further days, no merge, the files
//!   already written stay.
//!
//! With `merge` and at least one finished day, the per-day outputs are merged afterwards
//! (`merge::merge_chain`, one workbook per report of the chain).
//!
//! Reusing outputs (phase 7): `skip_existing` skips a day+report whose output tables are ALL on
//! disk (`registry::output_complete`, the rule the merge uses); `no_sap` never runs anything:
//! every day+report of the chain must be complete (else one error listing what is missing,
//! before anything is merged) and `--merge` then re-merges the existing files. SAP is not even
//! connected in that mode (see [`run_rname_routed`]).
//!
//! `today` (the date folder of the outputs: `--output-date` or the real today) is captured ONCE
//! by [`run_iteration`] and handed to every run and to the merge, so they agree even when the iteration crosses midnight. The
//! SAP-touching calls sit behind [`DayRunner`]. Inside the loop `relabel` / `repack` read the
//! k2xx / relabel TSVs of the same run date from the same `today` folder; the chain order
//! guarantees the prerequisite wrote them first.
#![allow(dead_code)]

use anyhow::{anyhow, Result};
use chrono::NaiveDate;
use sap_scripting::GuiSession;
use std::path::PathBuf;

use super::dispatch;
use super::merge::{self, MergeOutcome};
use super::registry::{self, Completeness};
use super::run::RunDate;
use super::runner::NoDataError;

/// Options of an iteration.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct IterOpts {
    /// Merge the per-day outputs into one workbook per report afterwards.
    pub merge: bool,
    /// Resume: do not re-run a day+report whose output is complete.
    pub skip_existing: bool,
    /// Strict offline: never run, only verify (and merge) existing outputs.
    pub no_sap: bool,
    /// Single (non-loop) runs only: cover the report's own chain (prerequisites first) instead
    /// of just the report. A loop already includes the prerequisites, so it has no effect there.
    pub with_prereqs: bool,
}

impl IterOpts {
    /// The flags of the process-wide CLI overrides.
    pub fn from_overrides() -> Self {
        use crate::utils::cli_overrides as o;
        IterOpts {
            merge: o::merge(),
            skip_existing: o::skip_existing(),
            no_sap: o::no_sap(),
            with_prereqs: o::with_prereqs(),
        }
    }
}

/// Seam over everything that touches SAP or the file system.
pub trait DayRunner {
    /// Run `report` for `date` (one day inside an iteration); outputs go under the `today` folder.
    fn run(&mut self, report: &str, date: &RunDate, today: NaiveDate) -> Result<()>;
    /// How much of the output of `report` for `date` exists under the `today` folder.
    fn complete(&mut self, report: &str, date: &RunDate, today: NaiveDate) -> Completeness;
    /// Merge the chain of `rname` over `days` (`range` names the workbook; `today` folder).
    fn merge(
        &mut self,
        rname: &str,
        today: NaiveDate,
        range: &RunDate,
        days: &[NaiveDate],
    ) -> Result<Vec<(String, Result<MergeOutcome>)>>;
}

/// Result of a finished iteration (no hard error).
#[derive(Debug, Default)]
pub struct IterationSummary {
    /// Days whose whole chain ran.
    pub done: Vec<NaiveDate>,
    /// Days skipped for no data: (day, report, message).
    pub skipped: Vec<(NaiveDate, String, String)>,
    /// Day+report pairs not run because their output already existed (`--skip-existing`) or,
    /// with `--no-sap`, was verified.
    pub existing: Vec<(NaiveDate, String)>,
    /// Merge results, when a merge was requested and ran.
    pub merged: Option<Vec<(String, Result<MergeOutcome>)>>,
}

impl IterationSummary {
    /// Number of reports whose merge failed.
    pub fn merge_failures(&self) -> usize {
        self.merged
            .iter()
            .flatten()
            .filter(|(_, r)| r.is_err())
            .count()
    }
}

/// Real implementation over a live session.
pub struct RealDayRunner<'a> {
    pub session: &'a GuiSession,
}

/// The shared file-system check of [`DayRunner::complete`].
fn real_complete(report: &str, date: &RunDate, today: NaiveDate) -> Completeness {
    match registry::find_report(report) {
        Some(def) => {
            registry::output_complete(&dispatch::reports_dir_for(report), def, today, date)
        }
        None => Completeness::None,
    }
}

/// The shared merge of the real runners.
fn real_merge(
    rname: &str,
    today: NaiveDate,
    range: &RunDate,
    days: &[NaiveDate],
) -> Result<Vec<(String, Result<MergeOutcome>)>> {
    // the reports dir is the same for every report of the chain (CLI > [global] > default)
    let dir = dispatch::reports_dir_for(rname);
    merge::merge_chain(&dir, rname, today, range, days)
}

/// Runner for `--no-sap` (and fully-complete `--skip-existing`): file system only, it has no
/// session, so a call of `run` is a bug and an error.
pub struct OfflineDayRunner;

impl DayRunner for OfflineDayRunner {
    fn run(&mut self, report: &str, _date: &RunDate, _today: NaiveDate) -> Result<()> {
        Err(anyhow!(
            "internal error: '{report}' would need SAP, but this run is offline (no SAP session)"
        ))
    }

    fn complete(&mut self, report: &str, date: &RunDate, today: NaiveDate) -> Completeness {
        real_complete(report, date, today)
    }

    fn merge(
        &mut self,
        rname: &str,
        today: NaiveDate,
        range: &RunDate,
        days: &[NaiveDate],
    ) -> Result<Vec<(String, Result<MergeOutcome>)>> {
        real_merge(rname, today, range, days)
    }
}

impl DayRunner for RealDayRunner<'_> {
    fn run(&mut self, report: &str, date: &RunDate, today: NaiveDate) -> Result<()> {
        dispatch::run_report_on(self.session, report, date, today).map(|_| ())
    }

    fn complete(&mut self, report: &str, date: &RunDate, today: NaiveDate) -> Completeness {
        real_complete(report, date, today)
    }

    fn merge(
        &mut self,
        rname: &str,
        today: NaiveDate,
        range: &RunDate,
        days: &[NaiveDate],
    ) -> Result<Vec<(String, Result<MergeOutcome>)>> {
        real_merge(rname, today, range, days)
    }
}

/// Run the iteration on a live session (captures `today` once: `--output-date` or the real today).
pub fn run_iteration(
    session: &GuiSession,
    rname: &str,
    range: &RunDate,
    opts: IterOpts,
) -> Result<IterationSummary> {
    let today = crate::utils::cli_overrides::effective_today();
    run_iteration_with(&mut RealDayRunner { session }, rname, range, opts, today)
}

/// What a single (non-loop) run did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SingleOutcome {
    /// The report ran.
    Ran,
    /// `--skip-existing`: the output was complete, nothing ran.
    Existing,
    /// `--no-sap`: the output is complete, nothing ran.
    Ready,
    /// `--with-prereqs`: a chain member had no data; the rest of the chain was not run.
    NoData,
}

/// A single run of `rname` for `date` (`--rname` without `--loop`) with the reuse flags:
/// `no_sap` only verifies (incomplete = error), `skip_existing` runs only when incomplete.
/// Only the report itself is checked (a single run never auto-runs prerequisites either).
pub fn run_single_with(
    runner: &mut dyn DayRunner,
    rname: &str,
    date: &RunDate,
    opts: IterOpts,
    today: NaiveDate,
) -> Result<SingleOutcome> {
    if opts.with_prereqs {
        return run_single_chain(runner, rname, date, opts, today);
    }
    if !(opts.no_sap || opts.skip_existing) {
        runner.run(rname, date, today)?;
        return Ok(SingleOutcome::Ran);
    }
    let label = registry::date_label(date);
    let state = runner.complete(rname, date, today);
    if opts.no_sap {
        return match state {
            Completeness::Complete => {
                println!("{rname} {label}: output complete under {today}, ready (--no-sap).");
                Ok(SingleOutcome::Ready)
            }
            other => Err(anyhow!(
                "--no-sap: {rname} {label} is not complete under {today}: {}",
                describe_incomplete(&other)
            )),
        };
    }
    if state.is_complete() {
        println!("{rname} {label}: exists, skipped (--skip-existing).");
        return Ok(SingleOutcome::Existing);
    }
    runner.run(rname, date, today)?;
    Ok(SingleOutcome::Ran)
}

/// A single run with `--with-prereqs`: the chain of `rname` (prerequisites first, never anything
/// downstream) for this one date. Shares the per-day policy with the loop ([`run_day_chain`]):
/// no data stops the rest of the chain (skipped, not an error), any other error is a hard error.
/// `no_sap`: every chain member must be complete (one error listing all gaps), nothing runs.
/// `skip_existing`: only incomplete members run.
fn run_single_chain(
    runner: &mut dyn DayRunner,
    rname: &str,
    date: &RunDate,
    opts: IterOpts,
    today: NaiveDate,
) -> Result<SingleOutcome> {
    let chain = registry::chain(rname);
    if chain.is_empty() {
        return Err(anyhow!("unknown report '{rname}'"));
    }
    let label = registry::date_label(date);
    println!(
        "--with-prereqs: '{rname}' {label}, chain: {}",
        chain.join(" -> ")
    );
    if opts.no_sap {
        let mut gaps = Vec::new();
        for report in &chain {
            match runner.complete(report, date, today) {
                Completeness::Complete => {}
                other => gaps.push(format!("  {report}: {}", describe_incomplete(&other))),
            }
        }
        if !gaps.is_empty() {
            return Err(anyhow!(
                "--no-sap --with-prereqs: the chain of '{rname}' {label} is not complete under {today} (nothing can be generated without SAP):{nl}{gaps}",
                nl = "\n",
                gaps = gaps.join("\n")
            ));
        }
        println!(
            "{rname} {label}: {} complete under {today}, ready (--no-sap).",
            chain.join(" + ")
        );
        return Ok(SingleOutcome::Ready);
    }
    let mut summary = IterationSummary::default();
    let ok = run_day_chain(
        runner,
        rname,
        &chain,
        date,
        date.date1,
        1,
        opts,
        today,
        &mut summary,
    )?;
    if !ok {
        // no data somewhere in the chain: reported like a skipped loop day, not an error
        print_summary(&summary, 1, None);
        return Ok(SingleOutcome::NoData);
    }
    Ok(if summary.existing.len() == chain.len() {
        SingleOutcome::Existing
    } else {
        SingleOutcome::Ran
    })
}

/// One day (or one single-run date) over the chain, shared by the loop and `--with-prereqs`.
/// Returns `Ok(true)` when the whole chain finished (or existed), `Ok(false)` when a no-data
/// report cut the rest of the chain (recorded in `summary.skipped`); any other error prints
/// the summary and is returned.
#[allow(clippy::too_many_arguments)]
fn run_day_chain(
    runner: &mut dyn DayRunner,
    rname: &str,
    chain: &[&'static str],
    date: &RunDate,
    day: NaiveDate,
    total: usize,
    opts: IterOpts,
    today: NaiveDate,
    summary: &mut IterationSummary,
) -> Result<bool> {
    for report in chain {
        println!("--- {day}: {report} ---");
        if opts.skip_existing && runner.complete(report, date, today).is_complete() {
            println!("{day}: {report} exists, skipped");
            summary.existing.push((day, report.to_string()));
            continue;
        }
        match runner.run(report, date, today) {
            Ok(()) => {}
            Err(e) if e.downcast_ref::<NoDataError>().is_some() => {
                let msg = dispatch::describe_error(&e);
                println!("{day}: {report} skipped, {msg}");
                summary.skipped.push((day, report.to_string(), msg));
                return Ok(false);
            }
            Err(e) => {
                print_summary(summary, total, Some((day, report)));
                return Err(e.context(format!(
                    "run of '{rname}' stopped at {day}, report '{report}' (no merge; files already written stay)"
                )));
            }
        }
    }
    Ok(true)
}

fn describe_incomplete(c: &Completeness) -> String {
    match c {
        Completeness::Complete => "complete".to_string(),
        Completeness::None => "no output files".to_string(),
        Completeness::Missing(t) => format!("missing table(s): {}", t.join(", ")),
    }
}

/// Does this run have to connect to SAP at all? `--no-sap`: never. `--skip-existing`: only when
/// some day+report (the chain for `looped`, else the report itself) is not complete. Otherwise yes.
pub fn needs_sap(
    runner: &mut dyn DayRunner,
    rname: &str,
    date: &RunDate,
    looped: bool,
    opts: IterOpts,
    today: NaiveDate,
) -> bool {
    if opts.no_sap {
        return false;
    }
    if !opts.skip_existing {
        return true;
    }
    if looped {
        let chain = registry::chain(rname);
        date.days().iter().any(|&d| {
            chain
                .iter()
                .any(|r| !runner.complete(r, &RunDate::single(d), today).is_complete())
        })
    } else if opts.with_prereqs {
        registry::chain(rname)
            .iter()
            .any(|r| !runner.complete(r, date, today).is_complete())
    } else {
        !runner.complete(rname, date, today).is_complete()
    }
}

/// Entry of an unattended `--rname` run: `live` (connect to SAP and run; supplied by the caller)
/// is called ONLY when [`needs_sap`]; otherwise everything is handled on the file system through
/// `offline` and SAP is never connected. `today` is the outputs' date folder.
pub fn run_rname_routed(
    offline: &mut dyn DayRunner,
    rname: &str,
    date: &RunDate,
    looped: bool,
    opts: IterOpts,
    today: NaiveDate,
    live: impl FnOnce() -> Result<()>,
) -> Result<()> {
    if needs_sap(offline, rname, date, looped, opts, today) {
        return live();
    }
    if opts.no_sap {
        println!("--no-sap: SAP is not touched; reusing the outputs under the {today} folder.");
    } else {
        println!("--skip-existing: every output exists; SAP is not touched.");
    }
    if looped {
        run_iteration_with(offline, rname, date, opts, today).map(|_| ())
    } else {
        run_single_with(offline, rname, date, opts, today).map(|_| ())
    }
}

/// The iteration over any [`DayRunner`], with an explicit `today`. Prints the summary and
/// returns an error (after the summary) when a requested merge failed, wholly or for any report.
pub fn run_iteration_with(
    runner: &mut dyn DayRunner,
    rname: &str,
    range: &RunDate,
    opts: IterOpts,
    today: NaiveDate,
) -> Result<IterationSummary> {
    let summary = iterate_core(runner, rname, range, opts, today)?;
    print_summary(&summary, summary_total(range), None);
    let failed = summary.merge_failures();
    if failed > 0 {
        return Err(anyhow!(
            "{failed} merge(s) failed (see the summary above; reports that merged keep their workbooks)"
        ));
    }
    Ok(summary)
}

fn summary_total(range: &RunDate) -> usize {
    range.days().len()
}

/// The iteration without the final summary / merge-failure verdict. A merge that errors as a
/// whole is recorded as one failed entry named `rname`.
fn iterate_core(
    runner: &mut dyn DayRunner,
    rname: &str,
    range: &RunDate,
    opts: IterOpts,
    today: NaiveDate,
) -> Result<IterationSummary> {
    let chain = registry::chain(rname);
    if chain.is_empty() {
        return Err(anyhow!("unknown report '{rname}'"));
    }
    let days = range.days();
    if days.len() < 2 {
        return Err(anyhow!("--loop needs a range or week"));
    }
    let total = days.len();
    let mut summary = IterationSummary::default();
    if opts.no_sap {
        // offline: everything must exist, checked up front so nothing is merged on a gap
        let mut gaps = Vec::new();
        for &day in &days {
            for report in &chain {
                match runner.complete(report, &RunDate::single(day), today) {
                    Completeness::Complete => summary.existing.push((day, report.to_string())),
                    other => {
                        gaps.push(format!("  {day} {report}: {}", describe_incomplete(&other)))
                    }
                }
            }
        }
        if !gaps.is_empty() {
            return Err(anyhow!(
                "--no-sap: the output of '{rname}' is not complete under {today}                  (nothing merged):
{}",
                gaps.join("
")
            ));
        }
        summary.done = days.clone();
        println!(
            "--no-sap: all {total} day(s) x {} report(s) complete under {today}{}",
            chain.len(),
            if opts.merge { ", merging" } else { "" }
        );
    }
    println!(
        "Iterating '{rname}' over {total} day(s) ({} .. {}), chain: {}{}",
        days[0],
        days[total - 1],
        chain.join(" -> "),
        if opts.merge { ", merge afterwards" } else { "" }
    );

    // `--no-sap` has nothing to run (it was verified above)
    let run_days: &[NaiveDate] = if opts.no_sap { &[] } else { &days };
    for (i, &day) in run_days.iter().enumerate() {
        println!("=== Day {}/{total}: {day} ===", i + 1);
        let day_ok = run_day_chain(
            runner,
            rname,
            &chain,
            &RunDate::single(day),
            day,
            total,
            opts,
            today,
            &mut summary,
        )?;
        if day_ok {
            summary.done.push(day);
        }
    }

    if opts.merge {
        merge_tail(runner, rname, range, today, &mut summary);
    }
    Ok(summary)
}

/// The merge after the per-day runs: done + skipped days (a skipped day may still have
/// prerequisite output); a merge that errors as a whole is one failed entry named `rname`.
fn merge_tail(
    runner: &mut dyn DayRunner,
    rname: &str,
    range: &RunDate,
    today: NaiveDate,
    summary: &mut IterationSummary,
) {
    if summary.done.is_empty() {
        println!("No day finished: nothing to merge.");
        return;
    }
    let mut touched: Vec<NaiveDate> = summary.done.clone();
    touched.extend(summary.skipped.iter().map(|(d, _, _)| *d));
    touched.sort();
    touched.dedup();
    summary.merged = Some(match runner.merge(rname, today, range, &touched) {
        Ok(v) => v,
        Err(e) => vec![(rname.to_string(), Err(e))],
    });
}

// ------------------------------------------------------------------ parallel days (--conn-count)

/// The offline runner (merge without a SAP session) for [`finish_parallel`].
pub fn offline_runner() -> OfflineDayRunner {
    OfflineDayRunner
}

/// One finished day of a parallel run: (day, whole chain finished, its partial summary).
type DayResult = (NaiveDate, bool, IterationSummary);

/// State shared by the workers of a `--conn-count` run: a queue of days (each worker takes the
/// next one when it is free, so fast days do not wait for slow ones), the per-day results and
/// the first hard error. After a hard error no new day is started (running ones finish).
pub struct ParallelShared {
    queue: std::sync::Mutex<std::collections::VecDeque<NaiveDate>>,
    results: std::sync::Mutex<Vec<DayResult>>,
    error: std::sync::Mutex<Option<anyhow::Error>>,
    abort: std::sync::atomic::AtomicBool,
    pub total: usize,
}

impl ParallelShared {
    pub fn new(days: &[NaiveDate]) -> Self {
        Self {
            queue: std::sync::Mutex::new(days.iter().copied().collect()),
            results: std::sync::Mutex::new(Vec::new()),
            error: std::sync::Mutex::new(None),
            abort: std::sync::atomic::AtomicBool::new(false),
            total: days.len(),
        }
    }

    fn next_day(&self) -> Option<NaiveDate> {
        if self.abort.load(std::sync::atomic::Ordering::Acquire) {
            return None;
        }
        self.queue.lock().ok()?.pop_front()
    }
}

/// One worker: take days from the queue and run the chain of `rname` for each on its own
/// `runner` (its own SAP session) until the queue is empty or another worker failed hard.
pub fn parallel_worker(
    shared: &ParallelShared,
    runner: &mut dyn DayRunner,
    rname: &str,
    opts: IterOpts,
    today: NaiveDate,
) {
    let chain = registry::chain(rname);
    while let Some(day) = shared.next_day() {
        println!("=== Day {day} ===");
        let mut local = IterationSummary::default();
        let outcome = run_day_chain(
            runner,
            rname,
            &chain,
            &RunDate::single(day),
            day,
            shared.total,
            opts,
            today,
            &mut local,
        );
        match outcome {
            Ok(ok) => {
                if let Ok(mut r) = shared.results.lock() {
                    r.push((day, ok, local));
                }
            }
            Err(e) => {
                shared
                    .abort
                    .store(true, std::sync::atomic::Ordering::Release);
                if let Ok(mut slot) = shared.error.lock() {
                    if slot.is_none() {
                        *slot = Some(e);
                    } else {
                        println!("{day}: also failed: {e:#}");
                    }
                }
                return;
            }
        }
    }
}

/// After all workers returned: combine the per-day results in day order, apply the same policy
/// as the sequential loop (hard error = summary, no merge, `Err`; otherwise optional merge via
/// `merge_runner`, which needs no SAP session) and print the summary.
pub fn finish_parallel(
    shared: ParallelShared,
    merge_runner: &mut dyn DayRunner,
    rname: &str,
    range: &RunDate,
    opts: IterOpts,
    today: NaiveDate,
) -> Result<IterationSummary> {
    let total = shared.total;
    let mut results = shared.results.into_inner().unwrap_or_default();
    results.sort_by_key(|(d, _, _)| *d);
    let mut summary = IterationSummary::default();
    for (day, ok, part) in results {
        if ok {
            summary.done.push(day);
        }
        summary.skipped.extend(part.skipped);
        summary.existing.extend(part.existing);
    }
    if let Some(e) = shared.error.into_inner().unwrap_or(None) {
        print_summary(&summary, total, None);
        return Err(e);
    }
    if opts.merge {
        merge_tail(merge_runner, rname, range, today, &mut summary);
    }
    print_summary(&summary, total, None);
    let failed = summary.merge_failures();
    if failed > 0 {
        return Err(anyhow!(
            "{failed} merge(s) failed (see the summary above; reports that merged keep their workbooks)"
        ));
    }
    Ok(summary)
}

/// Lines of the final summary (pure, tested).
pub fn summary_lines(
    s: &IterationSummary,
    total: usize,
    failed: Option<(NaiveDate, &str)>,
) -> Vec<String> {
    let mut v = vec![format!(
        "Iteration summary: {} of {total} day(s) done, {} skipped (no data), {} failed",
        s.done.len(),
        s.skipped.len(),
        usize::from(failed.is_some())
    )];
    if !s.existing.is_empty() {
        let e: Vec<String> = s.existing.iter().map(|(d, r)| format!("{r}@{d}")).collect();
        v.push(format!(
            "  existing (not re-run): {} report run(s): {}",
            e.len(),
            e.join(", ")
        ));
    }
    if !s.done.is_empty() {
        let d: Vec<String> = s.done.iter().map(|d| d.to_string()).collect();
        v.push(format!("  done: {}", d.join(", ")));
    }
    for (d, r, m) in &s.skipped {
        v.push(format!("  skipped: {d} at {r} ({m})"));
    }
    if let Some((d, r)) = failed {
        v.push(format!(
            "  failed: {d} at {r}; the remaining days were not run, no merge"
        ));
    }
    if let Some(m) = &s.merged {
        for (name, res) in m {
            match res {
                Ok(o) => {
                    v.push(format!("  merged {name}: {}", o.path.display()));
                    let t: Vec<String> = o.tables.iter().map(|(n, c)| format!("{n} {c}")).collect();
                    v.push(format!("    rows: {}", t.join(", ")));
                    for n in &o.notes {
                        v.push(format!("    {n}"));
                    }
                    if !o.days_missing.is_empty() {
                        let d: Vec<String> = o.days_missing.iter().map(|d| d.to_string()).collect();
                        v.push(format!("    days without output: {}", d.join(", ")));
                    }
                }
                Err(e) => v.push(format!("  merge {name} FAILED: {e:#}")),
            }
        }
    }
    v
}

fn print_summary(s: &IterationSummary, total: usize, failed: Option<(NaiveDate, &str)>) {
    for l in summary_lines(s, total, failed) {
        println!("{l}");
    }
}

/// Paths of the merged workbooks that were written.
pub fn merged_paths(s: &IterationSummary) -> Vec<PathBuf> {
    s.merged
        .iter()
        .flatten()
        .filter_map(|(_, r)| r.as_ref().ok())
        .map(|o| o.path.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, day).unwrap()
    }

    fn today() -> NaiveDate {
        d(12)
    }

    fn week() -> RunDate {
        RunDate {
            date1: d(1),
            date2: Some(d(7)),
        }
    }

    fn go(f: &mut Fake, rname: &str, merge: bool) -> Result<IterationSummary> {
        run_iteration_with(
            f,
            rname,
            &week(),
            IterOpts {
                merge,
                ..IterOpts::default()
            },
            today(),
        )
    }

    fn opts_merge() -> IterOpts {
        IterOpts {
            merge: true,
            ..IterOpts::default()
        }
    }

    #[test]
    fn parallel_workers_share_the_days_and_merge_once() {
        let days: Vec<NaiveDate> = (1..=6).map(d).collect();
        let shared = ParallelShared::new(&days);
        let mut fakes = [Fake::default(), Fake::default(), Fake::default()];
        std::thread::scope(|s| {
            for f in fakes.iter_mut() {
                let shared = &shared;
                s.spawn(move || parallel_worker(shared, f, "k2xx", opts_merge(), today()));
            }
        });
        let ran: usize = fakes.iter().map(|f| f.calls.len()).sum();
        assert_eq!(ran, 6 * registry::chain("k2xx").len());
        let mut merger = Fake::default();
        let range = RunDate {
            date1: d(1),
            date2: Some(d(6)),
        };
        let sum =
            finish_parallel(shared, &mut merger, "k2xx", &range, opts_merge(), today()).unwrap();
        assert_eq!(sum.done, days);
        assert_eq!(merger.merges.len(), 1);
        assert_eq!(merger.merges[0].1, days);
    }

    #[test]
    fn parallel_hard_error_stops_new_days_and_skips_merge() {
        let days: Vec<NaiveDate> = (1..=6).map(d).collect();
        let shared = ParallelShared::new(&days);
        let first = registry::chain("k2xx")[0].to_string();
        let mut f = Fake {
            hard: Some((first, 2)),
            ..Fake::default()
        };
        parallel_worker(&shared, &mut f, "k2xx", opts_merge(), today());
        let mut merger = Fake::default();
        let err = finish_parallel(shared, &mut merger, "k2xx", &week(), opts_merge(), today());
        assert!(err.is_err());
        assert!(merger.merges.is_empty());
        // day 1 ran, day 2 failed, later days never started
        assert!(f.calls.iter().all(|(_, day)| *day <= d(2)));
    }

    #[derive(Default)]
    struct Fake {
        calls: Vec<(String, NaiveDate)>,
        todays: Vec<NaiveDate>,
        no_data: Vec<(String, u32)>,
        hard: Option<(String, u32)>,
        merges: Vec<(String, Vec<NaiveDate>)>,
        fail_merge_of: Option<String>,
        /// (report, day-of-month) -> state; absent = Complete when `all_complete`, else None
        states: Vec<(String, u32, Completeness)>,
        all_complete: bool,
    }

    impl DayRunner for Fake {
        fn complete(&mut self, report: &str, date: &RunDate, _today: NaiveDate) -> Completeness {
            let n = date.date1.format("%d").to_string().parse::<u32>().unwrap();
            self.states
                .iter()
                .find(|(r, d, _)| r == report && *d == n)
                .map(|(_, _, c)| c.clone())
                .unwrap_or(if self.all_complete {
                    Completeness::Complete
                } else {
                    Completeness::None
                })
        }

        fn run(&mut self, report: &str, date: &RunDate, today: NaiveDate) -> Result<()> {
            let day = date.date1;
            self.calls.push((report.to_string(), day));
            self.todays.push(today);
            let n = day.format("%d").to_string().parse::<u32>().unwrap();
            if self.hard == Some((report.to_string(), n)) {
                return Err(anyhow!("boom"));
            }
            if self.no_data.contains(&(report.to_string(), n)) {
                return Err(NoDataError {
                    step: "vt11".into(),
                    message: "none".into(),
                }
                .into());
            }
            Ok(())
        }

        fn merge(
            &mut self,
            rname: &str,
            today: NaiveDate,
            _range: &RunDate,
            days: &[NaiveDate],
        ) -> Result<Vec<(String, Result<MergeOutcome>)>> {
            self.todays.push(today);
            self.merges.push((rname.to_string(), days.to_vec()));
            Ok(registry::chain(rname)
                .into_iter()
                .map(|r| {
                    let res = if self.fail_merge_of.as_deref() == Some(r) {
                        Err(anyhow!("merge broke"))
                    } else {
                        Ok(MergeOutcome {
                            report: r.to_string(),
                            path: PathBuf::from(format!("{r}.xlsx")),
                            days_merged: days.to_vec(),
                            days_missing: vec![],
                            tables: vec![("pivot".into(), 3)],
                            notes: vec![],
                        })
                    };
                    (r.to_string(), res)
                })
                .collect())
        }
    }

    fn names(f: &Fake) -> Vec<String> {
        f.calls
            .iter()
            .map(|(r, d)| format!("{r}@{}", d.format("%d")))
            .collect()
    }

    #[test]
    fn week_runs_every_day_in_chain_order() {
        let mut f = Fake::default();
        let s = go(&mut f, "repack", false).unwrap();
        assert_eq!(s.done, week().days());
        assert!(s.skipped.is_empty() && s.merged.is_none());
        assert_eq!(f.calls.len(), 21);
        let n = names(&f);
        assert_eq!(
            &n[..6],
            [
                "k2xx@01",
                "relabel@01",
                "repack@01",
                "k2xx@02",
                "relabel@02",
                "repack@02"
            ]
        );
        assert_eq!(n[20], "repack@07");
        assert!(f.merges.is_empty());
    }

    #[test]
    fn independent_report_runs_once_per_day() {
        let mut f = Fake::default();
        go(&mut f, "xdock", false).unwrap();
        assert_eq!(f.calls.len(), 7);
        assert!(f.calls.iter().all(|(r, _)| r == "xdock"));
    }

    #[test]
    fn no_data_day_is_skipped_and_cuts_the_chain() {
        let mut f = Fake {
            no_data: vec![("relabel".into(), 2)],
            ..Fake::default()
        };
        let s = go(&mut f, "repack", false).unwrap();
        assert_eq!(s.done.len(), 6);
        assert_eq!(s.skipped.len(), 1);
        assert_eq!((s.skipped[0].0, s.skipped[0].1.as_str()), (d(2), "relabel"));
        let n = names(&f);
        assert!(n.contains(&"relabel@02".to_string()));
        assert!(!n.contains(&"repack@02".to_string()), "chain cut short");
        assert!(n.contains(&"k2xx@03".to_string()));
        assert_eq!(f.calls.len(), 20);
    }

    #[test]
    fn hard_error_stops_everything() {
        let mut f = Fake {
            hard: Some(("xdock".into(), 3)),
            ..Fake::default()
        };
        let e = go(&mut f, "xdock", true).unwrap_err();
        assert!(format!("{e:#}").contains("boom") && format!("{e:#}").contains("2026-11-03"));
        assert_eq!(names(&f), ["xdock@01", "xdock@02", "xdock@03"]);
        assert!(f.merges.is_empty(), "no merge after a hard error");
    }

    #[test]
    fn merge_only_when_requested_and_a_day_succeeded() {
        let mut f = Fake::default();
        let s = go(&mut f, "repack", true).unwrap();
        assert_eq!(f.merges.len(), 1);
        assert_eq!(f.merges[0].0, "repack");
        assert_eq!(f.merges[0].1, week().days());
        assert_eq!(merged_paths(&s).len(), 3);

        // every day without data: no merge, no error
        let mut f = Fake {
            no_data: (1..=7).map(|n| ("xdock".to_string(), n)).collect(),
            ..Fake::default()
        };
        let s = go(&mut f, "xdock", true).unwrap();
        assert!(s.done.is_empty() && s.merged.is_none() && f.merges.is_empty());
        assert_eq!(s.skipped.len(), 7);
    }

    #[test]
    fn today_is_the_same_for_every_run_and_the_merge() {
        let mut f = Fake::default();
        go(&mut f, "repack", true).unwrap();
        assert_eq!(f.todays.len(), 22);
        assert!(f.todays.iter().all(|t| *t == today()));
    }

    #[test]
    fn skipped_day_with_partial_chain_is_still_offered_to_the_merge() {
        let mut f = Fake {
            no_data: vec![("relabel".into(), 2)],
            ..Fake::default()
        };
        go(&mut f, "repack", true).unwrap();
        assert_eq!(f.merges[0].1.len(), 7);
    }

    #[test]
    fn merge_failure_of_one_report_is_reported_not_hidden() {
        let mut f = Fake {
            fail_merge_of: Some("k2xx".into()),
            ..Fake::default()
        };
        // the verdict is an Err, but the summary keeps the good workbooks
        let e = go(&mut f, "repack", true).unwrap_err();
        assert!(e.to_string().contains("1 merge(s) failed"), "{e:#}");
        let s = iterate_core(
            &mut f,
            "repack",
            &week(),
            IterOpts {
                merge: true,
                ..IterOpts::default()
            },
            today(),
        )
        .unwrap();
        let lines = summary_lines(&s, 7, None).join("\n");
        assert!(lines.contains("merge k2xx FAILED") && lines.contains("merged repack"));
        assert!(lines.contains("merged relabel"));
        assert_eq!(merged_paths(&s).len(), 2);
        assert_eq!(s.merge_failures(), 1);
    }

    #[test]
    fn whole_merge_error_is_a_failure_too() {
        struct Broken(Fake);
        impl DayRunner for Broken {
            fn run(&mut self, r: &str, d: &RunDate, t: NaiveDate) -> Result<()> {
                self.0.run(r, d, t)
            }
            fn complete(&mut self, r: &str, d: &RunDate, t: NaiveDate) -> Completeness {
                self.0.complete(r, d, t)
            }
            fn merge(
                &mut self,
                _: &str,
                _: NaiveDate,
                _: &RunDate,
                _: &[NaiveDate],
            ) -> Result<Vec<(String, Result<MergeOutcome>)>> {
                Err(anyhow!("disk gone"))
            }
        }
        let mut b = Broken(Fake::default());
        let e = run_iteration_with(
            &mut b,
            "xdock",
            &week(),
            IterOpts {
                merge: true,
                ..IterOpts::default()
            },
            today(),
        )
        .unwrap_err();
        assert!(e.to_string().contains("1 merge(s) failed"));
        let s = iterate_core(
            &mut b,
            "xdock",
            &week(),
            IterOpts {
                merge: true,
                ..IterOpts::default()
            },
            today(),
        )
        .unwrap();
        assert_eq!(s.done.len(), 7);
        assert!(summary_lines(&s, 7, None).join("\n").contains("disk gone"));
    }

    #[test]
    fn single_day_and_unknown_report_are_errors() {
        let mut f = Fake::default();
        let one = RunDate::single(d(1));
        let e =
            run_iteration_with(&mut f, "xdock", &one, IterOpts::default(), today()).unwrap_err();
        assert!(e.to_string().contains("--loop needs a range or week"));
        assert!(go(&mut f, "nope", false).is_err());
        assert!(f.calls.is_empty());
    }

    #[test]
    fn summary_text() {
        let s = IterationSummary {
            done: vec![d(1)],
            skipped: vec![(d(2), "xdock".into(), "NO DATA".into())],
            existing: vec![],
            merged: None,
        };
        let l = summary_lines(&s, 7, Some((d(3), "xdock")));
        assert!(l[0].contains("1 of 7") && l[0].contains("1 skipped") && l[0].contains("1 failed"));
        assert!(l.iter().any(|x| x.contains("failed: 2026-11-03")));
    }

    fn opts(merge: bool, skip_existing: bool, no_sap: bool) -> IterOpts {
        IterOpts {
            merge,
            skip_existing,
            no_sap,
            with_prereqs: false,
        }
    }

    fn missing(t: &[&str]) -> Completeness {
        Completeness::Missing(t.iter().map(|x| x.to_string()).collect())
    }

    #[test]
    fn skip_existing_skips_complete_and_runs_the_rest() {
        let mut f = Fake {
            states: vec![
                ("xdock".into(), 1, Completeness::Complete),
                ("xdock".into(), 2, Completeness::Complete),
                ("xdock".into(), 3, missing(&["pivot"])), // partial = incomplete
            ],
            ..Fake::default()
        };
        let s =
            run_iteration_with(&mut f, "xdock", &week(), opts(true, true, false), today()).unwrap();
        assert_eq!(
            names(&f),
            ["xdock@03", "xdock@04", "xdock@05", "xdock@06", "xdock@07"]
        );
        assert_eq!(s.existing.len(), 2);
        // skipped-existing days count as done and are merged
        assert_eq!(s.done, week().days());
        assert_eq!(f.merges[0].1, week().days());
        assert!(summary_lines(&s, 7, None)
            .join("\n")
            .contains("existing (not re-run): 2"));
    }

    #[test]
    fn skip_existing_applies_per_report_of_the_chain() {
        let mut f = Fake {
            states: vec![
                ("k2xx".into(), 1, Completeness::Complete),
                ("relabel".into(), 1, Completeness::Complete),
            ],
            ..Fake::default()
        };
        run_iteration_with(&mut f, "repack", &week(), opts(false, true, false), today()).unwrap();
        assert!(!names(&f).contains(&"k2xx@01".to_string()));
        assert!(!names(&f).contains(&"relabel@01".to_string()));
        assert!(names(&f).contains(&"repack@01".to_string()));
        assert_eq!(f.calls.len(), 19);
    }

    #[test]
    fn no_sap_never_runs_and_only_merges() {
        let mut f = Fake {
            all_complete: true,
            ..Fake::default()
        };
        let s = run_iteration_with(&mut f, "repack", &week(), opts(true, false, true), today())
            .unwrap();
        assert!(f.calls.is_empty(), "SAP steps must not run");
        assert_eq!(s.done, week().days());
        assert_eq!(f.merges.len(), 1);
        assert_eq!(merged_paths(&s).len(), 3);
        // without --merge: verify only
        let mut f = Fake {
            all_complete: true,
            ..Fake::default()
        };
        let s = run_iteration_with(&mut f, "xdock", &week(), opts(false, false, true), today())
            .unwrap();
        assert!(f.calls.is_empty() && f.merges.is_empty() && s.merged.is_none());
    }

    #[test]
    fn no_sap_lists_every_gap_and_merges_nothing() {
        let mut f = Fake {
            all_complete: true,
            states: vec![
                ("xdock".into(), 3, Completeness::None),
                ("xdock".into(), 5, missing(&["pivot", "vt11"])),
            ],
            ..Fake::default()
        };
        let e = run_iteration_with(&mut f, "xdock", &week(), opts(true, false, true), today())
            .unwrap_err();
        let m = format!("{e:#}");
        assert!(m.contains("2026-11-03 xdock: no output files"), "{m}");
        assert!(
            m.contains("2026-11-05 xdock: missing table(s): pivot, vt11"),
            "{m}"
        );
        assert!(f.calls.is_empty() && f.merges.is_empty());
    }

    #[test]
    fn no_sap_requires_prerequisite_outputs() {
        // repack complete everywhere, but day 2 has no k2xx output
        let mut f = Fake {
            all_complete: true,
            states: vec![("k2xx".into(), 2, Completeness::None)],
            ..Fake::default()
        };
        let e = run_iteration_with(&mut f, "repack", &week(), opts(true, false, true), today())
            .unwrap_err();
        assert!(format!("{e:#}").contains("2026-11-02 k2xx"), "{e:#}");
        assert!(f.merges.is_empty() && f.calls.is_empty());
    }

    #[test]
    fn output_date_reaches_every_run_and_the_merge() {
        let od = NaiveDate::from_ymd_opt(2026, 10, 1).unwrap();
        let mut f = Fake::default();
        run_iteration_with(&mut f, "xdock", &week(), opts(true, false, false), od).unwrap();
        assert_eq!(f.todays.len(), 8);
        assert!(f.todays.iter().all(|t| *t == od));
    }

    #[test]
    fn single_run_reuse_flags() {
        let one = RunDate::single(d(5));
        // plain: runs
        let mut f = Fake::default();
        assert_eq!(
            run_single_with(&mut f, "xdock", &one, IterOpts::default(), today()).unwrap(),
            SingleOutcome::Ran
        );
        // skip-existing: complete = skipped, incomplete = run
        let mut f = Fake {
            all_complete: true,
            ..Fake::default()
        };
        let o = opts(false, true, false);
        assert_eq!(
            run_single_with(&mut f, "xdock", &one, o, today()).unwrap(),
            SingleOutcome::Existing
        );
        assert!(f.calls.is_empty());
        let mut f = Fake {
            states: vec![("xdock".into(), 5, missing(&["pivot"]))],
            all_complete: true,
            ..Fake::default()
        };
        assert_eq!(
            run_single_with(&mut f, "xdock", &one, o, today()).unwrap(),
            SingleOutcome::Ran
        );
        // no-sap: ready or error, never runs
        let o = opts(false, false, true);
        let mut f = Fake {
            all_complete: true,
            ..Fake::default()
        };
        assert_eq!(
            run_single_with(&mut f, "xdock", &one, o, today()).unwrap(),
            SingleOutcome::Ready
        );
        let mut f = Fake::default();
        let e = run_single_with(&mut f, "xdock", &one, o, today()).unwrap_err();
        assert!(e.to_string().contains("no output files"), "{e}");
        assert!(f.calls.is_empty());
    }

    #[test]
    fn routing_never_connects_with_no_sap() {
        let mut live_called = false;
        let no_sap = opts(true, false, true);
        let mut f = Fake {
            all_complete: true,
            ..Fake::default()
        };
        run_rname_routed(&mut f, "xdock", &week(), true, no_sap, today(), || {
            live_called = true;
            Ok(())
        })
        .unwrap();
        assert!(!live_called, "no-sap must not create a SAP session");
        assert!(f.calls.is_empty() && f.merges.len() == 1);

        // incomplete + no-sap: still no session, an error
        let mut f = Fake::default();
        let r = run_rname_routed(&mut f, "xdock", &week(), true, no_sap, today(), || {
            live_called = true;
            Ok(())
        });
        assert!(r.is_err() && !live_called);

        // single date, no-sap
        let mut f = Fake {
            all_complete: true,
            ..Fake::default()
        };
        let one = RunDate::single(d(1));
        let o = opts(false, false, true);
        run_rname_routed(&mut f, "xdock", &one, false, o, today(), || {
            live_called = true;
            Ok(())
        })
        .unwrap();
        assert!(!live_called);
    }

    #[test]
    fn routing_connects_only_when_something_is_missing() {
        let mut live = 0;
        // legacy: always live
        let mut f = Fake::default();
        run_rname_routed(
            &mut f,
            "xdock",
            &week(),
            true,
            IterOpts::default(),
            today(),
            || {
                live += 1;
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(live, 1);
        // skip-existing with one gap: live
        let mut f = Fake {
            all_complete: true,
            states: vec![("xdock".into(), 4, Completeness::None)],
            ..Fake::default()
        };
        let o = opts(false, true, false);
        run_rname_routed(&mut f, "xdock", &week(), true, o, today(), || {
            live += 1;
            Ok(())
        })
        .unwrap();
        assert_eq!(live, 2);
        // skip-existing, all complete: offline, no live
        let mut f = Fake {
            all_complete: true,
            ..Fake::default()
        };
        let o = opts(true, true, false);
        run_rname_routed(&mut f, "xdock", &week(), true, o, today(), || {
            live += 1;
            Ok(())
        })
        .unwrap();
        assert_eq!(live, 2);
        assert!(f.calls.is_empty() && f.merges.len() == 1);
    }

    fn wp(skip_existing: bool, no_sap: bool) -> IterOpts {
        IterOpts {
            with_prereqs: true,
            ..opts(false, skip_existing, no_sap)
        }
    }

    fn one() -> RunDate {
        RunDate::single(d(5))
    }

    #[test]
    fn with_prereqs_runs_the_own_chain_in_order() {
        let mut f = Fake::default();
        let r = run_single_with(&mut f, "repack", &one(), wp(false, false), today()).unwrap();
        assert_eq!(r, SingleOutcome::Ran);
        assert_eq!(names(&f), ["k2xx@05", "relabel@05", "repack@05"]);
        // never downstream: relabel does not run repack
        let mut f = Fake::default();
        run_single_with(&mut f, "relabel", &one(), wp(false, false), today()).unwrap();
        assert_eq!(names(&f), ["k2xx@05", "relabel@05"]);
        // independent report: just itself
        let mut f = Fake::default();
        run_single_with(&mut f, "xdock", &one(), wp(false, false), today()).unwrap();
        assert_eq!(names(&f), ["xdock@05"]);
        assert!(f.todays.iter().all(|t| *t == today()));
    }

    #[test]
    fn with_prereqs_no_data_cuts_the_rest_of_the_chain() {
        let mut f = Fake {
            no_data: vec![("k2xx".into(), 5)],
            ..Fake::default()
        };
        let r = run_single_with(&mut f, "repack", &one(), wp(false, false), today()).unwrap();
        assert_eq!(r, SingleOutcome::NoData);
        assert_eq!(names(&f), ["k2xx@05"]);
        // any other error is hard
        let mut f = Fake {
            hard: Some(("relabel".into(), 5)),
            ..Fake::default()
        };
        let e = run_single_with(&mut f, "repack", &one(), wp(false, false), today()).unwrap_err();
        assert!(format!("{e:#}").contains("boom"));
        assert_eq!(names(&f), ["k2xx@05", "relabel@05"]);
    }

    #[test]
    fn with_prereqs_skip_existing_runs_only_incomplete_members() {
        let mut f = Fake {
            states: vec![
                ("k2xx".into(), 5, Completeness::Complete),
                ("relabel".into(), 5, missing(&["pivot"])),
                ("repack".into(), 5, Completeness::None),
            ],
            ..Fake::default()
        };
        let r = run_single_with(&mut f, "repack", &one(), wp(true, false), today()).unwrap();
        assert_eq!(r, SingleOutcome::Ran);
        assert_eq!(names(&f), ["relabel@05", "repack@05"]);
        // everything complete: nothing runs
        let mut f = Fake {
            all_complete: true,
            ..Fake::default()
        };
        let r = run_single_with(&mut f, "repack", &one(), wp(true, false), today()).unwrap();
        assert_eq!(r, SingleOutcome::Existing);
        assert!(f.calls.is_empty());
        // without the flag only the report itself is considered
        let mut f = Fake {
            states: vec![("repack".into(), 5, Completeness::Complete)],
            ..Fake::default()
        };
        let r = run_single_with(&mut f, "repack", &one(), opts(false, true, false), today());
        assert_eq!(r.unwrap(), SingleOutcome::Existing);
        assert!(f.calls.is_empty());
    }

    #[test]
    fn with_prereqs_no_sap_requires_the_whole_chain() {
        // k2xx missing, relabel partial, repack present
        let mut f = Fake {
            states: vec![
                ("k2xx".into(), 5, Completeness::None),
                ("relabel".into(), 5, missing(&["combined", "pivot"])),
                ("repack".into(), 5, Completeness::Complete),
            ],
            ..Fake::default()
        };
        let e = run_single_with(&mut f, "repack", &one(), wp(false, true), today()).unwrap_err();
        let m = format!("{e:#}");
        assert!(m.contains("k2xx: no output files"), "{m}");
        assert!(
            m.contains("relabel: missing table(s): combined, pivot"),
            "{m}"
        );
        assert!(!m.contains("repack:"), "{m}");
        assert!(f.calls.is_empty(), "nothing may run");
        // all present: ready
        let mut f = Fake {
            all_complete: true,
            ..Fake::default()
        };
        let r = run_single_with(&mut f, "repack", &one(), wp(false, true), today()).unwrap();
        assert_eq!(r, SingleOutcome::Ready);
        assert!(f.calls.is_empty());
        // without --with-prereqs the prerequisites are not checked (today's behaviour)
        let mut f = Fake {
            states: vec![("repack".into(), 5, Completeness::Complete)],
            ..Fake::default()
        };
        let r = run_single_with(&mut f, "repack", &one(), opts(false, false, true), today());
        assert_eq!(r.unwrap(), SingleOutcome::Ready);
    }

    #[test]
    fn with_prereqs_routing_never_connects_for_no_sap_and_complete_resume() {
        let mut live = 0;
        // no-sap with a gap: error, no SAP
        let mut f = Fake {
            states: vec![("k2xx".into(), 5, Completeness::None)],
            all_complete: true,
            ..Fake::default()
        };
        let r = run_rname_routed(
            &mut f,
            "repack",
            &one(),
            false,
            wp(false, true),
            today(),
            || {
                live += 1;
                Ok(())
            },
        );
        assert!(r.is_err() && live == 0 && f.calls.is_empty());
        // no-sap all present: ready, no SAP
        let mut f = Fake {
            all_complete: true,
            ..Fake::default()
        };
        run_rname_routed(
            &mut f,
            "repack",
            &one(),
            false,
            wp(false, true),
            today(),
            || {
                live += 1;
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(live, 0);
        // skip-existing: all chain members complete = no SAP; one gap in a prerequisite = SAP
        run_rname_routed(
            &mut f,
            "repack",
            &one(),
            false,
            wp(true, false),
            today(),
            || {
                live += 1;
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(live, 0);
        let mut f = Fake {
            states: vec![("k2xx".into(), 5, Completeness::None)],
            all_complete: true,
            ..Fake::default()
        };
        run_rname_routed(
            &mut f,
            "repack",
            &one(),
            false,
            wp(true, false),
            today(),
            || {
                live += 1;
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(live, 1);
        // without the flag the prerequisite gap does not matter (repack itself is complete)
        let o = opts(false, true, false);
        run_rname_routed(&mut f, "repack", &one(), false, o, today(), || {
            live += 1;
            Ok(())
        })
        .unwrap();
        assert_eq!(live, 1);
    }

    #[test]
    fn with_prereqs_has_no_effect_on_a_loop() {
        let mut a = Fake::default();
        let sa = run_iteration_with(&mut a, "repack", &week(), opts(true, false, false), today())
            .unwrap();
        let mut b = Fake::default();
        let with = IterOpts {
            with_prereqs: true,
            ..opts(true, false, false)
        };
        let sb = run_iteration_with(&mut b, "repack", &week(), with, today()).unwrap();
        assert_eq!(a.calls, b.calls);
        assert_eq!(a.merges, b.merges);
        assert_eq!((sa.done, sa.skipped.len()), (sb.done, sb.skipped.len()));
        // routing a loop with the flag takes the iteration path as well
        let mut f = Fake::default();
        run_rname_routed(&mut f, "repack", &week(), true, with, today(), || Ok(())).unwrap();
    }

    #[test]
    fn without_with_prereqs_a_single_run_is_unchanged() {
        let mut f = Fake::default();
        run_single_with(&mut f, "repack", &one(), IterOpts::default(), today()).unwrap();
        assert_eq!(names(&f), ["repack@05"]);
    }

    #[test]
    fn offline_runner_refuses_to_run() {
        let e = OfflineDayRunner
            .run("xdock", &RunDate::single(d(1)), today())
            .unwrap_err();
        assert!(e.to_string().contains("offline"));
    }
}
