//! "Create / save layout" item of the main menu: pick a tcode that has configured layouts
//! (`[layouts.<TCODE>.<NAME>]` in config.toml), pick the layout, say whether an existing layout
//! may be replaced, and run the same function as `--create-layout` on the CURRENT screen of the
//! session (the report's result list must be displayed in SAP). UNVERIFIED live.
use dialoguer::{Confirm, Input, Select};
use sap_scripting::*;

use crate::utils::layout_create::{create_layout_on_current_screen, profile_for};
use crate::utils::layout_store::LayoutStore;

/// Tcodes with at least one usable configured layout AND a layout profile (sorted).
pub fn menu_tcodes(store: &LayoutStore) -> Vec<String> {
    store
        .tcodes()
        .into_iter()
        .filter(|t| profile_for(t).is_ok())
        .collect()
}

fn pause() {
    let _: std::result::Result<String, _> = Input::new()
        .with_prompt("Press Enter to continue")
        .allow_empty(true)
        .interact_text();
}

/// Ask what to create and run it. Errors are printed, never propagated (menu loop).
pub fn run_layout_menu(session: &GuiSession) {
    let store = LayoutStore::load();
    let tcodes = menu_tcodes(&store);
    if tcodes.is_empty() {
        println!(
            "No layouts configured: add [layouts.<TCODE>.<NAME>] sections with a `columns` list to config.toml (see CONFIG.md)."
        );
        pause();
        return;
    }
    println!("Run the report in SAP first: the layout is created on the CURRENT screen.");
    let Some(ti) = Select::new()
        .with_prompt("Tcode")
        .items(&tcodes)
        .default(0)
        .interact_opt()
        .ok()
        .flatten()
    else {
        return;
    };
    let tcode = tcodes[ti].clone();
    let names = store.names_for(&tcode);
    let Some(ni) = Select::new()
        .with_prompt("Layout")
        .items(&names)
        .default(0)
        .interact_opt()
        .ok()
        .flatten()
    else {
        return;
    };
    let name = names[ni].clone();
    let overwrite = Confirm::new()
        .with_prompt(format!(
            "Replace layout '{name}' if it already exists in SAP?"
        ))
        .default(false)
        .interact()
        .unwrap_or(false);

    match create_layout_on_current_screen(session, &tcode, &name, overwrite) {
        Ok(outcome) => println!("{}", outcome.describe(&name)),
        Err(e) => eprintln!("Layout not created: {e:#}"),
    }
    pause();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_lists_only_supported_tcodes_with_layouts() {
        let store = LayoutStore::from_toml_str(
            r#"
[layouts.VT11.EXAMPLE]
columns = ["A"]
[layouts.MB52.EXAMPLE]
columns = ["A"]
[layouts.lx03.EXAMPLE]
columns = ["A"]
[layouts.VL06O.BROKEN]
columns = []
"#,
        )
        .unwrap();
        // MB52 has no profile, VL06O has no usable layout
        assert_eq!(menu_tcodes(&store), ["LX03", "VT11"]);
    }
}
