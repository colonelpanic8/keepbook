use std::collections::HashSet;
use std::sync::Arc;

use anyhow::Result;
use chrono::TimeZone;
use keepbook::clock::FixedClock;
use keepbook::market_data::{
    AssetId, FxRateKind, FxRatePoint, MarketDataService, MarketDataStore, MemoryMarketDataStore,
    PriceKind, PricePoint,
};
use keepbook::models::Asset;
use keepbook::storage::JsonFileStorage;
use keepbook::sync::SyncOrchestrator;

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

