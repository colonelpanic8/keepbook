use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};
use keepbook::config::ResolvedConfig;

#[derive(Parser)]
#[command(name = "keepbook")]
#[command(about = "Personal finance manager")]
struct Cli {
    /// Path to config file
    #[arg(short, long, default_value = "keepbook.toml")]
    config: PathBuf,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Show current configuration
    Config,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let config = ResolvedConfig::load_or_default(&cli.config)?;

    match cli.command {
        Some(Command::Config) => {
            println!("Config file: {}", cli.config.display());
            println!("Data directory: {}", config.data_dir.display());
        }
        None => {
            println!("Keepbook - Personal Finance Manager");
            println!("====================================\n");
            println!("Config: {}", cli.config.display());
            println!("Data directory: {}\n", config.data_dir.display());
            println!("Commands:");
            println!("  config    Show current configuration\n");
            println!("Run 'keepbook --help' for more options.");
        }
    }

    Ok(())
}
