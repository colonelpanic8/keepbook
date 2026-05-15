use super::*;
use crate::clock::{Clock, FixedClock};
use crate::config::{
    AiConfig, DisplayConfig, GitConfig, HistoryConfig, IgnoreConfig, PortfolioConfig,
    RefreshConfig, SpendingConfig, TrayConfig,
};
use crate::models::{
    Account, Asset, Connection, ConnectionConfig, ConnectionState, FixedIdGenerator, Id,
    Transaction,
};
use crate::storage::{MemoryStorage, Storage};
use chrono::{TimeZone, Utc};

fn test_config() -> ResolvedConfig {
    ResolvedConfig {
        data_dir: std::path::PathBuf::from("/tmp"),
        reporting_currency: "USD".to_string(),
        display: DisplayConfig::default(),
        refresh: RefreshConfig::default(),
        history: HistoryConfig::default(),
        tray: TrayConfig::default(),
        spending: SpendingConfig::default(),
        tags: Default::default(),
        portfolio: PortfolioConfig::default(),
        ignore: IgnoreConfig::default(),
        ai: AiConfig::default(),
        git: GitConfig::default(),
    }
}

async fn storage_with_transactions(transactions: &[Transaction]) -> Result<MemoryStorage> {
    let storage = MemoryStorage::new();
    let conn_id = Id::from_string("conn-1");
    let account_id = Id::from_string("acct-1");
    storage
        .save_connection(&Connection {
            config: ConnectionConfig {
                name: "Test".to_string(),
                synchronizer: "manual".to_string(),
                credentials: None,
                balance_staleness: None,
            },
            state: ConnectionState::new_with(
                conn_id.clone(),
                Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
            ),
        })
        .await?;
    storage
        .save_account(&Account::new_with(
            account_id.clone(),
            Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
            "Checking",
            conn_id,
        ))
        .await?;
    storage
        .append_transactions(&account_id, transactions)
        .await?;
    Ok(storage)
}

fn tx(id: &str, date: (i32, u32, u32), amount: &str, description: &str) -> Transaction {
    let ids = FixedIdGenerator::new([Id::from_string(id)]);
    let clock = FixedClock::new(
        Utc.with_ymd_and_hms(date.0, date.1, date.2, 12, 0, 0)
            .unwrap(),
    );
    Transaction::new_with_generator(&ids, &clock, amount, Asset::currency("USD"), description)
        .with_timestamp(clock.now())
}

#[tokio::test]
async fn detects_monthly_recurring_transactions_with_noisy_names() -> Result<()> {
    let storage = storage_with_transactions(&[
        tx("tx-1", (2026, 1, 14), "-11.99", "SPOTIFY USA 1234"),
        tx("tx-2", (2026, 2, 14), "-11.99", "Spotify.com"),
        tx("tx-3", (2026, 3, 15), "-11.99", "SPOTIFY USA 5678"),
        tx("tx-4", (2026, 3, 20), "-42.00", "Random Store"),
    ])
    .await?;

    let out = list_recurring_transactions(
        &storage,
        RecurringTransactionsOptions {
            start: Some("2026-01-01".to_string()),
            end: Some("2026-04-01".to_string()),
            include_ignored: false,
            include_possible: false,
            min_confidence: 0.70,
        },
        &test_config(),
    )
    .await?;

    assert_eq!(out.len(), 1);
    assert_eq!(out[0].normalized_name, "spotify usa");
    assert_eq!(out[0].cadence, "monthly");
    assert_eq!(out[0].status, "confirmed");
    assert_eq!(out[0].amount.typical, "-11.99");
    assert_eq!(out[0].occurrence_count, 3);
    Ok(())
}

#[tokio::test]
async fn hides_two_occurrence_candidates_unless_possible_is_included() -> Result<()> {
    let storage = storage_with_transactions(&[
        tx("tx-1", (2026, 1, 1), "-30.00", "Gym Membership"),
        tx("tx-2", (2026, 2, 1), "-30.00", "GYM MEMBERSHIP"),
    ])
    .await?;
    let config = test_config();

    let confirmed_only = list_recurring_transactions(
        &storage,
        RecurringTransactionsOptions {
            start: Some("2026-01-01".to_string()),
            end: Some("2026-03-01".to_string()),
            include_ignored: false,
            include_possible: false,
            min_confidence: 0.50,
        },
        &config,
    )
    .await?;
    assert!(confirmed_only.is_empty());

    let possible = list_recurring_transactions(
        &storage,
        RecurringTransactionsOptions {
            start: Some("2026-01-01".to_string()),
            end: Some("2026-03-01".to_string()),
            include_ignored: false,
            include_possible: true,
            min_confidence: 0.50,
        },
        &config,
    )
    .await?;
    assert_eq!(possible.len(), 1);
    assert_eq!(possible[0].status, "possible");
    Ok(())
}

#[tokio::test]
async fn stores_recurring_transaction_reviews_by_candidate_key() -> Result<()> {
    let storage = storage_with_transactions(&[
        tx("tx-1", (2026, 1, 14), "-11.99", "SPOTIFY USA 1234"),
        tx("tx-2", (2026, 2, 14), "-11.99", "Spotify.com"),
        tx("tx-3", (2026, 3, 15), "-11.99", "SPOTIFY USA 5678"),
    ])
    .await?;
    let config = test_config();
    let candidates = list_recurring_transactions(
        &storage,
        RecurringTransactionsOptions {
            start: Some("2026-01-01".to_string()),
            end: Some("2026-04-01".to_string()),
            include_ignored: false,
            include_possible: false,
            min_confidence: 0.70,
        },
        &config,
    )
    .await?;
    let candidate = candidates.first().context("expected recurring candidate")?;
    let key = recurring_transaction_candidate_key(candidate);

    set_recurring_transaction_review(
        &storage,
        &config,
        key.clone(),
        RecurringTransactionReviewStatus::Verified,
        candidate,
    )
    .await?;

    let reviews = list_recurring_transaction_reviews(&storage).await?;
    assert_eq!(reviews.len(), 1);
    assert_eq!(reviews[0].candidate_key, key);
    assert_eq!(reviews[0].status, "verified");
    assert_eq!(reviews[0].transactions.len(), 3);
    Ok(())
}
