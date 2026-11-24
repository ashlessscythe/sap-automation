//! Terminal setup/teardown: raw mode + alternate screen, restored on drop and on panic.
#![allow(dead_code)]

use crossterm::{
    cursor::{Hide, Show},
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io::{self, Stdout};
use std::sync::Once;

pub type ShellTerminal = Terminal<CrosstermBackend<Stdout>>;

/// Best-effort restore of the terminal (also used by the panic hook).
pub fn restore_terminal() {
    let _ = disable_raw_mode();
    let _ = execute!(
        io::stdout(),
        DisableMouseCapture,
        LeaveAlternateScreen,
        Show
    );
}

static HOOK: Once = Once::new();

/// Chain a panic hook that restores the terminal before the previous hook prints the message.
fn install_panic_hook() {
    HOOK.call_once(|| {
        let prev = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            restore_terminal();
            prev(info);
        }));
    });
}

/// Owns the terminal. Dropping it restores the normal screen.
pub struct TerminalGuard {
    terminal: ShellTerminal,
    active: bool,
}

impl TerminalGuard {
    pub fn new() -> io::Result<Self> {
        install_panic_hook();
        let mut guard = TerminalGuard {
            terminal: Terminal::new(CrosstermBackend::new(io::stdout()))?,
            active: false,
        };
        guard.enter()?;
        Ok(guard)
    }

    fn enter(&mut self) -> io::Result<()> {
        enable_raw_mode()?;
        execute!(io::stdout(), EnterAlternateScreen, EnableMouseCapture, Hide)?;
        self.terminal.clear()?;
        self.active = true;
        Ok(())
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn terminal(&mut self) -> &mut ShellTerminal {
        &mut self.terminal
    }

    /// Leave the alternate screen and raw mode (e.g. to run a dialoguer prompt).
    pub fn suspend(&mut self) -> io::Result<()> {
        if self.active {
            self.active = false;
            disable_raw_mode()?;
            execute!(
                io::stdout(),
                DisableMouseCapture,
                LeaveAlternateScreen,
                Show
            )?;
        }
        Ok(())
    }

    /// Re-enter the shell after [`suspend`](Self::suspend); the next draw repaints everything.
    pub fn resume(&mut self) -> io::Result<()> {
        if !self.active {
            self.enter()?;
        }
        Ok(())
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        if self.active {
            restore_terminal();
        }
    }
}
