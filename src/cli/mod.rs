pub mod add;
pub mod auth;
pub mod import;
pub mod list;
pub mod market_data;
pub mod portfolio;
pub mod propose;
pub mod proposed_edits;
pub mod remove;
pub mod repositories;
pub mod set;
pub mod spending;
pub mod sync;
pub mod transaction_rules;
pub mod tui;

use std::path::PathBuf;

use clap::{Parser, Subcommand};
use keepbook::config::default_config_path;

pub use add::AddCommand;
pub use auth::{AuthCommand, ChaseAuthCommand, SchwabAuthCommand};
pub use import::{ImportCommand, SchwabImportCommand};
pub use list::ListCommand;
pub use market_data::MarketDataCommand;
pub use portfolio::PortfolioCommand;
pub use propose::ProposeCommand;
pub use proposed_edits::ProposedEditsCommand;
pub use remove::RemoveCommand;
pub use repositories::RepositoriesCommand;
pub use set::SetCommand;
pub use spending::{SpendingArgs, SpendingTagsArgs};
pub use sync::{PriceSyncScopeCommand, SyncCommand};
pub use transaction_rules::TransactionRulesCommand;
pub use tui::{NetWorthIntervalArg, TuiViewArg};

const CLI_VERSION: &str = concat!(
    env!("CARGO_PKG_VERSION"),
    " (git commit ",
    env!("GIT_COMMIT_HASH"),
    ")"
);

pub fn parse_duration_arg(s: &str) -> Result<std::time::Duration, String> {
    keepbook::duration::parse_duration(s).map_err(|e| e.to_string())
}

#[derive(Parser)]
#[command(name = "keepbook")]
#[command(version = CLI_VERSION)]
#[command(about = "Personal finance manager")]
pub struct Cli {
    /// Path to config file
    #[arg(short, long, default_value_os_t = default_config_path())]
    pub config: PathBuf,

    /// Merge origin/master before executing the command.
    #[arg(long, global = true, conflicts_with = "skip_git_merge_master")]
    pub git_merge_master: bool,

    /// Skip merging origin/master even if enabled in config.
    #[arg(
        long = "skip-git-merge-master",
        global = true,
        conflicts_with = "git_merge_master"
    )]
    pub skip_git_merge_master: bool,

    /// Pull remote changes before commands that edit data.
    #[arg(long, global = true, conflicts_with = "skip_git_pull_before_edit")]
    pub git_pull_before_edit: bool,

    /// Skip pulling remote changes before editing even if enabled in config.
    #[arg(
        long = "skip-git-pull-before-edit",
        global = true,
        conflicts_with = "git_pull_before_edit"
    )]
    pub skip_git_pull_before_edit: bool,

    /// Push committed changes after sync commands complete.
    #[arg(long, global = true, conflicts_with = "skip_git_push_after_sync")]
    pub git_push_after_sync: bool,

    /// Skip pushing after sync even if enabled in config.
    #[arg(
        long = "skip-git-push-after-sync",
        global = true,
        conflicts_with = "git_push_after_sync"
    )]
    pub skip_git_push_after_sync: bool,

    /// Schwab username override for login autofill.
    ///
    /// Equivalent to setting KEEPBOOK_SCHWAB_USERNAME for this command invocation.
    #[arg(long, global = true)]
    pub schwab_username: Option<String>,

    /// Schwab password override for login autofill.
    ///
    /// Equivalent to setting KEEPBOOK_SCHWAB_PASSWORD for this command invocation.
    #[arg(long, global = true)]
    pub schwab_password: Option<String>,

    #[command(subcommand)]
    pub command: Option<Command>,
}

pub fn apply_runtime_credential_overrides(cli: &Cli) {
    if let Some(username) = &cli.schwab_username {
        std::env::set_var("KEEPBOOK_SCHWAB_USERNAME", username);
    }
    if let Some(password) = &cli.schwab_password {
        std::env::set_var("KEEPBOOK_SCHWAB_PASSWORD", password);
    }
}

#[derive(Subcommand)]
pub enum Command {
    /// Show current configuration
    Config,

    /// Manage declarative Keepbook data repositories
    #[command(subcommand)]
    Repositories(RepositoriesCommand),

    /// Add entities
    #[command(subcommand)]
    Add(AddCommand),

    /// List entities
    #[command(subcommand)]
    List(ListCommand),

    /// Remove entities
    #[command(subcommand)]
    Remove(RemoveCommand),

    /// Set/update values
    #[command(subcommand)]
    Set(SetCommand),

    /// Propose changes for later review
    #[command(subcommand)]
    Propose(ProposeCommand),

    /// Manage proposed transaction edits
    #[command(subcommand)]
    ProposedEdits(ProposedEditsCommand),

    /// Manage transaction annotation regex rules
    #[command(subcommand)]
    TransactionRules(TransactionRulesCommand),

    /// Import data from exported files
    #[command(subcommand)]
    Import(ImportCommand),

    /// Sync data from connections
    #[command(subcommand)]
    Sync(SyncCommand),

    /// Authentication commands for synchronizers
    #[command(subcommand)]
    Auth(AuthCommand),

    /// Market data commands
    #[command(subcommand)]
    MarketData(MarketDataCommand),

    /// Portfolio commands
    #[command(subcommand)]
    Portfolio(PortfolioCommand),

    /// Interactive terminal interface
    Tui {
        /// Initial view to open when starting the TUI
        #[arg(long, value_enum, default_value = "transactions")]
        view: TuiViewArg,

        /// Granularity of net-worth updates shown in the net-worth view
        #[arg(long, value_enum, default_value = "daily")]
        net_worth_interval: NetWorthIntervalArg,
    },

    /// Spending reports based on transaction logs
    Spending(SpendingArgs),

    /// Spending report grouped by top-level tag
    SpendingTags(SpendingTagsArgs),
}

impl Command {
    pub fn edits_data(&self) -> bool {
        match self {
            Command::Add(_)
            | Command::Remove(_)
            | Command::Set(_)
            | Command::Propose(_)
            | Command::Import(_)
            | Command::Sync(_)
            | Command::MarketData(MarketDataCommand::Fetch { .. }) => true,
            Command::ProposedEdits(ProposedEditsCommand::List { .. }) => false,
            Command::ProposedEdits(_) => true,
            Command::TransactionRules(TransactionRulesCommand::List) => false,
            Command::TransactionRules(TransactionRulesCommand::Apply { dry_run, .. }) => !*dry_run,
            Command::TransactionRules(_) => true,
            Command::Portfolio(PortfolioCommand::Snapshot {
                offline, dry_run, ..
            }) => !*offline && !*dry_run,
            _ => false,
        }
    }
}
