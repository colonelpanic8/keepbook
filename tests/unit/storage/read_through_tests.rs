use super::*;
use crate::models::{Account, Asset, AssetBalance, Connection, ConnectionConfig};
use crate::storage::MemoryStorage;
use chrono::{TimeZone, Utc};

async fn snapshot_at(day: u32, amount: &str) -> BalanceSnapshot {
    BalanceSnapshot::new(
        Utc.with_ymd_and_hms(2026, 2, day, 12, 0, 0).unwrap(),
        vec![AssetBalance::new(Asset::currency("USD"), amount)],
    )
}

#[tokio::test]
async fn a_balance_log_is_read_once_and_re_read_after_a_write() -> Result<()> {
    let inner = Arc::new(MemoryStorage::new());
    let connection = Connection::new(ConnectionConfig {
        name: "Bank".into(),
        synchronizer: "manual".into(),
        credentials: None,
        balance_staleness: None,
    });
    inner.save_connection(&connection).await?;
    let account = Account::new("Checking", connection.id().clone());
    inner.save_account(&account).await?;
    inner
        .append_balance_snapshot(&account.id, &snapshot_at(1, "10").await)
        .await?;

    let storage = ReadThroughStorage::new(inner.clone());
    assert_eq!(storage.list_accounts().await?.len(), 1);
    assert_eq!(storage.get_balance_snapshots(&account.id).await?.len(), 1);

    // Written behind the wrapper's back: the memoized read still stands, which
    // is what makes it safe only for the span of one request.
    inner
        .append_balance_snapshot(&account.id, &snapshot_at(2, "20").await)
        .await?;
    assert_eq!(storage.get_balance_snapshots(&account.id).await?.len(), 1);

    // Written through the wrapper: what was memoized is discarded.
    storage
        .append_balance_snapshot(&account.id, &snapshot_at(3, "30").await)
        .await?;
    assert_eq!(storage.get_balance_snapshots(&account.id).await?.len(), 3);

    Ok(())
}
