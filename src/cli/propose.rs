use clap::Subcommand;

#[derive(Subcommand)]
pub enum ProposeCommand {
    /// Propose a transaction annotation edit
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
}
