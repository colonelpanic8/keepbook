use anyhow::Result;
use keepbook::models::{Account, Connection, ConnectionConfig};
use keepbook::storage::{JsonFileStorage, Storage};

#[tokio::test]
async fn json_storage_skips_unsafe_connection_dirs_in_listing() -> Result<()> {
    let dir = tempfile::TempDir::new()?;
    let storage = JsonFileStorage::new(dir.path());

    // Create an "unsafe" directory name that would fail Id::is_path_safe.
    // Backslash is allowed in Unix filenames, but Keepbook ids intentionally disallow it.
    let unsafe_dir = dir.path().join("connections").join("bad\\id");
    tokio::fs::create_dir_all(&unsafe_dir).await?;

    // Also create a valid connection to ensure listing still works.
    let connection = Connection::new(ConnectionConfig {
        name: "Valid".to_string(),
        synchronizer: "manual".to_string(),
        credentials: None,
        balance_staleness: None,
    });
    storage
        .save_connection_config(connection.id(), &connection.config)
        .await?;
    storage.save_connection(&connection).await?;

    let connections = storage.list_connections().await?;
    assert_eq!(connections.len(), 1);
    assert_eq!(connections[0].name(), "Valid");

    Ok(())
}

#[tokio::test]
async fn json_storage_skips_unsafe_account_dirs_in_listing() -> Result<()> {
    let dir = tempfile::TempDir::new()?;
    let storage = JsonFileStorage::new(dir.path());

    let unsafe_dir = dir.path().join("accounts").join("bad\\id");
    tokio::fs::create_dir_all(&unsafe_dir).await?;

    // Create a valid account.
    let connection = Connection::new(ConnectionConfig {
        name: "Valid".to_string(),
        synchronizer: "manual".to_string(),
        credentials: None,
        balance_staleness: None,
    });
    storage
        .save_connection_config(connection.id(), &connection.config)
        .await?;
    storage.save_connection(&connection).await?;

    let account = Account::new("Checking", connection.id().clone());
    storage.save_account(&account).await?;

    let accounts = storage.list_accounts().await?;
    assert_eq!(accounts.len(), 1);
    assert_eq!(accounts[0].name, "Checking");

    Ok(())
}

