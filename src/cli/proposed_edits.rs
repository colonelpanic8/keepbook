use clap::Subcommand;

#[derive(Subcommand)]
pub enum ProposedEditsCommand {
    /// List proposed transaction edits
    List {
        /// Include approved, rejected, and removed proposals
        #[arg(long, default_value_t = false)]
        include_decided: bool,
    },

    /// Approve a proposed transaction edit and apply it
    Approve { id: String },

    /// Reject a proposed transaction edit
    Reject { id: String },

    /// Remove a proposed transaction edit from the active queue
    Remove { id: String },
}
