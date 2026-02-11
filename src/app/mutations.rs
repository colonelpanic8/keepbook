use std::collections::HashSet;
use std::str::FromStr;

use anyhow::{Context, Result};

use crate::clock::{Clock, SystemClock};
use crate::config::ResolvedConfig;
use crate::models::{
    Account, Asset, AssetBalance, BalanceSnapshot, Connection, ConnectionConfig, ConnectionState,
    Id, IdGenerator, UuidIdGenerator,
};
use crate::storage::Storage;

use super::maybe_auto_commit;

pub async fn remove_connection(
    storage: &dyn Storage,
    config: &ResolvedConfig,
    id_str: &str,
) -> Result<serde_json::Value> {
    let id = Id::from_string_checked(id_str)
        .with_context(|| format!("Invalid connection id: {id_str}"))?;

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

    let mut account_ids: Vec<Id> = Vec::new();
    let mut seen_ids: HashSet<Id> = HashSet::new();
    for id in &conn.state.account_ids {
        if valid_ids.contains(id) && seen_ids.insert(id.clone()) {
            account_ids.push(id.clone());
        }
    }

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
    storage: &dyn Storage,
    config: &ResolvedConfig,
    name: &str,
) -> Result<serde_json::Value> {
    add_connection_with(storage, config, name, &UuidIdGenerator, &SystemClock).await
}

pub async fn add_connection_with(
    storage: &dyn Storage,
    config: &ResolvedConfig,
    name: &str,
    ids: &dyn IdGenerator,
    clock: &dyn Clock,
) -> Result<serde_json::Value> {
    let existing = storage
        .list_connections()
        .await?
        .into_iter()
        .find(|conn| conn.config.name.eq_ignore_ascii_case(name));
    if existing.is_some() {
        anyhow::bail!("Connection name already exists: {name}");
    }

    let connection = Connection {
        config: ConnectionConfig {
            name: name.to_string(),
            synchronizer: "manual".to_string(),
            credentials: None,
            balance_staleness: None,
        },
        state: ConnectionState::new_with_generator(ids, clock),
    };

    let id = connection.state.id.to_string();

    // Write the config TOML since save_connection only writes state.
    storage
        .save_connection_config(connection.id(), &connection.config)
        .await?;

    // Save the connection (this creates the directory structure and symlinks)
    storage.save_connection(&connection).await?;

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
    storage: &dyn Storage,
    config: &ResolvedConfig,
    connection_id: &str,
    name: &str,
    tags: Vec<String>,
) -> Result<serde_json::Value> {
    add_account_with(
        storage,
        config,
        connection_id,
        name,
        tags,
        &UuidIdGenerator,
        &SystemClock,
    )
    .await
}

pub async fn add_account_with(
    storage: &dyn Storage,
    config: &ResolvedConfig,
    connection_id: &str,
    name: &str,
    tags: Vec<String>,
    ids: &dyn IdGenerator,
    clock: &dyn Clock,
) -> Result<serde_json::Value> {
    let conn_id = Id::from_string_checked(connection_id)
        .with_context(|| format!("Invalid connection id: {connection_id}"))?;

    // Verify connection exists
    let mut connection = storage
        .get_connection(&conn_id)
        .await?
        .context("Connection not found")?;

    // Create account
    let mut account = Account::new_with_generator(ids, clock, name, conn_id.clone());
    account.tags = tags;

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
    storage: &dyn Storage,
    config: &ResolvedConfig,
    account_id: &str,
    asset_str: &str,
    amount: &str,
) -> Result<serde_json::Value> {
    let amount = amount.trim();
    if amount.is_empty() {
        anyhow::bail!("Amount cannot be empty");
    }
    rust_decimal::Decimal::from_str(amount).with_context(|| format!("Invalid amount: {amount}"))?;

    let id = Id::from_string_checked(account_id)
        .with_context(|| format!("Invalid account id: {account_id}"))?;

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
