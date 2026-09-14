use std::path::PathBuf;

use clap::Subcommand;

#[derive(Subcommand)]
pub enum ImportCommand {
    /// Schwab import commands
    #[command(subcommand)]
    Schwab(SchwabImportCommand),
}

#[derive(Subcommand)]
pub enum SchwabImportCommand {
    /// Import transactions from a Schwab JSON export file
    Transactions {
        /// Account ID or name
        #[arg(long)]
        account: String,

        /// Path to Schwab-exported JSON file
        file: PathBuf,
    },
}
