//! Run a unit of work under the full-screen shell (or plainly when the shell is not wanted).
//!
//! [`run_with_shell`] starts the [`TerminalGuard`], runs the work on a scoped worker thread and
//! drives the event loop on the calling thread. While the shell is up, every `println!` /
//! `eprintln!` line goes into the shell log ([`crate::logging::attach_shell`]). When the work is
//! done the window stays open until `q` (or closes at once with `SAP_SHELL_AUTOCLOSE`), then the
//! terminal is restored and the tail of the log is printed to the normal screen.
#![allow(dead_code)]

use super::{
    draw_once, handle_event, lock, set_active_shell, shared, Level, Mode, SharedShell, ShellState,
    TerminalGuard, ViewState,
};
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use std::io::IsTerminal;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

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
        if k.kind != KeyEventKind::Release
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

/// Run `work` under the shell when [`shell_wanted`], else plainly (output exactly as before).
/// The work gets the [`SharedShell`] (a throw-away one when the shell is off, so callers need
/// not branch). `Err` from the work is logged, shown as FAILED and returned.
pub fn run_with_shell<T: Send>(
    title: &str,
    mode: Mode,
    tcode: Option<String>,
    work: impl FnOnce(SharedShell) -> anyhow::Result<T> + Send,
) -> anyhow::Result<T> {
    let mut st = ShellState::new(title, mode).with_current_command_line();
    st.tcode = tcode;
    let sh = shared(st);
    if !shell_wanted() {
        return work(sh);
    }
    let mut guard = match TerminalGuard::new() {
        Ok(g) => g,
        Err(_) => return work(sh),
    };
    set_active_shell(Some(sh.clone()));
    let sink = crate::logging::attach_shell();
    let done = AtomicBool::new(false);
    let autoclose = std::env::var_os("SAP_SHELL_AUTOCLOSE").is_some();

    let result = std::thread::scope(|scope| {
        let worker = scope.spawn(|| {
            let r = work(sh.clone());
            {
                let mut s = lock(&sh);
                match &r {
                    Ok(_) => s.log(Level::Info, None, finish_message(true)),
                    Err(e) => {
                        s.log(Level::Error, None, format!("Error: {e:#}"));
                        s.log(Level::Error, None, finish_message(false));
                    }
                }
                s.set_extra("result", if r.is_ok() { "done" } else { "FAILED" });
                s.finish();
            }
            done.store(true, Ordering::Release);
            r
        });
        let mut view = ViewState::default();
        let tick = Duration::from_millis(100);
        let mut ui_ok = true;
        while ui_ok {
            if draw_once(guard.terminal(), &sh, &mut view).is_err() {
                break;
            }
            let finished = done.load(Ordering::Acquire);
            if finished && (autoclose || view.quit) {
                break;
            }
            match event::poll(tick) {
                Ok(true) => loop {
                    let Ok(ev) = event::read() else {
                        ui_ok = false;
                        break;
                    };
                    if is_ctrl_c(&ev) {
                        // raw mode: Ctrl-C is a key, not a signal. Abort like before.
                        super::restore_terminal();
                        std::process::exit(130);
                    }
                    handle_event(&ev, &mut view);
                    if !matches!(event::poll(Duration::ZERO), Ok(true)) {
                        break;
                    }
                },
                Ok(false) => {}
                Err(_) => ui_ok = false,
            }
            if view.quit && !done.load(Ordering::Acquire) {
                view.quit = false;
                lock(&sh).set_extra("hint", "q quits when the run is done (Ctrl-C aborts)");
            }
        }
        worker.join()
    });

    // leave the alternate screen first, then detach, then print what would otherwise be lost
    drop(guard);
    drop(sink);
    set_active_shell(None);
    let tail: Vec<String> = {
        let s = lock(&sh);
        let n = s.log_len();
        s.log_entries()
            .skip(n.saturating_sub(TAIL_LINES))
            .map(format_tail_line)
            .collect()
    };
    for l in &tail {
        println!("{l}");
    }
    match result {
        Ok(r) => r,
        Err(p) => std::panic::resume_unwind(p),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    }

    #[test]
    fn ctrl_c_detected() {
        use crossterm::event::{KeyEvent, KeyEventState};
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
}
