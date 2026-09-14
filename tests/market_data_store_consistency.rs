use std::sync::Arc;

use anyhow::Result;
use chrono::{TimeZone, Utc};
use keepbook::market_data::{
    AssetId, FxRateKind, FxRatePoint, JsonlMarketDataStore, MarketDataService, MarketDataStore,
    MemoryMarketDataStore, PriceKind, PricePoint,
};
use keepbook::models::Asset;
use tempfile::TempDir;

async fn check_out_of_order_observations(store: &dyn MarketDataStore) -> Result<()> {
    let timestamp = Utc.with_ymd_and_hms(2026, 2, 6, 18, 0, 0).unwrap();
    let newer = PricePoint {
        asset_id: AssetId::from_asset(&Asset::equity("AAPL")),
        as_of_date: timestamp.date_naive(),
        timestamp,
        price: "200".into(),
        quote_currency: "USD".into(),
        kind: PriceKind::Close,
        source: "test".into(),
    };
    let older = PricePoint {
        timestamp: timestamp - chrono::Duration::hours(1),
        price: "100".into(),
        ..newer.clone()
    };
    store.put_prices(&[newer.clone(), older.clone()]).await?;
    store.put_prices(&[older]).await?;
    let price = store
        .get_price(&newer.asset_id, newer.as_of_date, newer.kind)
        .await?
        .unwrap();
    assert_eq!(price.price, "200");
    assert_eq!(
        store
            .get_all_prices(&newer.asset_id)
            .await?
            .iter()
            .max_by_key(|point| point.timestamp)
            .unwrap()
            .price,
        "200"
    );

    let newer = FxRatePoint {
        base: "USD".into(),
        quote: "EUR".into(),
        as_of_date: timestamp.date_naive(),
        timestamp,
        rate: "0.9".into(),
        kind: FxRateKind::Close,
        source: "test".into(),
    };
    let older = FxRatePoint {
        timestamp: timestamp - chrono::Duration::hours(1),
        rate: "0.8".into(),
        ..newer.clone()
    };
    store.put_fx_rates(&[newer.clone(), older.clone()]).await?;
    store.put_fx_rates(&[older]).await?;
    let rate = store
        .get_fx_rate("USD", "EUR", newer.as_of_date, newer.kind)
        .await?
        .unwrap();
    assert_eq!(rate.rate, "0.9");
    Ok(())
}

#[tokio::test]
async fn memory_store_preserves_newer_observations() -> Result<()> {
    check_out_of_order_observations(&MemoryMarketDataStore::new()).await
}

#[tokio::test]
async fn jsonl_store_preserves_newer_observations() -> Result<()> {
    let dir = TempDir::new()?;
    check_out_of_order_observations(&JsonlMarketDataStore::new(dir.path())).await
}

#[tokio::test]
async fn fx_future_projection_uses_latest_observation_on_earliest_date() -> Result<()> {
    let dir = TempDir::new()?;
    let store = Arc::new(JsonlMarketDataStore::new(dir.path()));
    let timestamp = Utc.with_ymd_and_hms(2026, 2, 6, 18, 0, 0).unwrap();
    let latest = FxRatePoint {
        base: "USD".into(),
        quote: "EUR".into(),
        as_of_date: timestamp.date_naive(),
        timestamp,
        rate: "0.9".into(),
        kind: FxRateKind::Close,
        source: "test".into(),
    };
    store
        .put_fx_rates(&[
            latest.clone(),
            FxRatePoint {
                timestamp: timestamp - chrono::Duration::hours(1),
                rate: "0.8".into(),
                ..latest.clone()
            },
            FxRatePoint {
                as_of_date: latest.as_of_date.succ_opt().unwrap(),
                timestamp: timestamp + chrono::Duration::days(1),
                rate: "0.7".into(),
                ..latest.clone()
            },
        ])
        .await?;
    for bounded in [false, true] {
        let mut service = MarketDataService::new(store.clone(), None).with_future_projection(true);
        if bounded {
            service = service.with_lookback_days(1);
        }
        let rate = service
            .fx_from_store("USD", "EUR", latest.as_of_date.pred_opt().unwrap())
            .await?
            .unwrap();
        assert_eq!(rate.timestamp, latest.timestamp);
        assert_eq!(rate.rate, "0.9");
    }
    Ok(())
}
