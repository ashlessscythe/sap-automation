//! "ABC Headless" submenu: entry point for the reports used by the headless ABC runner
//! (see `docs/abc_headless_plan.md`). Entries for steps without a standalone run (ZMDETPC,
//! LT23) are shown but only print a notice. The report entries (`mustgo`, `xdock`, `toyota`,
//! `misc`, `k2xx`) run the whole headless report after asking for the run date.
use sap_scripting::*;
use std::thread;
use std::time::Duration;

use crate::abc::dispatch;
use crate::abc::run::{parse_run_date, RunDate};
use crate::lx03_module::run_lx03_auto;
use crate::tui;
use crate::vl06o_module::run_vl06o_auto;
use crate::vt11_module::run_vt11_auto;
use crate::zmdesnr_module::run_zmdesnr_auto;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AbcChoice {
    Mustgo,
    Xdock,
    Toyota,
    Misc,
    K2xx,
    Vt11,
    Vl06o,
    Zmdesnr,
    Lx03,
    Lt23,
    Zmdetpc,
    Back,
}

/// Menu entries in display order.
pub const ABC_ENTRIES: [(AbcChoice, &str); 12] = [
    (
        AbcChoice::Mustgo,
        "mustgo - Headless report (asks for the run date)",
    ),
    (
        AbcChoice::Xdock,
        "xdock - Headless report (asks for the run date)",
    ),
    (
        AbcChoice::Toyota,
        "toyota - Headless report (asks for the run date)",
    ),
    (
        AbcChoice::Misc,
        "misc - Headless report (asks for the run date)",
    ),
    (
        AbcChoice::K2xx,
        "k2xx - Headless report (asks for the run date)",
    ),
    (AbcChoice::Vt11, "VT11 - Auto Run (from config)"),
    (AbcChoice::Vl06o, "VL06O - Auto Run (from config)"),
    (AbcChoice::Zmdesnr, "ZMDESNR - Auto Run (from config)"),
    (AbcChoice::Lx03, "LX03 - Auto Run (from config)"),
    (
        AbcChoice::Lt23,
        "LT23 - Transfer Orders (runs inside mustgo, no standalone run)",
    ),
    (
        AbcChoice::Zmdetpc,
        "ZMDETPC - Pallet Count (runs inside xdock/toyota/k2xx, no standalone run)",
    ),
    (AbcChoice::Back, "Back to Main Menu"),
];

pub fn choice_from_index(index: usize) -> Option<AbcChoice> {
    ABC_ENTRIES.get(index).map(|(c, _)| *c)
}

fn pause() {
    thread::sleep(Duration::from_secs(2));
}

/// Validate a run date typed in the menu (`DATE | A..B | WKnn[/year]`).
pub fn validate_run_date_input(
    raw: &str,
    default_year: i32,
) -> std::result::Result<RunDate, String> {
    parse_run_date(raw, default_year).map_err(|e| e.to_string())
}

/// Ask for the run date (default today) and run report `name` against the connected session.
fn run_report_interactive(session: &GuiSession, name: &str) {
    use chrono::Datelike;
    let today = chrono::Local::now().date_naive();
    let year = today.year();
    let raw: String = match dialoguer::Input::new()
        .with_prompt(format!("{name} run date (YYYY-MM-DD, A..B or WKnn)"))
        .default(today.format("%Y-%m-%d").to_string())
        .validate_with(|s: &String| validate_run_date_input(s, year).map(|_| ()))
        .interact_text()
    {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Input error: {e}");
            pause();
            return;
        }
    };
    let Ok(date) = validate_run_date_input(&raw, year) else {
        return;
    };
    // Same dispatch as `--rname=<name> --run-date=...` (CLI overrides/config resolved there).
    if let Err(e) = dispatch::run_report(session, name, &date) {
        eprintln!("{name} failed: {}", dispatch::describe_error(&e));
    }
    let _: std::result::Result<String, _> = dialoguer::Input::new()
        .with_prompt("Press Enter to continue")
        .allow_empty(true)
        .interact_text();
}

/// Show the submenu until the user goes back. `session` is `Some` only when SAP is
/// connected and logged in.
pub fn run_abc_menu(session: Option<&GuiSession>) {
    loop {
        let items: Vec<String> = ABC_ENTRIES
            .iter()
            .map(|(c, label)| match (c, session) {
                (AbcChoice::Back, _) | (_, Some(_)) => (*label).to_string(),
                (_, None) => format!("{label} (Not available - login required)"),
            })
            .collect();

        let choice = match tui::show_selection_menu("SAP Automation - ABC Headless", items, Some(0))
        {
            Ok(Some(i)) => choice_from_index(i),
            Ok(None) => return,
            Err(e) => {
                eprintln!("Error with TUI: {e}");
                return;
            }
        };

        let Some(choice) = choice else { continue };
        if choice == AbcChoice::Back {
            return;
        }
        if matches!(choice, AbcChoice::Lt23 | AbcChoice::Zmdetpc) {
            println!(
                "No standalone run: LT23 runs inside mustgo, ZMDETPC inside xdock/toyota/k2xx."
            );
            pause();
            continue;
        }
        let Some(session) = session else {
            println!("SAP connection and login are required.");
            pause();
            continue;
        };
        if matches!(
            choice,
            AbcChoice::Mustgo
                | AbcChoice::Xdock
                | AbcChoice::Toyota
                | AbcChoice::Misc
                | AbcChoice::K2xx
        ) {
            let name = match choice {
                AbcChoice::Mustgo => "mustgo",
                AbcChoice::Xdock => "xdock",
                AbcChoice::Misc => "misc",
                AbcChoice::K2xx => "k2xx",
                _ => "toyota",
            };
            run_report_interactive(session, name);
            continue;
        }
        let result = match choice {
            AbcChoice::Vt11 => run_vt11_auto(session),
            AbcChoice::Vl06o => run_vl06o_auto(session),
            AbcChoice::Zmdesnr => run_zmdesnr_auto(session),
            AbcChoice::Lx03 => run_lx03_auto(session),
            _ => Ok(()),
        };
        if let Err(e) = result {
            eprintln!("Error running {choice:?}: {e}");
            pause();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_date_input_validation() {
        assert!(validate_run_date_input("2026-11-05", 2026).is_ok());
        assert!(validate_run_date_input("WK45", 2026).is_ok());
        assert!(validate_run_date_input("2026-11-01..2026-11-07", 2026).is_ok());
        assert!(validate_run_date_input("11/05/2026", 2026).is_err());
        assert!(validate_run_date_input("", 2026).is_err());
    }

    #[test]
    fn mustgo_is_listed_first() {
        assert_eq!(ABC_ENTRIES[0].0, AbcChoice::Mustgo);
    }

    #[test]
    fn back_is_last_and_indices_map() {
        assert_eq!(choice_from_index(0), Some(AbcChoice::Mustgo));
        assert_eq!(choice_from_index(1), Some(AbcChoice::Xdock));
        assert_eq!(choice_from_index(2), Some(AbcChoice::Toyota));
        assert_eq!(choice_from_index(3), Some(AbcChoice::Misc));
        assert_eq!(choice_from_index(4), Some(AbcChoice::K2xx));
        assert_eq!(choice_from_index(5), Some(AbcChoice::Vt11));
        assert_eq!(
            choice_from_index(ABC_ENTRIES.len() - 1),
            Some(AbcChoice::Back)
        );
        assert_eq!(choice_from_index(ABC_ENTRIES.len()), None);
    }
}
