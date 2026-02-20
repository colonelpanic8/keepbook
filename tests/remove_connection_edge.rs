use std::path::Path;

use anyhow::Result;
use keepbook::app::remove_connection;
use keepbook::config::{
    DisplayConfig, GitConfig, RefreshConfig, ResolvedConfig, SpendingConfig, TrayConfig,
};
use keepbook::models::{Account, Connection, ConnectionConfig};
use keepbook::storage::{JsonFileStorage, Storage};
use tempfile::TempDir;

fn resolved_config(data_dir: &Path) -> ResolvedConfig {
    ResolvedConfig {
        data_dir: data_dir.to_path_buf(),
        reporting_currency: "USD".to_string(),
        display: DisplayConfig::default(),
        refresh: RefreshConfig::default(),
        tray: TrayConfig::default(),
        spending: SpendingConfig::default(),
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
async fn remove_connection_deletes_accounts_by_connection_id() -> Result<()> {
    let dir = TempDir::new()?;
    let storage = JsonFileStorage::new(dir.path());
    let config = resolved_config(dir.path());

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

    remove_connection(&storage, &config, connection.id().as_str()).await?;

    let still_exists = storage.get_account(&account.id).await?.is_some();
    assert!(
        !still_exists,
        "account should be deleted even if connection state has no account_ids"
    );

    Ok(())
}

#[tokio::test]
async fn remove_connection_does_not_delete_foreign_accounts() -> Result<()> {
    let dir = TempDir::new()?;
    let storage = JsonFileStorage::new(dir.path());
    let config = resolved_config(dir.path());

    let mut connection_a = Connection::new(ConnectionConfig {
        name: "Bank A".to_string(),
        synchronizer: "manual".to_string(),
        credentials: None,
        balance_staleness: None,
    });
    let connection_b = Connection::new(ConnectionConfig {
        name: "Bank B".to_string(),
        synchronizer: "manual".to_string(),
        credentials: None,
        balance_staleness: None,
    });

    write_connection_config(&storage, &connection_a).await?;
    write_connection_config(&storage, &connection_b).await?;
    storage.save_connection(&connection_a).await?;
    storage.save_connection(&connection_b).await?;

    let account_b = Account::new("Savings", connection_b.id().clone());
    storage.save_account(&account_b).await?;

    // Corrupt connection A state with a foreign account id.
    connection_a.state.account_ids = vec![account_b.id.clone()];
    storage.save_connection(&connection_a).await?;

    remove_connection(&storage, &config, connection_a.id().as_str()).await?;

    let still_exists = storage.get_account(&account_b.id).await?.is_some();
    assert!(still_exists, "foreign account should not be deleted");

    Ok(())
}
