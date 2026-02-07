use anyhow::Result;
use keepbook::models::{
    Account, Asset, AssetBalance, BalanceSnapshot, Connection, ConnectionConfig, Id,
};
use keepbook::storage::{JsonFileStorage, Storage};
use tempfile::TempDir;

async fn write_connection_config(storage: &JsonFileStorage, connection: &Connection) -> Result<()> {
    storage
        .save_connection_config(connection.id(), &connection.config)
        .await?;
    Ok(())
}

#[tokio::test]
async fn invalid_account_ids_in_state_do_not_break_balance_lookup() -> Result<()> {
    let dir = TempDir::new()?;
    let storage = JsonFileStorage::new(dir.path());

    let mut connection = Connection::new(ConnectionConfig {
        name: "Test Bank".to_string(),
        synchronizer: "manual".to_string(),
        credentials: None,
        balance_staleness: None,
    });

    // Inject an unsafe account id into connection state.
    connection.state.account_ids = vec![Id::from_string("../evil")];

    write_connection_config(&storage, &connection).await?;

    let state_path = dir
        .path()
        .join("connections")
        .join(connection.id().to_string())
        .join("connection.json");
    tokio::fs::create_dir_all(state_path.parent().unwrap()).await?;
    tokio::fs::write(state_path, serde_json::to_string_pretty(&connection.state)?).await?;

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
