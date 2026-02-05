use anyhow::Result;
use keepbook::app::list_connections;
use keepbook::models::{Connection, ConnectionConfig, ConnectionStatus};
use keepbook::storage::{JsonFileStorage, Storage};

#[tokio::test]
async fn list_connections_renders_status_as_snake_case() -> Result<()> {
    let dir = tempfile::TempDir::new()?;
    let storage = JsonFileStorage::new(dir.path());

    let mut connection = Connection::new(ConnectionConfig {
        name: "Test".to_string(),
        synchronizer: "manual".to_string(),
        credentials: None,
        balance_staleness: None,
    });
    connection.state.status = ConnectionStatus::PendingReauth;

    storage
        .save_connection_config(connection.id(), &connection.config)
        .await?;
    storage.save_connection(&connection).await?;

    let connections = list_connections(&storage).await?;
    assert_eq!(connections.len(), 1);
    assert_eq!(connections[0].status, "pending_reauth");

    Ok(())
}

