mod support;

use std::path::Path;

use anyhow::Result;
use chrono::{NaiveDate, TimeZone, Utc};
use keepbook::app::{fetch_historical_prices, PriceHistoryRequest};
use keepbook::config::{GitConfig, RefreshConfig, ResolvedConfig};
use keepbook::market_data::{JsonlMarketDataStore, MarketDataStore, PriceKind};
use keepbook::models::{Account, Asset, AssetBalance, BalanceSnapshot, Id};
use keepbook::storage::{JsonFileStorage, Storage};
use support::{fx_rate_point, price_point};
use tempfile::TempDir;

fn resolved_config(data_dir: &Path) -> ResolvedConfig {
    ResolvedConfig {
        data_dir: data_dir.to_path_buf(),
        reporting_currency: "USD".to_string(),
        refresh: RefreshConfig::default(),
        git: GitConfig::default(),
    }
}

async fn create_account(storage: &JsonFileStorage, name: &str) -> Result<Account> {
    let connection_id = Id::new();
    let account = Account::new(name, connection_id);
    storage.save_account(&account).await?;
    Ok(account)
}

async fn add_balance(
    storage: &JsonFileStorage,
    account_id: &Id,
    date: NaiveDate,
    asset: Asset,
) -> Result<()> {
    let timestamp = Utc.from_utc_datetime(&date.and_hms_opt(0, 0, 0).unwrap());
    let snapshot = BalanceSnapshot::new(
        timestamp,
        vec![AssetBalance::new(asset, "100.00")],
    );
    storage.append_balance_snapshot(account_id, &snapshot).await?;
    Ok(())
}

#[tokio::test]
async fn price_history_errors_when_no_balances() -> Result<()> {
    let dir = TempDir::new()?;
    let storage = JsonFileStorage::new(dir.path());
    let config = resolved_config(dir.path());

    let account = create_account(&storage, "Checking").await?;
    let account_id = account.id.to_string();

    let err = fetch_historical_prices(PriceHistoryRequest {
        storage: &storage,
        config: &config,
        account: Some(account_id.as_str()),
        connection: None,
        start: Some("2024-01-01"),
        end: Some("2024-01-02"),
        interval: "daily",
        lookback_days: 0,
        request_delay_ms: 0,
        currency: None,
        include_fx: false,
    })
    .await
    .err()
    .expect("expected error when no balances exist");

    assert!(err.to_string().contains("No balances found"));

    Ok(())
}

#[tokio::test]
async fn price_history_errors_when_start_after_end() -> Result<()> {
    let dir = TempDir::new()?;
    let storage = JsonFileStorage::new(dir.path());
    let config = resolved_config(dir.path());

    let account = create_account(&storage, "Checking").await?;
    add_balance(
        &storage,
        &account.id,
        NaiveDate::from_ymd_opt(2024, 1, 2).unwrap(),
        Asset::currency("USD"),
    )
    .await?;

    let account_id = account.id.to_string();
    let err = fetch_historical_prices(PriceHistoryRequest {
        storage: &storage,
        config: &config,
        account: Some(account_id.as_str()),
        connection: None,
        start: Some("2024-02-01"),
        end: Some("2024-01-01"),
        interval: "daily",
        lookback_days: 0,
        request_delay_ms: 0,
        currency: None,
        include_fx: false,
    })
    .await
    .err()
    .expect("expected start > end error");

    assert!(err
        .to_string()
        .contains("Start date must be on or before end date"));

    Ok(())
}

#[tokio::test]
async fn price_history_errors_on_invalid_interval() -> Result<()> {
    let dir = TempDir::new()?;
    let storage = JsonFileStorage::new(dir.path());
    let config = resolved_config(dir.path());

    let account = create_account(&storage, "Checking").await?;
    add_balance(
        &storage,
        &account.id,
        NaiveDate::from_ymd_opt(2024, 1, 2).unwrap(),
        Asset::currency("USD"),
    )
    .await?;

    let account_id = account.id.to_string();
    let err = fetch_historical_prices(PriceHistoryRequest {
        storage: &storage,
        config: &config,
        account: Some(account_id.as_str()),
        connection: None,
        start: Some("2024-01-02"),
        end: Some("2024-01-02"),
        interval: "hourly",
        lookback_days: 0,
        request_delay_ms: 0,
        currency: None,
        include_fx: false,
    })
    .await
    .err()
    .expect("expected invalid interval error");

    assert!(err.to_string().contains("Invalid interval"));

    Ok(())
}

#[tokio::test]
async fn price_history_infers_start_date_from_balances() -> Result<()> {
    let dir = TempDir::new()?;
    let storage = JsonFileStorage::new(dir.path());
    let config = resolved_config(dir.path());

    let account = create_account(&storage, "Checking").await?;
    add_balance(
        &storage,
        &account.id,
        NaiveDate::from_ymd_opt(2024, 1, 5).unwrap(),
        Asset::currency("USD"),
    )
    .await?;
    add_balance(
        &storage,
        &account.id,
        NaiveDate::from_ymd_opt(2024, 1, 2).unwrap(),
        Asset::currency("USD"),
    )
    .await?;

    let account_id = account.id.to_string();
    let output = fetch_historical_prices(PriceHistoryRequest {
        storage: &storage,
        config: &config,
        account: Some(account_id.as_str()),
        connection: None,
        start: None,
        end: Some("2024-01-02"),
        interval: "daily",
        lookback_days: 0,
        request_delay_ms: 0,
        currency: None,
        include_fx: false,
    })
    .await?;

    assert_eq!(output.start_date, "2024-01-02");
    assert_eq!(output.earliest_balance_date.as_deref(), Some("2024-01-02"));
    assert_eq!(output.days, 1);
    assert_eq!(output.points, 1);

    Ok(())
}

#[tokio::test]
async fn price_history_uses_cached_price_with_lookback() -> Result<()> {
    let dir = TempDir::new()?;
    let storage = JsonFileStorage::new(dir.path());
    let config = resolved_config(dir.path());

    let account = create_account(&storage, "Brokerage").await?;
    add_balance(
        &storage,
        &account.id,
        NaiveDate::from_ymd_opt(2024, 1, 3).unwrap(),
        Asset::equity("AAPL"),
    )
    .await?;

    let store = JsonlMarketDataStore::new(dir.path());
    let cached = price_point(
        &Asset::equity("AAPL"),
        NaiveDate::from_ymd_opt(2024, 1, 2).unwrap(),
        "188.00",
        "USD",
        PriceKind::Close,
    );
    store.put_prices(std::slice::from_ref(&cached)).await?;

    let account_id = account.id.to_string();
    let output = fetch_historical_prices(PriceHistoryRequest {
        storage: &storage,
        config: &config,
        account: Some(account_id.as_str()),
        connection: None,
        start: Some("2024-01-03"),
        end: Some("2024-01-03"),
        interval: "daily",
        lookback_days: 2,
        request_delay_ms: 0,
        currency: None,
        include_fx: false,
    })
    .await?;

    assert_eq!(output.points, 1);
    assert_eq!(output.prices.attempted, 1);
    assert_eq!(output.prices.lookback, 1);
    assert_eq!(output.prices.missing, 0);

    Ok(())
}

#[tokio::test]
async fn price_history_records_missing_prices() -> Result<()> {
    let dir = TempDir::new()?;
    let storage = JsonFileStorage::new(dir.path());
    let config = resolved_config(dir.path());

    let account = create_account(&storage, "Brokerage").await?;
    add_balance(
        &storage,
        &account.id,
        NaiveDate::from_ymd_opt(2024, 1, 2).unwrap(),
        Asset::equity("AAPL"),
    )
    .await?;

    let account_id = account.id.to_string();
    let output = fetch_historical_prices(PriceHistoryRequest {
        storage: &storage,
        config: &config,
        account: Some(account_id.as_str()),
        connection: None,
        start: Some("2024-01-02"),
        end: Some("2024-01-02"),
        interval: "daily",
        lookback_days: 0,
        request_delay_ms: 0,
        currency: None,
        include_fx: false,
    })
    .await?;

    assert_eq!(output.points, 1);
    assert_eq!(output.prices.attempted, 1);
    assert_eq!(output.prices.missing, 1);
    assert_eq!(output.failure_count, 1);
    assert_eq!(output.failures.len(), 1);
    assert_eq!(output.failures[0].kind, "price");

    Ok(())
}

#[tokio::test]
async fn price_history_uses_cached_fx_rates() -> Result<()> {
    let dir = TempDir::new()?;
    let storage = JsonFileStorage::new(dir.path());
    let config = resolved_config(dir.path());

    let account = create_account(&storage, "Checking").await?;
    add_balance(
        &storage,
        &account.id,
        NaiveDate::from_ymd_opt(2024, 1, 2).unwrap(),
        Asset::currency("EUR"),
    )
    .await?;

    let store = JsonlMarketDataStore::new(dir.path());
    let cached = fx_rate_point("EUR", "USD", NaiveDate::from_ymd_opt(2024, 1, 2).unwrap(), "1.10");
    store.put_fx_rates(std::slice::from_ref(&cached)).await?;

    let account_id = account.id.to_string();
    let output = fetch_historical_prices(PriceHistoryRequest {
        storage: &storage,
        config: &config,
        account: Some(account_id.as_str()),
        connection: None,
        start: Some("2024-01-02"),
        end: Some("2024-01-02"),
        interval: "daily",
        lookback_days: 0,
        request_delay_ms: 0,
        currency: None,
        include_fx: true,
    })
    .await?;

    let fx = output.fx.expect("fx stats should be present");
    assert_eq!(fx.attempted, 1);
    assert_eq!(fx.existing, 1);
    assert_eq!(fx.lookback, 0);
    assert_eq!(fx.missing, 0);
    assert_eq!(output.failure_count, 0);

    Ok(())
}
