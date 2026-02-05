use anyhow::Result;
use keepbook::models::{Account, Connection, ConnectionConfig, ConnectionState, Id};
use keepbook::storage::{JsonFileStorage, Storage};
use tempfile::TempDir;

async fn write_connection_config(
    storage: &JsonFileStorage,
    id: &Id,
    connection: &Connection,
) -> Result<()> {
    storage
        .save_connection_config(id, &connection.config)
        .await?;
    Ok(())
}

#[tokio::test]
async fn load_connection_overrides_mismatched_state_id() -> Result<()> {
    let dir = TempDir::new()?;
    let storage = JsonFileStorage::new(dir.path());

    let dir_id = Id::new();
    let mut connection = Connection {
        config: ConnectionConfig {
            name: "Test Bank".to_string(),
            synchronizer: "manual".to_string(),
            credentials: None,
            balance_staleness: None,
        },
        state: ConnectionState::new(),
    };
    connection.state.id = Id::from_string("mismatch-id");

    write_connection_config(&storage, &dir_id, &connection).await?;

    let state_path = dir
        .path()
        .join("connections")
        .join(dir_id.to_string())
        .join("connection.json");
    tokio::fs::write(state_path, serde_json::to_string_pretty(&connection.state)?).await?;

    let loaded = storage
        .get_connection(&dir_id)
        .await?
        .expect("connection should load");

    assert_eq!(loaded.id(), &dir_id);

    Ok(())
}

#[tokio::test]
async fn load_account_overrides_mismatched_account_id() -> Result<()> {
    let dir = TempDir::new()?;
    let storage = JsonFileStorage::new(dir.path());

    let dir_id = Id::new();
    let mut account = Account::new("Checking", Id::new());
    account.id = Id::from_string("mismatch-account-id");

    let account_path = dir
        .path()
        .join("accounts")
        .join(dir_id.to_string())
        .join("account.json");
    tokio::fs::create_dir_all(account_path.parent().unwrap()).await?;
    tokio::fs::write(account_path, serde_json::to_string_pretty(&account)?).await?;

    let loaded = storage
        .get_account(&dir_id)
        .await?
        .expect("account should load");

    assert_eq!(loaded.id, dir_id);

    Ok(())
}
