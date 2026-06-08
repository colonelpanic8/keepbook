use std::collections::HashMap;

use anyhow::Result;
use chrono::{TimeZone, Utc};

use crate::app::{add_transaction_rule, list_transactions, TransactionRule};
use crate::config::{
    AiConfig, DisplayConfig, GitConfig, HistoryConfig, IgnoreConfig, PortfolioConfig,
    RefreshConfig, SpendingConfig, TagsConfig, TrayConfig,
};
use crate::models::{Account, Asset, Connection, ConnectionConfig, Id, Transaction};
use crate::storage::{MemoryStorage, Storage};
use crate::sync::{PriceRefreshResult, SyncResult, SyncWithPricesResult};

use super::*;

fn resolved_config(data_dir: &std::path::Path) -> ResolvedConfig {
    ResolvedConfig {
        data_dir: data_dir.to_path_buf(),
        reporting_currency: "USD".to_string(),
        display: DisplayConfig::default(),
        refresh: RefreshConfig::default(),
        history: HistoryConfig::default(),
        tray: TrayConfig::default(),
        spending: SpendingConfig::default(),
        tags: TagsConfig {
            aliases: HashMap::new(),
            parents: HashMap::new(),
        },
        portfolio: PortfolioConfig::default(),
        ignore: IgnoreConfig::default(),
        ai: AiConfig::default(),
        git: GitConfig::default(),
    }
}

#[tokio::test]
async fn post_sync_rule_application_scopes_to_synced_connection() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let storage = MemoryStorage::new();
    let config = resolved_config(dir.path());

    let connection = Connection::new(ConnectionConfig {
        name: "Mock Bank".to_string(),
        synchronizer: "mock".to_string(),
        credentials: None,
        balance_staleness: None,
    });
    let other_connection = Connection::new(ConnectionConfig {
        name: "Other Bank".to_string(),
        synchronizer: "mock".to_string(),
        credentials: None,
        balance_staleness: None,
    });
    storage.save_connection(&connection).await?;
    storage.save_connection(&other_connection).await?;

    let account = Account::new_with(
        Id::from_string("acct-synced"),
        Utc::now(),
        "Synced Checking",
        connection.id().clone(),
    );
    let other_account = Account::new_with(
        Id::from_string("acct-other"),
        Utc::now(),
        "Other Checking",
        other_connection.id().clone(),
    );
    storage.save_account(&account).await?;
    storage.save_account(&other_account).await?;

    let synced_tx = Transaction::new("-12.00", Asset::currency("USD"), "Coffee Shop")
        .with_id(Id::from_string("tx-synced"))
        .with_timestamp(Utc.with_ymd_and_hms(2026, 2, 10, 12, 0, 0).unwrap());
    let other_tx = Transaction::new("-13.00", Asset::currency("USD"), "Coffee Shop")
        .with_id(Id::from_string("tx-other"))
        .with_timestamp(Utc.with_ymd_and_hms(2026, 2, 10, 12, 0, 0).unwrap());
    storage
        .append_transactions(&account.id, std::slice::from_ref(&synced_tx))
        .await?;
    storage
        .append_transactions(&other_account.id, std::slice::from_ref(&other_tx))
        .await?;

    add_transaction_rule(
        &config,
        TransactionRule {
            set_tags: Some(vec!["Coffee".to_string()]),
            set_subtags: None,
            set_description: None,
            match_account_id: None,
            match_account_name: None,
            match_description: Some("(?i)coffee".to_string()),
            match_tag: None,
            match_subtag: None,
            match_status: None,
            match_amount: None,
        },
    )
    .await?;

    let outcome = SyncOutcome::Synced {
        report: SyncWithPricesResult {
            result: SyncResult {
                connection: connection.clone(),
                accounts: vec![],
                balances: vec![],
                transactions: vec![],
            },
            stored_prices: 0,
            refresh: PriceRefreshResult::default(),
        },
    };

    let rule_result =
        apply_transaction_rules_after_synced_outcome(&storage, &config, &outcome).await?;
    let rule_result = rule_result.expect("synced outcomes should apply transaction rules");
    assert_eq!(rule_result["matched_count"], 1);
    assert_eq!(rule_result["updated_count"], 1);

    let transactions = list_transactions(
        &storage,
        Some("2026-02-01".to_string()),
        Some("2026-02-28".to_string()),
        None,
        false,
        false,
        &config,
    )
    .await?;
    let synced_out = transactions
        .iter()
        .find(|tx| tx.id == synced_tx.id.to_string())
        .expect("synced transaction should be listed");
    assert_eq!(synced_out.tags, vec!["Coffee".to_string()]);
    let other_out = transactions
        .iter()
        .find(|tx| tx.id == other_tx.id.to_string())
        .expect("other transaction should be listed");
    assert!(other_out.tags.is_empty());

    Ok(())
}
