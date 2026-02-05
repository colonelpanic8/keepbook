use anyhow::Result;
use keepbook::models::{Account, Connection, ConnectionConfig};
use keepbook::storage::{JsonFileStorage, Storage};
use tempfile::TempDir;

#[tokio::test]
async fn list_connections_skips_invalid_configs() -> Result<()> {
    let dir = TempDir::new()?;
    let storage = JsonFileStorage::new(dir.path());

    let good = Connection::new(ConnectionConfig {
        name: "Good".to_string(),
        synchronizer: "manual".to_string(),
        credentials: None,
        balance_staleness: None,
    });
    storage
        .save_connection_config(good.id(), &good.config)
        .await?;
    storage.save_connection(&good).await?;

    let bad_dir = dir.path().join("connections").join("bad-conn");
    std::fs::create_dir_all(&bad_dir)?;
    std::fs::write(bad_dir.join("connection.toml"), "this is not valid toml")?;

    let connections = storage.list_connections().await?;
    assert_eq!(connections.len(), 1);
    assert_eq!(connections[0].id(), good.id());

    Ok(())
}

#[tokio::test]
async fn list_accounts_skips_invalid_json() -> Result<()> {
    let dir = TempDir::new()?;
    let storage = JsonFileStorage::new(dir.path());

    let connection = Connection::new(ConnectionConfig {
        name: "Bank".to_string(),
        synchronizer: "manual".to_string(),
        credentials: None,
        balance_staleness: None,
    });
    storage
        .save_connection_config(connection.id(), &connection.config)
        .await?;
    storage.save_connection(&connection).await?;

    let good = Account::new("Checking", connection.id().clone());
    storage.save_account(&good).await?;

    let bad_dir = dir.path().join("accounts").join("bad-acct");
    std::fs::create_dir_all(&bad_dir)?;
    std::fs::write(bad_dir.join("account.json"), "{not valid json")?;

    let accounts = storage.list_accounts().await?;
    assert_eq!(accounts.len(), 1);
    assert_eq!(accounts[0].id, good.id);

    Ok(())
}

