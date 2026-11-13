//! Parallel SAP sessions (feasibility work).
//!
//! * `--test --tcodes=zvt11,vt11`: open one NEW session per tcode and start each tcode at the same time.
//! * `--rname xdock --rname k2xx --run-date=<date> --multi-conn`: one NEW session per report, each report
//!   runs the normal single-run flow (`run_rname_unattended`) in its own session and thread.
//!
//! One thread and one COM instance per session: the COM proxies are not `Send`, so every thread
//! creates and uses its own objects. The sessions are created from, and closed on, connection 0.

use anyhow::{anyhow, bail, Result};
use sap_scripting::*;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

/// SAP GUI default: at most 6 sessions (`rdisp/max_alt_modes`) per connection.
pub const MAX_SESSIONS: usize = 6;

/// How long every tcode screen stays open in `--test` mode so the overlap is visible.
const HOLD: Duration = Duration::from_secs(10);

/// Split `zvt11,vt11` (or `xdock|k2xx`) into trimmed items, lower/upper-casing left to the caller.
pub fn split_list(raw: &str) -> Vec<String> {
    raw.split([',', '|'])
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Split `zvt11,vt11` into upper-case tcodes (2..=6, no blanks).
pub fn parse_tcodes(raw: &str) -> Result<Vec<String>> {
    let list: Vec<String> = split_list(raw)
        .into_iter()
        .map(|s| s.to_uppercase())
        .collect();
    if list.len() < 2 || list.len() > MAX_SESSIONS {
        bail!(
            "--tcodes needs 2 to {MAX_SESSIONS} comma-separated tcodes (got {})",
            list.len()
        );
    }
    Ok(list)
}

/// Fails when `open + wanted` sessions would exceed the per-connection limit.
pub fn check_limit(open: usize, wanted: usize) -> Result<()> {
    if open + wanted > MAX_SESSIONS {
        bail!(
            "{open} session(s) already open + {wanted} new > {MAX_SESSIONS} (SAP limit per connection); \
             close some windows or use fewer parallel jobs"
        );
    }
    Ok(())
}

fn session_ids(connection: &GuiConnection) -> Result<Vec<String>> {
    let children = GuiConnectionExt::children(connection)?;
    let mut ids = Vec::new();
    for i in 0..children.count()? {
        let c = children.element_at(i)?;
        ids.push(GuiComponentExt::id(&c)?);
    }
    Ok(ids)
}

/// Number of sessions currently open on connection 0.
fn open_session_count() -> Result<usize> {
    let com = SAPComInstance::new()?;
    let wrapper = com.sap_wrapper()?;
    let engine = wrapper.scripting_engine()?;
    let connection = crate::app::get_or_create_connection(&engine)?;
    Ok(session_ids(&connection)?.len())
}

fn stamp(t0: Instant) -> String {
    format!("+{:>6.2}s", t0.elapsed().as_secs_f32())
}

/// `/app/con[0]/ses[3]` -> `s3` (the whole id, sanitised, when it has no `ses[n]`).
pub fn session_tag(session_id: &str) -> String {
    match session_id.rsplit_once("ses[") {
        Some((_, rest)) => format!("s{}", rest.trim_end_matches(']')),
        None => session_id
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .collect(),
    }
}

/// A job runs on its own thread against its own session.
type Job = Box<dyn FnOnce(&GuiSession) -> Result<()> + Send>;

struct Outcome {
    label: String,
    session_id: String,
    started: String,
    ended: String,
    result: Result<()>,
}

/// Own COM instance, attach to the session by id, run the job.
fn worker(label: String, session_id: String, job: Job, t0: Instant) -> Outcome {
    let started = stamp(t0);
    let result = (|| -> Result<()> {
        let com = SAPComInstance::new()?;
        let wrapper = com.sap_wrapper()?;
        let engine = wrapper.scripting_engine()?;
        let comp = GuiContainerExt::find_by_id(&engine, session_id.clone())?;
        let session: GuiSession = comp
            .downcast()
            .ok_or_else(|| anyhow!("{session_id} is not a GuiSession"))?;
        crate::utils::sap_export_utils::set_session_tag(&session_tag(&session_id));
        println!("[{label}] {started} started in {session_id}");
        crate::logging::status("running");
        job(&session)
    })();
    crate::logging::status(if result.is_ok() { "done" } else { "FAILED" });
    Outcome {
        label,
        session_id,
        started,
        ended: stamp(t0),
        result,
    }
}

/// Create one new session per job on connection 0, run all jobs at the same time, close the
/// sessions again and print a table. Err when any job failed.
fn run_jobs(jobs: Vec<(String, Job)>) -> Result<()> {
    let n = jobs.len();
    let t0 = Instant::now();
    // asynchronous, non-blocking console output for the whole parallel run (flushed on drop)
    let _log = crate::logging::start();

    let com = SAPComInstance::new()?;
    let wrapper = com.sap_wrapper()?;
    let engine = wrapper.scripting_engine()?;
    let connection = crate::app::get_or_create_connection(&engine)?;
    let before = session_ids(&connection)?;
    if before.is_empty() {
        bail!("the SAP connection has no session to create new ones from");
    }
    check_limit(before.len(), n)?;
    println!("{} session(s) open, creating {n} more ...", before.len());

    let first: GuiSession = GuiConnectionExt::children(&connection)?
        .element_at(0)?
        .downcast()
        .ok_or_else(|| anyhow!("first child of the connection is not a session"))?;
    for _ in 0..n {
        first.create_session()?;
        // let SAP finish one window before asking for the next
        thread::sleep(Duration::from_millis(800));
    }

    // wait until all new sessions are listed
    let deadline = Instant::now() + Duration::from_secs(15);
    let new_ids = loop {
        let new: Vec<String> = session_ids(&connection)?
            .into_iter()
            .filter(|id| !before.contains(id))
            .collect();
        if new.len() >= n {
            break new;
        }
        if Instant::now() > deadline {
            bail!(
                "only {} of {n} new sessions appeared within 15 s",
                new.len()
            );
        }
        thread::sleep(Duration::from_millis(300));
    };
    println!("new sessions ({}): {}", stamp(t0), new_ids.join(", "));

    let (tx, rx) = mpsc::channel();
    let mut handles = Vec::new();
    for ((label, job), id) in jobs.into_iter().zip(new_ids.iter().cloned()) {
        let tx = tx.clone();
        handles.push(thread::spawn(move || {
            let _ = tx.send(worker(label, id, job, t0));
        }));
    }
    drop(tx);
    let mut outcomes: Vec<Outcome> = rx.iter().collect();
    for h in handles {
        let _ = h.join();
    }
    outcomes.sort_by(|a, b| a.session_id.cmp(&b.session_id));

    // close what we created
    for id in &new_ids {
        if let Err(e) = connection.close_session(id.clone()) {
            eprintln!("could not close {id}: {e}");
        }
    }

    println!("\njob       session                started    ended      result");
    let mut failed = n.saturating_sub(outcomes.len());
    for o in &outcomes {
        let r = match &o.result {
            Ok(()) => "ok".to_string(),
            Err(e) => {
                failed += 1;
                format!("ERROR: {e:#}")
            }
        };
        println!(
            "{:<9} {:<22} {:<10} {:<10} {}",
            o.label, o.session_id, o.started, o.ended, r
        );
    }
    if failed > 0 {
        bail!("{failed} job(s) failed");
    }
    println!("\nAll {n} jobs ran in parallel sessions.");
    Ok(())
}

/// `--test --tcodes=...`: only start each tcode, hold the screens for a few seconds.
pub fn run(tcodes_raw: &str) -> Result<()> {
    let tcodes = parse_tcodes(tcodes_raw)?;
    let jobs = tcodes
        .into_iter()
        .map(|t| {
            let tc = t.clone();
            let job: Job = Box::new(move |s: &GuiSession| {
                s.start_transaction(tc)?;
                thread::sleep(HOLD);
                Ok(())
            });
            (t, job)
        })
        .collect();
    run_jobs(jobs)
}

/// Validated report names of the repeated `--rname ... --multi-conn` (2..=6, known, no duplicates).
pub fn parse_rnames(raw: &[String]) -> Result<Vec<String>> {
    let mut out: Vec<String> = Vec::new();
    for n in raw {
        let name = crate::abc::dispatch::check_report(n.trim())
            .map_err(|e| anyhow!("Invalid --rname value: {e}"))?
            .to_string();
        if out.contains(&name) {
            bail!("--rname lists '{name}' twice");
        }
        out.push(name);
    }
    if out.len() < 2 || out.len() > MAX_SESSIONS {
        bail!(
            "--multi-conn needs 2 to {MAX_SESSIONS} reports, e.g. --rname xdock --rname k2xx (got {})",
            out.len()
        );
    }
    Ok(out)
}

/// `--rname a --rname b --run-date=D --multi-conn`: each report runs its normal single-run flow in its
/// own new session, all at the same time. Config and CLI overrides are process-wide and shared.
pub fn run_reports(
    rnames: &[String],
    date: &crate::abc::run::RunDate,
    skip_sap_check: bool,
) -> Result<()> {
    crate::utils::cli_overrides::set_unattended(true);
    let jobs = rnames
        .iter()
        .map(|r| {
            let (name, d) = (r.clone(), *date);
            let job: Job = Box::new(move |s: &GuiSession| {
                let (a, b) = d.as_range();
                let when = if a == b {
                    a.to_string()
                } else {
                    format!("{a}..{b}")
                };
                crate::logging::task(&format!("{name} for {when}"));
                crate::utils::unattended_runner::run_rname_unattended(s, &name, &d, skip_sap_check)
            });
            (r.clone(), job)
        })
        .collect();
    run_jobs(jobs)
}

/// `--rname=X --run-date=A..B --loop --conn-count=N`: N new sessions stay open and take the days
/// of the range from a shared queue (a free session starts the next day, sessions are reused,
/// not respawned). The merge, when requested, runs once afterwards without SAP. Same policy as
/// the sequential loop: a day without data is skipped, any other error stops new days and
/// there is no merge.
pub fn run_loop_parallel(
    rname: &str,
    range: &crate::abc::run::RunDate,
    conn_count: Option<usize>,
    opts: crate::abc::iterate::IterOpts,
    today: chrono::NaiveDate,
) -> Result<()> {
    use crate::abc::iterate::{finish_parallel, offline_runner, parallel_worker, ParallelShared};
    use std::sync::Arc;

    crate::utils::cli_overrides::set_unattended(true);
    let days = range.days();
    if days.len() < 2 {
        bail!("parallel days need a range or week with --loop");
    }
    // explicit `--conn-count`: exactly that many (never more than days); `--multi-conn`: one per
    // day, limited by the free session slots of the connection (6 minus the open windows)
    let workers = match conn_count {
        Some(n) => n.min(days.len()),
        None => {
            let free = MAX_SESSIONS.saturating_sub(open_session_count()?);
            if free < 2 {
                bail!(
                    "only {free} free session slot(s) (SAP allows {MAX_SESSIONS} per connection); \
                     close some windows"
                );
            }
            days.len().min(free)
        }
    };
    let shared = Arc::new(ParallelShared::new(&days));
    let jobs = (1..=workers)
        .map(|i| {
            let (shared, name) = (Arc::clone(&shared), rname.to_string());
            let job: Job = Box::new(move |s: &GuiSession| {
                let mut runner = crate::abc::iterate::RealDayRunner { session: s };
                parallel_worker(&shared, &mut runner, &name, opts, today);
                Ok(())
            });
            (format!("conn{i}"), job)
        })
        .collect();
    println!(
        "Running '{rname}' over {} day(s) on {workers} parallel session(s)...",
        days.len()
    );
    run_jobs(jobs)?;
    let shared = Arc::try_unwrap(shared)
        .map_err(|_| anyhow!("internal error: a worker still holds the shared state"))?;
    let _log = crate::logging::start();
    finish_parallel(shared, &mut offline_runner(), rname, range, opts, today).map(|_| ())
}

/// `--tcode lt03 --multi-conn | --conn-count=N`: the shipments are work units on a shared queue.
/// N sessions (exactly `--conn-count`, never more than shipments; with `--multi-conn` one per
/// shipment limited by the free session slots) each take the next shipment when they are free, so
/// 3 shipments on `--conn-count=2` use two sessions for the first two shipments and the first
/// session that finishes takes the third. A unit that fails stops new units from starting (the
/// running ones finish); a shipment without data is skipped. The log, results and summary are
/// written once at the end, like a single-session run.
pub fn run_lt03_parallel(conn_count: Option<usize>) -> Result<()> {
    use crate::lt03_module::{
        finish_run, output_dir, plan_units, read_done_deliveries, run_unit,
        settings_from_environment, shipments_from_environment, RealManualTo, RunReport, UnitReport,
        UnitStatus,
    };
    use std::collections::{HashSet, VecDeque};
    use std::sync::{Arc, Mutex};

    crate::utils::cli_overrides::set_unattended(true);
    let st = Arc::new(settings_from_environment()?);
    let shipments = shipments_from_environment(&st)?;
    let units = plan_units(&shipments, true);
    if units.len() < 2 {
        bail!(
            "parallel sessions need at least 2 shipments (got {})",
            units.len()
        );
    }
    let workers = match conn_count {
        Some(n) => n.min(units.len()),
        None => {
            let free = MAX_SESSIONS.saturating_sub(open_session_count()?);
            if free < 2 {
                bail!(
                    "only {free} free session slot(s) (SAP allows {MAX_SESSIONS} per connection);                      close some windows"
                );
            }
            units.len().min(free)
        }
    };
    let skip = Arc::new(if st.resume && !st.lt.dry_run {
        read_done_deliveries(&output_dir(&st.reports_dir, st.lt.today))
    } else {
        HashSet::new()
    });

    struct Shared {
        queue: VecDeque<(usize, crate::lt03_module::Unit)>,
        reports: Vec<(usize, UnitReport)>,
        abort: bool,
    }
    let shared = Arc::new(Mutex::new(Shared {
        queue: units.iter().cloned().enumerate().collect(),
        reports: Vec::new(),
        abort: false,
    }));
    let mut first = crate::lt03_runner::RunLog::default();
    crate::lt03_module::announce_settings(&st, &mut first);
    let log = Arc::new(Mutex::new(first));

    let jobs = (1..=workers)
        .map(|i| {
            let (st, skip, shared, log) = (
                Arc::clone(&st),
                Arc::clone(&skip),
                Arc::clone(&shared),
                Arc::clone(&log),
            );
            let job: Job = Box::new(move |session: &GuiSession| {
                let mut sap = RealManualTo::new(session, &st);
                loop {
                    let next = {
                        let mut g = shared.lock().unwrap();
                        if g.abort {
                            None
                        } else {
                            g.queue.pop_front()
                        }
                    };
                    let Some((idx, unit)) = next else { break };
                    crate::logging::task(&format!("shipment {}", unit.tag));
                    let mut local = crate::lt03_runner::RunLog::default();
                    let rep = run_unit(&mut sap, &st, &unit, &skip, &mut local);
                    log.lock().unwrap().lines.extend(local.lines);
                    let mut g = shared.lock().unwrap();
                    if matches!(rep.status, UnitStatus::Failed(_)) {
                        g.abort = true;
                    }
                    g.reports.push((idx, rep));
                }
                Ok(())
            });
            (format!("conn{i}"), job)
        })
        .collect();
    println!(
        "Running LT03 for {} shipment(s) on {workers} parallel session(s)...",
        units.len()
    );
    run_jobs(jobs)?;

    let mut shared = Arc::try_unwrap(shared)
        .map_err(|_| anyhow!("internal error: a worker still holds the shared state"))?
        .into_inner()
        .map_err(|_| anyhow!("internal error: poisoned lock"))?;
    shared.reports.sort_by_key(|(i, _)| *i);
    let mut log = Arc::try_unwrap(log)
        .map_err(|_| anyhow!("internal error: a worker still holds the log"))?
        .into_inner()
        .map_err(|_| anyhow!("internal error: poisoned lock"))?;
    log.lines.sort_by_key(|l| l.at);
    let rep = RunReport {
        units: shared.reports.into_iter().map(|(_, r)| r).collect(),
        dry_run: st.lt.dry_run,
        today: st.lt.today,
    };
    let _log = crate::logging::start();
    finish_run(&st, &rep, &log)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_uppercases() {
        assert_eq!(parse_tcodes("zvt11, vt11").unwrap(), vec!["ZVT11", "VT11"]);
    }

    #[test]
    fn rejects_wrong_counts() {
        assert!(parse_tcodes("vt11").is_err());
        assert!(parse_tcodes("a,b,c,d,e,f,g").is_err());
        assert!(parse_tcodes("a,,").is_err());
    }

    #[test]
    fn splits_on_comma_and_pipe() {
        assert_eq!(split_list("xdock|k2xx"), vec!["xdock", "k2xx"]);
        assert_eq!(split_list(" a , b,"), vec!["a", "b"]);
    }

    #[test]
    fn rnames_validated() {
        let v = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(
            parse_rnames(&v(&["xdock", "k2xx"])).unwrap(),
            vec!["xdock", "k2xx"]
        );
        assert!(parse_rnames(&v(&["xdock"])).is_err());
        assert!(parse_rnames(&v(&["xdock", "xdock"])).is_err());
        assert!(parse_rnames(&v(&["xdock", "nope"])).is_err());
    }

    #[test]
    fn session_tags() {
        assert_eq!(session_tag("/app/con[0]/ses[3]"), "s3");
        assert_eq!(session_tag("weird id"), "weirdid");
    }

    #[test]
    fn session_limit() {
        assert!(check_limit(1, 2).is_ok());
        assert!(check_limit(1, 5).is_ok());
        assert!(check_limit(2, 5).is_err());
    }
}
