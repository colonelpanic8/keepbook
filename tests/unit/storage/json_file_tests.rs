use super::*;
use chrono::{TimeZone, Utc};

use crate::models::{
    Account, Asset, AssetBalance, BalanceSnapshot, Connection, ConnectionConfig, ConnectionState,
    Id, Transaction, TransactionAnnotationPatch, TransactionStandardizedMetadata,
};
use crate::storage::Storage;

#[test]
fn sanitize_name_replaces_path_separators() {
    assert_eq!(
        JsonFileStorage::sanitize_name("foo/bar"),
        Some("foo-bar".to_string())
    );
    assert_eq!(
        JsonFileStorage::sanitize_name("foo\\bar"),
        Some("foo-bar".to_string())
    );
    assert_eq!(JsonFileStorage::sanitize_name("   "), None);
    assert_eq!(JsonFileStorage::sanitize_name("."), None);
    assert_eq!(JsonFileStorage::sanitize_name(".."), None);
}

#[tokio::test]
async fn list_accounts_returns_ids_in_sorted_order() -> anyhow::Result<()> {
    let temp = tempfile::tempdir()?;
    let storage = JsonFileStorage::new(temp.path());
    let created_at = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
    let connection_id = Id::from_string("conn-1");

    storage
        .save_account(&Account::new_with(
            Id::from_string("acct-b"),
            created_at,
            "B",
            connection_id.clone(),
        ))
        .await?;
    storage
        .save_account(&Account::new_with(
            Id::from_string("acct-a"),
            created_at,
            "A",
            connection_id,
        ))
        .await?;

    let ids: Vec<String> = storage
        .list_accounts()
        .await?
        .into_iter()
        .map(|a| a.id.to_string())
        .collect();
    assert_eq!(ids, vec!["acct-a".to_string(), "acct-b".to_string()]);
    Ok(())
}

#[tokio::test]
async fn list_connections_returns_ids_in_sorted_order() -> anyhow::Result<()> {
    let temp = tempfile::tempdir()?;
    let storage = JsonFileStorage::new(temp.path());
    let created_at = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();

    let conn_b = Connection {
        config: ConnectionConfig {
            name: "B".to_string(),
            synchronizer: "manual".to_string(),
            credentials: None,
            balance_staleness: None,
        },
        state: ConnectionState::new_with(Id::from_string("conn-b"), created_at),
    };
    let conn_a = Connection {
        config: ConnectionConfig {
            name: "A".to_string(),
            synchronizer: "manual".to_string(),
            credentials: None,
            balance_staleness: None,
        },
        state: ConnectionState::new_with(Id::from_string("conn-a"), created_at),
    };

    storage
        .save_connection_config(conn_b.id(), &conn_b.config)
        .await?;
    storage.save_connection(&conn_b).await?;
    storage
        .save_connection_config(conn_a.id(), &conn_a.config)
        .await?;
    storage.save_connection(&conn_a).await?;

    let ids: Vec<String> = storage
        .list_connections()
        .await?
        .into_iter()
        .map(|c| c.id().to_string())
        .collect();
    assert_eq!(ids, vec!["conn-a".to_string(), "conn-b".to_string()]);
    Ok(())
}

#[tokio::test]
async fn balance_snapshot_cache_refreshes_when_file_changes() -> anyhow::Result<()> {
    let temp = tempfile::tempdir()?;
    let storage = JsonFileStorage::new(temp.path());
    let account_id = Id::from_string("acct-1");

    let first = BalanceSnapshot::new(
        Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap(),
        vec![AssetBalance::new(Asset::currency("USD"), "10.0")],
    );
    let second = BalanceSnapshot::new(
        Utc.with_ymd_and_hms(2024, 2, 1, 0, 0, 0).unwrap(),
        vec![AssetBalance::new(Asset::currency("USD"), "20.0")],
    );

    storage.append_balance_snapshot(&account_id, &first).await?;
    assert_eq!(storage.get_balance_snapshots(&account_id).await?.len(), 1);
    assert!(storage
        .cache
        .lock()
        .expect("storage cache poisoned")
        .balance_snapshots
        .contains_key(&account_id));

    let path = storage.balances_file(&account_id)?;
    let mut file = fs::OpenOptions::new().append(true).open(&path).await?;
    file.write_all(serde_json::to_string(&second)?.as_bytes())
        .await?;
    file.write_all(b"\n").await?;

    let snapshots = storage.get_balance_snapshots(&account_id).await?;
    assert_eq!(snapshots.len(), 2);
    assert_eq!(snapshots[1].balances[0].amount, "20.0");

    Ok(())
}

#[tokio::test]
async fn recompact_all_jsonl_compacts_and_sorts_logs() -> anyhow::Result<()> {
    let temp = tempfile::tempdir()?;
    let storage = JsonFileStorage::new(temp.path());
    let created_at = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
    let connection_id = Id::from_string("conn-1");
    let account_id = Id::from_string("acct-1");

    storage
        .save_account(&Account::new_with(
            account_id.clone(),
            created_at,
            "Checking",
            connection_id,
        ))
        .await?;

    let older_snapshot = BalanceSnapshot::new(
        Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap(),
        vec![AssetBalance::new(Asset::currency("USD"), "10.0")],
    );
    let newer_snapshot = BalanceSnapshot::new(
        Utc.with_ymd_and_hms(2024, 6, 1, 0, 0, 0).unwrap(),
        vec![AssetBalance::new(Asset::currency("USD"), "20.0")],
    );
    storage
        .append_balance_snapshot(&account_id, &newer_snapshot)
        .await?;
    storage
        .append_balance_snapshot(&account_id, &older_snapshot)
        .await?;

    let tx_old = Transaction::new("-10.0", Asset::currency("USD"), "old")
        .with_id(Id::from_string("tx-1"))
        .with_timestamp(Utc.with_ymd_and_hms(2024, 2, 1, 10, 0, 0).unwrap());
    let tx_new = Transaction::new("-12.0", Asset::currency("USD"), "new")
        .with_id(Id::from_string("tx-1"))
        .with_timestamp(Utc.with_ymd_and_hms(2024, 2, 2, 10, 0, 0).unwrap());
    let tx_other = Transaction::new("5.0", Asset::currency("USD"), "credit")
        .with_id(Id::from_string("tx-2"))
        .with_timestamp(Utc.with_ymd_and_hms(2024, 1, 15, 10, 0, 0).unwrap());
    storage
        .append_transactions(&account_id, &[tx_old, tx_new, tx_other])
        .await?;

    let patch_note = TransactionAnnotationPatch {
        transaction_id: Id::from_string("tx-anno"),
        timestamp: Utc.with_ymd_and_hms(2024, 2, 3, 12, 0, 0).unwrap(),
        description: None,
        note: Some(Some("memo".to_string())),
        tags: None,
        subtags: None,
        effective_date: None,
    };
    let patch_category = TransactionAnnotationPatch {
        transaction_id: Id::from_string("tx-anno"),
        timestamp: Utc.with_ymd_and_hms(2024, 2, 4, 12, 0, 0).unwrap(),
        description: None,
        note: None,
        tags: Some(Some(vec!["food".to_string()])),
        subtags: Some(Some(vec!["coffee".to_string()])),
        effective_date: None,
    };
    let patch_set_then_clear_a = TransactionAnnotationPatch {
        transaction_id: Id::from_string("tx-clear"),
        timestamp: Utc.with_ymd_and_hms(2024, 2, 5, 12, 0, 0).unwrap(),
        description: Some(Some("temp".to_string())),
        note: None,
        tags: None,
        subtags: None,
        effective_date: None,
    };
    let patch_set_then_clear_b = TransactionAnnotationPatch {
        transaction_id: Id::from_string("tx-clear"),
        timestamp: Utc.with_ymd_and_hms(2024, 2, 6, 12, 0, 0).unwrap(),
        description: Some(None),
        note: None,
        tags: None,
        subtags: None,
        effective_date: None,
    };
    storage
        .append_transaction_annotation_patches(
            &account_id,
            &[
                patch_category,
                patch_note,
                patch_set_then_clear_a,
                patch_set_then_clear_b,
            ],
        )
        .await?;

    let stats = storage.recompact_all_jsonl().await?;
    assert_eq!(stats.accounts_processed, 1);
    assert_eq!(stats.files_rewritten, 3);
    assert_eq!(stats.balance_snapshots_before, 2);
    assert_eq!(stats.balance_snapshots_after, 2);
    assert_eq!(stats.transactions_before, 3);
    assert_eq!(stats.transactions_after, 2);
    assert_eq!(stats.annotation_patches_before, 4);
    assert_eq!(stats.annotation_patches_after, 1);

    let snapshots = storage.get_balance_snapshots(&account_id).await?;
    assert_eq!(snapshots.len(), 2);
    assert!(snapshots[0].timestamp < snapshots[1].timestamp);

    let tx_raw = storage.get_transactions_raw(&account_id).await?;
    assert_eq!(tx_raw.len(), 2);
    assert!(tx_raw[0].timestamp <= tx_raw[1].timestamp);
    assert_eq!(tx_raw[0].id.as_str(), "tx-2");
    assert_eq!(tx_raw[1].id.as_str(), "tx-1");
    assert_eq!(tx_raw[1].description, "new");

    let patches = storage
        .get_transaction_annotation_patches(&account_id)
        .await?;
    assert_eq!(patches.len(), 1);
    assert_eq!(patches[0].transaction_id.as_str(), "tx-anno");
    assert_eq!(
        patches[0].note.as_ref().cloned().flatten(),
        Some("memo".to_string())
    );
    assert_eq!(
        patches[0].tags.as_ref().cloned().flatten(),
        Some(vec!["food".to_string()])
    );
    assert_eq!(
        patches[0].subtags.as_ref().cloned().flatten(),
        Some(vec!["coffee".to_string()])
    );

    Ok(())
}

#[tokio::test]
async fn backfill_transaction_metadata_all_persists_backfilled_fields() -> anyhow::Result<()> {
    let temp = tempfile::tempdir()?;
    let storage = JsonFileStorage::new(temp.path());
    let created_at = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
    let account_id = Id::from_string("acct-1");

    storage
        .save_account(&Account::new_with(
            account_id.clone(),
            created_at,
            "Checking",
            Id::from_string("conn-1"),
        ))
        .await?;

    let tx = Transaction {
        id: Id::from_string("tx-1"),
        timestamp: Utc.with_ymd_and_hms(2024, 2, 1, 10, 0, 0).unwrap(),
        amount: "-10.0".to_string(),
        asset: Asset::currency("USD"),
        description: "Coffee".to_string(),
        status: crate::models::TransactionStatus::Posted,
        synchronizer_data: serde_json::json!({
            "merchant_category_code": "5814",
            "etu_standard_expense_category_code": "FOOD_AND_DRINK",
        }),
        standardized_metadata: Some(TransactionStandardizedMetadata {
            merchant_name: Some("Existing Merchant".to_string()),
            merchant_category_code: None,
            merchant_category_label: None,
            transaction_kind: None,
            is_internal_transfer_hint: None,
        }),
    };
    storage.append_transactions(&account_id, &[tx]).await?;

    let tx_path = storage.transactions_file(&account_id)?;
    let before = storage.read_jsonl::<Transaction>(&tx_path).await?;
    assert_eq!(before.len(), 1);
    assert_eq!(
        before[0]
            .standardized_metadata
            .as_ref()
            .and_then(|m| m.merchant_category_label.as_deref()),
        None
    );

    let stats = storage.backfill_transaction_metadata_all().await?;
    assert_eq!(stats.accounts_processed, 1);
    assert_eq!(stats.files_rewritten, 1);
    assert_eq!(stats.transactions_examined, 1);
    assert_eq!(stats.transactions_updated, 1);

    let after = storage.read_jsonl::<Transaction>(&tx_path).await?;
    assert_eq!(after.len(), 1);
    let metadata = after[0]
        .standardized_metadata
        .as_ref()
        .expect("expected metadata");
    assert_eq!(metadata.merchant_name.as_deref(), Some("Existing Merchant"));
    assert_eq!(metadata.merchant_category_code.as_deref(), Some("5814"));
    assert_eq!(
        metadata.merchant_category_label.as_deref(),
        Some("Food And Drink")
    );

    Ok(())
}
