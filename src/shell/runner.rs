//! Run a unit of work under the full-screen shell (or plainly when the shell is not wanted).
//!
//! [`run_with_shell`] starts the [`TerminalGuard`], runs the work on a scoped worker thread and
//! drives the event loop on the calling thread. While the shell is up, every `println!` /
//! `eprintln!` line goes into the shell log ([`crate::logging::attach_shell`]). When the work is
//! done the window stays open until `q` (or closes at once with `SAP_SHELL_AUTOCLOSE`), then the
//! terminal is restored and the tail of the log is printed to the normal screen.
//!
//! [`run_with_shell_opts`] with [`ShellOpts::interactive`] is the interactive session: the shell
//! is entered once, the worker thread (which owns COM and the SAP session) posts menus and
//! acknowledgements through [`super::request`], and prompts reached by actions suspend / resume
//! the terminal through [`super::prompt`]. The event loop here is the only place that touches
//! the terminal.
#![allow(dead_code)]

use super::menu::{MenuEvent, UiRequest};
use super::prompt::{self, PromptGate};
use super::request;
use super::{
    draw_once, handle_event, lock, set_active_shell, shared, Level, Mode, SharedShell, ShellState,
    TerminalGuard, ViewState,
};
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use std::cell::RefCell;
use std::io::IsTerminal;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Log lines echoed to the normal screen after the shell closes.
pub const TAIL_LINES: usize = 40;

/// Pure decision: is the shell used? Needs a real terminal on stdin and stdout and none of the
/// opt-outs (`SAP_NO_STATUS`, `SAP_NO_SHELL`, `--json`).
pub fn shell_enabled(
    stdout_tty: bool,
    stdin_tty: bool,
    no_status: bool,
    no_shell: bool,
    json: bool,
) -> bool {
    stdout_tty && stdin_tty && !no_status && !no_shell && !json
}

/// [`shell_enabled`] for this process (terminals, environment, `--json`).
pub fn shell_wanted() -> bool {
    shell_enabled(
        std::io::stdout().is_terminal(),
        std::io::stdin().is_terminal(),
        std::env::var_os("SAP_NO_STATUS").is_some(),
        std::env::var_os("SAP_NO_SHELL").is_some(),
        crate::utils::cli_overrides::cli_overrides().lt03.json,
    )
}

fn is_ctrl_c(ev: &Event) -> bool {
    matches!(ev, Event::Key(k)
        if k.kind == KeyEventKind::Press
            && k.code == KeyCode::Char('c')
            && k.modifiers.contains(KeyModifiers::CONTROL))
}

/// Log text for the final state.
pub fn finish_message(ok: bool) -> &'static str {
    if ok {
        "Finished - press q to exit"
    } else {
        "FAILED - press q to exit"
    }
}

/// Plain one-line rendering of a log entry for the normal screen.
pub fn format_tail_line(e: &super::LogEntry) -> String {
    match &e.tag {
        Some(t) => format!("{} {} [{}] {}", e.timestamp, e.level.label(), t, e.text),
        None => format!("{} {} {}", e.timestamp, e.level.label(), e.text),
    }
}

/// How a shell session behaves.
#[derive(Clone, Copy, Debug)]
pub struct ShellOpts {
    /// Keep the window open after the work ended until `q` (unattended runs).
    pub hold_on_finish: bool,
    /// Worker may post menus / acknowledgements ([`super::request`]); interactive sessions.
    pub interactive: bool,
    /// Log lines echoed to the normal screen after the shell closes.
    pub tail_lines: usize,
}

impl ShellOpts {
    pub fn unattended() -> Self {
        ShellOpts {
            hold_on_finish: true,
            interactive: false,
            tail_lines: TAIL_LINES,
        }
    }

    pub fn interactive() -> Self {
        ShellOpts {
            hold_on_finish: false,
            interactive: true,
            tail_lines: 1,
        }
    }
}

/// Keys that return from the "done / FAILED" screen to the menu: everything except the scroll
/// keys (so the output can still be read), key releases and bare modifiers.
pub fn is_ack_key(ev: &Event) -> bool {
    match ev {
        Event::Key(k) if k.kind == KeyEventKind::Press => !matches!(
            k.code,
            KeyCode::Up
                | KeyCode::Down
                | KeyCode::PageUp
                | KeyCode::PageDown
                | KeyCode::Home
                | KeyCode::End
                | KeyCode::Char('f')
                | KeyCode::Char('?')
                | KeyCode::F(1)
                | KeyCode::Modifier(_)
                | KeyCode::Null
        ),
        _ => false,
    }
}

/// Route one terminal event according to what the worker waits for. Returns true when the screen
/// should be redrawn at once.
pub fn dispatch_event(ev: &Event, sh: &SharedShell, view: &mut ViewState) -> bool {
    // the help overlay swallows input: any key closes it
    if view.help {
        return match ev {
            Event::Key(k) if k.kind == KeyEventKind::Press => {
                view.help = false;
                true
            }
            Event::Resize(_, _) => true,
            _ => false,
        };
    }
    let req = lock(sh).request.clone();
    if is_help_key(ev, !view.menu.filtering || req.is_none()) {
        view.help = true;
        return true;
    }
    match req {
        Some(UiRequest::Menu(spec)) => {
            view.menu.sync(&spec);
            match view.menu.handle_event(ev, &spec) {
                MenuEvent::Choose(i) => {
                    request::answer(sh, Some(i));
                    view.end();
                    true
                }
                MenuEvent::Cancel => {
                    request::answer(sh, None);
                    view.end();
                    true
                }
                MenuEvent::Redraw => true,
                MenuEvent::None => false,
            }
        }
        Some(UiRequest::Acknowledge { .. }) => {
            if is_ack_key(ev) {
                request::answer(sh, Some(0));
                view.end();
                true
            } else {
                let r = handle_event(ev, view);
                view.quit = false;
                r
            }
        }
        None => handle_event(ev, view),
    }
}

/// `?` or F1 (the `?` only when it is not text being typed into the menu filter).
pub fn is_help_key(ev: &Event, allow_char: bool) -> bool {
    matches!(ev, Event::Key(k)
        if k.kind == KeyEventKind::Press
            && (k.code == KeyCode::F(1) || (allow_char && k.code == KeyCode::Char('?'))))
}

/// Throw away key events typed while the terminal belonged to a prompt.
fn drain_input() {
    while matches!(event::poll(Duration::ZERO), Ok(true)) {
        if event::read().is_err() {
            break;
        }
    }
}

/// Echo the log lines written since `*printed` to the normal screen (a prompt is about to run and
/// its context text would otherwise stay hidden in the shell's log pane).
fn echo_recent(sh: &SharedShell, printed: &mut u64) {
    let s = lock(sh);
    let new = s.total_logged.saturating_sub(*printed) as usize;
    *printed = s.total_logged;
    let n = s.log_len();
    let take = new.min(n).min(30);
    ::std::println!();
    for e in s.log_entries().skip(n - take) {
        ::std::println!("{}", format_tail_line(e));
    }
}

/// Ends the UI side for good (also when the UI thread unwinds): releases a worker that waits on
/// a menu / acknowledge request or on a prompt handshake, so `thread::scope` cannot hang.
struct ReleaseOnDrop<'a> {
    sh: &'a SharedShell,
    gate: Arc<PromptGate>,
}

impl Drop for ReleaseOnDrop<'_> {
    fn drop(&mut self) {
        request::set_interactive(false);
        request::cancel_pending(self.sh);
        self.gate.close();
    }
}

struct DoneOnDrop<'a> {
    done: &'a AtomicBool,
    crashed: &'a AtomicBool,
}

impl Drop for DoneOnDrop<'_> {
    fn drop(&mut self) {
        if std::thread::panicking() {
            self.crashed.store(true, Ordering::Release);
        }
        self.done.store(true, Ordering::Release);
    }
}

/// Run `work` under the shell when [`shell_wanted`], else plainly (output exactly as before).
/// The work gets the [`SharedShell`] (a throw-away one when the shell is off, so callers need
/// not branch). `Err` from the work is logged, shown as FAILED and returned.
pub fn run_with_shell<T: Send>(
    title: &str,
    mode: Mode,
    tcode: Option<String>,
    work: impl FnOnce(SharedShell) -> anyhow::Result<T> + Send,
) -> anyhow::Result<T> {
    run_with_shell_opts(title, mode, tcode, ShellOpts::unattended(), work)
}

/// [`run_with_shell`] with explicit [`ShellOpts`] (the interactive session uses
/// [`ShellOpts::interactive`]: one shell for the whole session, menus and actions on the worker).
pub fn run_with_shell_opts<T: Send>(
    title: &str,
    mode: Mode,
    tcode: Option<String>,
    opts: ShellOpts,
    work: impl FnOnce(SharedShell) -> anyhow::Result<T> + Send,
) -> anyhow::Result<T> {
    let mut st = ShellState::new(title, mode).with_current_command_line();
    st.tcode = tcode;
    if opts.interactive {
        st.command_line.clear();
    }
    let sh = shared(st);
    if !shell_wanted() {
        return work(sh);
    }
    let guard = match TerminalGuard::new() {
        Ok(g) => g,
        Err(_) => return work(sh),
    };
    let gate = Arc::new(PromptGate::new());
    gate.set_ui_thread();
    prompt::set_gate(Some(gate.clone()));
    request::set_interactive(opts.interactive);
    set_active_shell(Some(sh.clone()));
    let sink = crate::logging::attach_shell();
    let done = AtomicBool::new(false);
    let crashed = AtomicBool::new(false);
    let autoclose = std::env::var_os("SAP_SHELL_AUTOCLOSE").is_some();
    let guard = RefCell::new(guard);

    let result = std::thread::scope(|scope| {
        let worker = scope.spawn(|| {
            let _done = DoneOnDrop {
                done: &done,
                crashed: &crashed,
            };
            let r = work(sh.clone());
            {
                let mut s = lock(&sh);
                match &r {
                    Ok(_) if opts.hold_on_finish => s.log(Level::Info, None, finish_message(true)),
                    Ok(_) => {}
                    Err(e) => {
                        s.log(Level::Error, None, format!("Error: {e:#}"));
                        if opts.hold_on_finish {
                            s.log(Level::Error, None, finish_message(false));
                        }
                    }
                }
                s.set_extra("result", if r.is_ok() { "done" } else { "FAILED" });
                s.finish();
            }
            r
        });
        let _release = ReleaseOnDrop {
            sh: &sh,
            gate: gate.clone(),
        };
        let mut view = ViewState::default();
        let tick = Duration::from_millis(30);
        let redraw_every = Duration::from_millis(100);
        let mut last_draw: Option<Instant> = None;
        let mut dirty = true;
        let mut printed = lock(&sh).total_logged;
        let mut ui_ok = true;
        while ui_ok {
            // 1. a worker wants the normal terminal: hand it over and wait (no drawing, no input)
            let serviced = gate.ui_service(
                || {
                    guard.borrow_mut().suspend()?;
                    echo_recent(&sh, &mut printed);
                    Ok(())
                },
                || {
                    let r = guard.borrow_mut().resume();
                    drain_input();
                    r
                },
            );
            match serviced {
                Ok(true) => {
                    dirty = true;
                    last_draw = None;
                }
                Ok(false) => {}
                Err(_) => break,
            }
            // 2. draw (on change, and at least every 100 ms for the clock / new log lines)
            if dirty || last_draw.is_none_or(|t| t.elapsed() >= redraw_every) {
                if draw_once(guard.borrow_mut().terminal(), &sh, &mut view).is_err() {
                    break;
                }
                last_draw = Some(Instant::now());
                dirty = false;
            }
            let finished = done.load(Ordering::Acquire);
            if crashed.load(Ordering::Acquire)
                || super::panicked()
                || (finished && (autoclose || view.quit || !opts.hold_on_finish))
            {
                break;
            }
            // 3. input (a pending suspend request is noticed within one tick)
            match event::poll(tick) {
                Ok(true) => loop {
                    let Ok(ev) = event::read() else {
                        ui_ok = false;
                        break;
                    };
                    if is_ctrl_c(&ev) {
                        // raw mode: Ctrl-C is a key, not a signal. Abort like before.
                        super::restore_terminal();
                        ::std::eprintln!("Aborted (Ctrl-C)");
                        std::process::exit(130);
                    }
                    dirty |= dispatch_event(&ev, &sh, &mut view);
                    if !matches!(event::poll(Duration::ZERO), Ok(true)) {
                        break;
                    }
                },
                Ok(false) => {}
                Err(_) => ui_ok = false,
            }
            if view.quit && !done.load(Ordering::Acquire) {
                view.quit = false;
                lock(&sh).set_extra(
                    "hint",
                    if opts.interactive {
                        "action running - wait for it to finish (Ctrl-C aborts)"
                    } else {
                        "q quits when the run is done (Ctrl-C aborts)"
                    },
                );
                dirty = true;
            }
        }
        // release a worker that still waits on a request or a prompt; if it keeps running (the UI
        // failed), it continues on the normal terminal with plain output
        request::set_interactive(false);
        request::cancel_pending(&sh);
        gate.close();
        if !done.load(Ordering::Acquire) {
            let _ = guard.borrow_mut().suspend();
            crate::logging::detach_shell();
        }
        worker.join()
    });

    // leave the alternate screen first, then detach, then print what would otherwise be lost
    drop(guard);
    drop(sink);
    request::set_interactive(false);
    prompt::set_gate(None);
    set_active_shell(None);
    let tail: Vec<String> = {
        let s = lock(&sh);
        let n = s.log_len();
        s.log_entries()
            .skip(n.saturating_sub(opts.tail_lines))
            .map(format_tail_line)
            .collect()
    };
    for l in &tail {
        println!("{l}");
    }
    // an unfinished `print!` line (no newline yet) must not vanish with the shell
    let pending = prompt::take_pending();
    if !pending.is_empty() {
        println!("{pending}");
    }
    match result {
        Ok(r) => r,
        Err(p) => std::panic::resume_unwind(p),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEvent, KeyEventState};

    fn key_ev(c: KeyCode, kind: KeyEventKind) -> Event {
        Event::Key(KeyEvent {
            code: c,
            modifiers: KeyModifiers::NONE,
            kind,
            state: KeyEventState::NONE,
        })
    }

    #[test]
    fn enable_decision() {
        assert!(shell_enabled(true, true, false, false, false));
        assert!(!shell_enabled(false, true, false, false, false));
        assert!(!shell_enabled(true, false, false, false, false));
        assert!(!shell_enabled(true, true, true, false, false));
        assert!(!shell_enabled(true, true, false, true, false));
        assert!(!shell_enabled(true, true, false, false, true));
    }

    #[test]
    fn tail_line_format() {
        let e = super::super::LogEntry {
            level: Level::Warn,
            tag: Some("s2".into()),
            timestamp: "12:00:01".into(),
            text: "x".into(),
        };
        assert_eq!(format_tail_line(&e), "12:00:01 WRN [s2] x");
        let e = super::super::LogEntry { tag: None, ..e };
        assert_eq!(format_tail_line(&e), "12:00:01 WRN x");
    }

    #[test]
    fn plain_run_passes_result_through() {
        // test processes have no terminal on stdin/stdout: the shell is off
        let r = run_with_shell("t", Mode::Unattended, None, |_| Ok(7)).unwrap();
        assert_eq!(r, 7);
        let e = run_with_shell::<()>("t", Mode::Unattended, None, |_| anyhow::bail!("x"));
        assert!(e.is_err());
        let r = run_with_shell_opts(
            "t",
            Mode::Interactive,
            None,
            ShellOpts::interactive(),
            |_| Ok(1),
        )
        .unwrap();
        assert_eq!(r, 1);
    }

    #[test]
    fn ctrl_c_detected() {
        let k = |c, m| {
            Event::Key(KeyEvent {
                code: KeyCode::Char(c),
                modifiers: m,
                kind: KeyEventKind::Press,
                state: KeyEventState::NONE,
            })
        };
        assert!(is_ctrl_c(&k('c', KeyModifiers::CONTROL)));
        assert!(!is_ctrl_c(&k('c', KeyModifiers::NONE)));
        assert!(!is_ctrl_c(&k('q', KeyModifiers::CONTROL)));
    }

    #[test]
    fn ack_keys() {
        let p = KeyEventKind::Press;
        assert!(is_ack_key(&key_ev(KeyCode::Enter, p)));
        assert!(is_ack_key(&key_ev(KeyCode::Char('q'), p)));
        assert!(is_ack_key(&key_ev(KeyCode::Esc, p)));
        assert!(is_ack_key(&key_ev(KeyCode::Char(' '), p)));
        assert!(!is_ack_key(&key_ev(KeyCode::PageUp, p)));
        assert!(!is_ack_key(&key_ev(KeyCode::Char('f'), p)));
        assert!(!is_ack_key(&key_ev(KeyCode::Enter, KeyEventKind::Release)));
        assert!(!is_ack_key(&Event::Resize(1, 1)));
    }

    #[test]
    fn dispatch_menu_and_ack_answer_the_worker() {
        use crate::shell::menu::MenuSpec;
        let p = KeyEventKind::Press;
        let sh = shared(ShellState::new("t", Mode::Interactive));
        let mut view = ViewState::default();
        // menu: Down + Enter answers and clears the request
        lock(&sh).request = Some(UiRequest::Menu(MenuSpec {
            title: "m".into(),
            items: vec!["a".into(), "b".into()],
            default: None,
            id: 7,
        }));
        assert!(dispatch_event(&key_ev(KeyCode::Down, p), &sh, &mut view));
        assert!(lock(&sh).request.is_some(), "still waiting");
        assert!(dispatch_event(&key_ev(KeyCode::Enter, p), &sh, &mut view));
        assert!(lock(&sh).request.is_none());
        // ack: scrolling does not answer, a letter does
        lock(&sh).request = Some(UiRequest::Acknowledge { ok: true });
        dispatch_event(&key_ev(KeyCode::PageUp, p), &sh, &mut view);
        assert!(lock(&sh).request.is_some());
        assert!(!view.quit);
        dispatch_event(&key_ev(KeyCode::Char('q'), p), &sh, &mut view);
        assert!(lock(&sh).request.is_none());
        // help overlay: ? opens, any key closes without reaching the menu / quit logic
        assert!(dispatch_event(
            &key_ev(KeyCode::Char('?'), p),
            &sh,
            &mut view
        ));
        assert!(view.help);
        assert!(dispatch_event(
            &key_ev(KeyCode::Char('q'), p),
            &sh,
            &mut view
        ));
        assert!(!view.help && !view.quit);
        // key releases never act
        assert!(!dispatch_event(
            &key_ev(KeyCode::Char('q'), KeyEventKind::Release),
            &sh,
            &mut view
        ));
        assert!(!view.quit);
        // no request: q is the old "quit when done" key
        dispatch_event(&key_ev(KeyCode::Char('q'), p), &sh, &mut view);
        assert!(view.quit);
    }
}
