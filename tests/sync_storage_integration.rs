mod support;

use anyhow::Result;
use keepbook::storage::{JsonFileStorage, Storage};
use keepbook::sync::{AccountBalances, AccountListing, Synchronizer};
use support::{mock_connection, MockSynchronizer};
use tempfile::TempDir;

#[tokio::test]
async fn failed_sync_write_does_not_advance_connection_cursor() -> Result<()> {
    for blocked_file in ["balances.jsonl", "transactions.jsonl"] {
        let dir = TempDir::new()?;
        let storage = JsonFileStorage::new(dir.path());
        let mut connection = mock_connection("Mock Bank");
        connection.state.synchronizer_data = serde_json::json!({"transactions_cursor": "old"});
        storage
            .save_connection_config(connection.id(), &connection.config)
            .await?;
        storage.save_connection(&connection).await?;

        let mut result = MockSynchronizer::new()
            .sync(&mut connection, &storage)
            .await?;
        result.connection.state.synchronizer_data =
            serde_json::json!({"transactions_cursor": "new"});
        let account_id = &result.accounts[0].id;
        let blocked_path = dir
            .path()
            .join("accounts")
            .join(account_id.as_str())
            .join(blocked_file);
        tokio::fs::create_dir_all(&blocked_path).await?;

        assert!(result.save(&storage).await.is_err());
        let persisted = storage.get_connection(connection.id()).await?.unwrap();
        assert_eq!(
            persisted.state.synchronizer_data["transactions_cursor"], "old",
            "{blocked_file}"
        );

        tokio::fs::remove_dir(&blocked_path).await?;
        result.save(&storage).await?;
        let persisted = storage.get_connection(connection.id()).await?.unwrap();
        assert_eq!(
            persisted.state.synchronizer_data["transactions_cursor"],
            "new"
        );
        assert_eq!(storage.get_transactions(account_id).await?.len(), 1);
        assert!(storage
            .get_latest_balance_snapshot(account_id)
            .await?
            .is_some());
    }
    Ok(())
}

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

#[tokio::test]
async fn unavailable_balances_leave_stored_history_untouched() -> Result<()> {
    let dir = TempDir::new()?;
    let storage = JsonFileStorage::new(dir.path());
    let mut connection = mock_connection("Mock Bank");
    storage
        .save_connection_config(connection.id(), &connection.config)
        .await?;
    storage.save_connection(&connection).await?;

    let result = MockSynchronizer::new()
        .sync(&mut connection, &storage)
        .await?;
    result.save(&storage).await?;
    let account_id = result.accounts[0].id.clone();
    let stored = storage.get_balance_snapshots(&account_id).await?;
    assert_eq!(stored.len(), 1);

    // A failed balance fetch must not be recorded as "holds nothing".
    let mut failed = MockSynchronizer::new()
        .sync(&mut connection, &storage)
        .await?;
    failed.balances = vec![(
        account_id.clone(),
        AccountBalances::unavailable("detail request failed"),
    )];
    failed.save(&storage).await?;

    let after = storage.get_balance_snapshots(&account_id).await?;
    assert_eq!(after.len(), 1, "unavailable balances must not be appended");
    assert_eq!(after[0].balances[0].amount, stored[0].balances[0].amount);

    Ok(())
}

#[tokio::test]
async fn an_empty_snapshot_records_that_the_account_holds_nothing() -> Result<()> {
    let dir = TempDir::new()?;
    let storage = JsonFileStorage::new(dir.path());
    let mut connection = mock_connection("Mock Bank");
    storage
        .save_connection_config(connection.id(), &connection.config)
        .await?;
    storage.save_connection(&connection).await?;

    let result = MockSynchronizer::new()
        .sync(&mut connection, &storage)
        .await?;
    result.save(&storage).await?;
    let account_id = result.accounts[0].id.clone();

    let mut emptied = MockSynchronizer::new()
        .sync(&mut connection, &storage)
        .await?;
    emptied.balances = vec![(account_id.clone(), AccountBalances::snapshot(Vec::new()))];
    emptied.save(&storage).await?;

    let latest = storage
        .get_latest_balance_snapshot(&account_id)
        .await?
        .expect("empty snapshot should be recorded");
    assert!(latest.balances.is_empty());

    Ok(())
}

#[tokio::test]
async fn a_partial_account_listing_does_not_deactivate_missing_accounts() -> Result<()> {
    let dir = TempDir::new()?;
    let storage = JsonFileStorage::new(dir.path());
    let mut connection = mock_connection("Mock Bank");
    storage
        .save_connection_config(connection.id(), &connection.config)
        .await?;
    storage.save_connection(&connection).await?;

    let result = MockSynchronizer::new()
        .sync(&mut connection, &storage)
        .await?;
    result.save(&storage).await?;
    let account_id = result.accounts[0].id.clone();

    let mut partial = MockSynchronizer::new()
        .sync(&mut connection, &storage)
        .await?;
    partial.accounts.clear();
    partial.balances.clear();
    partial.transactions.clear();
    partial.account_listing = AccountListing::partial("account list request failed");
    partial.save(&storage).await?;

    let account = storage.get_account(&account_id).await?.unwrap();
    assert!(
        account.active,
        "a partial listing says nothing about missing accounts"
    );
    assert_eq!(storage.get_balance_snapshots(&account_id).await?.len(), 1);

    // A complete listing that omits the account still deactivates it.
    let mut complete = MockSynchronizer::new()
        .sync(&mut connection, &storage)
        .await?;
    complete.accounts.clear();
    complete.balances.clear();
    complete.transactions.clear();
    complete.save(&storage).await?;

    let account = storage.get_account(&account_id).await?.unwrap();
    assert!(!account.active);

    Ok(())
}
