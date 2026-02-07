use std::collections::HashSet;
use std::sync::Arc;

use anyhow::{Context, Result};
use chrono::TimeZone;
use keepbook::clock::FixedClock;
use keepbook::market_data::{
    AssetId, FxRateKind, FxRatePoint, MarketDataService, MarketDataStore, MemoryMarketDataStore,
    PriceKind, PricePoint,
};
use keepbook::models::Asset;
use keepbook::storage::JsonFileStorage;
use keepbook::storage::MemoryStorage;
use keepbook::sync::SyncOrchestrator;

use crate::support::MockMarketDataSource;

mod support;

#[tokio::test]
async fn ensure_prices_counts_cached_prices_as_skipped() -> Result<()> {
    let dir = tempfile::TempDir::new()?;
    let storage = Arc::new(JsonFileStorage::new(dir.path()));

    let now = chrono::Utc.with_ymd_and_hms(2026, 2, 5, 12, 0, 0).unwrap();
    let clock = Arc::new(FixedClock::new(now));
    let date = now.date_naive();

    let store = Arc::new(MemoryMarketDataStore::new());
    let asset = Asset::equity("AAPL");
    store
        .put_prices(&[PricePoint {
            asset_id: AssetId::from_asset(&asset),
            as_of_date: date,
            timestamp: now,
            price: "100".to_string(),
            quote_currency: "USD".to_string(),
            kind: PriceKind::Close,
            source: "test".to_string(),
        }])
        .await?;

    let market_data = MarketDataService::new(store, None).with_clock(clock.clone());
    let orchestrator =
        SyncOrchestrator::new(storage, market_data, "USD".to_string()).with_clock(clock);

    let mut assets = HashSet::new();
    assets.insert(asset);

    let refresh = orchestrator.ensure_prices(&assets, date, false).await?;
    assert_eq!(refresh.fetched, 0, "no network fetch should occur");
    assert_eq!(refresh.skipped, 1, "cached close price should be skipped");
    assert!(refresh.failed.is_empty());

    Ok(())
}

#[tokio::test]
async fn ensure_prices_force_refresh_counts_as_fetched_when_sources_return_data() -> Result<()> {
    let storage = Arc::new(MemoryStorage::new());
    let store = Arc::new(MemoryMarketDataStore::new());

    let asset = Asset::equity("AAPL");
    let asset_id = AssetId::from_asset(&asset);
    let date = chrono::NaiveDate::from_ymd_opt(2024, 1, 2).unwrap();

    // Seed cache with an older point.
    let cached = PricePoint {
        asset_id: asset_id.clone(),
        as_of_date: date,
        timestamp: chrono::Utc.with_ymd_and_hms(2024, 1, 2, 0, 0, 0).unwrap(),
        price: "100".to_string(),
        quote_currency: "USD".to_string(),
        kind: PriceKind::Close,
        source: "cache".to_string(),
    };
    store.put_prices(std::slice::from_ref(&cached)).await?;

    let refreshed = PricePoint {
        asset_id: asset_id.clone(),
        as_of_date: date,
        timestamp: chrono::Utc.with_ymd_and_hms(2024, 1, 2, 1, 0, 0).unwrap(),
        price: "101".to_string(),
        quote_currency: "USD".to_string(),
        kind: PriceKind::Close,
        source: "mock".to_string(),
    };
    let provider = MockMarketDataSource::new().with_price(refreshed.clone());
    let market_data = MarketDataService::new(store.clone(), Some(Arc::new(provider)));

    let orchestrator = SyncOrchestrator::new(storage, market_data, "USD".to_string());
    let mut assets = HashSet::new();
    assets.insert(asset);

    let result = orchestrator.ensure_prices(&assets, date, true).await?;
    assert_eq!(result.fetched, 1);
    assert_eq!(result.skipped, 0);

    let loaded = store
        .get_price(&asset_id, date, PriceKind::Close)
        .await?
        .context("expected stored price")?;
    assert_eq!(loaded.price, "101");
    assert_eq!(loaded.source, "mock");

    Ok(())
}

#[tokio::test]
async fn ensure_prices_counts_cached_fx_as_skipped() -> Result<()> {
    let dir = tempfile::TempDir::new()?;
    let storage = Arc::new(JsonFileStorage::new(dir.path()));

    let now = chrono::Utc.with_ymd_and_hms(2026, 2, 5, 12, 0, 0).unwrap();
    let clock = Arc::new(FixedClock::new(now));
    let date = now.date_naive();

    let store = Arc::new(MemoryMarketDataStore::new());
    store
        .put_fx_rates(&[FxRatePoint {
            base: "EUR".to_string(),
            quote: "USD".to_string(),
            as_of_date: date,
            timestamp: now,
            rate: "1.1".to_string(),
            kind: FxRateKind::Close,
            source: "test".to_string(),
        }])
        .await?;

    let market_data = MarketDataService::new(store, None).with_clock(clock.clone());
    let orchestrator =
        SyncOrchestrator::new(storage, market_data, "USD".to_string()).with_clock(clock);

    let mut assets = HashSet::new();
    assets.insert(Asset::currency("EUR"));

    let refresh = orchestrator.ensure_prices(&assets, date, false).await?;
    assert_eq!(refresh.fetched, 0);
    assert_eq!(refresh.skipped, 1, "cached FX rate should be skipped");
    assert!(refresh.failed.is_empty());

    Ok(())
}
