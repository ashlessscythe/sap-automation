//! Persistent full-screen shell (header, command line, log window, status bar).
//!
//! Phase 1: self-contained foundation, not yet wired into the main flow. A UI thread owns the
//! [`TerminalGuard`] and calls [`run_event_loop`]; worker threads update the shared
//! [`ShellState`] through [`SharedShell`] (e.g. [`push_log`]).
#![allow(dead_code)]

pub mod runner;
pub mod state;
pub mod terminal;
pub mod ui;

pub use runner::{run_with_shell, shell_enabled, shell_wanted};
pub use state::{
    command_line_from, format_elapsed, quote_arg, truncate_ellipsis, Level, LogEntry, Mode,
    SapStatus, SessionRow, SessionSnapshot, ShellState, ViewState, LOG_CAP,
};
pub use terminal::{restore_terminal, ShellTerminal, TerminalGuard};
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
        Event::Key(k) if k.kind != KeyEventKind::Release => {
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

    #[test]
    fn unless_shell_skips_while_shell_is_active() {
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
    fn shared_state_updates_from_helper() {
        let s = shared(ShellState::new("t", Mode::Interactive));
        push_log(&s, Level::Warn, Some("s1"), "hi");
        assert_eq!(lock(&s).count(Level::Warn), 1);
    }
}
