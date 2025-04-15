use dialoguer::Select;
use sap_scripting::*;
use std::thread;
use std::time::Duration;

mod app;
mod utils;
mod vl06o;
mod vl06o_module;
mod vt11;
mod vt11_module;
mod zmdesnr;
mod zmdesnr_module;

use app::*;
use utils::config_ops::handle_configure_reports_dir;
use utils::excel_file_ops::handle_read_excel_file;
use utils::loop_config::{handle_configure_loop, run_loop};
use vl06o_module::{run_vl06o_auto, run_vl06o_module};
use vt11_module::{run_vt11_auto, run_vt11_module};
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

        // Create menu options based on SAP connection and login status
        let options = if sap_connected {
            if is_logged_in {
                vec![
                    "Log in to SAP",
                    "VT11 - Shipment List Planning",
                    "VT11 - Auto Run (from config)",
                    "VL06O - List of Outbound Deliveries",
                    "VL06O - Auto Run (from config)",
                    "ZMDESNR - Serial Number History",
                    "ZMDESNR - Auto Run (from config)",
                    "Run Loop (using config)",
                    "Configure Reports Directory",
                    "Configure SAP Parameters",
                    "Configure Loop",
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
                    "ZMDESNR - Serial Number History (Not available - Login required)",
                    "ZMDESNR - Auto Run (Not available - Login required)",
                    "Run Loop (Not available - Login required)",
                    "Configure Reports Directory",
                    "Configure SAP Parameters",
                    "Configure Loop",
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
                "ZMDESNR - Serial Number History (Not available - SAP connection required)",
                "ZMDESNR - Auto Run (Not available - SAP connection required)",
                "Run Loop (Not available - SAP connection required)",
                "Configure Reports Directory",
                "Configure SAP Parameters",
                "Configure Loop",
                "Read Excel File",
                "Log out of SAP (Not available - SAP connection required)",
                "Exit",
            ]
        };

        let choice = Select::new()
            .with_prompt("Choose an option")
            .items(&options)
            .default(0)
            .interact()
            .unwrap();

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
            6 => {
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
            7 => {
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
            8 => {
                // Configure Reports Directory (available regardless of SAP connection)
                if let Err(e) = handle_configure_reports_dir() {
                    eprintln!("Error configuring reports directory: {}", e);
                    thread::sleep(Duration::from_secs(2));
                }
            }
            9 => {
                // Configure SAP Parameters (available regardless of SAP connection)
                if let Err(e) = utils::config_ops::handle_configure_sap_params() {
                    eprintln!("Error configuring SAP parameters: {}", e);
                    thread::sleep(Duration::from_secs(2));
                }
            }
            10 => {
                // Configure Loop (available regardless of SAP connection)
                if let Err(e) = handle_configure_loop() {
                    eprintln!("Error configuring loop: {}", e);
                    thread::sleep(Duration::from_secs(2));
                }
            }
            11 => {
                // Read Excel File (available regardless of SAP connection)
                if let Err(e) = handle_read_excel_file() {
                    eprintln!("Error reading Excel file: {}", e);
                    thread::sleep(Duration::from_secs(2));
                }
            }
            12 => {
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
            13 => {
                // Exit application
                clear_screen();
                println!("Exiting application...");
                return Ok(());
            }
            _ => {} // no-op
        }
    }
}
