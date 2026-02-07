use anyhow::Result;
use keepbook::models::{Account, Connection, ConnectionConfig};
use keepbook::storage::{JsonFileStorage, Storage};
use tempfile::TempDir;

async fn write_connection_config(storage: &JsonFileStorage, connection: &Connection) -> Result<()> {
    storage
        .save_connection_config(connection.id(), &connection.config)
        .await?;
    Ok(())
}

#[cfg(unix)]
#[tokio::test]
async fn rebuild_all_symlinks_includes_accounts_by_connection_id() -> Result<()> {
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

    let (conn_created, account_created, warnings) = storage.rebuild_all_symlinks().await?;
    assert_eq!(warnings.len(), 0);
    assert_eq!(conn_created, 1);
    assert_eq!(account_created, 1);

    let link_path = dir
        .path()
        .join("connections")
        .join(connection.id().to_string())
        .join("accounts")
        .join("Checking");
    let metadata = std::fs::symlink_metadata(&link_path)?;
    assert!(metadata.file_type().is_symlink());

    Ok(())
}
