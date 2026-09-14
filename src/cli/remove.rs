use clap::Subcommand;

#[derive(Subcommand)]
pub enum RemoveCommand {
    /// Remove a connection and all its accounts
    Connection {
        /// Connection ID to remove
        id: String,
    },
}
