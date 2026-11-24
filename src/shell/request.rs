//! Worker -> UI requests of an interactive shell session: a menu to choose from, or an
//! acknowledgement ("done / FAILED - press any key") after an action.
//!
//! The worker posts a [`UiRequest`] into the shared [`ShellState`] and blocks on a channel; the UI
//! thread shows it, and [`answer`]s through that channel. Because it is a plain channel, a closed
//! UI (the sender is dropped by [`cancel_pending`]) wakes the worker with "cancelled" instead of
//! hanging it.
#![allow(dead_code)]

use super::menu::{MenuSpec, UiRequest};
use super::state::SECTION_EXTRA;
use super::{active_shell, lock, prompt, Level, SharedShell};
use std::io;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Mutex, MutexGuard};

static REPLY: Mutex<Option<Sender<Option<usize>>>> = Mutex::new(None);
static REQ_LOCK: Mutex<()> = Mutex::new(());
static NEXT_ID: AtomicU64 = AtomicU64::new(1);
static INTERACTIVE: AtomicBool = AtomicBool::new(false);

fn relock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

/// Mark an interactive session as running (UI loop up, worker may post requests).
pub(crate) fn set_interactive(on: bool) {
    INTERACTIVE.store(on, Ordering::Release);
}

/// True when a worker thread may post requests (an interactive shell session is up).
pub fn requests_available() -> bool {
    INTERACTIVE.load(Ordering::Acquire)
        && active_shell().is_some()
        && !prompt::current_gate().is_none_or(|g| g.is_ui_thread())
}

/// Post `req`, block for the answer. `None` = cancelled / UI gone.
fn ask(req: UiRequest) -> Option<usize> {
    let sh = active_shell()?;
    let _one_at_a_time = relock(&REQ_LOCK);
    let (tx, rx) = channel();
    *relock(&REPLY) = Some(tx);
    lock(&sh).request = Some(req);
    rx.recv().ok().flatten()
}

/// UI side: deliver the answer of the current request (and clear it from the state).
pub fn answer(shell: &SharedShell, reply: Option<usize>) {
    lock(shell).request = None;
    if let Some(tx) = relock(&REPLY).take() {
        let _ = tx.send(reply);
    }
}

/// UI side: the loop is ending; release a worker that still waits.
pub fn cancel_pending(shell: &SharedShell) {
    answer(shell, None);
}

/// Menu inside the shell. `None` when no interactive shell session is up (caller falls back to
/// its own terminal menu); `Some(Ok(None))` = the user cancelled.
pub fn try_menu(
    title: &str,
    items: &[String],
    default: Option<usize>,
) -> Option<io::Result<Option<usize>>> {
    if !requests_available() {
        return None;
    }
    let spec = MenuSpec {
        title: title.to_string(),
        items: items.to_vec(),
        default,
        id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
    };
    Some(Ok(ask(UiRequest::Menu(spec))))
}

/// Short tcode-like chip for the header from a menu label: `VT11 - Auto Run` gives `VT11`.
pub fn tcode_from_label(label: &str) -> Option<String> {
    let (head, _) = label.split_once(" - ")?;
    let head = head.trim();
    (!head.is_empty() && head.chars().count() <= 20).then(|| head.to_string())
}

/// Section a main-menu label belongs to (`Configure ...` entries = the config editor).
pub fn section_from_label(label: &str) -> Option<&'static str> {
    label.starts_with("Configure ").then_some("config")
}

/// State of a running menu action (see [`begin_action`]).
pub struct ActionScope {
    errors_before: Option<usize>,
}

/// Switch the shell to its "running" view for the menu action `label`.
pub fn begin_action(label: &str) -> ActionScope {
    let Some(sh) = active_shell().filter(|_| INTERACTIVE.load(Ordering::Acquire)) else {
        return ActionScope {
            errors_before: None,
        };
    };
    let mut s = lock(&sh);
    match section_from_label(label) {
        Some(section) => {
            // header chip CONFIG, footer "section: config"; free-text prompts print a banner
            s.tcode = Some(section.to_uppercase());
            s.set_extra(SECTION_EXTRA, section);
        }
        None => {
            s.tcode = tcode_from_label(label);
            s.remove_extra(SECTION_EXTRA);
        }
    }
    s.command_line = format!("menu: {label}");
    s.started = std::time::Instant::now();
    s.finished_after = None;
    s.remove_extra("result");
    s.log(Level::Info, None, format!("--- {label} ---"));
    ActionScope {
        errors_before: Some(s.count(Level::Error)),
    }
}

/// Action finished: log done / FAILED (any error line since [`begin_action`] counts as failed),
/// wait for a key, then go back to the menu view. No-op outside an interactive shell.
pub fn end_action(scope: ActionScope) {
    let Some(before) = scope.errors_before else {
        return;
    };
    let Some(sh) = active_shell() else { return };
    let ok = {
        let mut s = lock(&sh);
        let ok = s.count(Level::Error) <= before;
        if ok {
            s.log(
                Level::Info,
                None,
                "Done - press any key / q to return to the menu",
            );
        } else {
            s.log(
                Level::Error,
                None,
                "FAILED - press any key / q to return to the menu",
            );
        }
        s.set_extra("result", if ok { "done" } else { "FAILED" });
        s.finish();
        ok
    };
    let _ = ask(UiRequest::Acknowledge { ok });
    let mut s = lock(&sh);
    s.tcode = None;
    s.command_line.clear();
    s.remove_extra("result");
    s.remove_extra(SECTION_EXTRA);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tcode_chip_from_label() {
        assert_eq!(
            tcode_from_label("VT11 - Auto Run (from config)").as_deref(),
            Some("VT11")
        );
        assert_eq!(
            tcode_from_label("149 Report - Material Not TSP").as_deref(),
            Some("149 Report")
        );
        assert_eq!(tcode_from_label("Run Loop (using config)"), None);
        assert_eq!(tcode_from_label(" - x"), None);
    }

    #[test]
    fn config_labels_are_a_section() {
        assert_eq!(section_from_label("Configure Loop"), Some("config"));
        assert_eq!(
            section_from_label("Configure Global and SAP Parameters"),
            Some("config")
        );
        assert_eq!(section_from_label("VT11 - Auto Run"), None);
        assert_eq!(section_from_label("Run Loop (using config)"), None);
    }

    #[test]
    fn nothing_available_without_a_session() {
        assert!(!requests_available());
        assert!(try_menu("t", &["a".into()], None).is_none());
        // begin/end are inert
        let sc = begin_action("VT11 - x");
        end_action(sc);
    }
}
