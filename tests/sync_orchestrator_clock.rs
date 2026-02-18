use std::sync::Arc;

use anyhow::Result;
use chrono::TimeZone;
use keepbook::clock::FixedClock;
use keepbook::market_data::{
    AssetId, MarketDataService, MarketDataStore, MemoryMarketDataStore, PriceKind, PricePoint,
};
use keepbook::models::Asset;
use keepbook::storage::{JsonFileStorage, Storage};
use keepbook::sync::{SyncOptions, SyncOrchestrator};

mod support;
use support::{mock_connection, MockSynchronizer};

#[tokio::test]
async fn sync_orchestrator_uses_injected_clock_for_snapshot_and_price_date() -> Result<()> {
    let dir = tempfile::TempDir::new()?;
    let storage = JsonFileStorage::new(dir.path());
    let storage_arc = Arc::new(storage.clone());

    let fixed_now = chrono::Utc
        .with_ymd_and_hms(2026, 2, 5, 12, 34, 56)
        .unwrap();
    let clock = Arc::new(FixedClock::new(fixed_now));

    let store = Arc::new(MemoryMarketDataStore::new());
    let fixed_date = fixed_now.date_naive();
    let asset = Asset::equity("AAPL");
    store
        .put_prices(&[PricePoint {
            asset_id: AssetId::from_asset(&asset),
            as_of_date: fixed_date,
            timestamp: fixed_now,
            price: "100".to_string(),
            quote_currency: "USD".to_string(),
            kind: PriceKind::Close,
            source: "test".to_string(),
        }])
        .await?;

    let market_data = MarketDataService::new(store, None).with_clock(clock.clone());
    let orchestrator =
        SyncOrchestrator::new(storage_arc, market_data, "USD".to_string()).with_clock(clock);

    let synchronizer = MockSynchronizer::new().with_asset(asset);
    let mut connection = mock_connection("Test");

    let options = SyncOptions::default();
    let report = orchestrator
        .sync_with_prices(&synchronizer, &mut connection, false, &options)
        .await?;

    // Snapshot timestamps should come from the orchestrator clock.
    let account_id = report.result.accounts[0].id.clone();
    let snapshots = storage.get_balance_snapshots(&account_id).await?;
    assert_eq!(snapshots.len(), 1);
    assert_eq!(snapshots[0].timestamp, fixed_now);

    // Price refresh should use the orchestrator clock's "today" date, and count cache hits as skipped.
    assert_eq!(report.refresh.fetched, 0);
    assert_eq!(report.refresh.skipped, 1);
    assert!(report.refresh.failed.is_empty());

    Ok(())
}
