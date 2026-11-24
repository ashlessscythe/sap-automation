//! Persistent full-screen shell (header, command line, log window, status bar).
//!
//! Phase 1: self-contained foundation, not yet wired into the main flow. A UI thread owns the
//! [`TerminalGuard`] and calls [`run_event_loop`]; worker threads update the shared
//! [`ShellState`] through [`SharedShell`] (e.g. [`push_log`]).
#![allow(dead_code)]

pub mod dialog;
pub mod menu;
pub mod prompt;
pub mod request;
pub mod runner;
pub mod state;
pub mod terminal;
pub mod ui;

pub use menu::{MenuSpec, MenuView, UiRequest};
pub use prompt::{read_line, read_password, with_prompt, with_prompt_ctx};
pub use request::{begin_action, end_action, try_menu};
pub use runner::{run_with_shell, run_with_shell_opts, shell_enabled, shell_wanted, ShellOpts};
pub use state::{
    command_line_from, format_elapsed, quote_arg, truncate_ellipsis, Level, LogEntry, Mode,
    SapStatus, SessionRow, SessionSnapshot, ShellState, ViewState, LOG_CAP,
};
pub use terminal::{panicked, restore_terminal, ShellTerminal, TerminalGuard};
pub use ui::draw;

use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers, MouseEventKind};
use std::io;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

pub type SharedShell = Arc<Mutex<ShellState>>;

static ACTIVE: Mutex<Option<SharedShell>> = Mutex::new(None);

/// The shell that currently owns the screen (set by [`run_with_shell`]), if any.
pub fn active_shell() -> Option<SharedShell> {
    ACTIVE.lock().unwrap_or_else(|p| p.into_inner()).clone()
}

pub(crate) fn set_active_shell(s: Option<SharedShell>) {
    *ACTIVE.lock().unwrap_or_else(|p| p.into_inner()) = s;
}

/// Update the footer's SAP status; no-op when no shell is active.
pub fn set_sap_status(status: SapStatus) {
    if let Some(sh) = active_shell() {
        lock(&sh).sap = status;
    }
}

/// Pure gate: raw screen writes (clear, cursor moves) are allowed only when neither the log
/// sink nor an active shell owns the terminal.
pub fn raw_screen_allowed(sink_active: bool, shell_active: bool) -> bool {
    !sink_active && !shell_active
}

/// Run `f` (a raw terminal write such as a clear) only while the shell is NOT up; while it is,
/// such writes would corrupt the alternate screen, so `f` is skipped.
pub fn unless_shell(f: impl FnOnce()) {
    if raw_screen_allowed(
        crate::logging::shell_sink_active(),
        active_shell().is_some(),
    ) {
        f();
    }
}

/// Shared `clear_screen()` for the interactive modules: exactly the old
/// `execute!(stdout(), Clear(All))` when the shell is off, a no-op while it is up.
pub fn clear_screen() {
    unless_shell(|| {
        crossterm::execute!(
            io::stdout(),
            crossterm::terminal::Clear(crossterm::terminal::ClearType::All)
        )
        .unwrap();
    });
}

pub fn shared(state: ShellState) -> SharedShell {
    Arc::new(Mutex::new(state))
}

/// Lock, recovering from a poisoned mutex (a panicking worker must not blind the UI).
pub fn lock(shell: &SharedShell) -> MutexGuard<'_, ShellState> {
    shell.lock().unwrap_or_else(|p| p.into_inner())
}

/// Convenience for worker threads.
pub fn push_log(shell: &SharedShell, level: Level, tag: Option<&str>, text: impl Into<String>) {
    lock(shell).log(level, tag, text);
}

/// Apply one terminal event to the view. Returns true when a redraw is needed.
pub fn handle_event(ev: &Event, view: &mut ViewState) -> bool {
    match ev {
        Event::Resize(_, _) => true,
        Event::Mouse(m) => match m.kind {
            MouseEventKind::ScrollUp => {
                view.scroll_up(3);
                true
            }
            MouseEventKind::ScrollDown => {
                view.scroll_down(3);
                true
            }
            _ => false,
        },
        Event::Key(k) if k.kind == KeyEventKind::Press => {
            match k.code {
                KeyCode::Char('q') | KeyCode::Esc => view.quit = true,
                KeyCode::Char('c') if k.modifiers.contains(KeyModifiers::CONTROL) => {
                    view.quit = true
                }
                KeyCode::Up => view.scroll_up(1),
                KeyCode::Down => view.scroll_down(1),
                KeyCode::PageUp => view.page_up(),
                KeyCode::PageDown => view.page_down(),
                KeyCode::Home => view.home(),
                KeyCode::End => view.end(),
                KeyCode::Char('f') => view.toggle_follow(),
                _ => return false,
            }
            true
        }
        _ => false,
    }
}

/// Draw once from a snapshot of the shared state.
pub fn draw_once(
    term: &mut ShellTerminal,
    shell: &SharedShell,
    view: &mut ViewState,
) -> io::Result<()> {
    let mut snapshot = lock(shell).clone();
    // parallel session rows come from the shared logging board; draw itself stays pure
    snapshot.sessions = crate::logging::session_snapshot();
    term.draw(|f| draw(f, &snapshot, view))?;
    Ok(())
}

/// Redraw every `tick` and after every burst of events until `view.quit` or `should_stop()`.
/// State updates from other threads show up on the next tick.
pub fn run_event_loop(
    guard: &mut TerminalGuard,
    shell: &SharedShell,
    view: &mut ViewState,
    tick: Duration,
    mut should_stop: impl FnMut() -> bool,
) -> io::Result<()> {
    loop {
        draw_once(guard.terminal(), shell, view)?;
        if view.quit || should_stop() {
            return Ok(());
        }
        if event::poll(tick)? {
            // Drain everything pending so a burst of keys / resizes costs one redraw.
            loop {
                let ev = event::read()?;
                handle_event(&ev, view);
                if !event::poll(Duration::ZERO)? {
                    break;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEvent, KeyEventState};

    fn key(code: KeyCode) -> Event {
        Event::Key(KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        })
    }

    #[test]
    fn keys_drive_view() {
        let mut v = ViewState {
            page: 5,
            total_lines: 50,
            ..Default::default()
        };
        assert!(handle_event(&key(KeyCode::PageUp), &mut v));
        assert!(!v.follow);
        assert!(handle_event(&key(KeyCode::Char('f')), &mut v));
        assert!(v.follow);
        assert!(handle_event(&Event::Resize(10, 10), &mut v));
        assert!(!handle_event(&key(KeyCode::Char('z')), &mut v));
        handle_event(&key(KeyCode::Char('q')), &mut v);
        assert!(v.quit);
    }

    #[test]
    fn raw_screen_gate() {
        assert!(raw_screen_allowed(false, false));
        assert!(!raw_screen_allowed(true, false));
        assert!(!raw_screen_allowed(false, true));
        assert!(!raw_screen_allowed(true, true));
    }

    /// Tests that use the global ACTIVE slot run one at a time.
    static ACTIVE_SERIAL: Mutex<()> = Mutex::new(());

    #[test]
    fn unless_shell_skips_while_shell_is_active() {
        let _s = ACTIVE_SERIAL.lock().unwrap_or_else(|p| p.into_inner());
        // shell tests that touch the global ACTIVE slot live in this one test
        let mut ran = false;
        unless_shell(|| ran = true);
        assert!(ran, "shell off: runs");
        set_active_shell(Some(shared(ShellState::new("t", Mode::Interactive))));
        let mut ran = false;
        unless_shell(|| ran = true);
        clear_screen(); // must not touch the terminal (and must not panic)
        set_active_shell(None);
        assert!(!ran, "shell on: skipped");
    }

    #[test]
    fn menu_and_action_requests_roundtrip_between_worker_and_ui() {
        use std::time::Duration;
        let _s = ACTIVE_SERIAL.lock().unwrap_or_else(|p| p.into_inner());
        let _sink = crate::logging::attach_shell();
        let sh = shared(ShellState::new("t", Mode::Interactive));
        set_active_shell(Some(sh.clone()));
        prompt::set_gate(Some(Arc::new(prompt::PromptGate::new())));
        request::set_interactive(true);
        struct Cleanup;
        impl Drop for Cleanup {
            fn drop(&mut self) {
                request::set_interactive(false);
                prompt::set_gate(None);
                set_active_shell(None);
            }
        }
        let _cleanup = Cleanup;
        let wait_request = |sh: &SharedShell| {
            for _ in 0..500 {
                if lock(sh).request.is_some() {
                    return lock(sh).request.clone().unwrap();
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            panic!("no request posted");
        };
        let worker = std::thread::spawn(|| {
            let choice = try_menu("m", &["a".to_string(), "b".to_string()], Some(1))
                .expect("available")
                .unwrap();
            let scope = begin_action("VT11 - Auto");
            eprintln!("Error: boom");
            end_action(scope);
            let cancelled = try_menu("m2", &["a".to_string()], None).unwrap().unwrap();
            (choice, cancelled)
        });
        match wait_request(&sh) {
            UiRequest::Menu(spec) => {
                assert_eq!((spec.title.as_str(), spec.default), ("m", Some(1)))
            }
            other => panic!("{other:?}"),
        }
        request::answer(&sh, Some(1));
        // the action ended with an error line: FAILED acknowledge, header shows the chip meanwhile
        match wait_request(&sh) {
            UiRequest::Acknowledge { ok } => assert!(!ok),
            other => panic!("{other:?}"),
        }
        assert_eq!(lock(&sh).tcode.as_deref(), Some("VT11"));
        request::answer(&sh, Some(0));
        match wait_request(&sh) {
            UiRequest::Menu(spec) => assert_eq!(spec.title, "m2"),
            other => panic!("{other:?}"),
        }
        request::cancel_pending(&sh);
        assert_eq!(worker.join().unwrap(), (Some(1), None));
        assert!(lock(&sh).tcode.is_none());
    }

    #[test]
    fn config_action_sets_chip_and_dialog_select_uses_shell_menu() {
        let _s = ACTIVE_SERIAL.lock().unwrap_or_else(|p| p.into_inner());
        let _sink = crate::logging::attach_shell();
        let sh = shared(ShellState::new("t", Mode::Interactive));
        set_active_shell(Some(sh.clone()));
        prompt::set_gate(Some(Arc::new(prompt::PromptGate::new())));
        request::set_interactive(true);
        struct Cleanup;
        impl Drop for Cleanup {
            fn drop(&mut self) {
                request::set_interactive(false);
                prompt::set_gate(None);
                set_active_shell(None);
            }
        }
        let _cleanup = Cleanup;
        let _scope = begin_action("Configure Loop");
        assert_eq!(lock(&sh).tcode.as_deref(), Some("CONFIG"));
        assert!(lock(&sh).context_banner().is_some());
        let worker = std::thread::spawn(|| {
            dialog::Select::new()
                .with_prompt("Choose an option")
                .items(["a", "b", "c"])
                .default(1)
                .interact()
                .unwrap()
        });
        for _ in 0..500 {
            if lock(&sh).request.is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        match lock(&sh).request.clone() {
            Some(UiRequest::Menu(spec)) => {
                assert_eq!(spec.title, "Choose an option");
                assert_eq!(spec.items, ["a", "b", "c"]);
                assert_eq!(spec.default, Some(1));
            }
            other => panic!("{other:?}"),
        }
        request::answer(&sh, Some(2));
        assert_eq!(worker.join().unwrap(), 2);
        // leave without the acknowledge round trip
        end_action_state_reset(&sh);
    }

    fn end_action_state_reset(sh: &SharedShell) {
        let mut s = lock(sh);
        s.tcode = None;
        s.remove_extra("section");
    }

    #[test]
    fn shared_state_updates_from_helper() {
        let s = shared(ShellState::new("t", Mode::Interactive));
        push_log(&s, Level::Warn, Some("s1"), "hi");
        assert_eq!(lock(&s).count(Level::Warn), 1);
    }
}
