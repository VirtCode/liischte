use clap::{Parser, Subcommand};

#[derive(Parser)]
#[clap(version = option_env!("TAG").unwrap_or("unknown"), about)]
#[command(disable_help_subcommand = true)]
pub struct Args {
    #[clap(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand)]
pub enum Command {
    /// pass a message to a given module
    Pass {
        /// module to pass message to
        module: String,
        /// message that is passed to the module
        message: String,
    },
}

/// reads the comman from the commandline arguments, exits the program if cli is
/// misused
pub fn read_command() -> Option<Command> {
    Args::parse().command
}
