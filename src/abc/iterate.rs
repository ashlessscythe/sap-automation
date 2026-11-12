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
//! `today` (the date folder of the outputs) is captured ONCE by [`run_iteration`] and handed to
//! every run and to the merge, so they agree even when the iteration crosses midnight. The
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
use super::registry;
use super::run::RunDate;
use super::runner::NoDataError;

/// Options of an iteration.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct IterOpts {
    /// Merge the per-day outputs into one workbook per report afterwards.
    pub merge: bool,
}

/// Seam over everything that touches SAP or the file system.
pub trait DayRunner {
    /// Run `report` for the single day `day`; outputs go under the `today` folder.
    fn run(&mut self, report: &str, day: NaiveDate, today: NaiveDate) -> Result<()>;
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

impl DayRunner for RealDayRunner<'_> {
    fn run(&mut self, report: &str, day: NaiveDate, today: NaiveDate) -> Result<()> {
        dispatch::run_report_on(self.session, report, &RunDate::single(day), today).map(|_| ())
    }

    fn merge(
        &mut self,
        rname: &str,
        today: NaiveDate,
        range: &RunDate,
        days: &[NaiveDate],
    ) -> Result<Vec<(String, Result<MergeOutcome>)>> {
        // the reports dir is the same for every report of the chain (CLI > [global] > default)
        let dir = dispatch::reports_dir_for(rname);
        merge::merge_chain(&dir, rname, today, range, days)
    }
}

/// Run the iteration on a live session (captures `today` once).
pub fn run_iteration(
    session: &GuiSession,
    rname: &str,
    range: &RunDate,
    opts: IterOpts,
) -> Result<IterationSummary> {
    let today = chrono::Local::now().date_naive();
    run_iteration_with(&mut RealDayRunner { session }, rname, range, opts, today)
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
    println!(
        "Iterating '{rname}' over {total} day(s) ({} .. {}), chain: {}{}",
        days[0],
        days[total - 1],
        chain.join(" -> "),
        if opts.merge { ", merge afterwards" } else { "" }
    );

    for (i, &day) in days.iter().enumerate() {
        println!("=== Day {}/{total}: {day} ===", i + 1);
        let mut day_ok = true;
        for report in &chain {
            println!("--- {day}: {report} ---");
            match runner.run(report, day, today) {
                Ok(()) => {}
                Err(e) if e.downcast_ref::<NoDataError>().is_some() => {
                    let msg = dispatch::describe_error(&e);
                    println!("{day}: {report} skipped, {msg}");
                    summary.skipped.push((day, report.to_string(), msg));
                    day_ok = false;
                    break;
                }
                Err(e) => {
                    print_summary(&summary, total, Some((day, report)));
                    return Err(e.context(format!(
                        "iteration of '{rname}' stopped at {day}, report '{report}' \
                         (no merge; files already written stay)"
                    )));
                }
            }
        }
        if day_ok {
            summary.done.push(day);
        }
    }

    if opts.merge {
        if summary.done.is_empty() {
            println!("No day finished: nothing to merge.");
        } else {
            // days that were touched at all (a skipped day may still have prerequisite output)
            let mut touched: Vec<NaiveDate> = summary.done.clone();
            touched.extend(summary.skipped.iter().map(|(d, _, _)| *d));
            touched.sort();
            touched.dedup();
            summary.merged = Some(match runner.merge(rname, today, range, &touched) {
                Ok(v) => v,
                Err(e) => vec![(rname.to_string(), Err(e))],
            });
        }
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
        run_iteration_with(f, rname, &week(), IterOpts { merge }, today())
    }

    #[derive(Default)]
    struct Fake {
        calls: Vec<(String, NaiveDate)>,
        todays: Vec<NaiveDate>,
        no_data: Vec<(String, u32)>,
        hard: Option<(String, u32)>,
        merges: Vec<(String, Vec<NaiveDate>)>,
        fail_merge_of: Option<String>,
    }

    impl DayRunner for Fake {
        fn run(&mut self, report: &str, day: NaiveDate, today: NaiveDate) -> Result<()> {
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
        let s = iterate_core(&mut f, "repack", &week(), IterOpts { merge: true }, today()).unwrap();
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
            fn run(&mut self, r: &str, d: NaiveDate, t: NaiveDate) -> Result<()> {
                self.0.run(r, d, t)
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
        let e = run_iteration_with(&mut b, "xdock", &week(), IterOpts { merge: true }, today())
            .unwrap_err();
        assert!(e.to_string().contains("1 merge(s) failed"));
        let s = iterate_core(&mut b, "xdock", &week(), IterOpts { merge: true }, today()).unwrap();
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
            merged: None,
        };
        let l = summary_lines(&s, 7, Some((d(3), "xdock")));
        assert!(l[0].contains("1 of 7") && l[0].contains("1 skipped") && l[0].contains("1 failed"));
        assert!(l.iter().any(|x| x.contains("failed: 2026-11-03")));
    }
}
