use clap::Subcommand;

#[derive(Subcommand)]
pub enum ListCommand {
    /// List all connections
    Connections,

    /// List all accounts
    Accounts,

    /// List configured price sources
    PriceSources,

    /// List latest balances for all accounts
    Balances,

    /// List all transactions
    Transactions {
        /// Start date (YYYY-MM-DD, default: 30 days ago)
        #[arg(long)]
        start: Option<String>,

        /// End date (YYYY-MM-DD, default: today)
        #[arg(long)]
        end: Option<String>,

        /// Sort transactions by amount (ascending)
        #[arg(long, default_value_t = false)]
        sort_by_amount: bool,

        /// Include transactions that would otherwise be ignored by spending/list ignore rules
        #[arg(long, default_value_t = false)]
        include_ignored: bool,
    },

    /// Detect recurring transaction candidates
    RecurringTransactions {
        /// Start date (YYYY-MM-DD, default: all available history)
        #[arg(long)]
        start: Option<String>,

        /// End date (YYYY-MM-DD, default: today)
        #[arg(long)]
        end: Option<String>,

        /// Include transactions that would otherwise be ignored by spending/list ignore rules
        #[arg(long, default_value_t = false)]
        include_ignored: bool,

        /// Include lower-confidence candidates with only partial history
        #[arg(long, default_value_t = false)]
        include_possible: bool,

        /// Minimum confidence score from 0.0 to 1.0
        #[arg(long, default_value_t = 0.70)]
        min_confidence: f64,

        /// Include candidates that have been dismissed in review
        #[arg(long, default_value_t = false)]
        include_dismissed: bool,
    },

    /// List everything
    All,
}
