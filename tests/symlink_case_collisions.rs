use anyhow::Result;
use keepbook::models::{Connection, ConnectionConfig};
use keepbook::storage::{JsonFileStorage, Storage};

#[cfg(unix)]
#[tokio::test]
async fn rebuild_connection_symlinks_treats_names_case_insensitively() -> Result<()> {
    let dir = tempfile::TempDir::new()?;
    let storage = JsonFileStorage::new(dir.path());

    let conn_a = Connection::new(ConnectionConfig {
        name: "Test Bank".to_string(),
        synchronizer: "manual".to_string(),
        credentials: None,
        balance_staleness: None,
    });
    let conn_b = Connection::new(ConnectionConfig {
        name: "test bank".to_string(),
        synchronizer: "manual".to_string(),
        credentials: None,
        balance_staleness: None,
    });

    storage
        .save_connection_config(conn_a.id(), &conn_a.config)
        .await?;
    storage.save_connection(&conn_a).await?;
    storage
        .save_connection_config(conn_b.id(), &conn_b.config)
        .await?;
    storage.save_connection(&conn_b).await?;

    let (created, warnings) = storage.rebuild_connection_symlinks().await?;

    // Only one should be created, since add_connection enforces case-insensitive uniqueness.
    assert_eq!(created, 1);
    assert_eq!(warnings.len(), 1);

    // Exactly one symlink should exist.
    let by_name = dir.path().join("connections").join("by-name");
    let entries: Vec<_> = std::fs::read_dir(by_name)?.collect();
    assert_eq!(entries.len(), 1);

    Ok(())
}
