//! Prompts inside the full-screen shell: suspend / resume handshake.
//!
//! The UI thread owns the terminal (alternate screen, raw mode, mouse capture). Worker threads
//! run the SAP actions and may reach a prompt (`dialoguer`, `read_line`, `rpassword`, the
//! `tui::show_*` dialogs) that needs the NORMAL terminal. [`with_prompt`] is the one central
//! mechanism: it asks the UI thread to give the terminal back, runs the prompt, and asks it to
//! take the terminal again.
//!
//! # Handshake (see [`Handshake`], a pure state machine)
//!
//! ```text
//!   worker                      UI thread
//!   ------                      ---------
//!   request_suspend  ------->   sees SuspendRequested (checked every loop pass, <= 30 ms)
//!   (waits, condvar)            leaves alt screen / raw mode / mouse capture, prints context
//!                    <-------   Suspended
//!   runs the prompt             blocked in `ui_service`: does NOT draw, does NOT read events
//!   request_resume   ------->   sees ResumeRequested
//!   (waits, condvar)            re-enters the shell, drains stale key events, full repaint
//!                    <-------   Running
//! ```
//!
//! # Why it cannot deadlock
//!
//! * Exactly one side acts in every phase: `Running` (nobody waits), `SuspendRequested` (UI acts,
//!   worker waits), `Suspended` (worker acts, UI waits), `ResumeRequested` (UI acts, worker waits).
//! * The UI never waits for anything but the worker's `request_resume`; the worker's resume is
//!   issued from a drop guard, so it also happens when the prompt panics or returns early.
//! * Every wait is a `Condvar::wait_timeout` loop that also checks `Closed`. The UI closes the gate
//!   (drop guard) when its loop ends or panics, which releases a worker that waits for a suspend
//!   that will never come; the worker then runs the prompt unsuspended (the terminal is already
//!   back to normal in that case).
//! * Prompts of several threads are serialised by a separate mutex that the UI never takes.
//! * A prompt reached from the UI thread itself, or nested inside another prompt of the same
//!   thread, runs directly (it would otherwise wait for itself).
//! * Without a gate (shell off) [`with_prompt`] is exactly `f()`.
#![allow(dead_code)]

use std::cell::Cell;
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::ThreadId;
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// Shell owns the terminal.
    Running,
    /// A worker wants the terminal; the UI has not yet given it back.
    SuspendRequested,
    /// The terminal is normal; the worker may prompt.
    Suspended,
    /// The worker is done; the UI has not yet taken the terminal back.
    ResumeRequested,
    /// The UI is gone; nothing is ever suspended again.
    Closed,
}

/// Pure state machine of the handshake (no threads, no terminal).
#[derive(Clone, Debug)]
pub struct Handshake {
    phase: Phase,
}

impl Default for Handshake {
    fn default() -> Self {
        Handshake::new()
    }
}

impl Handshake {
    pub fn new() -> Self {
        Handshake {
            phase: Phase::Running,
        }
    }

    pub fn phase(&self) -> Phase {
        self.phase
    }

    /// Worker: ask for the terminal. `false` when this is not possible (closed, or another
    /// prompt is in flight, which callers prevent by serialising).
    pub fn request_suspend(&mut self) -> bool {
        if self.phase == Phase::Running {
            self.phase = Phase::SuspendRequested;
            true
        } else {
            false
        }
    }

    pub fn ui_must_suspend(&self) -> bool {
        self.phase == Phase::SuspendRequested
    }

    /// UI: the terminal has been handed back.
    pub fn ui_suspended(&mut self) {
        if self.phase == Phase::SuspendRequested {
            self.phase = Phase::Suspended;
        }
    }

    pub fn is_suspended(&self) -> bool {
        self.phase == Phase::Suspended
    }

    /// Worker: the prompt is over (always called, also after a panic).
    pub fn request_resume(&mut self) {
        match self.phase {
            Phase::Suspended | Phase::SuspendRequested => self.phase = Phase::ResumeRequested,
            _ => {}
        }
    }

    pub fn ui_must_resume(&self) -> bool {
        self.phase == Phase::ResumeRequested
    }

    /// UI: the shell owns the terminal again.
    pub fn ui_resumed(&mut self) {
        if self.phase == Phase::ResumeRequested {
            self.phase = Phase::Running;
        }
    }

    /// UI is gone for good.
    pub fn close(&mut self) {
        self.phase = Phase::Closed;
    }

    pub fn is_closed(&self) -> bool {
        self.phase == Phase::Closed
    }
}

const WAIT_SLICE: Duration = Duration::from_millis(25);

/// The handshake plus its synchronisation primitives.
pub struct PromptGate {
    hs: Mutex<Handshake>,
    cv: Condvar,
    ui_thread: Mutex<Option<ThreadId>>,
    serial: Mutex<()>,
}

fn relock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

impl Default for PromptGate {
    fn default() -> Self {
        PromptGate::new()
    }
}

impl PromptGate {
    pub fn new() -> Self {
        PromptGate {
            hs: Mutex::new(Handshake::new()),
            cv: Condvar::new(),
            ui_thread: Mutex::new(None),
            serial: Mutex::new(()),
        }
    }

    pub fn phase(&self) -> Phase {
        relock(&self.hs).phase()
    }

    /// Remember the calling thread as the UI thread (prompts from it run directly).
    pub fn set_ui_thread(&self) {
        *relock(&self.ui_thread) = Some(std::thread::current().id());
    }

    pub fn is_ui_thread(&self) -> bool {
        *relock(&self.ui_thread) == Some(std::thread::current().id())
    }

    /// Worker side: block until the UI has handed the terminal back. `false` = closed or busy;
    /// run the prompt without suspension.
    pub fn enter(&self) -> bool {
        let mut hs = relock(&self.hs);
        if !hs.request_suspend() {
            return false;
        }
        self.cv.notify_all();
        loop {
            if hs.is_suspended() {
                return true;
            }
            if hs.is_closed() {
                return false;
            }
            hs = self
                .cv
                .wait_timeout(hs, WAIT_SLICE)
                .unwrap_or_else(|p| p.into_inner())
                .0;
        }
    }

    /// Worker side: the prompt is over; block until the UI owns the terminal again.
    pub fn leave(&self) {
        let mut hs = relock(&self.hs);
        hs.request_resume();
        self.cv.notify_all();
        while !(hs.phase() == Phase::Running || hs.is_closed()) {
            hs = self
                .cv
                .wait_timeout(hs, WAIT_SLICE)
                .unwrap_or_else(|p| p.into_inner())
                .0;
        }
    }

    /// UI side, called once per loop pass: if a worker wants the terminal, run `suspend`, wait
    /// (not drawing, not reading events) until the worker is done, then run `resume`.
    /// Returns `Ok(true)` when a suspension happened. The state machine always completes, even
    /// when `suspend` / `resume` fail (the first error is returned afterwards), so a terminal
    /// error can never leave the worker waiting.
    pub fn ui_service(
        &self,
        suspend: impl FnOnce() -> std::io::Result<()>,
        resume: impl FnOnce() -> std::io::Result<()>,
    ) -> std::io::Result<bool> {
        if !relock(&self.hs).ui_must_suspend() {
            return Ok(false);
        }
        let first = suspend();
        {
            let mut hs = relock(&self.hs);
            hs.ui_suspended();
            self.cv.notify_all();
            while !(hs.ui_must_resume() || hs.is_closed()) {
                hs = self
                    .cv
                    .wait_timeout(hs, WAIT_SLICE)
                    .unwrap_or_else(|p| p.into_inner())
                    .0;
            }
        }
        let second = resume();
        {
            let mut hs = relock(&self.hs);
            hs.ui_resumed();
            self.cv.notify_all();
        }
        first.and(second).map(|_| true)
    }

    /// UI side: the loop is over. Releases any waiting worker.
    pub fn close(&self) {
        relock(&self.hs).close();
        self.cv.notify_all();
    }

    fn serial_lock(&self) -> MutexGuard<'_, ()> {
        relock(&self.serial)
    }
}

/// Closes the gate when dropped (also when the UI thread unwinds).
pub struct CloseOnDrop(pub Arc<PromptGate>);

impl Drop for CloseOnDrop {
    fn drop(&mut self) {
        self.0.close();
    }
}

static GATE: Mutex<Option<Arc<PromptGate>>> = Mutex::new(None);

/// The gate of the running shell, if any.
pub fn current_gate() -> Option<Arc<PromptGate>> {
    relock(&GATE).clone()
}

pub(crate) fn set_gate(g: Option<Arc<PromptGate>>) {
    *relock(&GATE) = g;
}

thread_local! {
    static IN_PROMPT: Cell<bool> = const { Cell::new(false) };
}

struct Leave<'a>(&'a PromptGate);

impl Drop for Leave<'_> {
    fn drop(&mut self) {
        IN_PROMPT.with(|c| c.set(false));
        self.0.leave();
    }
}

/// Run a prompt that needs the normal terminal. With the shell up the UI thread gives the
/// terminal back for the duration of `f` (see the module docs); without the shell this is `f()`.
/// `f` gets `true` when the terminal was suspended.
pub fn with_prompt_ctx<R>(f: impl FnOnce(bool) -> R) -> R {
    let Some(gate) = current_gate() else {
        return f(false);
    };
    if gate.is_ui_thread() || IN_PROMPT.with(Cell::get) {
        return f(false);
    }
    let _serial = gate.serial_lock();
    if !gate.enter() {
        return f(false);
    }
    IN_PROMPT.with(|c| c.set(true));
    let _leave = Leave(&gate);
    // the plain screen has no header / footer: say where we are (config section)
    if let Some(banner) = super::active_shell().and_then(|sh| super::lock(&sh).context_banner()) {
        use std::io::Write;
        ::std::println!("{banner}");
        let _ = std::io::stdout().flush();
    }
    f(true)
}

/// [`with_prompt_ctx`] without the flag.
pub fn with_prompt<R>(f: impl FnOnce() -> R) -> R {
    with_prompt_ctx(|_| f())
}

// ---- text written with print! while the shell is up (a prompt that was never newline-ended) ----

static PENDING: Mutex<String> = Mutex::new(String::new());

/// Append `print!` text that has no newline yet (shell mode only).
pub fn push_pending(text: &str) {
    relock(&PENDING).push_str(text);
}

/// Take (and clear) the unfinished line.
pub fn take_pending() -> String {
    std::mem::take(&mut *relock(&PENDING))
}

/// Pure rule: a prompt can only be answered with a terminal on stdin AND stdout, and never in
/// `--json` mode (a calling program reads stdout and cannot type).
pub fn prompts_possible(stdin_tty: bool, stdout_tty: bool, json: bool) -> bool {
    stdin_tty && stdout_tty && !json
}

/// [`prompts_possible`] for this process.
pub fn prompts_allowed() -> bool {
    use std::io::IsTerminal;
    prompts_possible(
        std::io::stdin().is_terminal(),
        std::io::stdout().is_terminal(),
        crate::utils::cli_overrides::cli_overrides().lt03.json,
    )
}

/// The error every prompt returns instead of blocking when [`prompts_allowed`] is false.
pub fn no_prompt_error() -> std::io::Error {
    std::io::Error::other(
        "input needed but no terminal is attached (stdin/stdout piped or --json):          pass the value as a flag or set it in config.toml",
    )
}

/// Guard of every prompt entry point: `Err` instead of waiting on a stdin nobody types on.
/// A running shell always means terminals, so it is never refused.
pub fn ensure_can_prompt() -> std::io::Result<()> {
    if current_gate().is_some() || prompts_allowed() {
        Ok(())
    } else {
        Err(no_prompt_error())
    }
}

/// `stdin().read_line` for prompts. The `print!` text written just before it (the prompt) is
/// shown on the normal screen, the read happens while the terminal is suspended.
pub fn read_line(buf: &mut String) -> std::io::Result<usize> {
    ensure_can_prompt()?;
    let prompt = take_pending();
    let start = buf.len();
    let r = with_prompt_ctx(|_| {
        if !prompt.is_empty() {
            use std::io::Write;
            ::std::print!("{prompt}");
            let _ = std::io::stdout().flush();
        }
        std::io::stdin().read_line(buf)
    });
    if !prompt.is_empty() && crate::logging::shell_sink_active() {
        // keep the exchange visible in the log pane
        let answer = buf[start..].trim_end();
        crate::logging::emit(false, format_args!("{prompt}{answer}"));
    }
    r
}

/// `rpassword::read_password` for prompts (same suspend / pending-prompt handling as
/// [`read_line`]; the typed password is never logged).
pub fn read_password() -> std::io::Result<String> {
    ensure_can_prompt()?;
    let prompt = take_pending();
    with_prompt_ctx(|_| {
        if !prompt.is_empty() {
            use std::io::Write;
            ::std::print!("{prompt}");
            let _ = std::io::stdout().flush();
        }
        rpassword::read_password()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_matrix() {
        assert!(prompts_possible(true, true, false));
        assert!(!prompts_possible(false, true, false));
        assert!(!prompts_possible(true, false, false));
        assert!(!prompts_possible(true, true, true));
    }

    #[test]
    fn prompts_error_instead_of_blocking_without_a_terminal() {
        // cargo test runs with stdin not a terminal (or captured); no gate is installed
        if !prompts_allowed() {
            let mut buf = String::new();
            let e = read_line(&mut buf).unwrap_err();
            assert!(e.to_string().contains("no terminal"));
            assert!(read_password().is_err());
        }
    }
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    #[test]
    fn state_machine_happy_path() {
        let mut h = Handshake::new();
        assert_eq!(h.phase(), Phase::Running);
        assert!(!h.ui_must_suspend());
        assert!(h.request_suspend());
        assert!(h.ui_must_suspend());
        assert!(!h.request_suspend(), "one prompt at a time");
        h.ui_suspended();
        assert!(h.is_suspended());
        h.request_resume();
        assert!(h.ui_must_resume());
        h.ui_resumed();
        assert_eq!(h.phase(), Phase::Running);
    }

    #[test]
    fn state_machine_ignores_out_of_order_calls_and_closes() {
        let mut h = Handshake::new();
        h.ui_suspended();
        h.request_resume();
        h.ui_resumed();
        assert_eq!(h.phase(), Phase::Running, "no-ops from Running");
        assert!(h.request_suspend());
        h.close();
        assert!(h.is_closed());
        assert!(!h.request_suspend());
        h.ui_suspended();
        h.ui_resumed();
        assert!(h.is_closed(), "closed is terminal");
    }

    #[test]
    fn gate_without_ui_thread_runs_unsuspended_after_close() {
        let g = Arc::new(PromptGate::new());
        g.close();
        assert!(!g.enter());
        g.leave(); // must not hang
    }

    /// Fake UI loop on its own thread, worker prompts on this one.
    #[test]
    fn threaded_handshake_orders_events_and_never_overlaps() {
        let gate = Arc::new(PromptGate::new());
        let log = Arc::new(Mutex::new(Vec::<&'static str>::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let ui = {
            let (gate, log, stop) = (gate.clone(), log.clone(), stop.clone());
            std::thread::spawn(move || {
                gate.set_ui_thread();
                let _close = CloseOnDrop(gate.clone());
                while !stop.load(Ordering::Acquire) {
                    let l1 = log.clone();
                    let l2 = log.clone();
                    let _ = gate.ui_service(
                        move || {
                            l1.lock().unwrap().push("ui-suspend");
                            Ok(())
                        },
                        move || {
                            l2.lock().unwrap().push("ui-resume");
                            Ok(())
                        },
                    );
                    std::thread::sleep(Duration::from_millis(2));
                }
            })
        };
        // the worker side uses the gate directly (the global gate is not touched by this test)
        for _ in 0..3 {
            assert!(gate.enter());
            log.lock().unwrap().push("prompt");
            gate.leave();
        }
        stop.store(true, Ordering::Release);
        ui.join().unwrap();
        let l = log.lock().unwrap().clone();
        assert_eq!(
            l,
            [
                "ui-suspend",
                "prompt",
                "ui-resume",
                "ui-suspend",
                "prompt",
                "ui-resume",
                "ui-suspend",
                "prompt",
                "ui-resume"
            ]
        );
    }

    #[test]
    fn closing_releases_a_waiting_worker_and_terminal_errors_do_not_hang() {
        // UI never services: close() must free the worker
        let gate = Arc::new(PromptGate::new());
        let g2 = gate.clone();
        let t = std::thread::spawn(move || g2.enter());
        std::thread::sleep(Duration::from_millis(60));
        gate.close();
        assert!(!t.join().unwrap());

        // suspend/resume closures failing still complete the handshake
        let gate = Arc::new(PromptGate::new());
        let n = Arc::new(AtomicUsize::new(0));
        let ui = {
            let (gate, n) = (gate.clone(), n.clone());
            std::thread::spawn(move || {
                let _c = CloseOnDrop(gate.clone());
                loop {
                    let r = gate.ui_service(
                        || Err(std::io::Error::other("boom")),
                        || Err(std::io::Error::other("boom2")),
                    );
                    if r.is_err() {
                        n.fetch_add(1, Ordering::SeqCst);
                        return;
                    }
                    std::thread::sleep(Duration::from_millis(2));
                }
            })
        };
        assert!(gate.enter());
        gate.leave();
        ui.join().unwrap();
        assert_eq!(n.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn with_prompt_is_plain_without_gate_and_pending_text_roundtrips() {
        // tests in this module never install the global gate
        assert!(current_gate().is_none());
        assert_eq!(with_prompt(|| 5), 5);
        with_prompt_ctx(|s| assert!(!s));
        push_pending("a");
        push_pending("b");
        assert_eq!(take_pending(), "ab");
        assert_eq!(take_pending(), "");
    }
}
