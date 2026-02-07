use anyhow::Result;
use chrono::TimeZone;
use keepbook::models::{Asset, Connection, ConnectionConfig, Id, Transaction, TransactionStatus};
use keepbook::storage::{JsonFileStorage, MemoryStorage, Storage};
use tempfile::TempDir;

#[tokio::test]
async fn json_storage_get_transactions_dedupes_by_id_last_wins() -> Result<()> {
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

    let account = keepbook::models::Account::new("Checking", connection.id().clone());
    storage.save_account(&account).await?;

    let tx_id = Id::from_string("tx-1");
    let older = Transaction::new("-1", Asset::currency("USD"), "Old")
        .with_id(tx_id.clone())
        .with_timestamp(chrono::Utc.with_ymd_and_hms(2026, 2, 1, 0, 0, 0).unwrap())
        .with_status(TransactionStatus::Pending);
    let newer = Transaction::new("-1", Asset::currency("USD"), "New")
        .with_id(tx_id.clone())
        .with_timestamp(chrono::Utc.with_ymd_and_hms(2026, 2, 2, 0, 0, 0).unwrap())
        .with_status(TransactionStatus::Posted);

    // Force duplicates into the backing JSONL file.
    storage.append_transactions(&account.id, &[older]).await?;
    storage.append_transactions(&account.id, &[newer]).await?;

    let loaded = storage.get_transactions(&account.id).await?;
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].id, tx_id);
    assert_eq!(loaded[0].description, "New");
    assert_eq!(loaded[0].status, TransactionStatus::Posted);
    Ok(())
}

#[tokio::test]
async fn json_storage_get_transactions_raw_includes_duplicates_in_append_order() -> Result<()> {
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

    let account = keepbook::models::Account::new("Checking", connection.id().clone());
    storage.save_account(&account).await?;

    let tx_id = Id::from_string("tx-1");
    let older = Transaction::new("-1", Asset::currency("USD"), "Old").with_id(tx_id.clone());
    let newer = Transaction::new("-1", Asset::currency("USD"), "New").with_id(tx_id.clone());

    storage
        .append_transactions(&account.id, &[older.clone()])
        .await?;
    storage
        .append_transactions(&account.id, &[newer.clone()])
        .await?;

    let raw = storage.get_transactions_raw(&account.id).await?;
    assert_eq!(raw.len(), 2);
    assert_eq!(raw[0].description, "Old");
    assert_eq!(raw[1].description, "New");
    Ok(())
}

#[tokio::test]
async fn memory_storage_get_transactions_dedupes_by_id_last_wins() -> Result<()> {
    let storage = MemoryStorage::new();

    let connection = Connection::new(ConnectionConfig {
        name: "Bank".to_string(),
        synchronizer: "manual".to_string(),
        credentials: None,
        balance_staleness: None,
    });
    storage.save_connection(&connection).await?;

    let account = keepbook::models::Account::new("Checking", connection.id().clone());
    storage.save_account(&account).await?;

    let tx_id = Id::from_string("tx-1");
    let first = Transaction::new("-1", Asset::currency("USD"), "Old").with_id(tx_id.clone());
    let second = Transaction::new("-1", Asset::currency("USD"), "New").with_id(tx_id.clone());

    storage.append_transactions(&account.id, &[first]).await?;
    storage.append_transactions(&account.id, &[second]).await?;

    let loaded = storage.get_transactions(&account.id).await?;
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].description, "New");
    Ok(())
}

#[tokio::test]
async fn memory_storage_get_transactions_raw_includes_duplicates_in_append_order() -> Result<()> {
    let storage = MemoryStorage::new();

    let connection = Connection::new(ConnectionConfig {
        name: "Bank".to_string(),
        synchronizer: "manual".to_string(),
        credentials: None,
        balance_staleness: None,
    });
    storage.save_connection(&connection).await?;

    let account = keepbook::models::Account::new("Checking", connection.id().clone());
    storage.save_account(&account).await?;

    let tx_id = Id::from_string("tx-1");
    let first = Transaction::new("-1", Asset::currency("USD"), "Old").with_id(tx_id.clone());
    let second = Transaction::new("-1", Asset::currency("USD"), "New").with_id(tx_id.clone());

    storage.append_transactions(&account.id, &[first]).await?;
    storage.append_transactions(&account.id, &[second]).await?;

    let raw = storage.get_transactions_raw(&account.id).await?;
    assert_eq!(raw.len(), 2);
    assert_eq!(raw[0].description, "Old");
    assert_eq!(raw[1].description, "New");
    Ok(())
}
