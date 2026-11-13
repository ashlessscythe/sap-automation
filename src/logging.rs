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
//! Declared first in `lib.rs` with `#[macro_use]` so the macros are in scope crate-wide. Plain
//! `print!` is not covered (it stays direct).

use std::fmt::Arguments;
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Mutex, OnceLock};

enum Msg {
    Line { stderr: bool, text: String },
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
            if tx.send(Msg::Line { stderr, text }).is_ok() {
                return;
            }
        }
    }
    // writer thread gone: never lose a line
    ::std::println!("{args}");
}

fn spawn_writer() -> Sender<Msg> {
    let (tx, rx) = mpsc::channel::<Msg>();
    std::thread::Builder::new()
        .name("log-writer".into())
        .spawn(move || {
            let mut out = std::io::BufWriter::with_capacity(64 * 1024, std::io::stdout());
            let write =
                |out: &mut std::io::BufWriter<std::io::Stdout>, m: Msg| -> Option<Sender<()>> {
                    match m {
                        Msg::Line {
                            stderr: false,
                            text,
                        } => {
                            let _ = writeln!(out, "{text}");
                            None
                        }
                        Msg::Line { stderr: true, text } => {
                            let _ = out.flush();
                            let _ = writeln!(std::io::stderr(), "{text}");
                            None
                        }
                        Msg::Flush(ack) => Some(ack),
                    }
                };
            while let Ok(first) = rx.recv() {
                let mut acks = Vec::new();
                acks.extend(write(&mut out, first));
                while let Ok(more) = rx.try_recv() {
                    acks.extend(write(&mut out, more));
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

/// Switch to asynchronous logging until the guard is dropped.
pub fn start() -> LogGuard {
    let tx = TX.get_or_init(|| Mutex::new(spawn_writer()));
    let _ = tx; // writer thread lives for the process
    ENABLED.store(true, Ordering::Release);
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
        flush();
        ENABLED.store(false, Ordering::Release);
        flush();
    }
}
