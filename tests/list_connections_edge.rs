use anyhow::Result;
use keepbook::app::list_connections;
use keepbook::models::{Account, Connection, ConnectionConfig};
use keepbook::storage::{JsonFileStorage, Storage};
use tempfile::TempDir;

async fn write_connection_config(
    storage: &JsonFileStorage,
    connection: &Connection,
) -> Result<()> {
    let config_path = storage.connection_config_path(connection.id())?;
    tokio::fs::create_dir_all(config_path.parent().unwrap()).await?;
    let config_toml = toml::to_string_pretty(&connection.config)?;
    tokio::fs::write(&config_path, config_toml).await?;
    Ok(())
}

#[tokio::test]
async fn list_connections_counts_accounts_by_connection_id() -> Result<()> {
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

    let connections = list_connections(&storage).await?;
    assert_eq!(connections.len(), 1);
    assert_eq!(connections[0].account_count, 1);

    Ok(())
}
