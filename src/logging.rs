//! Opt-in asynchronous, non-blocking console logging for parallel runs.
//!
//! The crate shadows `println!` / `eprintln!` with macros that call [`emit`]. While logging is
//! NOT started (the default, every normal run) [`emit`] is exactly `std::println!` /
//! `std::eprintln!`. After [`start`] (parallel runs only) lines are pushed on a channel and a
//! dedicated thread writes them through a buffered stdout, so SAP worker threads never wait for
//! the console (or for each other on the stdout lock). Lines of a worker thread get a `[s3]`
//! prefix (its session tag, see `sap_export_utils::set_session_tag`). The returned guard flushes
//! everything and switches back to direct printing when dropped.
//!
//! # Status panel
//!
//! When stdout is a terminal, the writer thread keeps a small grid pinned below the scrolling
//! output: a header (elapsed time, progress) and one row per session tag with its state, its
//! current task (`relabel for 2026-11-05`, set with [`task`]) and its latest log line. Before each
//! batch of lines the panel is erased, the lines are written and the panel is redrawn (also once a
//! second so the clock ticks). Not a terminal (pipe, file, CI) or `SAP_NO_STATUS` set: no panel,
//! the output is unchanged. [`status`] sets the state, [`progress`] the `done/total` counter.
//!
//! Declared first in `lib.rs` with `#[macro_use]` so the macros are in scope crate-wide. Plain
//! `print!` is not covered (it stays direct).

use std::collections::BTreeMap;
use std::fmt::Arguments;
use std::io::{IsTerminal, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

enum Msg {
    Line {
        stderr: bool,
        text: String,
        tag: String,
        last: String,
    },
    Status {
        tag: String,
        state: Option<String>,
        task: Option<String>,
    },
    Progress {
        done: usize,
        total: usize,
    },
    Begin,
    Finish(Sender<()>),
    Flush(Sender<()>),
}

static ENABLED: AtomicBool = AtomicBool::new(false);
static TX: OnceLock<Mutex<Sender<Msg>>> = OnceLock::new();

macro_rules! println {
    () => {
        $crate::logging::emit(false, format_args!(""))
    };
    ($($arg:tt)*) => {
        $crate::logging::emit(false, format_args!($($arg)*))
    };
}

macro_rules! eprintln {
    () => {
        $crate::logging::emit(true, format_args!(""))
    };
    ($($arg:tt)*) => {
        $crate::logging::emit(true, format_args!($($arg)*))
    };
}

/// Print one line (see the module docs). Not meant to be called directly.
pub fn emit(stderr: bool, args: Arguments<'_>) {
    if !ENABLED.load(Ordering::Acquire) {
        if stderr {
            ::std::eprintln!("{args}");
        } else {
            ::std::println!("{args}");
        }
        return;
    }
    let tag = crate::utils::sap_export_utils::session_tag_name();
    let body = args.to_string();
    let last = body.lines().last().unwrap_or("").trim().to_string();
    let text = if tag.is_empty() {
        body
    } else {
        body.lines()
            .map(|l| format!("[{tag}] {l}"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    if let Some(tx) = TX.get() {
        if let Ok(tx) = tx.lock() {
            if tx
                .send(Msg::Line {
                    stderr,
                    text,
                    tag: tag.clone(),
                    last,
                })
                .is_ok()
            {
                return;
            }
        }
    }
    // writer thread gone: never lose a line
    ::std::println!("{args}");
}

struct Row {
    state: String,
    task: String,
    last: String,
    since: Instant,
}

impl Default for Row {
    fn default() -> Self {
        Self {
            state: String::new(),
            task: String::new(),
            last: String::new(),
            since: Instant::now(),
        }
    }
}

/// The pinned status panel (see the module docs). All drawing state lives on the writer thread.
struct Panel {
    tty: bool,
    active: bool,
    t0: Instant,
    rows: BTreeMap<String, Row>,
    progress: Option<(usize, usize)>,
    drawn: usize,
}

fn mmss(secs: u64) -> String {
    format!("{:02}:{:02}", secs / 60, secs % 60)
}

fn fit(s: &str, w: usize) -> String {
    let n = s.chars().count();
    if n <= w {
        format!("{s:<w$}")
    } else {
        let mut t: String = s.chars().take(w.saturating_sub(1)).collect();
        t.push('~');
        t
    }
}

impl Panel {
    fn new() -> Self {
        Self {
            tty: std::io::stdout().is_terminal() && std::env::var_os("SAP_NO_STATUS").is_none(),
            active: false,
            t0: Instant::now(),
            rows: BTreeMap::new(),
            progress: None,
            drawn: 0,
        }
    }

    /// The panel as plain text lines (header first, then a column title and one row per session),
    /// each at most `width - 1` chars.
    fn render(&self, width: usize, max_rows: usize) -> Vec<String> {
        let secs = self.t0.elapsed().as_secs();
        let running = self
            .rows
            .values()
            .filter(|r| !matches!(r.state.as_str(), "done" | "FAILED"))
            .count();
        let mut head = format!(" status | {}", mmss(secs));
        if let Some((d, t)) = self.progress {
            head.push_str(&format!(" | days {d}/{t}"));
        }
        head.push_str(&format!(
            " | {running}/{} sessions active ",
            self.rows.len()
        ));
        let w = width.saturating_sub(1).max(40);
        // columns: session 4, state 8, task 28, time 5, rest = latest line
        let rest = w.saturating_sub(4 + 8 + 28 + 5 + 4).max(5);
        let mut lines = vec![head];
        lines.push(format!(
            "{} {} {} {} {}",
            fit("ses", 4),
            fit("state", 8),
            fit("task", 28),
            fit("time", 5),
            fit("latest", rest)
        ));
        for (tag, r) in self.rows.iter().take(max_rows) {
            let t = if matches!(r.state.as_str(), "done" | "FAILED") {
                String::new()
            } else {
                mmss(r.since.elapsed().as_secs())
            };
            lines.push(format!(
                "{} {} {} {} {}",
                fit(tag, 4),
                fit(&r.state, 8),
                fit(&r.task, 28),
                fit(&t, 5),
                fit(&r.last, rest)
            ));
        }
        lines
            .into_iter()
            .map(|l| fit(&l, w).trim_end().to_string())
            .collect()
    }

    fn erase(&mut self, out: &mut impl Write) {
        use crossterm::cursor::{MoveToColumn, MoveUp};
        use crossterm::terminal::{Clear, ClearType};
        if self.drawn > 0 {
            let _ = crossterm::queue!(
                out,
                MoveUp(self.drawn as u16),
                MoveToColumn(0),
                Clear(ClearType::FromCursorDown)
            );
            self.drawn = 0;
        }
    }

    fn draw(&mut self, out: &mut impl Write) {
        if !self.tty || !self.active || self.rows.is_empty() {
            return;
        }
        let (w, h) = crossterm::terminal::size().unwrap_or((110, 30));
        let lines = self.render(w as usize, (h as usize).saturating_sub(5).min(12));
        for (i, l) in lines.iter().enumerate() {
            if i < 2 {
                let _ = crossterm::queue!(
                    out,
                    crossterm::style::PrintStyledContent(crossterm::style::Stylize::reverse(
                        l.as_str()
                    ))
                );
            } else {
                let _ = write!(out, "{l}");
            }
            let _ = write!(out, "\r\n");
        }
        self.drawn = lines.len();
    }

    fn apply(&mut self, m: Msg, out: &mut impl Write) -> Option<Sender<()>> {
        match m {
            Msg::Line {
                stderr,
                text,
                tag,
                last,
            } => {
                if stderr {
                    let _ = out.flush();
                    let _ = writeln!(std::io::stderr(), "{text}");
                } else {
                    let _ = writeln!(out, "{text}");
                }
                if self.active && !tag.is_empty() {
                    let row = self.rows.entry(tag).or_default();
                    if row.state.is_empty() {
                        row.state = "running".into();
                    }
                    if !last.is_empty() {
                        row.last = last;
                    }
                }
                None
            }
            Msg::Status { tag, state, task } => {
                let row = self.rows.entry(tag).or_default();
                if let Some(s) = state {
                    row.state = s;
                }
                if let Some(t) = task {
                    row.task = t;
                    row.since = Instant::now();
                }
                None
            }
            Msg::Progress { done, total } => {
                self.progress = Some((done, total));
                None
            }
            Msg::Begin => {
                self.active = true;
                self.t0 = Instant::now();
                self.rows.clear();
                self.progress = None;
                None
            }
            Msg::Finish(ack) => {
                self.active = false;
                self.rows.clear();
                Some(ack)
            }
            Msg::Flush(ack) => Some(ack),
        }
    }
}

fn spawn_writer() -> Sender<Msg> {
    let (tx, rx) = mpsc::channel::<Msg>();
    std::thread::Builder::new()
        .name("log-writer".into())
        .spawn(move || {
            let mut out = std::io::BufWriter::with_capacity(64 * 1024, std::io::stdout());
            let mut panel = Panel::new();
            loop {
                let mut acks = Vec::new();
                if panel.tty {
                    // terminals that support it show erase+write+redraw as one frame
                    let _ = crossterm::queue!(out, crossterm::terminal::BeginSynchronizedUpdate);
                }
                match rx.recv_timeout(Duration::from_secs(1)) {
                    Ok(first) => {
                        // let a burst of lines from several sessions arrive, so it costs one
                        // erase/redraw instead of one per line (the visible flicker)
                        if panel.tty {
                            std::thread::sleep(Duration::from_millis(30));
                        }
                        panel.erase(&mut out);
                        acks.extend(panel.apply(first, &mut out));
                        while let Ok(more) = rx.try_recv() {
                            acks.extend(panel.apply(more, &mut out));
                        }
                    }
                    // idle: only refresh the clock
                    Err(mpsc::RecvTimeoutError::Timeout) => panel.erase(&mut out),
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                }
                panel.draw(&mut out);
                if panel.tty {
                    let _ = crossterm::queue!(out, crossterm::terminal::EndSynchronizedUpdate);
                }
                let _ = out.flush();
                for a in acks {
                    let _ = a.send(());
                }
            }
        })
        .expect("spawn log writer");
    tx
}

fn send(m: Msg) {
    if ENABLED.load(Ordering::Acquire) {
        if let Some(tx) = TX.get() {
            if let Ok(tx) = tx.lock() {
                let _ = tx.send(m);
            }
        }
    }
}

/// Set the state shown in the panel for the calling thread's session (`running`, `done`,
/// `FAILED`, ...). No effect outside parallel runs or on a thread without a session tag.
pub fn status(state: &str) {
    let tag = crate::utils::sap_export_utils::session_tag_name();
    if !tag.is_empty() {
        send(Msg::Status {
            tag,
            state: Some(state.to_string()),
            task: None,
        });
    }
}

/// Set the task text of the calling thread's session (`relabel for 2026-11-05`) and restart its
/// row timer.
pub fn task(text: &str) {
    let tag = crate::utils::sap_export_utils::session_tag_name();
    if !tag.is_empty() {
        send(Msg::Status {
            tag,
            state: None,
            task: Some(text.to_string()),
        });
    }
}

/// Set the `done/total` counter in the panel header.
pub fn progress(done: usize, total: usize) {
    send(Msg::Progress { done, total });
}

/// Switch to asynchronous logging until the guard is dropped.
pub fn start() -> LogGuard {
    let tx = TX.get_or_init(|| Mutex::new(spawn_writer()));
    let _ = tx; // writer thread lives for the process
    ENABLED.store(true, Ordering::Release);
    send(Msg::Begin);
    LogGuard
}

/// Wait until every queued line is written.
pub fn flush() {
    if let Some(tx) = TX.get() {
        let (ack_tx, ack_rx) = mpsc::channel();
        if let Ok(tx) = tx.lock() {
            if tx.send(Msg::Flush(ack_tx)).is_err() {
                return;
            }
        }
        let _ = ack_rx.recv_timeout(std::time::Duration::from_secs(5));
    }
}

pub struct LogGuard;

impl Drop for LogGuard {
    fn drop(&mut self) {
        // erase the panel and wait until everything is written
        if let Some(tx) = TX.get() {
            let (ack_tx, ack_rx) = mpsc::channel();
            let sent = tx
                .lock()
                .map(|tx| tx.send(Msg::Finish(ack_tx)).is_ok())
                .unwrap_or(false);
            if sent {
                let _ = ack_rx.recv_timeout(Duration::from_secs(5));
            }
        }
        ENABLED.store(false, Ordering::Release);
        flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panel_renders_header_and_rows_truncated() {
        let mut p = Panel::new();
        p.progress = Some((2, 8));
        p.rows.insert(
            "s3".into(),
            Row {
                state: "running".into(),
                task: "relabel for 2026-11-05".into(),
                last: "x".repeat(200),
                ..Row::default()
            },
        );
        p.rows.insert(
            "s4".into(),
            Row {
                state: "done".into(),
                ..Row::default()
            },
        );
        let l = p.render(100, 12);
        assert_eq!(l.len(), 4);
        assert!(l[0].contains("days 2/8") && l[0].contains("1/2 sessions"));
        assert!(l[2].starts_with("s3") && l[2].contains("relabel for 2026-11-05"));
        assert!(l.iter().all(|x| x.chars().count() <= 99));
    }
}
