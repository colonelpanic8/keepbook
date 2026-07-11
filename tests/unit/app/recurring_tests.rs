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
        tx("tx-4", (2026, 4, 14), "-11.99", "Spotify.com"),
        tx("tx-5", (2026, 3, 20), "-42.00", "Random Store"),
    ])
    .await?;

    let out = list_recurring_transactions(
        &storage,
        RecurringTransactionsOptions {
            start: Some("2026-01-01".to_string()),
            end: Some("2026-05-01".to_string()),
            include_ignored: false,
            include_possible: false,
            min_confidence: 0.70,
        },
        &test_config(),
    )
    .await?;

    assert_eq!(out.len(), 1);
    assert_eq!(out[0].normalized_name, "spotify");
    assert_eq!(out[0].cadence, "monthly");
    assert_eq!(out[0].status, "confirmed");
    assert_eq!(out[0].amount.typical, "-11.99");
    assert_eq!(out[0].estimated_interval_days, "30.44");
    assert_eq!(out[0].estimated_recurring_cost, "11.99");
    assert_eq!(out[0].estimated_annual_cost, "143.88");
    assert_eq!(out[0].occurrence_count, 4);

    let json = serde_json::to_value(&out[0])?;
    assert_eq!(json["estimated_interval_days"], "30.44");
    assert_eq!(json["estimated_recurring_cost"], "11.99");
    assert_eq!(json["estimated_annual_cost"], "143.88");
    Ok(())
}

#[tokio::test]
async fn rejects_two_occurrence_coincidences_even_when_possible_is_included() -> Result<()> {
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
    assert!(possible.is_empty());
    Ok(())
}

#[tokio::test]
async fn rejects_irregular_same_price_purchases() -> Result<()> {
    let storage = storage_with_transactions(&[
        tx("tx-1", (2025, 4, 28), "-9.95", "Chipotle Mexican Grill"),
        tx("tx-2", (2025, 6, 6), "-9.95", "Chipotle Mexican Grill"),
        tx("tx-3", (2025, 6, 12), "-9.95", "Chipotle Mexican Grill"),
        tx("tx-4", (2025, 7, 3), "-9.95", "Chipotle Mexican Grill"),
        tx("tx-5", (2025, 7, 30), "-9.95", "Chipotle Mexican Grill"),
    ])
    .await?;

    let out = list_recurring_transactions(
        &storage,
        RecurringTransactionsOptions {
            start: Some("2025-04-01".to_string()),
            end: Some("2025-08-01".to_string()),
            include_ignored: false,
            include_possible: true,
            min_confidence: 0.0,
        },
        &test_config(),
    )
    .await?;

    assert!(out.is_empty());
    Ok(())
}

#[tokio::test]
async fn rejects_income_and_expired_patterns() -> Result<()> {
    let storage = storage_with_transactions(&[
        tx("income-1", (2025, 1, 1), "1000", "Monthly deposit"),
        tx("income-2", (2025, 2, 1), "1000", "Monthly deposit"),
        tx("income-3", (2025, 3, 1), "1000", "Monthly deposit"),
        tx("income-4", (2025, 4, 1), "1000", "Monthly deposit"),
        tx("old-1", (2025, 1, 15), "-20", "Cancelled service"),
        tx("old-2", (2025, 2, 15), "-20", "Cancelled service"),
        tx("old-3", (2025, 3, 15), "-20", "Cancelled service"),
        tx("old-4", (2025, 4, 15), "-20", "Cancelled service"),
    ])
    .await?;

    let out = list_recurring_transactions(
        &storage,
        RecurringTransactionsOptions {
            start: Some("2025-01-01".to_string()),
            end: Some("2026-01-01".to_string()),
            include_ignored: false,
            include_possible: true,
            min_confidence: 0.0,
        },
        &test_config(),
    )
    .await?;

    assert!(out.is_empty());
    Ok(())
}

#[tokio::test]
async fn uses_only_the_latest_uninterrupted_schedule_run() -> Result<()> {
    let storage = storage_with_transactions(&[
        tx("old-1", (2025, 1, 14), "-82.99", "Streaming service"),
        tx("old-2", (2025, 2, 14), "-82.99", "Streaming service"),
        tx("new-1", (2026, 3, 15), "-82.99", "Streaming service"),
        tx("new-2", (2026, 4, 14), "-82.99", "Streaming service"),
        tx("new-3", (2026, 5, 14), "-82.99", "Streaming service"),
        tx("new-4", (2026, 6, 14), "-82.99", "Streaming service"),
    ])
    .await?;

    let out = list_recurring_transactions(
        &storage,
        RecurringTransactionsOptions {
            start: Some("2025-01-01".to_string()),
            end: Some("2026-07-01".to_string()),
            include_ignored: false,
            include_possible: false,
            min_confidence: 0.70,
        },
        &test_config(),
    )
    .await?;

    assert_eq!(out.len(), 1);
    assert_eq!(out[0].occurrence_count, 4);
    assert_eq!(out[0].first_seen, "2026-03-15");
    assert_eq!(out[0].last_seen, "2026-06-14");
    Ok(())
}

#[tokio::test]
async fn rejects_regular_but_unpredictably_priced_purchases() -> Result<()> {
    let storage = storage_with_transactions(&[
        tx("tx-1", (2026, 1, 1), "-10", "Variable bill"),
        tx("tx-2", (2026, 2, 1), "-25", "Variable bill"),
        tx("tx-3", (2026, 3, 1), "-8", "Variable bill"),
        tx("tx-4", (2026, 4, 1), "-40", "Variable bill"),
    ])
    .await?;

    let out = list_recurring_transactions(
        &storage,
        RecurringTransactionsOptions {
            start: Some("2026-01-01".to_string()),
            end: Some("2026-05-01".to_string()),
            include_ignored: false,
            include_possible: true,
            min_confidence: 0.0,
        },
        &test_config(),
    )
    .await?;

    assert!(out.is_empty());
    Ok(())
}

#[tokio::test]
async fn annualizes_yearly_costs_without_multiplying_them() -> Result<()> {
    let storage = storage_with_transactions(&[
        tx("tx-1", (2023, 5, 1), "-120", "Annual membership"),
        tx("tx-2", (2024, 5, 1), "-120", "Annual membership"),
        tx("tx-3", (2025, 5, 2), "-120", "Annual membership"),
    ])
    .await?;

    let out = list_recurring_transactions(
        &storage,
        RecurringTransactionsOptions {
            start: Some("2023-01-01".to_string()),
            end: Some("2026-04-01".to_string()),
            include_ignored: false,
            include_possible: false,
            min_confidence: 0.70,
        },
        &test_config(),
    )
    .await?;

    assert_eq!(out.len(), 1);
    assert_eq!(out[0].cadence, "yearly");
    assert_eq!(out[0].estimated_interval_days, "365.25");
    assert_eq!(out[0].estimated_recurring_cost, "120");
    assert_eq!(out[0].estimated_annual_cost, "120");
    Ok(())
}

#[tokio::test]
async fn stores_recurring_transaction_reviews_by_candidate_key() -> Result<()> {
    let storage = storage_with_transactions(&[
        tx("tx-1", (2026, 1, 14), "-11.99", "SPOTIFY USA 1234"),
        tx("tx-2", (2026, 2, 14), "-11.99", "Spotify.com"),
        tx("tx-3", (2026, 3, 15), "-11.99", "SPOTIFY USA 5678"),
        tx("tx-4", (2026, 4, 14), "-11.99", "Spotify.com"),
    ])
    .await?;
    let config = test_config();
    let candidates = list_recurring_transactions(
        &storage,
        RecurringTransactionsOptions {
            start: Some("2026-01-01".to_string()),
            end: Some("2026-05-01".to_string()),
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
    assert_eq!(reviews[0].transactions.len(), 4);
    Ok(())
}
