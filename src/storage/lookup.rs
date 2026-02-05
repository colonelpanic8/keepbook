use anyhow::Result;

use crate::models::{Account, Connection, Id};

use super::Storage;

pub async fn find_connection(
    storage: &dyn Storage,
    id_or_name: &str,
) -> Result<Option<Connection>> {
    if Id::is_path_safe(id_or_name) {
        let id = Id::from_string(id_or_name);
        if let Some(conn) = storage.get_connection(&id).await? {
            return Ok(Some(conn));
        }
    }

    let connections = storage.list_connections().await?;
    let mut matches: Vec<Connection> = connections
        .into_iter()
        .filter(|conn| conn.config.name.eq_ignore_ascii_case(id_or_name))
        .collect();

    if matches.is_empty() {
        return Ok(None);
    }

    if matches.len() > 1 {
        let ids: Vec<String> = matches.iter().map(|c| c.id().to_string()).collect();
        anyhow::bail!("Multiple connections named '{id_or_name}'. Use an ID instead: {ids:?}");
    }

    Ok(matches.pop())
}

pub async fn find_account(storage: &dyn Storage, id_or_name: &str) -> Result<Option<Account>> {
    if Id::is_path_safe(id_or_name) {
        let id = Id::from_string(id_or_name);
        if let Some(account) = storage.get_account(&id).await? {
            return Ok(Some(account));
        }
    }

    let accounts = storage.list_accounts().await?;
    let mut matches: Vec<Account> = accounts
        .into_iter()
        .filter(|a| a.name.eq_ignore_ascii_case(id_or_name))
        .collect();

    if matches.is_empty() {
        return Ok(None);
    }

    if matches.len() > 1 {
        let ids: Vec<String> = matches.iter().map(|a| a.id.to_string()).collect();
        anyhow::bail!("Multiple accounts named '{id_or_name}'. Use an ID instead: {ids:?}");
    }

    Ok(matches.pop())
}

#[cfg(test)]
mod tests {
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
}
