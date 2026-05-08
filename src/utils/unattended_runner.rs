use crate::app::get_or_create_connection;
use crate::utils::cli_overrides::cli_overrides;
use crate::utils::loop_config::LoopConfig;
use crate::utils::sequence_config::{execute_menu_option, get_menu_option_name, SequenceConfig};
use anyhow::Result;
use sap_scripting::*;
use std::thread;
use std::time::Duration;

/// Returns true when `tcode` matches the 149 report (case-insensitive).
fn is_149_tcode(tcode: &str) -> bool {
    tcode.eq_ignore_ascii_case("y_dn3_47000149")
}

/// Run the loop configuration unattended
pub fn run_loop_unattended(session: &GuiSession, skip_sap_check: bool) -> Result<()> {
    println!("Starting unattended loop execution...");

    if !skip_sap_check {
        // Check if logged in
        let transaction = session.info()?.transaction()?;
        if transaction.contains("S000") {
            return Err(anyhow::anyhow!("Not logged into SAP. Please log in first."));
        }
    }

    println!("Running loop configuration...");
    run_loop_unattended_internal(session)?;
    println!("Loop execution completed successfully.");

    Ok(())
}

/// Internal non-interactive loop execution function
fn run_loop_unattended_internal(session: &GuiSession) -> Result<()> {
    println!("Run Loop from Configuration (Unattended Mode)");
    println!("============================================");

    // Load loop configuration (CLI overrides already merged in by LoopConfig::load)
    let config = match LoopConfig::load() {
        Ok(cfg) => cfg,
        Err(e) => {
            return Err(anyhow::anyhow!("Error loading loop configuration: {}", e));
        }
    };

    // Required-field validation (no sane default → must come from flag or config)
    if config.tcode.is_empty() {
        return Err(anyhow::anyhow!(
            "Missing flag for tcode, enter with --tcode (or set [loop].tcode in config.toml)"
        ));
    }
    if is_149_tcode(&config.tcode) && config.tcode_run_type.as_deref().unwrap_or("").is_empty() {
        return Err(anyhow::anyhow!(
            "Missing flag for tcode-run-type, enter with --tcode-run-type=rcv|mat|tsp \
             (or set [loop].tcode_run_type in config.toml; required for 149 reports)"
        ));
    }

    if let Some(line) = cli_overrides().summary_line() {
        println!("CLI overrides applied (these win over config.toml): {}", line);
    }

    println!(
        "Running TCode '{}' in a loop with the following configuration:",
        config.tcode
    );
    if let Some(run_type) = &config.tcode_run_type {
        println!("TCode Run Type: {}", run_type);
    } else {
        println!("TCode Run Type: (default)");
    }
    if config.iterations == 0 {
        println!("Iterations: infinite (until Ctrl+C)");
    } else {
        println!("Iterations: {}", config.iterations);
    }
    println!("Delay: {} seconds", config.delay_seconds);

    if !config.params.is_empty() {
        println!("\nParameters:");
        for (key, value) in &config.params {
            println!("  {}: {}", key, value);
        }
    }

    println!("\nStarting loop execution...");

    // Run the TCode in a loop
    let mut iteration = 1;
    loop {
        // Display iteration information
        if config.iterations == 0 {
            println!(
                "\nIteration {} (infinite loop, press Ctrl+C to stop)",
                iteration
            );
        } else {
            println!("\nIteration {}/{}", iteration, config.iterations);
        }

        // Check if the TCode is active
        if !crate::utils::sap_tcode_utils::check_tcode(
            session,
            &config.tcode,
            Some(true),
            Some(true),
        )? {
            println!("Failed to activate TCode '{}'", config.tcode);
            break;
        }

        // Run the TCode with the configured parameters
        match config.tcode.as_str() {
            "VL06O" => {
                crate::vl06o_module::run_vl06o_auto(session)?;
            }
            "VT11" => {
                crate::vt11_module::run_vt11_auto(session)?;
            }
            "ZMDESNR" => {
                crate::zmdesnr_module::run_zmdesnr_auto(session)?;
            }
            "y_dn3_47000149" | "Y_DN3_47000149" => {
                // Handle 149 report with different run types
                match config.tcode_run_type.as_deref() {
                    Some("rcv") => {
                        println!("Running 149 RCV auto...");
                        crate::y_149_rcv_module::run_149_rcv_auto(session)?;
                    }
                    Some("mat") | Some("tsp") => {
                        println!("Running 149 Material Not TSP auto...");
                        crate::y_149_material_module::run_149_material_auto(session)?;
                    }
                    Some("") | None => {
                        println!("Running 149 regular auto...");
                        crate::y_149_module::run_149_auto(session)?;
                    }
                    Some(unknown_type) => {
                        println!("Unknown run type '{}', using default", unknown_type);
                        crate::y_149_module::run_149_auto(session)?;
                    }
                }
            }
            _ => {
                println!("Unknown TCode '{}', skipping", config.tcode);
            }
        }

        // Check if we should continue the loop
        if config.iterations > 0 && iteration >= config.iterations {
            break;
        }

        // Wait before next iteration (unless this was the last one)
        if config.iterations == 0 || iteration < config.iterations {
            println!(
                "Waiting {} seconds before next iteration...",
                config.delay_seconds
            );
            thread::sleep(Duration::from_secs(config.delay_seconds));
        }

        iteration += 1;
    }

    println!("Loop execution completed.");
    Ok(())
}

/// Run the sequence configuration unattended
pub fn run_sequence_unattended(session: &GuiSession, skip_sap_check: bool) -> Result<()> {
    println!("Starting unattended sequence execution...");

    if !skip_sap_check {
        // Check if logged in
        let transaction = session.info()?.transaction()?;
        if transaction.contains("S000") {
            return Err(anyhow::anyhow!("Not logged into SAP. Please log in first."));
        }
    }

    println!("Running sequence configuration...");
    run_sequence_unattended_internal(session)?;
    println!("Sequence execution completed successfully.");

    Ok(())
}

/// Internal non-interactive sequence execution function
fn run_sequence_unattended_internal(session: &GuiSession) -> Result<()> {
    println!("Run Sequence from Configuration (Unattended Mode)");
    println!("================================================");

    // Load sequence configuration (CLI sequence-level overrides already merged in)
    let config = match SequenceConfig::load() {
        Ok(cfg) => cfg,
        Err(e) => {
            return Err(anyhow::anyhow!(
                "Error loading sequence configuration: {}",
                e
            ));
        }
    };

    // Required: sequence options. There is no sane default — the user must
    // pick which menu items run (via the interactive `Configure Sequence`).
    if config.options.is_empty() {
        return Err(anyhow::anyhow!(
            "No sequence options configured. Sequence uses config.toml — \
             create and set up [sequence].options (run interactively and pick \
             `Configure Sequence`, or hand-edit config.toml)."
        ));
    }

    if let Some(line) = cli_overrides().summary_line() {
        println!("CLI overrides applied (these win over config.toml): {}", line);
    }

    println!("Running sequence with the following configuration:");
    println!("Options:");
    if config.options.is_empty() {
        println!("  No options configured");
    } else {
        for (i, option_id) in config.options.iter().enumerate() {
            println!("  {}. {}", i + 1, get_menu_option_name(option_id));
        }
    }

    if config.iterations == 0 {
        println!("Iterations: infinite (until Ctrl+C)");
    } else {
        println!("Iterations: {}", config.iterations);
    }
    println!("Delay between iterations: {} seconds", config.delay_seconds);
    println!(
        "Interval between steps: {} seconds",
        config.interval_seconds
    );

    if !config.params.is_empty() {
        println!("\nParameters:");
        for (key, value) in &config.params {
            println!("  {}: {}", key, value);
        }
    }

    println!("\nStarting sequence execution...");

    // Run the sequence in a loop
    let mut iteration = 1;
    loop {
        // Display iteration information
        if config.iterations == 0 {
            println!(
                "\nIteration {} (infinite loop, press Ctrl+C to stop)",
                iteration
            );
        } else {
            println!("\nIteration {}/{}", iteration, config.iterations);
        }

        // Run each step in the sequence
        for (step_index, option) in config.options.iter().enumerate() {
            println!(
                "\nRunning step {} of {}: Option {}",
                step_index + 1,
                config.options.len(),
                option
            );

            // Execute the selected option
            println!("Running: {}", get_menu_option_name(option));
            if let Err(e) = execute_menu_option(session, option) {
                eprintln!("Error executing option: {}", e);
            }

            // If this is not the last step, wait for the interval
            if step_index < config.options.len() - 1 {
                println!(
                    "Waiting {} seconds before next step...",
                    config.interval_seconds
                );
                thread::sleep(Duration::from_secs(config.interval_seconds));
            }
        }

        // Check if we should continue the loop
        if config.iterations > 0 && iteration >= config.iterations {
            break;
        }

        // Wait before next iteration (unless this was the last one)
        if config.iterations == 0 || iteration < config.iterations {
            println!(
                "Waiting {} seconds before next iteration...",
                config.delay_seconds
            );
            thread::sleep(Duration::from_secs(config.delay_seconds));
        }

        iteration += 1;
    }

    println!("Sequence execution completed.");
    Ok(())
}

/// Run a single TCode auto-flow once (single-shot mode, used when `--tcode=...`
/// is passed without `--run-loop` or `--run-sequence`).
///
/// `tcode` should be the resolved (already uppercased) TCode name from CLI
/// overrides. `tcode_run_type` is required for 149 reports.
pub fn run_single_tcode_unattended(
    session: &GuiSession,
    tcode: &str,
    tcode_run_type: Option<&str>,
    skip_sap_check: bool,
) -> Result<()> {
    println!("Starting single-shot TCode execution...");

    if !skip_sap_check {
        let transaction = session.info()?.transaction()?;
        if transaction.contains("S000") {
            return Err(anyhow::anyhow!("Not logged into SAP. Please log in first."));
        }
    }

    if is_149_tcode(tcode) && tcode_run_type.unwrap_or("").is_empty() {
        return Err(anyhow::anyhow!(
            "Missing flag for tcode-run-type, enter with --tcode-run-type=rcv|mat|tsp \
             (required for 149 reports)"
        ));
    }

    if let Some(line) = cli_overrides().summary_line() {
        println!("CLI overrides applied (these win over config.toml): {}", line);
    }

    println!("Running TCode '{}' once...", tcode);

    // Activate the TCode (start_at_main=true, exit_existing=true) just like the
    // loop runner does so we don't pile up sessions.
    if !crate::utils::sap_tcode_utils::check_tcode(session, tcode, Some(true), Some(true))? {
        return Err(anyhow::anyhow!("Failed to activate TCode '{}'", tcode));
    }

    match tcode {
        "VL06O" => crate::vl06o_module::run_vl06o_auto(session)?,
        "VT11" => crate::vt11_module::run_vt11_auto(session)?,
        "ZVT11" => crate::zvt11_module::run_zvt11_auto(session)?,
        "ZMDESNR" => crate::zmdesnr_module::run_zmdesnr_auto(session)?,
        "Y_DN3_47000149" => match tcode_run_type {
            Some("rcv") => {
                println!("Running 149 RCV auto...");
                crate::y_149_rcv_module::run_149_rcv_auto(session)?;
            }
            Some("mat") | Some("tsp") => {
                println!("Running 149 Material Not TSP auto...");
                crate::y_149_material_module::run_149_material_auto(session)?;
            }
            Some(other) => {
                return Err(anyhow::anyhow!(
                    "Unknown --tcode-run-type='{}'. Use rcv | mat | tsp.",
                    other
                ));
            }
            None => unreachable!("validated above"),
        },
        other => {
            return Err(anyhow::anyhow!(
                "Single-shot mode does not support TCode '{}'. Supported: \
                 VT11, ZVT11, VL06O, ZMDESNR, Y_DN3_47000149.",
                other
            ));
        }
    }

    println!("Single-shot TCode execution completed.");
    Ok(())
}

/// Initialize SAP connection for unattended execution
pub fn init_sap_connection() -> Result<(
    SAPComInstance,
    SAPWrapper,
    GuiApplication,
    GuiConnection,
    GuiSession,
)> {
    println!("Initializing SAP connection for unattended execution...");

    // Initialize COM environment
    let com_instance = SAPComInstance::new()?;
    println!("✓ COM environment initialized");

    // Get SAP wrapper
    let wrapper = com_instance.sap_wrapper()?;
    println!("✓ SAP wrapper obtained");

    // Get the scripting engine
    let engine = wrapper.scripting_engine()?;
    println!("✓ Scripting engine obtained");

    // Get connection or create a new one
    let connection = get_or_create_connection(&engine)?;
    println!("✓ SAP connection established");

    // Get the first session
    let children = GuiConnectionExt::children(&connection)?;
    let element = children.element_at(0)?;
    let session = element
        .downcast()
        .ok_or_else(|| anyhow::anyhow!("Failed to get SAP session"))?;
    println!("✓ SAP session obtained");

    println!("SAP connection initialized successfully.");
    Ok((com_instance, wrapper, engine, connection, session))
}
