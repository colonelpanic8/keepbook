use super::*;

#[tokio::test]
async fn memory_storage_errors_on_missing_connection_balances() -> Result<()> {
    let storage = MemoryStorage::new();
    let missing = Id::new();

    let err = storage
        .get_latest_balances_for_connection(&missing)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("Connection not found"));

    Ok(())
}
