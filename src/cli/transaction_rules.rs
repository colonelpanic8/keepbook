use clap::Subcommand;

#[derive(Subcommand)]
pub enum TransactionRulesCommand {
    /// Append a transaction annotation regex rule
    Add {
        /// Tags to assign when the rule matches. Repeat for multiple tags.
        #[arg(long = "set-tag")]
        set_tags: Vec<String>,

        /// Subtags to assign when the rule matches. Repeat for multiple subtags.
        #[arg(long = "set-subtag")]
        set_subtags: Vec<String>,

        /// Description override to assign when the rule matches
        #[arg(long = "set-description")]
        set_description: Option<String>,

        /// Account ID regex matcher
        #[arg(long = "match-account-id")]
        match_account_id: Option<String>,

        /// Account name regex matcher
        #[arg(long = "match-account-name")]
        match_account_name: Option<String>,

        /// Transaction description regex matcher
        #[arg(long = "match-description")]
        match_description: Option<String>,

        /// Resolved top-level tag regex matcher
        #[arg(long = "match-tag")]
        match_tag: Option<String>,

        /// Resolved subtag regex matcher
        #[arg(long = "match-subtag")]
        match_subtag: Option<String>,

        /// Transaction status regex matcher
        #[arg(long = "match-status")]
        match_status: Option<String>,

        /// Transaction amount regex matcher
        #[arg(long = "match-amount")]
        match_amount: Option<String>,
    },

    /// List configured transaction annotation regex rules
    List,

    /// Apply transaction annotation regex rules to existing transactions
    Apply {
        /// Start date (YYYY-MM-DD, default: no lower bound)
        #[arg(long)]
        start: Option<String>,

        /// End date (YYYY-MM-DD, default: no upper bound)
        #[arg(long)]
        end: Option<String>,

        /// Filter to a single account by ID or name (mutually exclusive with --connection)
        #[arg(long)]
        account: Option<String>,

        /// Filter to a single connection by ID or name (mutually exclusive with --account)
        #[arg(long)]
        connection: Option<String>,

        /// Replace existing explicit annotation fields touched by matching rules
        #[arg(long, default_value_t = false)]
        overwrite: bool,

        /// Show matching updates without writing annotation patches
        #[arg(long, default_value_t = false)]
        dry_run: bool,
    },
}
