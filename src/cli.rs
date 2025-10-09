use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "sap_automation")]
#[command(about = "SAP GUI Automation utilities")]
#[command(version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Run the loop configuration unattended
    #[command(name = "run-loop")]
    RunLoop {
        /// Skip SAP connection check (for testing)
        #[arg(long, default_value = "false")]
        skip_sap_check: bool,
        /// Keep the system awake during execution
        #[arg(long, default_value = "false")]
        keep_awake: bool,
    },
    
    /// Run the sequence configuration unattended
    #[command(name = "run-sequence")]
    RunSequence {
        /// Skip SAP connection check (for testing)
        #[arg(long, default_value = "false")]
        skip_sap_check: bool,
        /// Keep the system awake during execution
        #[arg(long, default_value = "false")]
        keep_awake: bool,
    },
}

impl Cli {
    pub fn parse() -> Self {
        <Cli as clap::Parser>::parse()
    }
}
