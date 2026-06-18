use std::collections::HashMap;
use std::io::{self, Write};
use std::sync::Arc;

use anyhow::Result;

use crate::config::ResolvedConfig;
use crate::market_data::{JsonlMarketDataStore, MarketDataServiceBuilder};
use crate::models::{Connection, Id};
use crate::storage::{CompactionStorage, MetadataBackfillStorage, Storage, SymlinkStorage};
use crate::sync::{
    AuthPrompter, DefaultSynchronizerFactory, FixedAuthPrompter, GitAutoCommitter, SyncContext,
    SyncOptions, SyncOutcome, SyncService, TransactionSyncMode,
};

use super::{
    apply_transaction_rules_without_auto_commit, maybe_auto_commit, ApplyTransactionRulesOptions,
};

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

fn env_enabled(key: &str) -> bool {
    match std::env::var(key) {
        Ok(v) => !(v == "0" || v.eq_ignore_ascii_case("false") || v.eq_ignore_ascii_case("no")),
        Err(_) => false,
    }
}

fn env_disabled(key: &str) -> bool {
    match std::env::var(key) {
        Ok(v) => v == "1" || v.eq_ignore_ascii_case("true") || v.eq_ignore_ascii_case("yes"),
        Err(_) => false,
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
    let auth_prompter: Arc<dyn AuthPrompter> = if env_enabled("KEEPBOOK_AUTO_LOGIN") {
        Arc::new(FixedAuthPrompter::allow())
    } else if env_enabled("KEEPBOOK_NONINTERACTIVE") {
        Arc::new(FixedAuthPrompter::deny())
    } else {
        Arc::new(StdinPrompter)
    };
    let mut git_config = config.git.clone();
    git_config.auto_commit =
        config.git.auto_commit && !env_disabled("KEEPBOOK_DISABLE_AUTO_COMMIT");
    git_config.auto_push = config.git.auto_push && !env_disabled("KEEPBOOK_DISABLE_AUTO_PUSH");
    let context = SyncContext::new(storage, market_data, config.reporting_currency.clone())
        .with_auth_prompter(auth_prompter)
        .with_auto_committer(Arc::new(GitAutoCommitter::new(
            config.data_dir.clone(),
            git_config,
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

fn transaction_rules_apply_summary(result: &serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "success": result.get("success").cloned().unwrap_or(serde_json::Value::Bool(false)),
        "path": result.get("path").cloned().unwrap_or(serde_json::Value::Null),
        "rule_count": result.get("rule_count").cloned().unwrap_or(serde_json::Value::from(0)),
        "invalid_rule_count": result.get("invalid_rule_count").cloned().unwrap_or(serde_json::Value::from(0)),
        "matched_count": result.get("matched_count").cloned().unwrap_or(serde_json::Value::from(0)),
        "updated_count": result.get("updated_count").cloned().unwrap_or(serde_json::Value::from(0)),
        "skipped_existing_action_count": result
            .get("skipped_existing_action_count")
            .cloned()
            .unwrap_or(serde_json::Value::from(0)),
    })
}

fn transaction_rules_updated_count(result: &serde_json::Value) -> u64 {
    result
        .get("updated_count")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0)
}

async fn apply_transaction_rules_after_synced_outcome(
    storage: &dyn Storage,
    config: &ResolvedConfig,
    outcome: &SyncOutcome,
) -> Result<Option<serde_json::Value>> {
    let SyncOutcome::Synced { report } = outcome else {
        return Ok(None);
    };

    let connection_id = report.result.connection.id().to_string();
    let result = apply_transaction_rules_without_auto_commit(
        storage,
        config,
        ApplyTransactionRulesOptions {
            start: None,
            end: None,
            account: None,
            connection: Some(connection_id),
            overwrite: false,
            dry_run: false,
        },
    )
    .await?;

    Ok(Some(result))
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
    use anyhow::Context;
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
    let service = build_sync_service(storage.clone(), config).await;
    let options = SyncOptions { transactions };
    let outcome = service
        .sync_connection_with_options(id_or_name, &options)
        .await?;
    let rule_result =
        apply_transaction_rules_after_synced_outcome(storage.as_ref(), config, &outcome).await?;
    if rule_result
        .as_ref()
        .map(transaction_rules_updated_count)
        .unwrap_or(0)
        > 0
    {
        maybe_auto_commit(
            config,
            &format!("apply transaction rules after sync {id_or_name}"),
        );
    }

    let mut output = sync_outcome_to_json(outcome);
    if let Some(rule_result) = rule_result {
        output["transaction_rules"] = transaction_rules_apply_summary(&rule_result);
    }
    Ok(output)
}

pub async fn sync_connection_if_stale(
    storage: Arc<dyn Storage>,
    config: &ResolvedConfig,
    id_or_name: &str,
    transactions: TransactionSyncMode,
) -> Result<serde_json::Value> {
    let service = build_sync_service(storage.clone(), config).await;
    let options = SyncOptions { transactions };
    let outcome = service
        .sync_connection_if_stale_with_options(id_or_name, &config.refresh, &options)
        .await?;
    let rule_result =
        apply_transaction_rules_after_synced_outcome(storage.as_ref(), config, &outcome).await?;
    if rule_result
        .as_ref()
        .map(transaction_rules_updated_count)
        .unwrap_or(0)
        > 0
    {
        maybe_auto_commit(
            config,
            &format!("apply transaction rules after sync {id_or_name}"),
        );
    }

    let mut output = sync_outcome_to_json(outcome);
    if let Some(rule_result) = rule_result {
        output["transaction_rules"] = transaction_rules_apply_summary(&rule_result);
    }
    Ok(output)
}

pub async fn sync_all(
    storage: Arc<dyn Storage>,
    config: &ResolvedConfig,
    transactions: TransactionSyncMode,
) -> Result<serde_json::Value> {
    let service = build_sync_service(storage.clone(), config).await;
    let options = SyncOptions { transactions };
    let outcomes = service.sync_all_with_options(&options).await?;
    let mut any_rule_updates = false;
    let mut results = Vec::with_capacity(outcomes.len());
    for outcome in outcomes {
        let rule_result =
            apply_transaction_rules_after_synced_outcome(storage.as_ref(), config, &outcome)
                .await?;
        if rule_result
            .as_ref()
            .map(transaction_rules_updated_count)
            .unwrap_or(0)
            > 0
        {
            any_rule_updates = true;
        }
        let mut result = sync_outcome_to_json(outcome);
        if let Some(rule_result) = rule_result {
            result["transaction_rules"] = transaction_rules_apply_summary(&rule_result);
        }
        results.push(result);
    }
    if any_rule_updates {
        maybe_auto_commit(config, "apply transaction rules after sync all");
    }

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
    let service = build_sync_service(storage.clone(), config).await;
    let options = SyncOptions { transactions };
    let outcomes = service
        .sync_all_if_stale_with_options(&config.refresh, &options)
        .await?;
    let mut any_rule_updates = false;
    let mut results = Vec::with_capacity(outcomes.len());
    for outcome in outcomes {
        let rule_result =
            apply_transaction_rules_after_synced_outcome(storage.as_ref(), config, &outcome)
                .await?;
        if rule_result
            .as_ref()
            .map(transaction_rules_updated_count)
            .unwrap_or(0)
            > 0
        {
            any_rule_updates = true;
        }
        let mut result = sync_outcome_to_json(outcome);
        if let Some(rule_result) = rule_result {
            result["transaction_rules"] = transaction_rules_apply_summary(&rule_result);
        }
        results.push(result);
    }
    if any_rule_updates {
        maybe_auto_commit(config, "apply transaction rules after sync all");
    }

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
    let storage_stats = storage.recompact_all_jsonl().await?;
    let market_data_stats = JsonlMarketDataStore::new(&config.data_dir)
        .recompact_all_jsonl()
        .await?;
    maybe_auto_commit(config, "sync recompact");
    Ok(serde_json::json!({
        "storage_jsonl": storage_stats,
        "market_data_jsonl": market_data_stats,
    }))
}

pub async fn sync_backfill_metadata(
    storage: &dyn MetadataBackfillStorage,
    config: &ResolvedConfig,
) -> Result<serde_json::Value> {
    let stats = storage.backfill_transaction_metadata_all().await?;
    maybe_auto_commit(config, "sync backfill-metadata");
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

#[cfg(test)]
#[path = "../../tests/unit/app/sync_tests.rs"]
mod sync_tests;
