use sap_scripting::*;
use windows::core::Result;

use crate::utils::choose_layout_utils::choose_layout;
use crate::utils::config_types::SapConfig;
use crate::utils::sap_ctrl_utils::exist_ctrl;
use crate::utils::sap_export_utils::export_local_file;
use crate::utils::sap_tcode_utils::{assert_tcode, variant_select};

/// Struct to hold LX03 export parameters
#[derive(Debug)]
pub struct LX03Params {
    pub sap_variant_name: Option<String>,
    pub layout_row: Option<String>,
    pub export_type: Option<u8>,
    pub t_code: String,
}

impl Default for LX03Params {
    fn default() -> Self {
        Self {
            sap_variant_name: None,
            layout_row: None,
            export_type: None,
            t_code: "LX03".to_string(),
        }
    }
}

fn resolve_export_type(params: &LX03Params) -> u8 {
    if let Some(export_type) = params.export_type {
        return export_type;
    }
    if let Ok(config) = SapConfig::load() {
        if let Some(export_type) = config.get_effective_export_type("LX03") {
            return export_type;
        }
    }
    1 // text with tabs
}

/// Run LX03 export with the given parameters
pub fn run_export(session: &GuiSession, params: &LX03Params) -> Result<bool> {
    println!("Running LX03 export...");

    if !assert_tcode(session, "LX03", Some(0))? {
        println!("Failed to activate LX03 transaction");
        return Ok(false);
    }

    if let Some(variant_name) = &params.sap_variant_name {
        if !variant_name.is_empty() && !variant_select(session, &params.t_code, variant_name)? {
            println!(
                "Failed to select variant '{}' for tCode '{}'",
                variant_name, params.t_code
            );
            return Ok(false);
        }
    }

    if let Ok(btn) = session.find_by_id("wnd[0]/tbar[1]/btn[8]".to_string()) {
        if let Some(button) = btn.downcast::<GuiButton>() {
            button.press()?;
        }
    }

    if let Some(layout_row) = &params.layout_row {
        if !layout_row.is_empty() {
            match choose_layout(session, &params.t_code, layout_row) {
                Ok(message) if message.is_empty() => {}
                Ok(message) => {
                    println!("Message after choosing layout {}: {}", layout_row, message);
                }
                Err(e) => {
                    println!("Error choosing layout {}: {}", layout_row, e);
                    return Ok(false);
                }
            }

            let err_ctl = exist_ctrl(session, 1, "", true)?;
            if err_ctl.cband {
                if let Ok(window) = session.find_by_id("wnd[1]".to_string()) {
                    if let Some(modal_window) = window.downcast::<GuiFrameWindow>() {
                        modal_window.close()?;
                    }
                }
                println!("Layout ({}) not found.", layout_row);
                return Ok(false);
            }
        }
    }

    let export_type = resolve_export_type(params);

    if let Ok(menu) = session.find_by_id("wnd[0]/mbar/menu[0]/menu[1]/menu[2]".to_string()) {
        if let Some(menu_item) = menu.downcast::<GuiMenu>() {
            menu_item.select()?;
        } else {
            println!("Failed to open LX03 local file export menu");
            return Ok(false);
        }
    } else {
        println!("LX03 local file export menu not found");
        return Ok(false);
    }

    match export_local_file(session, "LX03", export_type, None) {
        Ok(path) if path.is_empty() => Ok(true),
        Ok(_) => Ok(true),
        Err(e) => {
            println!("Error exporting LX03 data: {}", e);
            Ok(false)
        }
    }
}
