use std::sync::Arc;

use anyhow::Result;
use keepbook::app;
use keepbook::config::ResolvedConfig;
use keepbook::storage::{JsonFileStorage, Storage};
use keepbook::sync::TransactionSyncMode;

use crate::cli::{PriceSyncScopeCommand, SyncCommand};

pub async fn run(
    command: SyncCommand,
    storage: &JsonFileStorage,
    storage_arc: &Arc<dyn Storage>,
    config: &ResolvedConfig,
    push_after_sync: bool,
) -> Result<()> {
    match command {
        SyncCommand::Connection {
            id_or_name,
            if_stale,
            transactions,
        } => {
            let transactions: TransactionSyncMode = transactions.into();
            let result = if if_stale {
                app::sync_connection_if_stale(
                    storage_arc.clone(),
                    config,
                    &id_or_name,
                    transactions,
                )
                .await?
            } else {
                app::sync_connection(storage_arc.clone(), config, &id_or_name, transactions).await?
            };
            app::maybe_push_after_sync(config, push_after_sync)?;
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
        SyncCommand::All {
            if_stale,
            transactions,
        } => {
            let transactions: TransactionSyncMode = transactions.into();
            let result = if if_stale {
                app::sync_all_if_stale(storage_arc.clone(), config, transactions).await?
            } else {
                app::sync_all(storage_arc.clone(), config, transactions).await?
            };
            app::maybe_push_after_sync(config, push_after_sync)?;
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
        SyncCommand::Prices { opts, scope } => {
            let result = app::sync_prices(
                storage_arc.clone(),
                config,
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
            app::maybe_push_after_sync(config, push_after_sync)?;
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
        SyncCommand::Symlinks => {
            let result = app::sync_symlinks(storage, config).await?;
            app::maybe_push_after_sync(config, push_after_sync)?;
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
        SyncCommand::Recompact => {
            let result = app::sync_recompact(storage, config).await?;
            app::maybe_push_after_sync(config, push_after_sync)?;
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
        SyncCommand::BackfillMetadata => {
            let result = app::sync_backfill_metadata(storage, config).await?;
            app::maybe_push_after_sync(config, push_after_sync)?;
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
    }
    Ok(())
}
