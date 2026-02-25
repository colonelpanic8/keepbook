use std::collections::HashMap;
use std::io::{self, Write};
use std::sync::Arc;

use anyhow::{Context, Result};

use crate::config::ResolvedConfig;
use crate::market_data::MarketDataServiceBuilder;
use crate::models::{Connection, Id};
use crate::storage::{CompactionStorage, Storage, SymlinkStorage};
use crate::sync::{
    AuthPrompter, DefaultSynchronizerFactory, GitAutoCommitter, SyncContext, SyncOptions,
    SyncOutcome, SyncService, TransactionSyncMode,
};

use super::maybe_auto_commit;

struct StdinPrompter;

impl AuthPrompter for StdinPrompter {
    fn confirm_login(&self, prompt: &str) -> Result<bool> {
        eprint!("{prompt} [Y/n] ");
        io::stderr().flush()?;
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let input = input.trim().to_lowercase();
        Ok(input.is_empty() || input == "y" || input == "yes")
    }
}

pub(crate) async fn build_sync_service(
    storage: Arc<dyn Storage>,
    config: &ResolvedConfig,
) -> SyncService {
    build_sync_service_with_quote_staleness(storage, config, None).await
}

async fn build_sync_service_with_quote_staleness(
    storage: Arc<dyn Storage>,
    config: &ResolvedConfig,
    quote_staleness_override: Option<std::time::Duration>,
) -> SyncService {
    let quote_staleness = quote_staleness_override.unwrap_or(config.refresh.price_staleness);
    let market_data = MarketDataServiceBuilder::for_data_dir(&config.data_dir)
        .with_quote_staleness(quote_staleness)
        .build()
        .await;
    let context = SyncContext::new(storage, market_data, config.reporting_currency.clone())
        .with_auth_prompter(Arc::new(StdinPrompter))
        .with_auto_committer(Arc::new(GitAutoCommitter::new(
            config.data_dir.clone(),
            config.git.auto_commit,
            config.git.auto_push,
        )))
        .with_factory(Arc::new(DefaultSynchronizerFactory::new(Some(
            config.data_dir.clone(),
        ))));

    SyncService::new(context)
}

fn connection_object(connection: &Connection) -> serde_json::Value {
    serde_json::json!({
        "id": connection.id().to_string(),
        "name": connection.config.name
    })
}

fn sync_outcome_to_json(outcome: SyncOutcome) -> serde_json::Value {
    match outcome {
        SyncOutcome::Synced { report } => {
            let connection = &report.result.connection;
            let mut output = serde_json::json!({
                "success": true,
                "connection": connection_object(connection),
                "accounts_synced": report.result.accounts.len(),
                "prices_stored": report.stored_prices + report.refresh.fetched,
                "last_sync": report.result.connection.state.last_sync.as_ref().map(|ls| ls.at.to_rfc3339())
            });
            if connection.config.synchronizer == "chase" {
                output["downloaded"] = connection.state.synchronizer_data.clone();
            }
            output
        }
        SyncOutcome::SkippedManual { connection } => serde_json::json!({
            "success": true,
            "skipped": true,
            "reason": "manual",
            "connection": connection_object(&connection),
            "accounts_synced": 0,
            "prices_stored": 0,
            "last_sync": None::<String>
        }),
        SyncOutcome::SkippedNotStale { connection } => serde_json::json!({
            "success": true,
            "skipped": true,
            "reason": "not stale",
            "connection": connection.config.name
        }),
        SyncOutcome::AuthRequired { connection, error } => serde_json::json!({
            "success": false,
            "error": error,
            "connection": connection.config.name
        }),
        SyncOutcome::Failed { connection, error } => serde_json::json!({
            "success": false,
            "connection": connection.config.name,
            "error": error
        }),
    }
}

#[derive(Debug, Clone, Copy)]
enum PriceSyncScope {
    All,
    Connection,
    Account,
}

#[derive(Debug, Clone, Copy)]
pub enum SyncPricesScopeArg<'a> {
    /// Prompt user to choose (all/connection/account).
    Interactive,
    /// Use all accounts (based on latest stored balances).
    All,
    /// Use a specific connection; if None, prompt user to select one.
    Connection(Option<&'a str>),
    /// Use a specific account; if None, prompt user to select one.
    Account(Option<&'a str>),
}

fn prompt_select_index(prompt: &str, options: &[String]) -> Result<Option<usize>> {
    prompt_select_index_impl(prompt, options)
}

#[cfg(feature = "tui")]
fn prompt_select_index_impl(prompt: &str, options: &[String]) -> Result<Option<usize>> {
    use dialoguer::console::Term;
    use dialoguer::{theme::ColorfulTheme, Select};

    if options.is_empty() {
        return Ok(None);
    }

    let selection = Select::with_theme(&ColorfulTheme::default())
        .with_prompt(prompt)
        .items(options)
        .default(0)
        .interact_on_opt(&Term::stderr())
        .context("Failed to prompt for selection")?;

    Ok(selection)
}

#[cfg(not(feature = "tui"))]
fn prompt_select_index_impl(prompt: &str, options: &[String]) -> Result<Option<usize>> {
    if options.is_empty() {
        return Ok(None);
    }

    eprintln!("{prompt}");
    for (i, opt) in options.iter().enumerate() {
        eprintln!("{}) {opt}", i + 1);
    }
    loop {
        eprint!("Select [1-{}] (Enter to cancel): ", options.len());
        io::stderr().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let input = input.trim();
        if input.is_empty() {
            return Ok(None);
        }
        if input.eq_ignore_ascii_case("q") || input.eq_ignore_ascii_case("quit") {
            return Ok(None);
        }

        let n: usize = match input.parse() {
            Ok(n) => n,
            Err(_) => {
                eprintln!("Invalid selection: {input}");
                continue;
            }
        };
        if n == 0 || n > options.len() {
            eprintln!("Selection out of range: {n}");
            continue;
        }
        return Ok(Some(n - 1));
    }
}

async fn prompt_price_sync_scope(
    storage: &dyn Storage,
) -> Result<Option<(PriceSyncScope, Option<String>)>> {
    let mode_options = vec![
        "All (use latest balances across all accounts)".to_string(),
        "A connection".to_string(),
        "An account".to_string(),
    ];
    let idx = match prompt_select_index("What prices do you want to refresh?", &mode_options)? {
        Some(i) => i,
        None => return Ok(None),
    };

    match idx {
        0 => Ok(Some((PriceSyncScope::All, None))),
        1 => {
            let connections = storage.list_connections().await?;
            if connections.is_empty() {
                anyhow::bail!("No connections found");
            }
            let options: Vec<String> = connections
                .iter()
                .map(|c| format!("{} ({}) [{}]", c.config.name, c.id(), c.config.synchronizer))
                .collect();
            let sel = prompt_select_index("Select a connection:", &options)?;
            let Some(sel) = sel else { return Ok(None) };
            Ok(Some((
                PriceSyncScope::Connection,
                Some(connections[sel].id().to_string()),
            )))
        }
        2 => {
            let accounts = storage.list_accounts().await?;
            if accounts.is_empty() {
                anyhow::bail!("No accounts found");
            }
            let connections = storage.list_connections().await?;
            let conn_by_id: HashMap<Id, String> = connections
                .into_iter()
                .map(|c| (c.id().clone(), c.config.name))
                .collect();

            let options: Vec<String> = accounts
                .iter()
                .map(|a| {
                    let conn_name = conn_by_id
                        .get(&a.connection_id)
                        .cloned()
                        .unwrap_or_else(|| a.connection_id.to_string());
                    format!("{} ({}) [connection: {}]", a.name, a.id, conn_name)
                })
                .collect();
            let sel = prompt_select_index("Select an account:", &options)?;
            let Some(sel) = sel else { return Ok(None) };
            Ok(Some((
                PriceSyncScope::Account,
                Some(accounts[sel].id.to_string()),
            )))
        }
        _ => unreachable!(),
    }
}

fn price_refresh_result_to_json(result: crate::sync::PriceRefreshResult) -> serde_json::Value {
    let failures: Vec<_> = result
        .failed
        .into_iter()
        .map(|(asset, error)| serde_json::json!({ "asset": asset, "error": error }))
        .collect();
    serde_json::json!({
        "fetched": result.fetched,
        "skipped": result.skipped,
        "failed_count": failures.len(),
        "failures": failures,
    })
}

pub async fn sync_connection(
    storage: Arc<dyn Storage>,
    config: &ResolvedConfig,
    id_or_name: &str,
    transactions: TransactionSyncMode,
) -> Result<serde_json::Value> {
    let service = build_sync_service(storage, config).await;
    let options = SyncOptions { transactions };
    let outcome = service
        .sync_connection_with_options(id_or_name, &options)
        .await?;
    Ok(sync_outcome_to_json(outcome))
}

pub async fn sync_connection_if_stale(
    storage: Arc<dyn Storage>,
    config: &ResolvedConfig,
    id_or_name: &str,
    transactions: TransactionSyncMode,
) -> Result<serde_json::Value> {
    let service = build_sync_service(storage, config).await;
    let options = SyncOptions { transactions };
    let outcome = service
        .sync_connection_if_stale_with_options(id_or_name, &config.refresh, &options)
        .await?;
    Ok(sync_outcome_to_json(outcome))
}

pub async fn sync_all(
    storage: Arc<dyn Storage>,
    config: &ResolvedConfig,
    transactions: TransactionSyncMode,
) -> Result<serde_json::Value> {
    let service = build_sync_service(storage, config).await;
    let options = SyncOptions { transactions };
    let outcomes = service.sync_all_with_options(&options).await?;
    let results: Vec<_> = outcomes.into_iter().map(sync_outcome_to_json).collect();

    Ok(serde_json::json!({
        "results": results,
        "total": results.len()
    }))
}

pub async fn sync_all_if_stale(
    storage: Arc<dyn Storage>,
    config: &ResolvedConfig,
    transactions: TransactionSyncMode,
) -> Result<serde_json::Value> {
    let service = build_sync_service(storage, config).await;
    let options = SyncOptions { transactions };
    let outcomes = service
        .sync_all_if_stale_with_options(&config.refresh, &options)
        .await?;
    let results: Vec<_> = outcomes.into_iter().map(sync_outcome_to_json).collect();

    Ok(serde_json::json!({
        "results": results,
        "total": results.len()
    }))
}

pub async fn sync_prices(
    storage: Arc<dyn Storage>,
    config: &ResolvedConfig,
    scope: SyncPricesScopeArg<'_>,
    force: bool,
    quote_staleness_override: Option<std::time::Duration>,
) -> Result<serde_json::Value> {
    let service =
        build_sync_service_with_quote_staleness(storage.clone(), config, quote_staleness_override)
            .await;

    // Resolve scope. If not fully specified, prompt.
    let (scope, target): (PriceSyncScope, Option<String>) = match scope {
        SyncPricesScopeArg::All => (PriceSyncScope::All, None),
        SyncPricesScopeArg::Connection(Some(id_or_name)) => {
            (PriceSyncScope::Connection, Some(id_or_name.to_string()))
        }
        SyncPricesScopeArg::Account(Some(id_or_name)) => {
            (PriceSyncScope::Account, Some(id_or_name.to_string()))
        }
        SyncPricesScopeArg::Connection(None) => {
            let connections = storage.list_connections().await?;
            if connections.is_empty() {
                anyhow::bail!("No connections found");
            }
            let options: Vec<String> = connections
                .iter()
                .map(|c| format!("{} ({}) [{}]", c.config.name, c.id(), c.config.synchronizer))
                .collect();
            let sel = prompt_select_index("Select a connection:", &options)?;
            let Some(sel) = sel else {
                return Ok(serde_json::json!({ "success": false, "cancelled": true }));
            };
            (
                PriceSyncScope::Connection,
                Some(connections[sel].id().to_string()),
            )
        }
        SyncPricesScopeArg::Account(None) => {
            let accounts = storage.list_accounts().await?;
            if accounts.is_empty() {
                anyhow::bail!("No accounts found");
            }
            let connections = storage.list_connections().await?;
            let conn_by_id: HashMap<Id, String> = connections
                .into_iter()
                .map(|c| (c.id().clone(), c.config.name))
                .collect();

            let options: Vec<String> = accounts
                .iter()
                .map(|a| {
                    let conn_name = conn_by_id
                        .get(&a.connection_id)
                        .cloned()
                        .unwrap_or_else(|| a.connection_id.to_string());
                    format!("{} ({}) [connection: {}]", a.name, a.id, conn_name)
                })
                .collect();
            let sel = prompt_select_index("Select an account:", &options)?;
            let Some(sel) = sel else {
                return Ok(serde_json::json!({ "success": false, "cancelled": true }));
            };
            (PriceSyncScope::Account, Some(accounts[sel].id.to_string()))
        }
        SyncPricesScopeArg::Interactive => match prompt_price_sync_scope(storage.as_ref()).await? {
            Some((s, t)) => (s, t),
            None => {
                return Ok(serde_json::json!({
                    "success": false,
                    "cancelled": true
                }));
            }
        },
    };

    let result = match (scope, target.as_deref()) {
        (PriceSyncScope::All, _) => service.sync_prices_all(force).await?,
        (PriceSyncScope::Connection, Some(id_or_name)) => {
            service.sync_prices_connection(id_or_name, force).await?
        }
        (PriceSyncScope::Account, Some(id_or_name)) => {
            service.sync_prices_account(id_or_name, force).await?
        }
        _ => anyhow::bail!("Invalid sync prices scope"),
    };

    let scope_json = match (scope, target) {
        (PriceSyncScope::All, _) => serde_json::json!({ "type": "all" }),
        (PriceSyncScope::Connection, Some(t)) => {
            serde_json::json!({ "type": "connection", "id_or_name": t })
        }
        (PriceSyncScope::Account, Some(t)) => {
            serde_json::json!({ "type": "account", "id_or_name": t })
        }
        _ => serde_json::Value::Null,
    };

    Ok(serde_json::json!({
        "success": true,
        "scope": scope_json,
        "force": force,
        "quote_staleness_override_seconds": quote_staleness_override.map(|d| d.as_secs()),
        "result": price_refresh_result_to_json(result),
    }))
}

pub async fn sync_symlinks(
    storage: &dyn SymlinkStorage,
    config: &ResolvedConfig,
) -> Result<serde_json::Value> {
    let (conn_created, acct_created, warnings) = storage.rebuild_all_symlinks().await?;
    let result = serde_json::json!({
        "connection_symlinks_created": conn_created,
        "account_symlinks_created": acct_created,
        "warnings": warnings,
    });

    maybe_auto_commit(config, "sync symlinks");

    Ok(result)
}

pub async fn sync_recompact(
    storage: &dyn CompactionStorage,
    config: &ResolvedConfig,
) -> Result<serde_json::Value> {
    let stats = storage.recompact_all_jsonl().await?;
    maybe_auto_commit(config, "sync recompact");
    Ok(serde_json::to_value(stats)?)
}

pub async fn schwab_login(
    storage: Arc<dyn Storage>,
    config: &ResolvedConfig,
    id_or_name: Option<&str>,
) -> Result<serde_json::Value> {
    let service = build_sync_service(storage, config).await;
    let connection = service.login("schwab", id_or_name).await?;

    Ok(serde_json::json!({
        "success": true,
        "connection": connection_object(&connection),
        "message": "Session captured successfully"
    }))
}

pub async fn chase_login(
    storage: Arc<dyn Storage>,
    config: &ResolvedConfig,
    id_or_name: Option<&str>,
) -> Result<serde_json::Value> {
    let service = build_sync_service(storage, config).await;
    let connection = service.login("chase", id_or_name).await?;

    Ok(serde_json::json!({
        "success": true,
        "connection": connection_object(&connection),
        "message": "Session captured successfully"
    }))
}
