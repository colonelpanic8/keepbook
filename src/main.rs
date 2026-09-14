mod cli;
mod dispatch;

use std::sync::Arc;

use anyhow::Result;
use clap::{CommandFactory, Parser};
use keepbook::app;
use keepbook::config::ResolvedConfig;
use keepbook::repositories::setup_manifest_repositories;
use keepbook::storage::{JsonFileStorage, Storage};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

use cli::{apply_runtime_credential_overrides, Cli, Command, RepositoriesCommand};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize structured logging to stderr
    // Use RUST_LOG env var for filtering (default: info, suppress noisy chromiumoxide errors)
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| {
            EnvFilter::new(
                "info,chromiumoxide=warn,chromiumoxide::conn=off,chromiumoxide::handler=off",
            )
        }))
        .with(
            fmt::layer()
                .with_writer(std::io::stderr)
                .with_target(true)
                .with_level(true)
                .json(),
        )
        .init();

    let cli = Cli::parse();
    apply_runtime_credential_overrides(&cli);

    if let Some(Command::Repositories(RepositoriesCommand::Setup { app_config })) = &cli.command {
        let output = setup_manifest_repositories(app_config)?;
        println!("{}", serde_json::to_string_pretty(&output)?);
        if !output.ok {
            anyhow::bail!("one or more repositories failed setup");
        }
        return Ok(());
    }

    let config = ResolvedConfig::load_or_default(&cli.config)?;
    let storage = JsonFileStorage::new(&config.data_dir);
    let storage_arc: Arc<dyn Storage> = Arc::new(storage.clone());

    // Pre-command hook (decoupled from CLI parsing; CLI only computes enablement).
    let edits_data = cli
        .command
        .as_ref()
        .map(|command| command.edits_data())
        .unwrap_or(false);
    let push_after_sync = if cli.git_push_after_sync {
        true
    } else if cli.skip_git_push_after_sync {
        false
    } else {
        config.git.push_after_sync
    };
    let merge_enabled = if cli.git_merge_master {
        true
    } else if cli.skip_git_merge_master {
        false
    } else {
        config.git.merge_master_before_command
    };
    let pull_enabled = if cli.git_pull_before_edit {
        true
    } else if cli.skip_git_pull_before_edit {
        false
    } else {
        config.git.pull_before_edit
    };
    app::run_preflight(
        &config,
        app::PreflightOptions {
            merge_origin_master: merge_enabled,
            pull_remote: edits_data && pull_enabled,
        },
    )?;

    match cli.command {
        Some(Command::Config) => dispatch::config::run(&cli.config, &config)?,

        Some(Command::Repositories(_)) => {
            unreachable!("repository setup is handled before loading keepbook.toml")
        }

        Some(Command::Add(command)) => {
            dispatch::mutations::add(command, &storage_arc, &config).await?
        }

        Some(Command::Remove(command)) => {
            dispatch::mutations::remove(command, &storage_arc, &config).await?
        }

        Some(Command::Set(command)) => {
            dispatch::mutations::set(command, &storage_arc, &config).await?
        }

        Some(Command::Propose(command)) => {
            dispatch::mutations::propose(command, &storage_arc, &config).await?
        }

        Some(Command::ProposedEdits(command)) => {
            dispatch::mutations::proposed_edits(command, &storage_arc, &config).await?
        }

        Some(Command::TransactionRules(command)) => {
            dispatch::transaction_rules::run(command, &storage_arc, &config).await?
        }

        Some(Command::Import(command)) => {
            dispatch::import::run(command, &storage_arc, &config).await?
        }

        Some(Command::Sync(command)) => {
            dispatch::sync::run(command, &storage, &storage_arc, &config, push_after_sync).await?
        }

        Some(Command::Auth(command)) => dispatch::auth::run(command, &storage_arc, &config).await?,

        Some(Command::MarketData(command)) => {
            dispatch::market_data::run(command, &storage_arc, &config).await?
        }

        Some(Command::List(command)) => dispatch::list::run(command, &storage_arc, &config).await?,

        Some(Command::Tui {
            view,
            net_worth_interval,
        }) => dispatch::tui::run(&storage_arc, &config, view, net_worth_interval).await?,

        Some(Command::Portfolio(command)) => {
            dispatch::portfolio::run(command, &storage_arc, &config).await?
        }

        Some(Command::Spending(args)) => {
            dispatch::spending::spending(args, &storage_arc, &config).await?
        }

        Some(Command::SpendingTags(args)) => {
            dispatch::spending::spending_tags(args, &storage_arc, &config).await?
        }

        None => {
            Cli::command().print_help()?;
        }
    }

    Ok(())
}
