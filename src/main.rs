use sap_automation::utils::config_types::SapConfig;
use sap_scripting::*;
use std::thread;
use std::time::Duration;

mod app;
mod tui;
mod utils;
mod vl06o;
mod vl06o_delivery_module;
mod vl06o_module;
mod vt11;
mod vt11_module;
mod y_149;
mod y_149_material;
mod y_149_material_module;
mod y_149_module;
mod y_149_rcv;
mod y_149_rcv_module;
mod zmdesnr;
mod zmdesnr_module;

use app::*;
use utils::config_ops::handle_configure_reports_dir;
use utils::config_types::get_default_menu_option;
use utils::excel_file_ops::handle_read_excel_file;
use utils::loop_config::{handle_configure_loop, run_loop};
use utils::sequence_config::{handle_configure_sequence, run_sequence};
use vl06o_delivery_module::{run_vl06o_delivery_packages_auto, run_vl06o_delivery_packages_module};
use vl06o_module::{run_vl06o_auto, run_vl06o_date_update_module, run_vl06o_module};
use vt11_module::{run_vt11_auto, run_vt11_module};
use y_149_material_module::{run_149_material_auto, run_149_material_module};
use y_149_module::{run_149_auto, run_149_module};
use y_149_rcv_module::{run_149_rcv_auto, run_149_rcv_module};
use zmdesnr_module::{run_zmdesnr_auto, run_zmdesnr_module};

fn main() -> anyhow::Result<()> {
    // Initialize logging if needed
    // pretty_env_logger::init();

    // Flag to track if SAP is connected
    let mut sap_connected = false;

    // Optional variables to hold SAP components if connection is successful
    let mut com_instance: Option<SAPComInstance> = None;
    let mut wrapper: Option<SAPWrapper> = None;
    let mut engine: Option<GuiApplication> = None;
    let mut connection: Option<GuiConnection> = None;
    let mut session: Option<GuiSession> = None;

    // Try to initialize COM environment
    match SAPComInstance::new() {
        Ok(instance) => {
            com_instance = Some(instance);

            // Try to get SAP wrapper
            match com_instance.as_ref().unwrap().sap_wrapper() {
                Ok(w) => {
                    wrapper = Some(w);

                    // Try to get the scripting engine
                    match wrapper.as_ref().unwrap().scripting_engine() {
                        Ok(e) => {
                            engine = Some(e);

                            // Try to get connection or create a new one
                            match get_or_create_connection(engine.as_ref().unwrap()) {
                                Ok(conn) => {
                                    connection = Some(conn);

                                    // Try to get the first session
                                    match GuiConnectionExt::children(connection.as_ref().unwrap()) {
                                        Ok(children) => {
                                            if let Ok(element) = children.element_at(0) {
                                                if let Some(s) = element.downcast() {
                                                    session = Some(s);
                                                    sap_connected = true;
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            eprintln!("Warning: Failed to get SAP session: {}", e);
                                        }
                                    }
                                }
                                Err(e) => {
                                    eprintln!("Warning: Error getting SAP connection: {}", e);
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("Warning: Error getting SAP scripting engine: {}", e);
                            eprintln!("Make sure SAP GUI is running and scripting is enabled.");
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Warning: Error getting SAP wrapper: {}", e);
                    eprintln!("Make sure SAP GUI is installed and properly configured.");
                }
            }
        }
        Err(e) => {
            eprintln!("Warning: Couldn't initialize COM environment: {}", e);
        }
    }

    if !sap_connected {
        println!("SAP connection not available. Some features will be disabled.");
        thread::sleep(Duration::from_secs(2));
    }

    // Main application loop
    loop {
        clear_screen();

        // Check if already logged in (only if SAP is connected)
        let is_logged_in = if sap_connected {
            let transaction = session
                .as_ref()
                .unwrap()
                .info()
                .unwrap()
                .transaction()
                .unwrap();
            !transaction.contains("S000")
        } else {
            false
        };

        // Default option for the menu from config else 0
        let default_option = SapConfig::load()
            .map(|config| config.global?.default_menu_option)
            .unwrap_or_else(|_| Some(get_default_menu_option()));

        // Create menu options based on SAP connection and login status
        let options = if sap_connected {
            if is_logged_in {
                vec![
                    "Log in to SAP",
                    "VT11 - Shipment List Planning",
                    "VT11 - Auto Run (from config)",
                    "VL06O - List of Outbound Deliveries",
                    "VL06O - Auto Run (from config)",
                    "VL06O - Change Delivery Date",
                    "VL06O - List of Delivery Packages",
                    "VL06O - Auto Run Delivery Packages",
                    "ZMDESNR - Serial Number History",
                    "ZMDESNR - Auto Run (from config)",
                    "149 Report - y_dn3_47000149",
                    "149 Report - Material Not TSP",
                    "149 Report - RCV",
                    "149 Report - RCV Auto Run (from config)",
                    "149 Report - Auto Run (from config)",
                    "Run Loop (using config)",
                    "Run Sequence (using config)",
                    "Configure Reports Directory",
                    "Configure Global and SAP Parameters",
                    "Configure Loop",
                    "Configure Sequence",
                    "Read Excel File",
                    "Log out of SAP",
                    "Exit",
                ]
            } else {
                vec![
                    "Log in to SAP",
                    "VT11 - Shipment List Planning (Not available - Login required)",
                    "VT11 - Auto Run (Not available - Login required)",
                    "VL06O - List of Outbound Deliveries (Not available - Login required)",
                    "VL06O - Auto Run (Not available - Login required)",
                    "VL06O - Change Delivery Date (Not available - Login required)",
                    "VL06O - List of Delivery Packages (Not available - Login required)",
                    "VL06O - Auto Run Delivery Packages (Not available - Login required)",
                    "ZMDESNR - Serial Number History (Not available - Login required)",
                    "ZMDESNR - Auto Run (Not available - Login required)",
                    "149 Report - y_dn3_47000149 (Not available - Login required)",
                    "149 Report - Material Not TSP (Not available - Login required)",
                    "149 Report - RCV (Not available - Login required)",
                    "149 Report - Auto Run (Not available - Login required)",
                    "Run Loop (Not available - Login required)",
                    "Run Sequence (Not available - Login required)",
                    "Configure Reports Directory",
                    "Configure Global and SAP Parameters",
                    "Configure Loop",
                    "Configure Sequence",
                    "Read Excel File",
                    "Log out of SAP (Not available - Login required)",
                    "Exit",
                ]
            }
        } else {
            vec![
                "Log in to SAP (Not available - SAP connection required)",
                "VT11 - Shipment List Planning (Not available - SAP connection required)",
                "VT11 - Auto Run (Not available - SAP connection required)",
                "VL06O - List of Outbound Deliveries (Not available - SAP connection required)",
                "VL06O - Auto Run (Not available - SAP connection required)",
                "VL06O - Change Delivery Date (Not available - SAP connection required)",
                "VL06O - List of Delivery Packages (Not available - SAP connection required)",
                "VL06O - Auto Run Delivery Packages (Not available - SAP connection required)",
                "ZMDESNR - Serial Number History (Not available - SAP connection required)",
                "ZMDESNR - Auto Run (Not available - SAP connection required)",
                "149 Report - y_dn3_47000149 (Not available - SAP connection required)",
                "149 Report - Material Not TSP (Not available - SAP connection required)",
                "149 Report - RCV (Not available - SAP connection required)",
                "149 Report - RCV Auto Run (Not available - SAP connection required)",
                "149 Report - Auto Run (Not available - SAP connection required)",
                "Run Loop (Not available - SAP connection required)",
                "Run Sequence (Not available - SAP connection required)",
                "Configure Reports Directory",
                "Configure Global and SAP Parameters",
                "Configure Loop",
                "Configure Sequence",
                "Read Excel File",
                "Log out of SAP (Not available - SAP connection required)",
                "Exit",
            ]
        };

        let choice = match tui::show_selection_menu(
            "SAP Automation - Main Menu",
            options.into_iter().map(|s| s.to_string()).collect(),
            default_option,
        ) {
            Ok(Some(selected)) => selected,
            Ok(None) => {
                // User pressed Esc or q to exit
                clear_screen();
                println!("Exiting application...");
                return Ok(());
            }
            Err(e) => {
                eprintln!("Error with TUI: {}", e);
                return Err(anyhow::anyhow!("TUI error: {}", e));
            }
        };

        match choice {
            0 => {
                // Log in to SAP
                if sap_connected {
                    if let Err(e) = handle_login(session.as_ref().unwrap()) {
                        eprintln!("Error logging in: {}", e);
                        thread::sleep(Duration::from_secs(2));
                    }
                } else {
                    println!("SAP connection not available. Cannot log in.");
                    thread::sleep(Duration::from_secs(2));
                }
            }
            1 => {
                // Run VT11 module (only if logged in and SAP connected)
                if sap_connected && is_logged_in {
                    if let Err(e) = run_vt11_module(session.as_ref().unwrap()) {
                        eprintln!("Error running VT11 module: {}", e);
                        thread::sleep(Duration::from_secs(2));
                    }
                } else if sap_connected {
                    println!("You need to log in first.");
                    thread::sleep(Duration::from_secs(2));
                } else {
                    println!("SAP connection not available. Cannot run VT11 module.");
                    thread::sleep(Duration::from_secs(2));
                }
            }
            2 => {
                // Run VT11 Auto module (only if logged in and SAP connected)
                if sap_connected && is_logged_in {
                    if let Err(e) = run_vt11_auto(session.as_ref().unwrap()) {
                        eprintln!("Error running VT11 auto module: {}", e);
                        thread::sleep(Duration::from_secs(2));
                    }
                } else if sap_connected {
                    println!("You need to log in first.");
                    thread::sleep(Duration::from_secs(2));
                } else {
                    println!("SAP connection not available. Cannot run VT11 auto module.");
                    thread::sleep(Duration::from_secs(2));
                }
            }
            3 => {
                // Run VL06O module (only if logged in and SAP connected)
                if sap_connected && is_logged_in {
                    if let Err(e) = run_vl06o_module(session.as_ref().unwrap()) {
                        eprintln!("Error running VL06O module: {}", e);
                        thread::sleep(Duration::from_secs(2));
                    }
                } else if sap_connected {
                    println!("You need to log in first.");
                    thread::sleep(Duration::from_secs(2));
                } else {
                    println!("SAP connection not available. Cannot run VL06O module.");
                    thread::sleep(Duration::from_secs(2));
                }
            }
            4 => {
                // Run VL06O Auto module (only if logged in and SAP connected)
                if sap_connected && is_logged_in {
                    if let Err(e) = run_vl06o_auto(session.as_ref().unwrap()) {
                        eprintln!("Error running VL06O auto module: {}", e);
                        thread::sleep(Duration::from_secs(2));
                    }
                } else if sap_connected {
                    println!("You need to log in first.");
                    thread::sleep(Duration::from_secs(2));
                } else {
                    println!("SAP connection not available. Cannot run VL06O auto module.");
                    thread::sleep(Duration::from_secs(2));
                }
            }
            5 => {
                // Run VL06O Date Update module (only if logged in and SAP connected)
                if sap_connected && is_logged_in {
                    if let Err(e) = run_vl06o_date_update_module(session.as_ref().unwrap()) {
                        eprintln!("Error running VL06O date update module: {}", e);
                        thread::sleep(Duration::from_secs(2));
                    }
                } else if sap_connected {
                    println!("You need to log in first.");
                    thread::sleep(Duration::from_secs(2));
                } else {
                    println!("SAP connection not available. Cannot run VL06O date update module.");
                    thread::sleep(Duration::from_secs(2));
                }
            }
            6 => {
                // Run VL06O Delivery Packages module (only if logged in and SAP connected)
                if sap_connected && is_logged_in {
                    if let Err(e) = run_vl06o_delivery_packages_module(session.as_ref().unwrap()) {
                        eprintln!("Error running VL06O delivery packages module: {}", e);
                        thread::sleep(Duration::from_secs(2));
                    }
                } else if sap_connected {
                    println!("You need to log in first.");
                    thread::sleep(Duration::from_secs(2));
                } else {
                    println!(
                        "SAP connection not available. Cannot run VL06O delivery packages module."
                    );
                    thread::sleep(Duration::from_secs(2));
                }
            }
            7 => {
                // Run VL06O Delivery Packages Auto module (only if logged in and SAP connected)
                if sap_connected && is_logged_in {
                    if let Err(e) = run_vl06o_delivery_packages_auto(session.as_ref().unwrap()) {
                        eprintln!("Error running VL06O delivery packages auto module: {}", e);
                        thread::sleep(Duration::from_secs(2));
                    }
                } else if sap_connected {
                    println!("You need to log in first.");
                    thread::sleep(Duration::from_secs(2));
                } else {
                    println!("SAP connection not available. Cannot run VL06O delivery packages auto module.");
                    thread::sleep(Duration::from_secs(2));
                }
            }
            8 => {
                // Run ZMDESNR module (only if logged in and SAP connected)
                if sap_connected && is_logged_in {
                    if let Err(e) = run_zmdesnr_module(session.as_ref().unwrap()) {
                        eprintln!("Error running ZMDESNR module: {}", e);
                        thread::sleep(Duration::from_secs(2));
                    }
                } else if sap_connected {
                    println!("You need to log in first.");
                    thread::sleep(Duration::from_secs(2));
                } else {
                    println!("SAP connection not available. Cannot run ZMDESNR module.");
                    thread::sleep(Duration::from_secs(2));
                }
            }
            9 => {
                // Run ZMDESNR Auto module (only if logged in and SAP connected)
                if sap_connected && is_logged_in {
                    if let Err(e) = run_zmdesnr_auto(session.as_ref().unwrap()) {
                        eprintln!("Error running ZMDESNR auto module: {}", e);
                        thread::sleep(Duration::from_secs(2));
                    }
                } else if sap_connected {
                    println!("You need to log in first.");
                    thread::sleep(Duration::from_secs(2));
                } else {
                    println!("SAP connection not available. Cannot run ZMDESNR auto module.");
                    thread::sleep(Duration::from_secs(2));
                }
            }
            10 => {
                // Run 149 Report module (only if logged in and SAP connected)
                if sap_connected && is_logged_in {
                    if let Err(e) = run_149_module(session.as_ref().unwrap()) {
                        eprintln!("Error running 149 report module: {}", e);
                        thread::sleep(Duration::from_secs(2));
                    }
                } else if sap_connected {
                    println!("You need to log in first.");
                    thread::sleep(Duration::from_secs(2));
                } else {
                    println!("SAP connection not available. Cannot run 149 report module.");
                    thread::sleep(Duration::from_secs(2));
                }
            }
            11 => {
                // Run 149 Material Not TSP module (only if logged in and SAP connected)
                if sap_connected && is_logged_in {
                    if let Err(e) = run_149_material_module(session.as_ref().unwrap()) {
                        eprintln!("Error running 149 material not TSP module: {}", e);
                        thread::sleep(Duration::from_secs(2));
                    }
                } else if sap_connected {
                    println!("You need to log in first.");
                    thread::sleep(Duration::from_secs(2));
                } else {
                    println!(
                        "SAP connection not available. Cannot run 149 material not TSP module."
                    );
                    thread::sleep(Duration::from_secs(2));
                }
            }
            12 => {
                // Run 149 RCV module (only if logged in and SAP connected)
                if sap_connected && is_logged_in {
                    if let Err(e) = run_149_rcv_module(session.as_ref().unwrap()) {
                        eprintln!("Error running 149 RCV module: {}", e);
                        thread::sleep(Duration::from_secs(2));
                    }
                } else if sap_connected {
                    println!("You need to log in first.");
                    thread::sleep(Duration::from_secs(2));
                } else {
                    println!("SAP connection not available. Cannot run 149 RCV module.");
                    thread::sleep(Duration::from_secs(2));
                }
            }
            13 => {
                // Run 149 RCV Auto module (only if logged in and SAP connected)
                if sap_connected && is_logged_in {
                    if let Err(e) = run_149_rcv_auto(session.as_ref().unwrap()) {
                        eprintln!("Error running 149 RCV auto module: {}", e);
                        thread::sleep(Duration::from_secs(2));
                    }
                } else if sap_connected {
                    println!("You need to log in first.");
                    thread::sleep(Duration::from_secs(2));
                } else {
                    println!("SAP connection not available. Cannot run 149 RCV auto module.");
                    thread::sleep(Duration::from_secs(2));
                }
            }
            14 => {
                // Run 149 Report Auto module (only if logged in and SAP connected)
                if sap_connected && is_logged_in {
                    if let Err(e) = run_149_auto(session.as_ref().unwrap()) {
                        eprintln!("Error running 149 report auto module: {}", e);
                        thread::sleep(Duration::from_secs(2));
                    }
                } else if sap_connected {
                    println!("You need to log in first.");
                    thread::sleep(Duration::from_secs(2));
                } else {
                    println!("SAP connection not available. Cannot run 149 report auto module.");
                    thread::sleep(Duration::from_secs(2));
                }
            }
            15 => {
                // Run Loop (using config) (only if logged in and SAP connected)
                if sap_connected && is_logged_in {
                    if let Err(e) = run_loop(session.as_ref().unwrap()) {
                        eprintln!("Error running loop: {}", e);
                        thread::sleep(Duration::from_secs(2));
                    }
                } else if sap_connected {
                    println!("You need to log in first.");
                    thread::sleep(Duration::from_secs(2));
                } else {
                    println!("SAP connection not available. Cannot run loop.");
                    thread::sleep(Duration::from_secs(2));
                }
            }
            16 => {
                // Run Sequence (using config) (only if logged in and SAP connected)
                if sap_connected && is_logged_in {
                    if let Err(e) = run_sequence(session.as_ref().unwrap()) {
                        eprintln!("Error running sequence: {}", e);
                        thread::sleep(Duration::from_secs(2));
                    }
                } else if sap_connected {
                    println!("You need to log in first.");
                    thread::sleep(Duration::from_secs(2));
                } else {
                    println!("SAP connection not available. Cannot run sequence.");
                    thread::sleep(Duration::from_secs(2));
                }
            }
            17 => {
                // Configure Reports Directory (available regardless of SAP connection)
                if let Err(e) = handle_configure_reports_dir() {
                    eprintln!("Error configuring reports directory: {}", e);
                    thread::sleep(Duration::from_secs(2));
                }
            }
            18 => {
                // Configure SAP Parameters (available regardless of SAP connection)
                if let Err(e) = utils::config_handlers::handle_configure_sap_params() {
                    eprintln!("Error configuring SAP parameters: {}", e);
                    thread::sleep(Duration::from_secs(2));
                }
            }
            19 => {
                // Configure Loop (available regardless of SAP connection)
                if let Err(e) = handle_configure_loop() {
                    eprintln!("Error configuring loop: {}", e);
                    thread::sleep(Duration::from_secs(2));
                }
            }
            20 => {
                // Configure Sequence (available regardless of SAP connection)
                if let Err(e) = handle_configure_sequence() {
                    eprintln!("Error configuring sequence: {}", e);
                    thread::sleep(Duration::from_secs(2));
                }
            }
            21 => {
                // Read Excel File (available regardless of SAP connection)
                if let Err(e) = handle_read_excel_file() {
                    eprintln!("Error reading Excel file: {}", e);
                    thread::sleep(Duration::from_secs(2));
                }
            }
            22 => {
                // Log out of SAP (only if logged in and SAP connected)
                if sap_connected && is_logged_in {
                    if let Err(e) = handle_logout(session.as_ref().unwrap()) {
                        eprintln!("Error logging out: {}", e);
                        thread::sleep(Duration::from_secs(2));
                    }
                } else if sap_connected {
                    println!("You are not logged in.");
                    thread::sleep(Duration::from_secs(2));
                } else {
                    println!("SAP connection not available. Cannot log out.");
                    thread::sleep(Duration::from_secs(2));
                }
            }
            23 => {
                // Exit application
                clear_screen();
                println!("Exiting application...");
                return Ok(());
            }
            _ => {} // no-op
        }
    }
}
