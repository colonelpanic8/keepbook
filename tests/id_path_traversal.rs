use anyhow::Result;
use keepbook::models::Id;
use keepbook::storage::{JsonFileStorage, Storage};
use tempfile::TempDir;

#[tokio::test]
async fn storage_rejects_path_traversal_ids() -> Result<()> {
    let dir = TempDir::new()?;
    let storage = JsonFileStorage::new(dir.path());

    let bad_id = Id::from_string("../escape");

    let err = storage.get_account(&bad_id).await.unwrap_err();
    assert!(err.to_string().contains("Invalid id path segment"));

    let err = storage.get_connection(&bad_id).await.unwrap_err();
    assert!(err.to_string().contains("Invalid id path segment"));

    Ok(())
}
