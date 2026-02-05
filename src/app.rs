use std::collections::{HashMap, HashSet};
use std::io::{self, Write};
use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result};
use chrono::{Datelike, Duration, NaiveDate, Utc};
use serde::Serialize;
use tracing::warn;

use crate::config::ResolvedConfig;
use crate::git::{try_auto_commit, AutoCommitOutcome};
use crate::market_data::{
    AssetId, CryptoPriceRouter, EquityPriceRouter, FxRateKind, FxRatePoint, FxRateRouter,
    JsonlMarketDataStore, MarketDataService, MarketDataStore, PriceKind, PricePoint,
    PriceSourceRegistry,
};
use crate::models::{
    Account, Asset, AssetBalance, BalanceSnapshot, Connection, ConnectionConfig, ConnectionState,
    Id,
};
use crate::portfolio::{
    collect_change_points, filter_by_date_range, filter_by_granularity, CoalesceStrategy,
    CollectOptions, Granularity, Grouping, PortfolioQuery, PortfolioService,
};
use crate::staleness::{
    check_balance_staleness, check_price_staleness, log_balance_staleness, log_price_staleness,
    resolve_balance_staleness,
};
use crate::storage::{JsonFileStorage, Storage};
use crate::sync::synchronizers::{CoinbaseSynchronizer, SchwabSynchronizer};
use crate::sync::{AuthStatus, InteractiveAuth};

/// JSON output for connections
#[derive(Serialize)]
pub struct ConnectionOutput {
    pub id: String,
    pub name: String,
    pub synchronizer: String,
    pub status: String,
    pub account_count: usize,
    pub last_sync: Option<String>,
}

/// JSON output for accounts
#[derive(Serialize)]
pub struct AccountOutput {
    pub id: String,
    pub name: String,
    pub connection_id: String,
    pub tags: Vec<String>,
    pub active: bool,
}

/// JSON output for price sources
#[derive(Serialize)]
pub struct PriceSourceOutput {
    pub name: String,
    #[serde(rename = "type")]
    pub source_type: String,
    pub enabled: bool,
    pub priority: u32,
    pub has_credentials: bool,
}

/// JSON output for balances
#[derive(Serialize)]
pub struct BalanceOutput {
    pub account_id: String,
    pub asset: serde_json::Value,
    pub amount: String,
    pub timestamp: String,
}

/// JSON output for transactions
#[derive(Serialize)]
pub struct TransactionOutput {
    pub id: String,
    pub account_id: String,
    pub timestamp: String,
    pub description: String,
    pub amount: String,
    pub asset: serde_json::Value,
    pub status: String,
}

/// Combined output for list all
#[derive(Serialize)]
pub struct AllOutput {
    pub connections: Vec<ConnectionOutput>,
    pub accounts: Vec<AccountOutput>,
    pub price_sources: Vec<PriceSourceOutput>,
    pub balances: Vec<BalanceOutput>,
}

/// A single point in the net worth history
#[derive(Serialize)]
pub struct HistoryPoint {
    pub timestamp: String,
    pub date: String,
    pub total_value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub change_triggers: Option<Vec<String>>,
}

/// Output for portfolio history command
#[derive(Serialize)]
pub struct HistoryOutput {
    pub currency: String,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub granularity: String,
    pub points: Vec<HistoryPoint>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<HistorySummary>,
}

/// Summary statistics for the history
#[derive(Serialize)]
pub struct HistorySummary {
    pub initial_value: String,
    pub final_value: String,
    pub absolute_change: String,
    pub percentage_change: String,
}

/// Scope output for market data history fetch
#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PriceHistoryScopeOutput {
    Portfolio,
    Connection { id: String, name: String },
    Account { id: String, name: String },
}

/// Asset info output for market data history fetch
#[derive(Serialize)]
pub struct AssetInfoOutput {
    pub asset: Asset,
    pub asset_id: String,
}

/// Summary stats for market data history fetch
#[derive(Default, Serialize)]
pub struct PriceHistoryStats {
    pub attempted: usize,
    pub existing: usize,
    pub fetched: usize,
    pub lookback: usize,
    pub missing: usize,
}

/// Failure details for market data history fetch (sampled)
#[derive(Serialize)]
pub struct PriceHistoryFailure {
    pub kind: String,
    pub date: String,
    pub error: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub asset_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub asset: Option<Asset>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quote: Option<String>,
}

/// Output for market data history fetch
#[derive(Serialize)]
pub struct PriceHistoryOutput {
    pub scope: PriceHistoryScopeOutput,
    pub currency: String,
    pub interval: String,
    pub start_date: String,
    pub end_date: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub earliest_balance_date: Option<String>,
    pub days: usize,
    pub points: usize,
    pub assets: Vec<AssetInfoOutput>,
    pub prices: PriceHistoryStats,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fx: Option<PriceHistoryStats>,
    pub failure_count: usize,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub failures: Vec<PriceHistoryFailure>,
}

pub fn config_output(config_path: &Path, config: &ResolvedConfig) -> serde_json::Value {
    serde_json::json!({
        "config_file": config_path.display().to_string(),
        "data_directory": config.data_dir.display().to_string(),
        "git": {
            "auto_commit": config.git.auto_commit
        }
    })
}

pub async fn list_connections(storage: &JsonFileStorage) -> Result<Vec<ConnectionOutput>> {
    let connections = storage.list_connections().await?;
    let accounts = storage.list_accounts().await?;
    let mut accounts_by_connection: HashMap<Id, HashSet<Id>> = HashMap::new();
    for account in accounts {
        accounts_by_connection
            .entry(account.connection_id.clone())
            .or_default()
            .insert(account.id.clone());
    }
    let mut output = Vec::new();

    for c in connections {
        let valid_ids = accounts_by_connection
            .get(c.id())
            .cloned()
            .unwrap_or_default();
        let mut account_ids: HashSet<Id> = c
            .state
            .account_ids
            .iter()
            .filter(|id| valid_ids.contains(*id))
            .cloned()
            .collect();
        for account_id in valid_ids {
            account_ids.insert(account_id);
        }

        output.push(ConnectionOutput {
            id: c.id().to_string(),
            name: c.config.name.clone(),
            synchronizer: c.config.synchronizer.clone(),
            status: format!("{:?}", c.state.status).to_lowercase(),
            account_count: account_ids.len(),
            last_sync: c
                .state
                .last_sync
                .as_ref()
                .map(|ls| ls.at.to_rfc3339()),
        });
    }

    Ok(output)
}

pub async fn list_accounts(storage: &JsonFileStorage) -> Result<Vec<AccountOutput>> {
    let accounts = storage.list_accounts().await?;
    let mut output = Vec::new();

    for a in accounts {
        output.push(AccountOutput {
            id: a.id.to_string(),
            name: a.name.clone(),
            connection_id: a.connection_id.to_string(),
            tags: a.tags.clone(),
            active: a.active,
        });
    }

    Ok(output)
}

pub fn list_price_sources(data_dir: &Path) -> Result<Vec<PriceSourceOutput>> {
    let mut registry = PriceSourceRegistry::new(data_dir);
    registry.load()?;

    let mut output = Vec::new();
    for s in registry.sources() {
        output.push(PriceSourceOutput {
            name: s.name.clone(),
            source_type: format!("{:?}", s.config.source_type).to_lowercase(),
            enabled: s.config.enabled,
            priority: s.config.priority,
            has_credentials: s.config.credentials.is_some(),
        });
    }

    Ok(output)
}

pub async fn list_balances(storage: &JsonFileStorage) -> Result<Vec<BalanceOutput>> {
    let connections = storage.list_connections().await?;
    let accounts = storage.list_accounts().await?;
    let mut accounts_by_connection: HashMap<Id, HashSet<Id>> = HashMap::new();
    for account in accounts {
        accounts_by_connection
            .entry(account.connection_id.clone())
            .or_default()
            .insert(account.id);
    }
    let mut output = Vec::new();

    for conn in connections {
        let valid_ids = accounts_by_connection
            .get(conn.id())
            .cloned()
            .unwrap_or_default();
        let mut account_ids: Vec<Id> = conn
            .state
            .account_ids
            .iter()
            .filter(|id| valid_ids.contains(*id))
            .cloned()
            .collect();
        let mut seen_ids: HashSet<Id> = account_ids.iter().cloned().collect();
        for account_id in valid_ids {
            if seen_ids.insert(account_id.clone()) {
                account_ids.push(account_id);
            }
        }

        for account_id in &account_ids {
            if let Some(snapshot) = storage.get_latest_balance_snapshot(account_id).await? {
                for balance in snapshot.balances {
                    output.push(BalanceOutput {
                        account_id: account_id.to_string(),
                        asset: serde_json::to_value(&balance.asset)?,
                        amount: balance.amount,
                        timestamp: snapshot.timestamp.to_rfc3339(),
                    });
                }
            }
        }
    }

    Ok(output)
}

pub async fn list_transactions(storage: &JsonFileStorage) -> Result<Vec<TransactionOutput>> {
    let accounts = storage.list_accounts().await?;
    let mut output = Vec::new();

    for account in accounts {
        let transactions = storage.get_transactions(&account.id).await?;
        for tx in transactions {
            output.push(TransactionOutput {
                id: tx.id.to_string(),
                account_id: account.id.to_string(),
                timestamp: tx.timestamp.to_rfc3339(),
                description: tx.description.clone(),
                amount: tx.amount.clone(),
                asset: serde_json::to_value(&tx.asset).unwrap_or_default(),
                status: format!("{:?}", tx.status).to_lowercase(),
            });
        }
    }

    Ok(output)
}

pub async fn list_all(
    storage: &JsonFileStorage,
    config: &ResolvedConfig,
) -> Result<AllOutput> {
    Ok(AllOutput {
        connections: list_connections(storage).await?,
        accounts: list_accounts(storage).await?,
        price_sources: list_price_sources(&config.data_dir)?,
        balances: list_balances(storage).await?,
    })
}

pub async fn remove_connection(
    storage: &JsonFileStorage,
    config: &ResolvedConfig,
    id_str: &str,
) -> Result<serde_json::Value> {
    let id = Id::from_string(id_str);

    // Get connection info first
    let connection = storage.get_connection(&id).await?;
    let conn = match connection {
        Some(c) => c,
        None => {
            return Ok(serde_json::json!({
                "success": false,
                "error": "Connection not found",
                "id": id_str
            }));
        }
    };

    let name = conn.config.name.clone();
    let accounts = storage.list_accounts().await?;
    let valid_ids: HashSet<Id> = accounts
        .iter()
        .filter(|account| account.connection_id == *conn.id())
        .map(|account| account.id.clone())
        .collect();

    let mut account_ids: Vec<Id> = conn
        .state
        .account_ids
        .iter()
        .filter(|id| valid_ids.contains(*id))
        .cloned()
        .collect();
    let mut seen_ids: HashSet<Id> = account_ids.iter().cloned().collect();

    // Also include any accounts still linked to this connection ID (handles stale state).
    for account in accounts {
        if account.connection_id == *conn.id() && seen_ids.insert(account.id.clone()) {
            account_ids.push(account.id);
        }
    }

    // Delete all accounts belonging to this connection
    let mut deleted_accounts = 0;
    for account_id in &account_ids {
        if storage.delete_account(account_id).await? {
            deleted_accounts += 1;
        }
    }

    // Delete the connection
    storage.delete_connection(&id).await?;

    let result = serde_json::json!({
        "success": true,
        "connection": {
            "id": id_str,
            "name": name
        },
        "deleted_accounts": deleted_accounts,
        "account_ids": account_ids.iter().map(|id| id.to_string()).collect::<Vec<_>>()
    });

    maybe_auto_commit(config, &format!("remove connection {id_str}"));

    Ok(result)
}

pub async fn add_connection(
    storage: &JsonFileStorage,
    config: &ResolvedConfig,
    name: &str,
) -> Result<serde_json::Value> {
    let connection = Connection {
        config: ConnectionConfig {
            name: name.to_string(),
            synchronizer: "manual".to_string(),
            credentials: None,
            balance_staleness: None,
        },
        state: ConnectionState::new(),
    };

    let id = connection.state.id.to_string();

    // Save the connection (this creates the directory structure)
    storage.save_connection(&connection).await?;

    // Also write the config TOML since save_connection only writes state
    let config_path = storage.connection_config_path(&connection.state.id);
    let config_toml = toml::to_string_pretty(&connection.config)?;
    tokio::fs::create_dir_all(config_path.parent().unwrap()).await?;
    tokio::fs::write(&config_path, config_toml).await?;

    let result = serde_json::json!({
        "success": true,
        "connection": {
            "id": id,
            "name": name,
            "synchronizer": "manual"
        }
    });

    maybe_auto_commit(config, &format!("add connection {name}"));

    Ok(result)
}

pub async fn add_account(
    storage: &JsonFileStorage,
    config: &ResolvedConfig,
    connection_id: &str,
    name: &str,
    tags: Vec<String>,
) -> Result<serde_json::Value> {
    let conn_id = Id::from_string(connection_id);

    // Verify connection exists
    let mut connection = storage
        .get_connection(&conn_id)
        .await?
        .context("Connection not found")?;

    // Create account
    let account = Account {
        id: Id::new(),
        name: name.to_string(),
        connection_id: conn_id.clone(),
        tags,
        created_at: Utc::now(),
        active: true,
        synchronizer_data: serde_json::Value::Null,
    };

    let account_id = account.id.to_string();

    // Save account
    storage.save_account(&account).await?;

    // Update connection's account_ids
    connection.state.account_ids.push(account.id);
    storage.save_connection(&connection).await?;

    let result = serde_json::json!({
        "success": true,
        "account": {
            "id": account_id,
            "name": name,
            "connection_id": connection_id
        }
    });

    maybe_auto_commit(config, &format!("add account {name}"));

    Ok(result)
}

pub async fn set_balance(
    storage: &JsonFileStorage,
    config: &ResolvedConfig,
    account_id: &str,
    asset_str: &str,
    amount: &str,
) -> Result<serde_json::Value> {
    let id = Id::from_string(account_id);

    // Verify account exists
    storage
        .get_account(&id)
        .await?
        .context("Account not found")?;

    // Parse asset string (formats: "USD", "equity:AAPL", "crypto:BTC")
    let asset = parse_asset(asset_str)?;

    // Create balance snapshot with single asset
    let asset_balance = AssetBalance::new(asset.clone(), amount);
    let snapshot = BalanceSnapshot::now(vec![asset_balance]);

    // Append balance snapshot
    storage.append_balance_snapshot(&id, &snapshot).await?;

    let result = serde_json::json!({
        "success": true,
        "balance": {
            "account_id": account_id,
            "asset": serde_json::to_value(&asset)?,
            "amount": amount,
            "timestamp": snapshot.timestamp.to_rfc3339()
        }
    });

    maybe_auto_commit(config, &format!("set balance {account_id} {asset_str}"));

    Ok(result)
}

pub async fn sync_connection(
    storage: &JsonFileStorage,
    config: &ResolvedConfig,
    id_or_name: &str,
) -> Result<serde_json::Value> {
    let mut connection = find_connection(storage, id_or_name)
        .await?
        .context(format!("Connection not found: {id_or_name}"))?;

    let conn_name = connection.config.name.clone();
    let conn_id = connection.id().to_string();
    let synchronizer_type = connection.config.synchronizer.clone();

    if synchronizer_type == "manual" {
        let output = serde_json::json!({
            "success": true,
            "skipped": true,
            "reason": "manual",
            "connection": {
                "id": conn_id,
                "name": conn_name
            },
            "accounts_synced": 0,
            "prices_stored": 0,
            "last_sync": None::<String>
        });

        return Ok(output);
    }

    // Handle auth check for Schwab
    if synchronizer_type == "schwab" {
        let mut synchronizer = SchwabSynchronizer::from_connection(&connection, storage).await?;

        match synchronizer.check_auth().await? {
            AuthStatus::Valid => {}
            AuthStatus::Missing => {
                // Prompt user
                print!("No session found. Run login now? [Y/n] ");
                io::stdout().flush()?;
                let mut input = String::new();
                io::stdin().read_line(&mut input)?;
                let input = input.trim().to_lowercase();

                if input.is_empty() || input == "y" || input == "yes" {
                    synchronizer.login().await?;
                } else {
                    return Ok(serde_json::json!({
                        "success": false,
                        "error": "No session available",
                        "connection": conn_name
                    }));
                }
            }
            AuthStatus::Expired { reason } => {
                print!("Session expired ({reason}). Run login now? [Y/n] ");
                io::stdout().flush()?;
                let mut input = String::new();
                io::stdin().read_line(&mut input)?;
                let input = input.trim().to_lowercase();

                if input.is_empty() || input == "y" || input == "yes" {
                    synchronizer.login().await?;
                } else {
                    return Ok(serde_json::json!({
                        "success": false,
                        "error": format!("Session expired: {}", reason),
                        "connection": conn_name
                    }));
                }
            }
        }

        // Now sync
        let result = synchronizer
            .sync_with_storage(&mut connection, storage)
            .await?;
        result.save(storage).await?;

        // Store prices from sync result
        let prices_stored = store_sync_prices(&result, config).await?;

        let output = serde_json::json!({
            "success": true,
            "connection": {
                "id": conn_id,
                "name": conn_name
            },
            "accounts_synced": result.accounts.len(),
            "prices_stored": prices_stored,
            "last_sync": result.connection.state.last_sync.as_ref().map(|ls| ls.at.to_rfc3339())
        });

        maybe_auto_commit(config, &format!("sync connection {id_or_name}"));

        return Ok(output);
    }

    // Handle Coinbase
    if synchronizer_type == "coinbase" {
        let synchronizer = CoinbaseSynchronizer::from_connection(&connection, storage).await?;
        let result = synchronizer
            .sync_with_storage(&mut connection, storage)
            .await?;
        result.save(storage).await?;

        // Coinbase doesn't provide prices, so fetch them from configured sources
        let prices_fetched = fetch_crypto_prices(&result, config).await.unwrap_or(0);

        let output = serde_json::json!({
            "success": true,
            "connection": {
                "id": conn_id,
                "name": conn_name
            },
            "accounts_synced": result.accounts.len(),
            "prices_stored": prices_fetched,
            "last_sync": result.connection.state.last_sync.as_ref().map(|ls| ls.at.to_rfc3339())
        });

        maybe_auto_commit(config, &format!("sync connection {id_or_name}"));

        return Ok(output);
    }

    Err(anyhow::anyhow!(
        "Unknown synchronizer type: {synchronizer_type}"
    ))
}

pub async fn sync_connection_if_stale(
    storage: &JsonFileStorage,
    config: &ResolvedConfig,
    id_or_name: &str,
) -> Result<serde_json::Value> {
    let connection = find_connection(storage, id_or_name)
        .await?
        .context(format!("Connection not found: {id_or_name}"))?;

    let threshold = resolve_balance_staleness(None, &connection, &config.refresh);
    let check = check_balance_staleness(&connection, threshold);

    if !check.is_stale {
        return Ok(serde_json::json!({
            "success": true,
            "skipped": true,
            "reason": "not stale",
            "connection": connection.config.name
        }));
    }

    sync_connection(storage, config, id_or_name).await
}

pub async fn sync_all(storage: &JsonFileStorage, config: &ResolvedConfig) -> Result<serde_json::Value> {
    let connections = storage.list_connections().await?;

    let mut results = Vec::new();
    for conn in connections {
        let id_or_name = conn.id().to_string();
        match sync_connection(storage, config, &id_or_name).await {
            Ok(result) => results.push(result),
            Err(e) => results.push(serde_json::json!({
                "success": false,
                "connection": conn.config.name,
                "error": e.to_string()
            })),
        }
    }

    let output = serde_json::json!({
        "results": results,
        "total": results.len()
    });

    maybe_auto_commit(config, "sync all");

    Ok(output)
}

pub async fn sync_all_if_stale(
    storage: &JsonFileStorage,
    config: &ResolvedConfig,
) -> Result<serde_json::Value> {
    let connections = storage.list_connections().await?;
    let mut results = Vec::new();

    for connection in connections {
        let threshold = resolve_balance_staleness(None, &connection, &config.refresh);
        let check = check_balance_staleness(&connection, threshold);

        if !check.is_stale {
            results.push(serde_json::json!({
                "success": true,
                "skipped": true,
                "reason": "not stale",
                "connection": connection.config.name
            }));
            continue;
        }

        match sync_connection(storage, config, connection.id().as_ref()).await {
            Ok(result) => results.push(result),
            Err(e) => results.push(serde_json::json!({
                "success": false,
                "connection": connection.config.name,
                "error": e.to_string()
            })),
        }
    }

    let output = serde_json::json!({
        "results": results,
        "total": results.len()
    });

    maybe_auto_commit(config, "sync all");

    Ok(output)
}

pub async fn sync_symlinks(
    storage: &JsonFileStorage,
    config: &ResolvedConfig,
) -> Result<serde_json::Value> {
    let (conn_created, acct_created, warnings) = storage.rebuild_all_symlinks().await?;
    for warning in &warnings {
        eprintln!("Warning: {warning}");
    }
    let result = serde_json::json!({
        "connection_symlinks_created": conn_created,
        "account_symlinks_created": acct_created,
        "warnings": warnings.len()
    });

    maybe_auto_commit(config, "sync symlinks");

    Ok(result)
}

pub async fn schwab_login(
    storage: &JsonFileStorage,
    id_or_name: Option<&str>,
) -> Result<serde_json::Value> {
    // Find Schwab connection(s)
    let connections = storage.list_connections().await?;
    let schwab_connections: Vec<_> = connections
        .into_iter()
        .filter(|c| c.config.synchronizer == "schwab")
        .collect();

    let connection = match (id_or_name, schwab_connections.len()) {
        // Explicit ID/name provided
        (Some(id_or_name), _) => find_connection(storage, id_or_name)
            .await?
            .filter(|c| c.config.synchronizer == "schwab")
            .context(format!("Schwab connection not found: {id_or_name}"))?,
        // No ID, exactly one Schwab connection
        (None, 1) => schwab_connections.into_iter().next().unwrap(),
        // No ID, no Schwab connections
        (None, 0) => {
            return Err(anyhow::anyhow!("No Schwab connections found"));
        }
        // No ID, multiple Schwab connections
        (None, n) => {
            let names: Vec<_> = schwab_connections.iter().map(|c| &c.config.name).collect();
            return Err(anyhow::anyhow!(
                "Multiple Schwab connections found ({n}). Specify one: {names:?}"
            ));
        }
    };

    let conn_name = connection.config.name.clone();
    let conn_id = connection.id().to_string();

    let mut synchronizer = SchwabSynchronizer::from_connection(&connection, storage).await?;
    synchronizer.login().await?;

    Ok(serde_json::json!({
        "success": true,
        "connection": {
            "id": conn_id,
            "name": conn_name
        },
        "message": "Session captured successfully"
    }))
}

pub async fn store_sync_prices(
    result: &crate::sync::SyncResult,
    config: &ResolvedConfig,
) -> Result<usize> {
    let market_data_store = JsonlMarketDataStore::new(&config.data_dir);
    let mut count = 0;

    for (_, synced_balances) in &result.balances {
        for sb in synced_balances {
            if let Some(price) = &sb.price {
                market_data_store
                    .put_prices(std::slice::from_ref(price))
                    .await?;
                count += 1;
            }
        }
    }

    Ok(count)
}

pub async fn fetch_crypto_prices(
    result: &crate::sync::SyncResult,
    config: &ResolvedConfig,
) -> Result<usize> {
    // Load crypto price sources from registry
    let mut registry = PriceSourceRegistry::new(&config.data_dir);
    registry.load()?;
    let crypto_sources = registry.build_crypto_sources().await?;

    if crypto_sources.is_empty() {
        tracing::debug!("No crypto price sources configured, skipping price fetch");
        return Ok(0);
    }

    let crypto_router = Arc::new(CryptoPriceRouter::new(crypto_sources));
    let store = Arc::new(JsonlMarketDataStore::new(&config.data_dir));
    let market_data = MarketDataService::new(store, None).with_crypto_router(crypto_router);

    // Collect unique crypto assets from sync result
    let assets: HashSet<Asset> = result
        .balances
        .iter()
        .flat_map(|(_, sbs)| sbs.iter().map(|sb| sb.asset_balance.asset.clone()))
        .filter(|a| matches!(a, Asset::Crypto { .. }))
        .collect();

    let date = Utc::now().date_naive();
    let mut count = 0;

    for asset in assets {
        match market_data.price_close(&asset, date).await {
            Ok(_) => count += 1,
            Err(e) => tracing::warn!(asset = ?asset, error = %e, "Failed to fetch price"),
        }
    }

    Ok(count)
}

pub struct PriceHistoryRequest<'a> {
    pub storage: &'a JsonFileStorage,
    pub config: &'a ResolvedConfig,
    pub account: Option<&'a str>,
    pub connection: Option<&'a str>,
    pub start: Option<&'a str>,
    pub end: Option<&'a str>,
    pub interval: &'a str,
    pub lookback_days: u32,
    pub request_delay_ms: u64,
    pub currency: Option<String>,
    pub include_fx: bool,
}

#[derive(Debug, Clone, Copy)]
enum PriceHistoryInterval {
    Daily,
    Weekly,
    Monthly,
    Yearly,
}

impl PriceHistoryInterval {
    fn parse(value: &str) -> Result<Self> {
        match value.to_lowercase().as_str() {
            "daily" => Ok(Self::Daily),
            "weekly" => Ok(Self::Weekly),
            "monthly" => Ok(Self::Monthly),
            "yearly" | "annual" | "annually" => Ok(Self::Yearly),
            _ => anyhow::bail!(
                "Invalid interval: {value}. Use: daily, weekly, monthly, yearly, annual"
            ),
        }
    }

    fn as_str(&self) -> &'static str {
        match self {
            Self::Daily => "daily",
            Self::Weekly => "weekly",
            Self::Monthly => "monthly",
            Self::Yearly => "yearly",
        }
    }
}

struct AssetPriceCache {
    asset: Asset,
    asset_id: AssetId,
    prices: HashMap<NaiveDate, PricePoint>,
}

pub async fn fetch_historical_prices(request: PriceHistoryRequest<'_>) -> Result<PriceHistoryOutput> {
    use crate::market_data::{CryptoPriceRouter, EquityPriceRouter, FxRateRouter};

    let PriceHistoryRequest {
        storage,
        config,
        account,
        connection,
        start,
        end,
        interval,
        lookback_days,
        request_delay_ms,
        currency,
        include_fx,
    } = request;

    let (scope, accounts) = resolve_price_history_scope(storage, account, connection).await?;

    let mut assets: HashSet<Asset> = HashSet::new();
    let mut earliest_balance_date: Option<NaiveDate> = None;

    for account in &accounts {
        let snapshots = storage.get_balance_snapshots(&account.id).await?;
        for snapshot in snapshots {
            let date = snapshot.timestamp.date_naive();
            earliest_balance_date = Some(match earliest_balance_date {
                Some(current) => current.min(date),
                None => date,
            });
            for balance in snapshot.balances {
                assets.insert(balance.asset);
            }
        }
    }

    if assets.is_empty() {
        anyhow::bail!("No balances found for selected scope");
    }

    let start_date = match start {
        Some(value) => NaiveDate::parse_from_str(value, "%Y-%m-%d")
            .with_context(|| format!("Invalid start date: {value}"))?,
        None => earliest_balance_date.context("No balances found to infer start date")?,
    };

    let end_date = match end {
        Some(value) => NaiveDate::parse_from_str(value, "%Y-%m-%d")
            .with_context(|| format!("Invalid end date: {value}"))?,
        None => Utc::now().date_naive(),
    };

    if start_date > end_date {
        anyhow::bail!("Start date must be on or before end date");
    }

    let interval = PriceHistoryInterval::parse(interval)?;
    let anchor_day = start_date.day();
    let anchor_month = start_date.month();
    let aligned_start = align_start_date(start_date, interval, anchor_month, anchor_day);

    let target_currency = currency.unwrap_or_else(|| config.reporting_currency.clone());
    let target_currency_upper = target_currency.to_uppercase();

    let store: Arc<dyn MarketDataStore> = Arc::new(JsonlMarketDataStore::new(&config.data_dir));

    // Load configured price sources
    let mut registry = PriceSourceRegistry::new(&config.data_dir);
    registry.load()?;
    let equity_sources = registry.build_equity_sources().await?;
    let crypto_sources = registry.build_crypto_sources().await?;
    let fx_sources = registry.build_fx_sources().await?;

    let mut market_data =
        MarketDataService::new(store.clone(), None).with_lookback_days(lookback_days);
    if !equity_sources.is_empty() {
        market_data =
            market_data.with_equity_router(Arc::new(EquityPriceRouter::new(equity_sources)));
    }
    if !crypto_sources.is_empty() {
        market_data =
            market_data.with_crypto_router(Arc::new(CryptoPriceRouter::new(crypto_sources)));
    }
    if !fx_sources.is_empty() {
        market_data = market_data.with_fx_router(Arc::new(FxRateRouter::new(fx_sources)));
    }

    let mut asset_caches = Vec::new();
    for asset in assets {
        let asset_id = AssetId::from_asset(&asset);
        let prices = load_price_cache(&store, &asset_id).await?;
        asset_caches.push(AssetPriceCache {
            asset,
            asset_id,
            prices,
        });
    }

    asset_caches.sort_by(|a, b| a.asset_id.to_string().cmp(&b.asset_id.to_string()));

    let mut fx_cache: HashMap<(String, String), HashMap<NaiveDate, FxRatePoint>> = HashMap::new();

    if include_fx {
        for asset_cache in &asset_caches {
            if let Asset::Currency { iso_code } = &asset_cache.asset {
                let base = iso_code.to_uppercase();
                if base == target_currency_upper {
                    continue;
                }
                let key = (base.clone(), target_currency_upper.clone());
                if !fx_cache.contains_key(&key) {
                    fx_cache.insert(key.clone(), load_fx_cache(&store, &key.0, &key.1).await?);
                }
            }
        }
    }

    let mut price_stats = PriceHistoryStats::default();
    let mut fx_stats = PriceHistoryStats::default();
    let mut failures = Vec::new();
    let mut failure_count = 0usize;
    let failure_limit = 50usize;
    let request_delay = if request_delay_ms > 0 {
        Some(std::time::Duration::from_millis(request_delay_ms))
    } else {
        None
    };

    let mut current = aligned_start;
    let mut points = 0usize;
    {
        let mut fx_ctx = FxRateContext {
            market_data: &market_data,
            store: &store,
            fx_cache: &mut fx_cache,
            stats: &mut fx_stats,
            failures: &mut failures,
            failure_count: &mut failure_count,
            failure_limit,
            lookback_days,
        };

        while current <= end_date {
            points += 1;
            for asset_cache in asset_caches.iter_mut() {
                let mut should_delay = false;
                match &asset_cache.asset {
                    Asset::Currency { iso_code } => {
                        if include_fx {
                            let base = iso_code.to_uppercase();
                            if base != target_currency_upper {
                                ensure_fx_rate(&mut fx_ctx, &base, &target_currency_upper, current)
                                    .await?;
                            }
                        }
                    }
                    Asset::Equity { .. } | Asset::Crypto { .. } => {
                        price_stats.attempted += 1;
                        if let Some((price, exact)) =
                            resolve_cached_price(&asset_cache.prices, current, lookback_days)
                        {
                            if exact {
                                price_stats.existing += 1;
                            } else {
                                price_stats.lookback += 1;
                            }

                            if include_fx
                                && price.quote_currency.to_uppercase() != target_currency_upper
                            {
                                ensure_fx_rate(
                                    &mut fx_ctx,
                                    &price.quote_currency.to_uppercase(),
                                    &target_currency_upper,
                                    current,
                                )
                                .await?;
                            }
                            continue;
                        }

                        match market_data.price_close(&asset_cache.asset, current).await {
                            Ok(price) => {
                                let exact = price.as_of_date == current;
                                if exact {
                                    price_stats.fetched += 1;
                                } else {
                                    price_stats.lookback += 1;
                                }

                                upsert_price_cache(&mut asset_cache.prices, price.clone());
                                should_delay = request_delay.is_some();

                                if include_fx
                                    && price.quote_currency.to_uppercase() != target_currency_upper
                                {
                                    ensure_fx_rate(
                                        &mut fx_ctx,
                                        &price.quote_currency.to_uppercase(),
                                        &target_currency_upper,
                                        current,
                                    )
                                    .await?;
                                }
                            }
                            Err(e) => {
                                price_stats.missing += 1;
                                *fx_ctx.failure_count += 1;
                                if fx_ctx.failures.len() < fx_ctx.failure_limit {
                                    fx_ctx.failures.push(PriceHistoryFailure {
                                        kind: "price".to_string(),
                                        date: current.to_string(),
                                        error: e.to_string(),
                                        asset_id: Some(asset_cache.asset_id.to_string()),
                                        asset: Some(asset_cache.asset.clone()),
                                        base: None,
                                        quote: None,
                                    });
                                }
                                should_delay = request_delay.is_some();
                            }
                        }
                    }
                }

                if should_delay {
                    if let Some(delay) = request_delay {
                        tokio::time::sleep(delay).await;
                    }
                }
            }

            current = advance_interval_date(current, interval, anchor_day, anchor_month);
        }
    }

    let days = (end_date - start_date).num_days() as usize + 1;

    let assets_output = asset_caches
        .iter()
        .map(|cache| AssetInfoOutput {
            asset: cache.asset.clone(),
            asset_id: cache.asset_id.to_string(),
        })
        .collect();

    let output = PriceHistoryOutput {
        scope,
        currency: target_currency,
        interval: interval.as_str().to_string(),
        start_date: start_date.to_string(),
        end_date: end_date.to_string(),
        earliest_balance_date: earliest_balance_date.map(|d| d.to_string()),
        days,
        points,
        assets: assets_output,
        prices: price_stats,
        fx: if include_fx { Some(fx_stats) } else { None },
        failure_count,
        failures,
    };

    maybe_auto_commit(config, "market data fetch");

    Ok(output)
}

fn advance_interval_date(
    date: NaiveDate,
    interval: PriceHistoryInterval,
    anchor_day: u32,
    anchor_month: u32,
) -> NaiveDate {
    match interval {
        PriceHistoryInterval::Daily => date + Duration::days(1),
        PriceHistoryInterval::Weekly => date + Duration::days(7),
        PriceHistoryInterval::Monthly => next_month_end(date),
        PriceHistoryInterval::Yearly => add_years(date, 1, anchor_month, anchor_day),
    }
}

fn align_start_date(
    date: NaiveDate,
    interval: PriceHistoryInterval,
    anchor_month: u32,
    anchor_day: u32,
) -> NaiveDate {
    match interval {
        PriceHistoryInterval::Monthly => month_end(date),
        PriceHistoryInterval::Yearly => {
            let day = anchor_day.min(days_in_month(date.year(), anchor_month));
            NaiveDate::from_ymd_opt(date.year(), anchor_month, day).expect("valid yearly date")
        }
        _ => date,
    }
}

fn add_years(date: NaiveDate, years: i32, anchor_month: u32, anchor_day: u32) -> NaiveDate {
    let year = date.year() + years;
    let day = anchor_day.min(days_in_month(year, anchor_month));
    NaiveDate::from_ymd_opt(year, anchor_month, day).expect("valid yearly date")
}

fn next_month_end(date: NaiveDate) -> NaiveDate {
    let (year, month) = if date.month() == 12 {
        (date.year() + 1, 1)
    } else {
        (date.year(), date.month() + 1)
    };
    let day = days_in_month(year, month);
    NaiveDate::from_ymd_opt(year, month, day).expect("valid next month end")
}

fn month_end(date: NaiveDate) -> NaiveDate {
    let day = days_in_month(date.year(), date.month());
    NaiveDate::from_ymd_opt(date.year(), date.month(), day).expect("valid month end")
}

fn days_in_month(year: i32, month: u32) -> u32 {
    let (next_year, next_month) = if month == 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    };
    let first_next = NaiveDate::from_ymd_opt(next_year, next_month, 1).expect("valid next month");
    let last = first_next - Duration::days(1);
    last.day()
}

async fn resolve_price_history_scope(
    storage: &JsonFileStorage,
    account: Option<&str>,
    connection: Option<&str>,
) -> Result<(PriceHistoryScopeOutput, Vec<Account>)> {
    if account.is_some() && connection.is_some() {
        anyhow::bail!("Specify only one of --account or --connection");
    }

    if let Some(id_or_name) = account {
        let account = find_account(storage, id_or_name)
            .await?
            .context(format!("Account not found: {id_or_name}"))?;
        return Ok((
            PriceHistoryScopeOutput::Account {
                id: account.id.to_string(),
                name: account.name.clone(),
            },
            vec![account],
        ));
    }

    if let Some(id_or_name) = connection {
        let connection = find_connection(storage, id_or_name)
            .await?
            .context(format!("Connection not found: {id_or_name}"))?;
        let mut accounts = Vec::new();

        if !connection.state.account_ids.is_empty() {
            for account_id in &connection.state.account_ids {
                match storage.get_account(account_id).await? {
                    Some(account) => {
                        if account.connection_id != *connection.id() {
                            warn!(
                                connection_id = %connection.id(),
                                account_id = %account_id,
                                account_connection_id = %account.connection_id,
                                "account referenced by connection belongs to different connection"
                            );
                        } else {
                            accounts.push(account);
                        }
                    }
                    None => {
                        warn!(
                            connection_id = %connection.id(),
                            account_id = %account_id,
                            "account referenced by connection not found"
                        );
                    }
                }
            }
        }

        let mut seen_ids: HashSet<Id> =
            accounts.iter().map(|account| account.id.clone()).collect();

        let extra_accounts: Vec<Account> = storage
            .list_accounts()
            .await?
            .into_iter()
            .filter(|a| a.connection_id == *connection.id() && !seen_ids.contains(&a.id))
            .collect();

        for account in extra_accounts {
            seen_ids.insert(account.id.clone());
            accounts.push(account);
        }

        if accounts.is_empty() {
            anyhow::bail!("No accounts found for connection {}", connection.name());
        }

        return Ok((
            PriceHistoryScopeOutput::Connection {
                id: connection.id().to_string(),
                name: connection.name().to_string(),
            },
            accounts,
        ));
    }

    let accounts = storage.list_accounts().await?;
    if accounts.is_empty() {
        anyhow::bail!("No accounts found");
    }

    Ok((PriceHistoryScopeOutput::Portfolio, accounts))
}

async fn find_account(storage: &JsonFileStorage, id_or_name: &str) -> Result<Option<Account>> {
    let id = Id::from_string(id_or_name);
    if let Some(account) = storage.get_account(&id).await? {
        return Ok(Some(account));
    }

    let accounts = storage.list_accounts().await?;
    let mut matches: Vec<Account> = accounts
        .into_iter()
        .filter(|a| a.name.eq_ignore_ascii_case(id_or_name))
        .collect();

    if matches.is_empty() {
        return Ok(None);
    }

    if matches.len() > 1 {
        let ids: Vec<String> = matches.iter().map(|a| a.id.to_string()).collect();
        anyhow::bail!("Multiple accounts named '{id_or_name}'. Use an ID instead: {ids:?}");
    }

    Ok(matches.pop())
}

async fn load_price_cache(
    store: &Arc<dyn MarketDataStore>,
    asset_id: &AssetId,
) -> Result<HashMap<NaiveDate, PricePoint>> {
    let prices = store.get_all_prices(asset_id).await?;
    let mut map: HashMap<NaiveDate, PricePoint> = HashMap::new();

    for price in prices {
        if price.kind != PriceKind::Close {
            continue;
        }
        match map.get(&price.as_of_date) {
            Some(existing) if existing.timestamp >= price.timestamp => {}
            _ => {
                map.insert(price.as_of_date, price);
            }
        }
    }

    Ok(map)
}

async fn load_fx_cache(
    store: &Arc<dyn MarketDataStore>,
    base: &str,
    quote: &str,
) -> Result<HashMap<NaiveDate, FxRatePoint>> {
    let rates = store.get_all_fx_rates(base, quote).await?;
    let mut map: HashMap<NaiveDate, FxRatePoint> = HashMap::new();

    for rate in rates {
        if rate.kind != FxRateKind::Close {
            continue;
        }
        match map.get(&rate.as_of_date) {
            Some(existing) if existing.timestamp >= rate.timestamp => {}
            _ => {
                map.insert(rate.as_of_date, rate);
            }
        }
    }

    Ok(map)
}

fn resolve_cached_price(
    cache: &HashMap<NaiveDate, PricePoint>,
    date: NaiveDate,
    lookback_days: u32,
) -> Option<(PricePoint, bool)> {
    if let Some(price) = cache.get(&date) {
        return Some((price.clone(), true));
    }

    for offset in 1..=lookback_days {
        let target = date - Duration::days(offset as i64);
        if let Some(price) = cache.get(&target) {
            return Some((price.clone(), false));
        }
    }

    None
}

fn resolve_cached_fx(
    cache: &HashMap<NaiveDate, FxRatePoint>,
    date: NaiveDate,
    lookback_days: u32,
) -> Option<(FxRatePoint, bool)> {
    if let Some(rate) = cache.get(&date) {
        return Some((rate.clone(), true));
    }

    for offset in 1..=lookback_days {
        let target = date - Duration::days(offset as i64);
        if let Some(rate) = cache.get(&target) {
            return Some((rate.clone(), false));
        }
    }

    None
}

fn upsert_price_cache(cache: &mut HashMap<NaiveDate, PricePoint>, price: PricePoint) -> bool {
    match cache.get(&price.as_of_date) {
        Some(existing) if existing.timestamp >= price.timestamp => false,
        _ => {
            cache.insert(price.as_of_date, price);
            true
        }
    }
}

fn upsert_fx_cache(cache: &mut HashMap<NaiveDate, FxRatePoint>, rate: FxRatePoint) -> bool {
    match cache.get(&rate.as_of_date) {
        Some(existing) if existing.timestamp >= rate.timestamp => false,
        _ => {
            cache.insert(rate.as_of_date, rate);
            true
        }
    }
}

struct FxRateContext<'a> {
    market_data: &'a MarketDataService,
    store: &'a Arc<dyn MarketDataStore>,
    fx_cache: &'a mut HashMap<(String, String), HashMap<NaiveDate, FxRatePoint>>,
    stats: &'a mut PriceHistoryStats,
    failures: &'a mut Vec<PriceHistoryFailure>,
    failure_count: &'a mut usize,
    failure_limit: usize,
    lookback_days: u32,
}

async fn ensure_fx_rate(
    ctx: &mut FxRateContext<'_>,
    base: &str,
    quote: &str,
    date: NaiveDate,
) -> Result<()> {
    ctx.stats.attempted += 1;

    let base_upper = base.to_uppercase();
    let quote_upper = quote.to_uppercase();
    let key = (base_upper.clone(), quote_upper.clone());

    if !ctx.fx_cache.contains_key(&key) {
        ctx.fx_cache.insert(
            key.clone(),
            load_fx_cache(ctx.store, &base_upper, &quote_upper).await?,
        );
    }

    let cache = ctx
        .fx_cache
        .get(&key)
        .expect("fx cache should be initialized");

    if let Some((_, exact)) = resolve_cached_fx(cache, date, ctx.lookback_days) {
        if exact {
            ctx.stats.existing += 1;
        } else {
            ctx.stats.lookback += 1;
        }
        return Ok(());
    }

    match ctx
        .market_data
        .fx_close(&base_upper, &quote_upper, date)
        .await
    {
        Ok(rate) => {
            if rate.as_of_date == date {
                ctx.stats.fetched += 1;
            } else {
                ctx.stats.lookback += 1;
            }
            if let Some(cache) = ctx.fx_cache.get_mut(&key) {
                upsert_fx_cache(cache, rate);
            }
        }
        Err(e) => {
            ctx.stats.missing += 1;
            *ctx.failure_count += 1;
            if ctx.failures.len() < ctx.failure_limit {
                ctx.failures.push(PriceHistoryFailure {
                    kind: "fx".to_string(),
                    date: date.to_string(),
                    error: e.to_string(),
                    asset_id: None,
                    asset: None,
                    base: Some(base_upper),
                    quote: Some(quote_upper),
                });
            }
        }
    }

    Ok(())
}

pub async fn portfolio_snapshot(
    storage: &JsonFileStorage,
    config: &ResolvedConfig,
    currency: Option<String>,
    date: Option<String>,
    group_by: String,
    detail: bool,
    auto: bool,
    offline: bool,
    dry_run: bool,
    force_refresh: bool,
) -> Result<crate::portfolio::PortfolioSnapshot> {
    // Parse date
    let as_of_date = match date {
        Some(d) => NaiveDate::parse_from_str(&d, "%Y-%m-%d")
            .with_context(|| format!("Invalid date format: {d}"))?,
        None => Utc::now().date_naive(),
    };

    // Parse grouping
    let grouping = match group_by.as_str() {
        "asset" => Grouping::Asset,
        "account" => Grouping::Account,
        "both" => Grouping::Both,
        _ => anyhow::bail!("Invalid grouping: {group_by}. Use: asset, account, both"),
    };

    // Determine what to refresh based on flags
    // Default (no flags or --auto): auto-refresh stale data
    // --offline: no refresh
    // --dry-run: log staleness but no refresh
    // --force-refresh: refresh everything
    let should_refresh_balances = !offline && !dry_run;
    let should_refresh_prices = !offline && !dry_run;
    let ignore_staleness = force_refresh;

    // Explicit --auto flag has same behavior as default
    let _ = auto;

    // Build query
    let query = PortfolioQuery {
        as_of_date,
        currency: currency.unwrap_or_else(|| config.reporting_currency.clone()),
        grouping,
        include_detail: detail,
    };

    // Setup market data store
    let store = Arc::new(JsonlMarketDataStore::new(&config.data_dir));

    // Check which connections need syncing based on staleness
    let connections = storage.list_connections().await?;
    let mut connections_to_sync = Vec::new();

    for connection in &connections {
        let threshold = resolve_balance_staleness(None, connection, &config.refresh);
        let check = check_balance_staleness(connection, threshold);

        // Log if dry_run
        if dry_run {
            log_balance_staleness(&connection.config.name, &check);
        }

        // Add to sync list if stale (or force)
        if should_refresh_balances && (ignore_staleness || check.is_stale) {
            connections_to_sync.push(connection.clone());
        }
    }

    // Check price staleness for dry-run
    if dry_run {
        use std::collections::HashSet;

        // Load balances to find unique assets that need prices
        let snapshots = storage.get_latest_balances().await?;
        let mut seen_assets: HashSet<String> = HashSet::new();

        for (_, snapshot) in &snapshots {
            for asset_balance in &snapshot.balances {
                match &asset_balance.asset {
                    Asset::Equity { .. } | Asset::Crypto { .. } => {
                        let asset_id = AssetId::from_asset(&asset_balance.asset);
                        let asset_key = asset_id.to_string();

                        if seen_assets.contains(&asset_key) {
                            continue;
                        }
                        seen_assets.insert(asset_key.clone());

                        // Find most recent cached price (quote or close, with lookback)
                        let mut cached_price = None;

                        // Try Quote for today first
                        if let Some(p) = store
                            .get_price(&asset_id, query.as_of_date, PriceKind::Quote)
                            .await?
                        {
                            cached_price = Some(p);
                        }

                        // If no quote, try Close with lookback (7 days)
                        if cached_price.is_none() {
                            for offset in 0..=7i64 {
                                let target_date = query.as_of_date - Duration::days(offset);
                                if let Some(p) = store
                                    .get_price(&asset_id, target_date, PriceKind::Close)
                                    .await?
                                {
                                    cached_price = Some(p);
                                    break;
                                }
                            }
                        }

                        let check = check_price_staleness(
                            cached_price.as_ref(),
                            config.refresh.price_staleness,
                        );
                        log_price_staleness(&asset_key, &check);
                    }
                    Asset::Currency { .. } => {
                        // Currency doesn't need price lookup (only FX)
                    }
                }
            }
        }
    }

    // Sync stale connections
    if !connections_to_sync.is_empty() {
        for connection in &connections_to_sync {
            let _ = sync_connection(storage, config, connection.id().as_ref()).await;
        }
    }

    // Setup market data service with or without providers
    let market_data = if should_refresh_prices {
        // Load configured price sources from registry
        let mut registry = PriceSourceRegistry::new(&config.data_dir);
        registry.load()?;

        // Build routers from configured sources
        let equity_sources = registry.build_equity_sources().await?;
        let crypto_sources = registry.build_crypto_sources().await?;
        let fx_sources = registry.build_fx_sources().await?;

        let mut service = MarketDataService::new(store, None)
            .with_quote_staleness(config.refresh.price_staleness);

        if !equity_sources.is_empty() {
            let equity_router = EquityPriceRouter::new(equity_sources);
            service = service.with_equity_router(Arc::new(equity_router));
        }

        if !crypto_sources.is_empty() {
            let crypto_router = CryptoPriceRouter::new(crypto_sources);
            service = service.with_crypto_router(Arc::new(crypto_router));
        }

        if !fx_sources.is_empty() {
            let fx_router = FxRateRouter::new(fx_sources);
            service = service.with_fx_router(Arc::new(fx_router));
        }

        Arc::new(service)
    } else {
        Arc::new(
            MarketDataService::new(store, None)
                .with_quote_staleness(config.refresh.price_staleness),
        )
    };

    // Calculate and output
    let storage_arc: Arc<dyn Storage> = Arc::new(JsonFileStorage::new(&config.data_dir));
    let service = PortfolioService::new(storage_arc, market_data);
    let snapshot = service.calculate(&query).await?;

    maybe_auto_commit(config, "portfolio snapshot");

    Ok(snapshot)
}

pub async fn portfolio_history(
    _storage: &JsonFileStorage,
    config: &ResolvedConfig,
    currency: Option<String>,
    start: Option<String>,
    end: Option<String>,
    granularity: String,
    include_prices: bool,
) -> Result<HistoryOutput> {
    use rust_decimal::Decimal;
    use std::str::FromStr;

    // Parse date range
    let start_date = start
        .as_ref()
        .map(|s| {
            NaiveDate::parse_from_str(s, "%Y-%m-%d")
                .with_context(|| format!("Invalid start date: {s}"))
        })
        .transpose()?;
    let end_date = end
        .as_ref()
        .map(|s| {
            NaiveDate::parse_from_str(s, "%Y-%m-%d")
                .with_context(|| format!("Invalid end date: {s}"))
        })
        .transpose()?;

    // Parse granularity
    let granularity_enum = match granularity.as_str() {
        "none" | "full" => Granularity::Full,
        "hourly" => Granularity::Hourly,
        "daily" => Granularity::Daily,
        "weekly" => Granularity::Weekly,
        "monthly" => Granularity::Monthly,
        "yearly" => Granularity::Yearly,
        _ => anyhow::bail!(
            "Invalid granularity: {granularity}. Use: none, full, hourly, daily, weekly, monthly, yearly"
        ),
    };

    // Setup storage and market data store
    let store: Arc<dyn MarketDataStore> = Arc::new(JsonlMarketDataStore::new(&config.data_dir));
    let storage_arc: Arc<dyn Storage> = Arc::new(JsonFileStorage::new(&config.data_dir));

    // Collect change points
    let options = CollectOptions {
        account_ids: Vec::new(), // All accounts
        include_prices,
        include_fx: false,
        target_currency: currency.clone(),
    };

    let change_points = collect_change_points(&storage_arc, &store, &options).await?;

    // Filter by date range
    let filtered_by_date = filter_by_date_range(change_points, start_date, end_date);

    // Filter by granularity
    let filtered = filter_by_granularity(
        filtered_by_date,
        granularity_enum,
        CoalesceStrategy::Last,
    );

    if filtered.is_empty() {
        return Ok(HistoryOutput {
            currency: currency.unwrap_or_else(|| config.reporting_currency.clone()),
            start_date: start,
            end_date: end,
            granularity,
            points: Vec::new(),
            summary: None,
        });
    }

    // Setup market data service (offline mode - use cached data only)
    let market_data = Arc::new(
        MarketDataService::new(store, None).with_quote_staleness(config.refresh.price_staleness),
    );

    // Create portfolio service
    let service = PortfolioService::new(storage_arc, market_data);

    // Calculate portfolio value at each change point
    let target_currency = currency
        .clone()
        .unwrap_or_else(|| config.reporting_currency.clone());
    let mut history_points = Vec::with_capacity(filtered.len());

    for change_point in &filtered {
        let as_of_date = change_point.timestamp.date_naive();
        let query = PortfolioQuery {
            as_of_date,
            currency: target_currency.clone(),
            grouping: Grouping::Asset,
            include_detail: false,
        };

        let snapshot = service.calculate(&query).await?;

        // Format trigger descriptions
        let trigger_descriptions: Vec<String> = change_point
            .triggers
            .iter()
            .map(|t| match t {
                crate::portfolio::ChangeTrigger::Balance { account_id, asset } => {
                    format!(
                        "balance:{}:{}",
                        account_id,
                        serde_json::to_string(asset).unwrap_or_default()
                    )
                }
                crate::portfolio::ChangeTrigger::Price { asset_id } => {
                    format!("price:{asset_id}")
                }
                crate::portfolio::ChangeTrigger::FxRate { base, quote } => {
                    format!("fx:{base}/{quote}")
                }
            })
            .collect();

        history_points.push(HistoryPoint {
            timestamp: change_point.timestamp.to_rfc3339(),
            date: as_of_date.to_string(),
            total_value: snapshot.total_value,
            change_triggers: if trigger_descriptions.is_empty() {
                None
            } else {
                Some(trigger_descriptions)
            },
        });
    }

    // Calculate summary if we have points
    let summary = if history_points.len() >= 2 {
        let initial =
            Decimal::from_str(&history_points[0].total_value).unwrap_or(Decimal::ZERO);
        let final_val =
            Decimal::from_str(&history_points[history_points.len() - 1].total_value)
                .unwrap_or(Decimal::ZERO);
        let absolute_change = final_val - initial;
        let percentage_change = if initial != Decimal::ZERO {
            ((final_val - initial) / initial * Decimal::from(100))
                .round_dp(2)
                .to_string()
        } else {
            "N/A".to_string()
        };

        Some(HistorySummary {
            initial_value: initial.normalize().to_string(),
            final_value: final_val.normalize().to_string(),
            absolute_change: absolute_change.normalize().to_string(),
            percentage_change,
        })
    } else {
        None
    };

    Ok(HistoryOutput {
        currency: target_currency,
        start_date: start,
        end_date: end,
        granularity,
        points: history_points,
        summary,
    })
}

pub fn parse_asset(s: &str) -> Result<Asset> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        anyhow::bail!("Asset string cannot be empty");
    }
    if let Some((prefix, value)) = trimmed.split_once(':') {
        let value = value.trim();
        if value.is_empty() {
            anyhow::bail!("Asset value missing for prefix '{prefix}'");
        }
        match prefix.to_lowercase().as_str() {
            "equity" => return Ok(Asset::equity(value)),
            "crypto" => return Ok(Asset::crypto(value)),
            "currency" => return Ok(Asset::currency(value)),
            _ => {}
        }
    }

    // Assume it's a currency code
    Ok(Asset::currency(trimmed))
}

pub async fn find_connection(
    storage: &JsonFileStorage,
    id_or_name: &str,
) -> Result<Option<Connection>> {
    // Try by ID first
    let id = Id::from_string(id_or_name);
    if let Some(conn) = storage.get_connection(&id).await? {
        return Ok(Some(conn));
    }

    // Try by name
    let connections = storage.list_connections().await?;
    let mut matches: Vec<Connection> = connections
        .into_iter()
        .filter(|conn| conn.config.name.eq_ignore_ascii_case(id_or_name))
        .collect();

    if matches.is_empty() {
        return Ok(None);
    }

    if matches.len() > 1 {
        let ids: Vec<String> = matches.iter().map(|c| c.id().to_string()).collect();
        anyhow::bail!("Multiple connections named '{id_or_name}'. Use an ID instead: {ids:?}");
    }

    Ok(matches.pop())
}

fn maybe_auto_commit(config: &ResolvedConfig, action: &str) {
    if !config.git.auto_commit {
        return;
    }

    match try_auto_commit(&config.data_dir, action) {
        Ok(AutoCommitOutcome::Committed) => {
            tracing::info!("Auto-committed keepbook data");
        }
        Ok(AutoCommitOutcome::SkippedNoChanges) => {
            tracing::debug!("Auto-commit skipped: no changes");
        }
        Ok(AutoCommitOutcome::SkippedNotRepo { reason }) => {
            tracing::warn!("Auto-commit enabled but skipped: {reason}");
        }
        Err(err) => {
            tracing::warn!(error = %err, "Auto-commit failed");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Account, ConnectionConfig};
    use crate::storage::JsonFileStorage;
    use chrono::{DateTime, NaiveDate, Utc};
    use std::collections::HashMap;
    use tempfile::TempDir;

    fn connection_config(name: &str) -> ConnectionConfig {
        ConnectionConfig {
            name: name.to_string(),
            synchronizer: "mock".to_string(),
            credentials: None,
            balance_staleness: None,
        }
    }

    async fn write_connection_config(
        storage: &JsonFileStorage,
        conn: &Connection,
    ) -> anyhow::Result<()> {
        let config_path = storage.connection_config_path(conn.id());
        tokio::fs::create_dir_all(config_path.parent().unwrap()).await?;
        let config_toml = toml::to_string_pretty(&conn.config)?;
        tokio::fs::write(&config_path, config_toml).await?;
        Ok(())
    }

    fn sample_price(
        asset: &Asset,
        date: NaiveDate,
        timestamp: DateTime<Utc>,
    ) -> PricePoint {
        PricePoint {
            asset_id: AssetId::from_asset(asset),
            as_of_date: date,
            timestamp,
            price: "1.00".to_string(),
            quote_currency: "USD".to_string(),
            kind: PriceKind::Close,
            source: "test".to_string(),
        }
    }

    fn sample_fx_rate(base: &str, quote: &str, date: NaiveDate, timestamp: DateTime<Utc>) -> FxRatePoint {
        FxRatePoint {
            base: base.to_string(),
            quote: quote.to_string(),
            as_of_date: date,
            timestamp,
            rate: "1.25".to_string(),
            kind: FxRateKind::Close,
            source: "test".to_string(),
        }
    }

    #[test]
    fn parse_asset_handles_prefixes() -> anyhow::Result<()> {
        let equity = parse_asset("Equity:AAPL")?;
        match equity {
            Asset::Equity { ticker, .. } => assert_eq!(ticker, "AAPL"),
            _ => anyhow::bail!("expected equity asset"),
        }

        let crypto = parse_asset("CRYPTO:BTC")?;
        match crypto {
            Asset::Crypto { symbol, .. } => assert_eq!(symbol, "BTC"),
            _ => anyhow::bail!("expected crypto asset"),
        }

        let currency = parse_asset(" currency:usd ")?;
        match currency {
            Asset::Currency { iso_code } => assert_eq!(iso_code, "usd"),
            _ => anyhow::bail!("expected currency asset"),
        }

        Ok(())
    }

    #[test]
    fn parse_asset_rejects_empty_values() {
        assert!(parse_asset("").is_err());
        assert!(parse_asset("   ").is_err());
        assert!(parse_asset("equity:").is_err());
        assert!(parse_asset("crypto:   ").is_err());
        assert!(parse_asset("currency:").is_err());
    }

    #[test]
    fn align_start_date_monthly_uses_month_end() {
        let date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();
        let aligned = align_start_date(date, PriceHistoryInterval::Monthly, 1, 15);
        assert_eq!(aligned, NaiveDate::from_ymd_opt(2024, 1, 31).unwrap());
    }

    #[test]
    fn add_years_handles_leap_day() {
        let date = NaiveDate::from_ymd_opt(2024, 2, 29).unwrap();
        let next = add_years(date, 1, 2, 29);
        assert_eq!(next, NaiveDate::from_ymd_opt(2025, 2, 28).unwrap());
    }

    #[test]
    fn resolve_cached_price_prefers_exact_then_lookback() {
        let asset = Asset::equity("AAPL");
        let date = NaiveDate::from_ymd_opt(2024, 1, 10).unwrap();
        let exact = sample_price(&asset, date, Utc::now());
        let mut cache = HashMap::new();
        cache.insert(date, exact.clone());

        let (found, exact_hit) =
            resolve_cached_price(&cache, date, 3).expect("exact price");
        assert!(exact_hit);
        assert_eq!(found.as_of_date, date);

        cache.remove(&date);
        let lookback_date = date - chrono::Duration::days(1);
        let lookback = sample_price(&asset, lookback_date, Utc::now());
        cache.insert(lookback_date, lookback.clone());

        let (found, exact_hit) =
            resolve_cached_price(&cache, date, 3).expect("lookback price");
        assert!(!exact_hit);
        assert_eq!(found.as_of_date, lookback_date);
    }

    #[test]
    fn upsert_price_cache_prefers_newer_timestamp() {
        let asset = Asset::equity("AAPL");
        let date = NaiveDate::from_ymd_opt(2024, 1, 5).unwrap();
        let newer = sample_price(&asset, date, Utc::now());
        let older = sample_price(&asset, date, Utc::now() - chrono::Duration::minutes(5));

        let mut cache = HashMap::new();
        cache.insert(date, newer.clone());

        assert!(!upsert_price_cache(&mut cache, older));
        assert_eq!(cache.get(&date).unwrap().timestamp, newer.timestamp);

        let newest = sample_price(&asset, date, Utc::now() + chrono::Duration::minutes(1));
        assert!(upsert_price_cache(&mut cache, newest.clone()));
        assert_eq!(cache.get(&date).unwrap().timestamp, newest.timestamp);
    }

    #[test]
    fn resolve_cached_fx_prefers_exact_then_lookback() {
        let date = NaiveDate::from_ymd_opt(2024, 1, 10).unwrap();
        let exact = sample_fx_rate("EUR", "USD", date, Utc::now());
        let mut cache = HashMap::new();
        cache.insert(date, exact.clone());

        let (found, exact_hit) =
            resolve_cached_fx(&cache, date, 3).expect("exact rate");
        assert!(exact_hit);
        assert_eq!(found.as_of_date, date);

        cache.remove(&date);
        let lookback_date = date - chrono::Duration::days(2);
        let lookback = sample_fx_rate("EUR", "USD", lookback_date, Utc::now());
        cache.insert(lookback_date, lookback.clone());

        let (found, exact_hit) =
            resolve_cached_fx(&cache, date, 3).expect("lookback rate");
        assert!(!exact_hit);
        assert_eq!(found.as_of_date, lookback_date);
    }

    #[tokio::test]
    async fn find_account_errors_on_duplicate_names() -> anyhow::Result<()> {
        let dir = TempDir::new()?;
        let storage = JsonFileStorage::new(dir.path());
        let conn = Connection::new(connection_config("Test"));

        let account_one = Account::new("Checking", conn.id().clone());
        let account_two = Account::new("Checking", conn.id().clone());
        storage.save_account(&account_one).await?;
        storage.save_account(&account_two).await?;

        let err = find_account(&storage, "Checking")
            .await
            .expect_err("expected duplicate name error");
        assert!(err
            .to_string()
            .contains("Multiple accounts named 'Checking'"));

        Ok(())
    }

    #[tokio::test]
    async fn find_connection_errors_on_duplicate_names() -> anyhow::Result<()> {
        let dir = TempDir::new()?;
        let storage = JsonFileStorage::new(dir.path());

        let conn_one = Connection::new(connection_config("Duplicate"));
        let conn_two = Connection::new(connection_config("Duplicate"));

        write_connection_config(&storage, &conn_one).await?;
        write_connection_config(&storage, &conn_two).await?;
        storage.save_connection(&conn_one).await?;
        storage.save_connection(&conn_two).await?;

        let err = find_connection(&storage, "Duplicate")
            .await
            .expect_err("expected duplicate connection name error");
        assert!(err
            .to_string()
            .contains("Multiple connections named 'Duplicate'"));

        Ok(())
    }

    #[tokio::test]
    async fn resolve_scope_rejects_account_and_connection() -> anyhow::Result<()> {
        let dir = TempDir::new()?;
        let storage = JsonFileStorage::new(dir.path());

        let err = resolve_price_history_scope(&storage, Some("a"), Some("b"))
            .await
            .err()
            .expect("expected invalid scope error");
        assert!(err.to_string().contains("Specify only one"));

        Ok(())
    }

    #[tokio::test]
    async fn resolve_scope_connection_requires_accounts() -> anyhow::Result<()> {
        let dir = TempDir::new()?;
        let storage = JsonFileStorage::new(dir.path());
        let mut conn = Connection::new(connection_config("Test Connection"));

        let missing_account = Account::new("Missing", conn.id().clone());
        conn.state.account_ids = vec![missing_account.id.clone()];

        write_connection_config(&storage, &conn).await?;
        storage.save_connection(&conn).await?;

        let conn_id = conn.id().to_string();
        let err = resolve_price_history_scope(&storage, None, Some(conn_id.as_str()))
            .await
            .err()
            .expect("expected missing accounts error");
        assert!(err
            .to_string()
            .contains("No accounts found for connection"));

        Ok(())
    }

    #[tokio::test]
    async fn resolve_scope_connection_uses_accounts_by_connection_id() -> anyhow::Result<()> {
        let dir = TempDir::new()?;
        let storage = JsonFileStorage::new(dir.path());
        let conn = Connection::new(connection_config("Test Connection"));

        write_connection_config(&storage, &conn).await?;
        storage.save_connection(&conn).await?;

        let account = Account::new("Checking", conn.id().clone());
        storage.save_account(&account).await?;

        let conn_id = conn.id().to_string();
        let (scope, accounts) =
            resolve_price_history_scope(&storage, None, Some(conn_id.as_str())).await?;
        assert_eq!(accounts.len(), 1);
        assert_eq!(accounts[0].id, account.id);
        match scope {
            PriceHistoryScopeOutput::Connection { id, .. } => {
                assert_eq!(id, conn_id);
            }
            _ => anyhow::bail!("expected connection scope"),
        }

        Ok(())
    }

    #[tokio::test]
    async fn resolve_scope_connection_falls_back_when_state_ids_missing() -> anyhow::Result<()> {
        let dir = TempDir::new()?;
        let storage = JsonFileStorage::new(dir.path());
        let mut conn = Connection::new(connection_config("Test Connection"));

        conn.state.account_ids = vec![Id::from_string("missing-account")];

        write_connection_config(&storage, &conn).await?;
        storage.save_connection(&conn).await?;

        let account = Account::new("Checking", conn.id().clone());
        storage.save_account(&account).await?;

        let conn_id = conn.id().to_string();
        let (scope, accounts) =
            resolve_price_history_scope(&storage, None, Some(conn_id.as_str())).await?;
        assert_eq!(accounts.len(), 1);
        assert_eq!(accounts[0].id, account.id);
        match scope {
            PriceHistoryScopeOutput::Connection { id, .. } => {
                assert_eq!(id, conn_id);
            }
            _ => anyhow::bail!("expected connection scope"),
        }

        Ok(())
    }

    #[tokio::test]
    async fn resolve_scope_connection_includes_accounts_missing_from_state() -> anyhow::Result<()> {
        let dir = TempDir::new()?;
        let storage = JsonFileStorage::new(dir.path());
        let mut conn = Connection::new(connection_config("Test Connection"));

        let account_a = Account::new("Checking", conn.id().clone());
        conn.state.account_ids = vec![account_a.id.clone()];

        write_connection_config(&storage, &conn).await?;
        storage.save_connection(&conn).await?;

        let account_b = Account::new("Savings", conn.id().clone());
        storage.save_account(&account_a).await?;
        storage.save_account(&account_b).await?;

        let conn_id = conn.id().to_string();
        let (_, accounts) =
            resolve_price_history_scope(&storage, None, Some(conn_id.as_str())).await?;
        assert_eq!(accounts.len(), 2);

        Ok(())
    }
}
