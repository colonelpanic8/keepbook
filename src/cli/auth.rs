use clap::Subcommand;

#[derive(Subcommand)]
pub enum AuthCommand {
    /// Schwab authentication commands
    #[command(subcommand)]
    Schwab(SchwabAuthCommand),
    /// Chase authentication commands
    #[command(subcommand)]
    Chase(ChaseAuthCommand),
}

#[derive(Subcommand)]
pub enum SchwabAuthCommand {
    /// Login via browser to capture session
    Login {
        /// Connection ID or name (optional if only one Schwab connection)
        id_or_name: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum ChaseAuthCommand {
    /// Login via browser to capture session
    Login {
        /// Connection ID or name (optional if only one Chase connection)
        id_or_name: Option<String>,
    },
}
