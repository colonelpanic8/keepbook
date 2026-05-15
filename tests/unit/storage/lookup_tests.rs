use super::*;
use crate::models::{Account, ConnectionConfig};
use crate::storage::JsonFileStorage;
use tempfile::TempDir;

#[tokio::test]
async fn find_account_errors_on_duplicate_names() -> Result<()> {
    let dir = TempDir::new()?;
    let storage = JsonFileStorage::new(dir.path());

    let conn = crate::models::Connection::new(ConnectionConfig {
        name: "Bank".to_string(),
        synchronizer: "manual".to_string(),
        credentials: None,
        balance_staleness: None,
    });
    storage.save_connection(&conn).await?;

    let mut first = Account::new("Checking", conn.id().clone());
    let mut second = Account::new("Checking", conn.id().clone());
    first.id = Id::from_string("acct-1");
    second.id = Id::from_string("acct-2");
    storage.save_account(&first).await?;
    storage.save_account(&second).await?;

    let err = find_account(&storage, "Checking").await.unwrap_err();
    assert!(err.to_string().contains("Multiple accounts named"));

    Ok(())
}

#[tokio::test]
async fn find_connection_errors_on_duplicate_names() -> Result<()> {
    let dir = TempDir::new()?;
    let storage = JsonFileStorage::new(dir.path());

    let mut first = crate::models::Connection::new(ConnectionConfig {
        name: "Duplicate".to_string(),
        synchronizer: "manual".to_string(),
        credentials: None,
        balance_staleness: None,
    });
    let mut second = crate::models::Connection::new(ConnectionConfig {
        name: "Duplicate".to_string(),
        synchronizer: "manual".to_string(),
        credentials: None,
        balance_staleness: None,
    });
    first.state.account_ids = vec![];
    second.state.account_ids = vec![];
    storage
        .save_connection_config(first.id(), &first.config)
        .await?;
    storage
        .save_connection_config(second.id(), &second.config)
        .await?;
    storage.save_connection(&first).await?;
    storage.save_connection(&second).await?;

    let err = find_connection(&storage, "Duplicate").await.unwrap_err();
    assert!(err.to_string().contains("Multiple connections named"));

    Ok(())
}
