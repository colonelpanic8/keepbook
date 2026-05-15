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
#[path = "../../tests/unit/storage/lookup_tests.rs"]
mod lookup_tests;
