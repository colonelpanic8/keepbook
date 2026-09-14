use clap::Subcommand;

#[derive(Subcommand)]
pub enum AddCommand {
    /// Add a new connection
    Connection {
        /// Name for the connection
        name: String,

        /// Synchronizer to use (default: manual)
        #[arg(long, default_value = "manual")]
        synchronizer: String,
    },

    /// Add a new account to a connection
    Account {
        /// Connection ID to add the account to
        #[arg(long)]
        connection: String,

        /// Name for the account
        name: String,

        /// Tags for the account (can be specified multiple times)
        #[arg(long, short)]
        tag: Vec<String>,
    },
}
