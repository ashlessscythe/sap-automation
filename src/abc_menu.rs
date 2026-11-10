//! "ABC Headless" submenu: entry point for the reports used by the headless ABC runner
//! (see `docs/abc_headless_plan.md`). Entries for reports that are not implemented yet
//! (LT23, ZMDETPC) are shown but only print a notice.
use sap_scripting::*;
use std::thread;
use std::time::Duration;

use crate::lx03_module::run_lx03_auto;
use crate::tui;
use crate::vl06o_module::run_vl06o_auto;
use crate::vt11_module::run_vt11_auto;
use crate::zmdesnr_module::run_zmdesnr_auto;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AbcChoice {
    Vt11,
    Vl06o,
    Zmdesnr,
    Lx03,
    Lt23,
    Zmdetpc,
    Back,
}

/// Menu entries in display order.
pub const ABC_ENTRIES: [(AbcChoice, &str); 7] = [
    (AbcChoice::Vt11, "VT11 - Auto Run (from config)"),
    (AbcChoice::Vl06o, "VL06O - Auto Run (from config)"),
    (AbcChoice::Zmdesnr, "ZMDESNR - Auto Run (from config)"),
    (AbcChoice::Lx03, "LX03 - Auto Run (from config)"),
    (
        AbcChoice::Lt23,
        "LT23 - Transfer Orders (not implemented yet)",
    ),
    (
        AbcChoice::Zmdetpc,
        "ZMDETPC - Pallet Count (not implemented yet)",
    ),
    (AbcChoice::Back, "Back to Main Menu"),
];

pub fn choice_from_index(index: usize) -> Option<AbcChoice> {
    ABC_ENTRIES.get(index).map(|(c, _)| *c)
}

fn pause() {
    thread::sleep(Duration::from_secs(2));
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
            println!("Not implemented yet (see docs/abc_headless_plan.md, phases 4.4/4.5).");
            pause();
            continue;
        }
        let Some(session) = session else {
            println!("SAP connection and login are required.");
            pause();
            continue;
        };
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
    fn back_is_last_and_indices_map() {
        assert_eq!(choice_from_index(0), Some(AbcChoice::Vt11));
        assert_eq!(
            choice_from_index(ABC_ENTRIES.len() - 1),
            Some(AbcChoice::Back)
        );
        assert_eq!(choice_from_index(ABC_ENTRIES.len()), None);
    }
}
