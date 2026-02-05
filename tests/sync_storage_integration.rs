mod support;

use anyhow::Result;
use keepbook::storage::{JsonFileStorage, Storage};
use keepbook::sync::Synchronizer;
use support::{mock_connection, MockSynchronizer};
use tempfile::TempDir;

#[tokio::test]
async fn test_sync_result_persists_data() -> Result<()> {
    let dir = TempDir::new()?;
    let storage = JsonFileStorage::new(dir.path());

    let mut connection = mock_connection("Mock Bank");

    // Persist connection config so JsonFileStorage can reload it later.
    storage
        .save_connection_config(connection.id(), &connection.config)
        .await?;

    let synchronizer = MockSynchronizer::new();
    let result = synchronizer.sync(&mut connection, &storage).await?;
    result.save(&storage).await?;

    let loaded = storage
        .get_connection(connection.id())
        .await?
        .expect("connection should exist");
    assert_eq!(loaded.state.account_ids.len(), 1);

    let account_id = loaded.state.account_ids[0].clone();
    let account = storage
        .get_account(&account_id)
        .await?
        .expect("account should exist");
    assert_eq!(account.name, "Mock Checking");

    let snapshots = storage.get_balance_snapshots(&account_id).await?;
    assert_eq!(snapshots.len(), 1);
    assert_eq!(snapshots[0].balances.len(), 1);
    assert_eq!(snapshots[0].balances[0].amount, "123.45");

    let transactions = storage.get_transactions(&account_id).await?;
    assert_eq!(transactions.len(), 1);
    assert_eq!(transactions[0].description, "Test purchase");

    Ok(())
}

#[tokio::test]
async fn test_sync_twice_does_not_duplicate_transactions() -> Result<()> {
    let dir = TempDir::new()?;
    let storage = JsonFileStorage::new(dir.path());

    let mut connection = mock_connection("Mock Bank");

    // Persist connection config so JsonFileStorage can reload it later.
    storage
        .save_connection_config(connection.id(), &connection.config)
        .await?;

    let synchronizer = MockSynchronizer::new();

    for _ in 0..2 {
        let result = synchronizer.sync(&mut connection, &storage).await?;
        result.save(&storage).await?;
    }

    let loaded = storage
        .get_connection(connection.id())
        .await?
        .expect("connection should exist");
    assert_eq!(loaded.state.account_ids.len(), 1);

    let account_id = loaded.state.account_ids[0].clone();
    let transactions = storage.get_transactions(&account_id).await?;
    assert_eq!(
        transactions.len(),
        1,
        "same transaction id should not be appended twice"
    );

    Ok(())
}

#[cfg(unix)]
#[tokio::test]
async fn test_sync_result_creates_account_symlink() -> Result<()> {
    let dir = TempDir::new()?;
    let storage = JsonFileStorage::new(dir.path());

    let mut connection = mock_connection("Mock Bank");

    // Persist connection config so JsonFileStorage can reload it later.
    storage
        .save_connection_config(connection.id(), &connection.config)
        .await?;

    let synchronizer = MockSynchronizer::new();
    let result = synchronizer.sync(&mut connection, &storage).await?;
    result.save(&storage).await?;

    let link_path = dir
        .path()
        .join("connections")
        .join(connection.id().to_string())
        .join("accounts")
        .join("Mock Checking");
    let metadata = std::fs::symlink_metadata(&link_path)?;
    assert!(metadata.file_type().is_symlink());

    Ok(())
}
