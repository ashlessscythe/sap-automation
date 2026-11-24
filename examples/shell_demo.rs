//! Interactive demo of the full-screen shell: fake log lines, resize, scroll, `q` quits.
use sap_automation::shell::{self, Level, Mode, SapStatus, ShellState, TerminalGuard, ViewState};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

fn main() -> std::io::Result<()> {
    let mut st =
        ShellState::new("SAP Automation (demo)", Mode::Unattended).with_current_command_line();
    st.tcode = Some("VL06O".into());
    st.sap = SapStatus::Connected(Some("DEMO/100".into()));
    let shared = shell::shared(st);
    let stop = Arc::new(AtomicBool::new(false));

    let worker = {
        let shared = shared.clone();
        let stop = stop.clone();
        thread::spawn(move || {
            let mut i = 0u32;
            while !stop.load(Ordering::Relaxed) {
                let (level, text) = match i % 17 {
                    5 => (Level::Warn, format!("slow response on step {i}")),
                    11 => (Level::Error, format!("simulated failure {i}")),
                    _ => (Level::Info, format!("exported part {i}")),
                };
                let tag = format!("s{}", i % 3 + 1);
                shell::push_log(&shared, level, Some(&tag), text);
                shell::lock(&shared).set_extra("days", format!("{}/100", i % 100));
                i += 1;
                thread::sleep(Duration::from_millis(150));
            }
        })
    };

    let mut guard = TerminalGuard::new()?;
    let mut view = ViewState::default();
    let res = shell::run_event_loop(
        &mut guard,
        &shared,
        &mut view,
        Duration::from_millis(200),
        || false,
    );
    drop(guard);
    stop.store(true, Ordering::Relaxed);
    let _ = worker.join();
    res
}
