use std::path::Path;

use anyhow::Result;
use keepbook::app::list_balances;
use keepbook::config::{GitConfig, RefreshConfig, ResolvedConfig};
use keepbook::models::{
    Account, Asset, AssetBalance, BalanceSnapshot, Connection, ConnectionConfig,
};
use keepbook::storage::{JsonFileStorage, Storage};
use tempfile::TempDir;

fn resolved_config(data_dir: &Path) -> ResolvedConfig {
    ResolvedConfig {
        data_dir: data_dir.to_path_buf(),
        reporting_currency: "USD".to_string(),
        refresh: RefreshConfig::default(),
        git: GitConfig::default(),
    }
}

async fn write_connection_config(storage: &JsonFileStorage, connection: &Connection) -> Result<()> {
    storage
        .save_connection_config(connection.id(), &connection.config)
        .await?;
    Ok(())
}

#[tokio::test]
async fn list_balances_falls_back_to_accounts_by_connection_id() -> Result<()> {
    let dir = TempDir::new()?;
    let storage = JsonFileStorage::new(dir.path());
    let _config = resolved_config(dir.path());

    let connection = Connection::new(ConnectionConfig {
        name: "Test Bank".to_string(),
        synchronizer: "manual".to_string(),
        credentials: None,
        balance_staleness: None,
    });

    write_connection_config(&storage, &connection).await?;
    storage.save_connection(&connection).await?;

    let account = Account::new("Checking", connection.id().clone());
    storage.save_account(&account).await?;

    let snapshot = BalanceSnapshot::now(vec![AssetBalance::new(Asset::currency("USD"), "100")]);
    storage
        .append_balance_snapshot(&account.id, &snapshot)
        .await?;

    let balances = list_balances(&storage, &_config).await?;
    assert_eq!(balances.len(), 1, "expected balance even if state is empty");
    assert_eq!(
        balances[0].value_in_reporting_currency.as_deref(),
        Some("100")
    );
    assert_eq!(balances[0].reporting_currency, "USD");

    Ok(())
}

#[tokio::test]
async fn latest_balances_for_connection_includes_accounts_by_connection_id() -> Result<()> {
    let dir = TempDir::new()?;
    let storage = JsonFileStorage::new(dir.path());

    let connection = Connection::new(ConnectionConfig {
        name: "Test Bank".to_string(),
        synchronizer: "manual".to_string(),
        credentials: None,
        balance_staleness: None,
    });

    write_connection_config(&storage, &connection).await?;
    storage.save_connection(&connection).await?;

    let account = Account::new("Checking", connection.id().clone());
    storage.save_account(&account).await?;

    let snapshot = BalanceSnapshot::now(vec![AssetBalance::new(Asset::currency("USD"), "100")]);
    storage
        .append_balance_snapshot(&account.id, &snapshot)
        .await?;

    let balances = storage
        .get_latest_balances_for_connection(connection.id())
        .await?;

    assert_eq!(balances.len(), 1);
    assert_eq!(balances[0].0, account.id);

    Ok(())
}
