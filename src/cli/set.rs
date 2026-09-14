use clap::Subcommand;

#[derive(Subcommand)]
pub enum SetCommand {
    /// Set or update a balance for an account
    Balance {
        /// Account ID
        #[arg(long)]
        account: String,

        /// Asset type (e.g., "USD", "equity:AAPL", "crypto:BTC", "value:Expected Housing Value")
        #[arg(long)]
        asset: String,

        /// Amount
        #[arg(long)]
        amount: String,

        /// Optional total cost basis for this balance, in reporting/portfolio currency
        #[arg(long)]
        cost_basis: Option<String>,
    },

    /// Set account-level configuration values
    AccountConfig {
        /// Account ID or name
        #[arg(long)]
        account: String,

        /// Balance backfill policy: none, zero, carry_earliest
        #[arg(long, conflicts_with = "clear_balance_backfill")]
        balance_backfill: Option<String>,

        /// Clear balance backfill policy override
        #[arg(long)]
        clear_balance_backfill: bool,
    },

    /// Set a transaction annotation (append-only patch)
    Transaction {
        /// Account ID
        #[arg(long)]
        account: String,

        /// Transaction ID
        #[arg(long)]
        transaction: String,

        /// Override description
        #[arg(long, conflicts_with = "clear_description")]
        description: Option<String>,

        /// Clear description override
        #[arg(long)]
        clear_description: bool,

        /// Set note
        #[arg(long, conflicts_with = "clear_note")]
        note: Option<String>,

        /// Clear note
        #[arg(long)]
        clear_note: bool,

        /// Set tags (repeatable)
        #[arg(long, short)]
        tag: Vec<String>,

        /// Set tags to empty array
        #[arg(long, conflicts_with = "clear_tags")]
        tags_empty: bool,

        /// Clear tags field
        #[arg(long)]
        clear_tags: bool,

        /// Set subtags (repeatable)
        #[arg(long)]
        subtag: Vec<String>,

        /// Set subtags to empty array
        #[arg(long, conflicts_with = "clear_subtags")]
        subtags_empty: bool,

        /// Clear subtags field
        #[arg(long)]
        clear_subtags: bool,

        /// Override reporting date (YYYY-MM-DD) without changing synced timestamp
        #[arg(long, conflicts_with = "clear_effective_date")]
        effective_date: Option<String>,

        /// Clear reporting date override
        #[arg(long)]
        clear_effective_date: bool,
    },

    /// Set or clear the ignored-from-spending flag for transactions (append-only patch)
    TransactionIgnore {
        /// Account ID
        #[arg(long)]
        account: String,

        /// Transaction ID (repeatable)
        #[arg(long, required = true)]
        transaction: Vec<String>,

        /// Clear the ignore flag (also strips legacy ignore tags)
        #[arg(long)]
        clear: bool,
    },
}
