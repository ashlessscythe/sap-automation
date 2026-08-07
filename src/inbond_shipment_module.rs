//! Interactive menu wrapper for the inbond shipment → VL06O → 149 workflow.

use crossterm::{
    execute,
    terminal::{Clear, ClearType},
};
use dialoguer::Input;
use sap_scripting::*;
use std::io::{self, Write};
use windows::core::Result;

use crate::inbond_shipment::{run_inbond_shipment_flow, InbondSettings};

fn clear_screen() {
    let mut stdout = io::stdout();
    let _ = execute!(stdout, Clear(ClearType::All));
}

/// Prompt for a shipment number and run the inbond VL06O → 149 → paste-file flow.
pub fn run_inbond_shipment_module(session: &GuiSession) -> Result<()> {
    clear_screen();
    println!("Inbond - Shipment to 149");
    println!("========================");
    println!("VL06O (blank_/config) → deliveries → y_dn3_47000149 (inb_ship)");
    println!("→ timestamped paste-ready txt → Notepad for material.php\n");

    let settings = InbondSettings::load();
    println!("Settings:");
    println!("  VL06O variant : {}", settings.vl06o_variant);
    println!("  149 variant   : {}", settings.variant_149);
    println!("  149 layout    : {}", settings.layout_149);
    println!("  export_type   : {}", settings.export_type);
    println!("  open_notepad  : {}\n", settings.open_notepad);

    let shipment: String = Input::new()
        .with_prompt("Shipment number")
        .interact_text()
        .unwrap_or_default();

    if shipment.trim().is_empty() {
        println!("No shipment entered. Cancelled.");
        println!("\nPress Enter to return to main menu...");
        let mut input = String::new();
        let _ = io::stdin().read_line(&mut input);
        return Ok(());
    }

    match run_inbond_shipment_flow(session, shipment.trim(), &settings) {
        Ok(Some(path)) => {
            println!("\nInbond flow completed.");
            println!("Paste file: {}", path.display());
            println!("Shipment for web form: {}", shipment.trim());
        }
        Ok(None) => {
            println!("\nInbond flow did not complete successfully.");
        }
        Err(e) => {
            println!("\nError during inbond flow: {}", e);
        }
    }

    println!("\nPress Enter to return to main menu...");
    let mut input = String::new();
    let _ = io::stdin().read_line(&mut input);
    let _ = io::stdout().flush();

    Ok(())
}
