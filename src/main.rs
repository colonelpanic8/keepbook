mod cli;

use std::sync::Arc;

use anyhow::Result;
use clap::{CommandFactory, Parser};
use keepbook::app;
use keepbook::config::ResolvedConfig;
use keepbook::repositories::setup_manifest_repositories;
use keepbook::storage::{JsonFileStorage, Storage};
use keepbook::sync::TransactionSyncMode;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

use cli::{
    apply_runtime_credential_overrides, AddCommand, AuthCommand, ChaseAuthCommand, Cli, Command,
    ImportCommand, ListCommand, MarketDataCommand, PortfolioCommand, PriceSyncScopeCommand,
    ProposeCommand, ProposedEditsCommand, RemoveCommand, RepositoriesCommand, SchwabAuthCommand,
    SchwabImportCommand, SetCommand, SpendingArgs, SpendingTagsArgs, SyncCommand,
    TransactionRulesCommand,
};

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
        Some(Command::Config) => {
            let output = app::config_output(&cli.config, &config);
            println!("{}", serde_json::to_string_pretty(&output)?);
        }

        Some(Command::Repositories(_)) => {
            unreachable!("repository setup is handled before loading keepbook.toml")
        }

        Some(Command::Add(add_cmd)) => match add_cmd {
            AddCommand::Connection { name, synchronizer } => {
                let result =
                    app::add_connection(storage_arc.as_ref(), &config, &name, &synchronizer)
                        .await?;
                println!("{}", serde_json::to_string_pretty(&result)?);
            }
            AddCommand::Account {
                connection,
                name,
                tag,
            } => {
                let result =
                    app::add_account(storage_arc.as_ref(), &config, &connection, &name, tag)
                        .await?;
                println!("{}", serde_json::to_string_pretty(&result)?);
            }
        },

        Some(Command::Remove(remove_cmd)) => match remove_cmd {
            RemoveCommand::Connection { id } => {
                let result = app::remove_connection(storage_arc.as_ref(), &config, &id).await?;
                println!("{}", serde_json::to_string_pretty(&result)?);
            }
        },

        Some(Command::Set(set_cmd)) => match set_cmd {
            SetCommand::Balance {
                account,
                asset,
                amount,
                cost_basis,
            } => {
                let result = app::set_balance(
                    storage_arc.as_ref(),
                    &config,
                    &account,
                    &asset,
                    &amount,
                    cost_basis.as_deref(),
                )
                .await?;
                println!("{}", serde_json::to_string_pretty(&result)?);
            }
            SetCommand::AccountConfig {
                account,
                balance_backfill,
                clear_balance_backfill,
            } => {
                let result = app::set_account_config(
                    storage_arc.as_ref(),
                    &config,
                    &account,
                    balance_backfill.as_deref(),
                    clear_balance_backfill,
                )
                .await?;
                println!("{}", serde_json::to_string_pretty(&result)?);
            }
            SetCommand::Transaction {
                account,
                transaction,
                description,
                clear_description,
                note,
                clear_note,
                tag,
                tags_empty,
                clear_tags,
                subtag,
                subtags_empty,
                clear_subtags,
                effective_date,
                clear_effective_date,
            } => {
                let result = app::set_transaction_annotation(
                    storage_arc.as_ref(),
                    &config,
                    &account,
                    &transaction,
                    app::TransactionAnnotationInput {
                        description,
                        clear_description,
                        note,
                        clear_note,
                        tags: tag,
                        tags_empty,
                        clear_tags,
                        subtags: subtag,
                        subtags_empty,
                        clear_subtags,
                        effective_date,
                        clear_effective_date,
                    },
                )
                .await?;
                println!("{}", serde_json::to_string_pretty(&result)?);
            }
            SetCommand::TransactionIgnore {
                account,
                transaction,
                clear,
            } => {
                let targets = transaction
                    .into_iter()
                    .map(|transaction_id| (account.clone(), transaction_id))
                    .collect::<Vec<_>>();
                let result =
                    app::set_transaction_ignore(storage_arc.as_ref(), &config, targets, !clear)
                        .await?;
                println!("{}", serde_json::to_string_pretty(&result)?);
            }
        },

        Some(Command::Propose(propose_cmd)) => match propose_cmd {
            ProposeCommand::Transaction {
                account,
                transaction,
                description,
                clear_description,
                note,
                clear_note,
                tag,
                tags_empty,
                clear_tags,
                subtag,
                subtags_empty,
                clear_subtags,
                effective_date,
                clear_effective_date,
            } => {
                let result = app::propose_transaction_edit(
                    storage_arc.as_ref(),
                    &config,
                    &account,
                    &transaction,
                    app::TransactionAnnotationInput {
                        description,
                        clear_description,
                        note,
                        clear_note,
                        tags: tag,
                        tags_empty,
                        clear_tags,
                        subtags: subtag,
                        subtags_empty,
                        clear_subtags,
                        effective_date,
                        clear_effective_date,
                    },
                )
                .await?;
                println!("{}", serde_json::to_string_pretty(&result)?);
            }
        },

        Some(Command::ProposedEdits(proposed_cmd)) => match proposed_cmd {
            ProposedEditsCommand::List { include_decided } => {
                let result =
                    app::list_proposed_transaction_edits(storage_arc.as_ref(), include_decided)
                        .await?;
                println!("{}", serde_json::to_string_pretty(&result)?);
            }
            ProposedEditsCommand::Approve { id } => {
                let result =
                    app::approve_proposed_transaction_edit(storage_arc.as_ref(), &config, &id)
                        .await?;
                println!("{}", serde_json::to_string_pretty(&result)?);
            }
            ProposedEditsCommand::Reject { id } => {
                let result =
                    app::reject_proposed_transaction_edit(storage_arc.as_ref(), &config, &id)
                        .await?;
                println!("{}", serde_json::to_string_pretty(&result)?);
            }
            ProposedEditsCommand::Remove { id } => {
                let result =
                    app::remove_proposed_transaction_edit(storage_arc.as_ref(), &config, &id)
                        .await?;
                println!("{}", serde_json::to_string_pretty(&result)?);
            }
        },

        Some(Command::TransactionRules(transaction_rules_cmd)) => match transaction_rules_cmd {
            TransactionRulesCommand::Add {
                set_tags,
                set_subtags,
                set_description,
                match_account_id,
                match_account_name,
                match_description,
                match_tag,
                match_subtag,
                match_status,
                match_amount,
            } => {
                let result = app::add_transaction_rule(
                    &config,
                    app::TransactionRule {
                        set_tags: if set_tags.is_empty() {
                            None
                        } else {
                            Some(set_tags)
                        },
                        set_subtags: if set_subtags.is_empty() {
                            None
                        } else {
                            Some(set_subtags)
                        },
                        set_description,
                        match_account_id,
                        match_account_name,
                        match_description,
                        match_tag,
                        match_subtag,
                        match_status,
                        match_amount,
                    },
                )
                .await?;
                println!("{}", serde_json::to_string_pretty(&result)?);
            }
            TransactionRulesCommand::List => {
                let result = app::list_transaction_rules(&config)?;
                println!("{}", serde_json::to_string_pretty(&result)?);
            }
            TransactionRulesCommand::Apply {
                start,
                end,
                account,
                connection,
                overwrite,
                dry_run,
            } => {
                let result = app::apply_transaction_rules(
                    storage_arc.as_ref(),
                    &config,
                    app::ApplyTransactionRulesOptions {
                        start,
                        end,
                        account,
                        connection,
                        overwrite,
                        dry_run,
                    },
                )
                .await?;
                println!("{}", serde_json::to_string_pretty(&result)?);
            }
        },

        Some(Command::Import(import_cmd)) => match import_cmd {
            ImportCommand::Schwab(schwab_cmd) => match schwab_cmd {
                SchwabImportCommand::Transactions { account, file } => {
                    let result = app::import_schwab_transactions(
                        storage_arc.as_ref(),
                        &config,
                        &account,
                        &file,
                    )
                    .await?;
                    println!("{}", serde_json::to_string_pretty(&result)?);
                }
            },
        },

        Some(Command::Sync(sync_cmd)) => match sync_cmd {
            SyncCommand::Connection {
                id_or_name,
                if_stale,
                transactions,
            } => {
                let transactions: TransactionSyncMode = transactions.into();
                let result = if if_stale {
                    app::sync_connection_if_stale(
                        storage_arc.clone(),
                        &config,
                        &id_or_name,
                        transactions,
                    )
                    .await?
                } else {
                    app::sync_connection(storage_arc.clone(), &config, &id_or_name, transactions)
                        .await?
                };
                app::maybe_push_after_sync(&config, push_after_sync)?;
                println!("{}", serde_json::to_string_pretty(&result)?);
            }
            SyncCommand::All {
                if_stale,
                transactions,
            } => {
                let transactions: TransactionSyncMode = transactions.into();
                let result = if if_stale {
                    app::sync_all_if_stale(storage_arc.clone(), &config, transactions).await?
                } else {
                    app::sync_all(storage_arc.clone(), &config, transactions).await?
                };
                app::maybe_push_after_sync(&config, push_after_sync)?;
                println!("{}", serde_json::to_string_pretty(&result)?);
            }
            SyncCommand::Prices { opts, scope } => {
                let result = app::sync_prices(
                    storage_arc.clone(),
                    &config,
                    match &scope {
                        None => app::SyncPricesScopeArg::Interactive,
                        Some(PriceSyncScopeCommand::All) => app::SyncPricesScopeArg::All,
                        Some(PriceSyncScopeCommand::Connection { id_or_name }) => {
                            app::SyncPricesScopeArg::Connection(id_or_name.as_deref())
                        }
                        Some(PriceSyncScopeCommand::Account { id_or_name }) => {
                            app::SyncPricesScopeArg::Account(id_or_name.as_deref())
                        }
                    },
                    opts.force,
                    opts.quote_staleness,
                )
                .await?;
                app::maybe_push_after_sync(&config, push_after_sync)?;
                println!("{}", serde_json::to_string_pretty(&result)?);
            }
            SyncCommand::Symlinks => {
                let result = app::sync_symlinks(&storage, &config).await?;
                app::maybe_push_after_sync(&config, push_after_sync)?;
                println!("{}", serde_json::to_string_pretty(&result)?);
            }
            SyncCommand::Recompact => {
                let result = app::sync_recompact(&storage, &config).await?;
                app::maybe_push_after_sync(&config, push_after_sync)?;
                println!("{}", serde_json::to_string_pretty(&result)?);
            }
            SyncCommand::BackfillMetadata => {
                let result = app::sync_backfill_metadata(&storage, &config).await?;
                app::maybe_push_after_sync(&config, push_after_sync)?;
                println!("{}", serde_json::to_string_pretty(&result)?);
            }
        },

        Some(Command::Auth(auth_cmd)) => match auth_cmd {
            AuthCommand::Schwab(schwab_cmd) => match schwab_cmd {
                SchwabAuthCommand::Login { id_or_name } => {
                    let result =
                        app::schwab_login(storage_arc.clone(), &config, id_or_name.as_deref())
                            .await?;
                    println!("{}", serde_json::to_string_pretty(&result)?);
                }
            },
            AuthCommand::Chase(chase_cmd) => match chase_cmd {
                ChaseAuthCommand::Login { id_or_name } => {
                    let result =
                        app::chase_login(storage_arc.clone(), &config, id_or_name.as_deref())
                            .await?;
                    println!("{}", serde_json::to_string_pretty(&result)?);
                }
            },
        },

        Some(Command::MarketData(market_cmd)) => match market_cmd {
            MarketDataCommand::Fetch {
                account,
                connection,
                start,
                end,
                interval,
                lookback_days,
                request_delay_ms,
                currency,
                no_fx,
            } => {
                let output = app::fetch_historical_prices(app::PriceHistoryRequest {
                    storage: storage_arc.as_ref(),
                    config: &config,
                    account: account.as_deref(),
                    connection: connection.as_deref(),
                    start: start.as_deref(),
                    end: end.as_deref(),
                    interval: interval.as_str(),
                    lookback_days,
                    request_delay_ms,
                    currency,
                    include_fx: !no_fx,
                })
                .await?;
                println!("{}", serde_json::to_string_pretty(&output)?);
            }
        },

        Some(Command::List(list_cmd)) => match list_cmd {
            ListCommand::Connections => {
                let connections = app::list_connections(storage_arc.as_ref()).await?;
                println!("{}", serde_json::to_string_pretty(&connections)?);
            }

            ListCommand::Accounts => {
                let accounts = app::list_accounts(storage_arc.as_ref()).await?;
                println!("{}", serde_json::to_string_pretty(&accounts)?);
            }

            ListCommand::PriceSources => {
                let sources = app::list_price_sources(&config.data_dir)?;
                println!("{}", serde_json::to_string_pretty(&sources)?);
            }

            ListCommand::Balances => {
                let balances = app::list_balances(storage_arc.as_ref(), &config).await?;
                println!("{}", serde_json::to_string_pretty(&balances)?);
            }

            ListCommand::Transactions {
                start,
                end,
                sort_by_amount,
                include_ignored,
            } => {
                let transactions = app::list_transactions(
                    storage_arc.as_ref(),
                    start,
                    end,
                    None,
                    sort_by_amount,
                    !include_ignored,
                    &config,
                )
                .await?;
                println!("{}", serde_json::to_string_pretty(&transactions)?);
            }

            ListCommand::RecurringTransactions {
                start,
                end,
                include_ignored,
                include_possible,
                min_confidence,
                include_dismissed,
            } => {
                let recurring = app::list_reviewed_recurring_transactions(
                    storage_arc.as_ref(),
                    app::RecurringTransactionsOptions {
                        start,
                        end,
                        include_ignored,
                        include_possible,
                        min_confidence,
                    },
                    include_dismissed,
                    &config,
                )
                .await?;
                println!("{}", serde_json::to_string_pretty(&recurring)?);
            }

            ListCommand::All => {
                let output = app::list_all(storage_arc.as_ref(), &config).await?;
                println!("{}", serde_json::to_string_pretty(&output)?);
            }
        },

        Some(Command::Tui {
            view,
            net_worth_interval,
        }) => {
            keepbook::tui::run_tui(
                storage_arc.clone(),
                &config,
                keepbook::tui::TuiOptions {
                    start_view: view.into(),
                    net_worth_interval: net_worth_interval.into(),
                },
            )
            .await?;
        }

        Some(Command::Portfolio(portfolio_cmd)) => match portfolio_cmd {
            PortfolioCommand::Snapshot {
                currency,
                date,
                group_by,
                detail,
                capital_gains_tax_rate,
                equity_change_percent,
                target_pre_tax_total_value,
                auto,
                offline,
                dry_run,
                force_refresh,
            } => {
                let snapshot = app::portfolio_snapshot(
                    storage_arc.clone(),
                    &config,
                    app::PortfolioSnapshotRequest {
                        currency,
                        date,
                        group_by,
                        detail,
                        capital_gains_tax_rate,
                        equity_change_percent,
                        target_pre_tax_total_value,
                        auto,
                        offline,
                        dry_run,
                        force_refresh,
                    },
                )
                .await?;
                println!("{}", serde_json::to_string_pretty(&snapshot)?);
            }

            PortfolioCommand::Assets {
                date,
                include_amount_changes,
            } => {
                let output = app::portfolio_assets(
                    storage_arc.clone(),
                    &config,
                    date,
                    include_amount_changes,
                )
                .await?;
                println!("{}", serde_json::to_string_pretty(&output)?);
            }

            PortfolioCommand::TaxImpact {
                currency,
                date,
                capital_gains_tax_rate,
                min,
                max,
                points,
            } => {
                let output = app::portfolio_tax_impact(
                    storage_arc.clone(),
                    &config,
                    app::PortfolioTaxImpactRequest {
                        currency,
                        date,
                        capital_gains_tax_rate,
                        min,
                        max,
                        points,
                    },
                )
                .await?;
                println!("{}", serde_json::to_string_pretty(&output)?);
            }

            PortfolioCommand::History {
                currency,
                start,
                end,
                granularity,
                include_prices,
                no_include_prices,
                account,
                connection,
            } => {
                let granularity =
                    granularity.unwrap_or_else(|| config.history.portfolio_granularity.clone());
                let include_prices = if no_include_prices {
                    false
                } else if include_prices {
                    true
                } else {
                    config.history.include_prices
                };
                let selection = app::resolve_portfolio_history_selection(
                    storage_arc.as_ref(),
                    &config,
                    account.as_deref(),
                    connection.as_deref(),
                )
                .await?;
                let output = match selection {
                    app::PortfolioHistorySelection::Portfolio => {
                        app::portfolio_history(
                            storage_arc.clone(),
                            &config,
                            currency,
                            start,
                            end,
                            granularity,
                            include_prices,
                            false,
                        )
                        .await?
                    }
                    app::PortfolioHistorySelection::Accounts(account_ids) => {
                        app::portfolio_history_for_accounts(
                            storage_arc.clone(),
                            &config,
                            currency,
                            start,
                            end,
                            granularity,
                            include_prices,
                            false,
                            account_ids,
                        )
                        .await?
                    }
                    app::PortfolioHistorySelection::LatentCapitalGainsTax => {
                        app::latent_capital_gains_tax_history(
                            storage_arc.clone(),
                            &config,
                            currency,
                            start,
                            end,
                            granularity,
                            include_prices,
                            false,
                        )
                        .await?
                    }
                };
                println!("{}", serde_json::to_string_pretty(&output)?);
            }

            PortfolioCommand::ChangePoints {
                start,
                end,
                granularity,
                include_prices,
                no_include_prices,
            } => {
                let granularity =
                    granularity.unwrap_or_else(|| config.history.change_points_granularity.clone());
                let include_prices = if no_include_prices {
                    false
                } else if include_prices {
                    true
                } else {
                    config.history.include_prices
                };
                let output = app::portfolio_change_points(
                    storage_arc.clone(),
                    &config,
                    start,
                    end,
                    granularity,
                    include_prices,
                )
                .await?;
                println!("{}", serde_json::to_string_pretty(&output)?);
            }
        },

        Some(Command::Spending(args)) => {
            let SpendingArgs {
                period,
                period_alignment,
                start,
                end,
                currency,
                tz,
                week_start,
                bucket,
                account,
                connection,
                status,
                direction,
                group_by,
                top,
                lookback_days,
                include_noncurrency,
                include_empty,
            } = args;
            let output = app::spending_report(
                storage_arc.as_ref(),
                &config,
                app::SpendingReportOptions {
                    currency,
                    start,
                    end,
                    period,
                    period_alignment: Some(period_alignment),
                    tz,
                    week_start,
                    bucket,
                    account,
                    connection,
                    status,
                    direction,
                    group_by,
                    top,
                    lookback_days,
                    include_noncurrency,
                    include_empty,
                },
            )
            .await?;
            println!("{}", serde_json::to_string_pretty(&output)?);
        }

        Some(Command::SpendingTags(args)) => {
            let SpendingTagsArgs {
                period,
                period_alignment,
                start,
                end,
                currency,
                tz,
                week_start,
                bucket,
                account,
                connection,
                status,
                direction,
                top,
                lookback_days,
                include_noncurrency,
                include_empty,
            } = args;
            let output = app::spending_report(
                storage_arc.as_ref(),
                &config,
                app::SpendingReportOptions {
                    currency,
                    start,
                    end,
                    period,
                    period_alignment: Some(period_alignment),
                    tz,
                    week_start,
                    bucket,
                    account,
                    connection,
                    status,
                    direction,
                    group_by: "tag".to_string(),
                    top,
                    lookback_days,
                    include_noncurrency,
                    include_empty,
                },
            )
            .await?;
            println!("{}", serde_json::to_string_pretty(&output)?);
        }

        None => {
            Cli::command().print_help()?;
        }
    }

    Ok(())
}
