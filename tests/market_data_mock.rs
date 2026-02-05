mod support;

use std::sync::Arc;

use anyhow::Result;
use chrono::{Duration, NaiveDate, Utc};
use keepbook::market_data::{AssetId, FxRateKind, MarketDataService, MarketDataStore, MemoryMarketDataStore, PriceKind};
use keepbook::models::Asset;
use support::{fx_rate_point, price_point, price_point_with_timestamp, MockMarketDataSource};

#[tokio::test]
async fn test_price_close_fetches_and_caches() -> Result<()> {
    let store = Arc::new(MemoryMarketDataStore::new());
    let asset = Asset::equity("AAPL");
    let date = NaiveDate::from_ymd_opt(2024, 1, 2).unwrap();

    let price = price_point(&asset, date, "189.50", "USD", PriceKind::Close);
    let provider = MockMarketDataSource::new().with_price(price.clone());
    let service = MarketDataService::new(store.clone(), Some(Arc::new(provider)));

    let fetched = service.price_close(&asset, date).await?;
    assert_eq!(fetched.price, "189.50");

    let cached = store
        .get_price(&AssetId::from_asset(&asset), date, PriceKind::Close)
        .await?
        .expect("price should be cached");
    assert_eq!(cached.price, "189.50");

    let service_cached = MarketDataService::new(store.clone(), None);
    let cached_fetch = service_cached.price_close(&asset, date).await?;
    assert_eq!(cached_fetch.price, "189.50");

    Ok(())
}

#[tokio::test]
async fn test_price_latest_uses_fresh_cached_quote() -> Result<()> {
    let store = Arc::new(MemoryMarketDataStore::new());
    let asset = Asset::crypto("BTC");
    let date = NaiveDate::from_ymd_opt(2024, 1, 2).unwrap();

    let cached_quote = price_point_with_timestamp(
        &asset,
        date,
        "42000.00",
        "USD",
        PriceKind::Quote,
        Utc::now() - Duration::minutes(5),
    );
    store.put_prices(std::slice::from_ref(&cached_quote)).await?;

    let provider = MockMarketDataSource::new().fail_on_fetch();
    let service = MarketDataService::new(store.clone(), Some(Arc::new(provider)))
        .with_quote_staleness(std::time::Duration::from_secs(60 * 60));

    let latest = service.price_latest(&asset, date).await?;
    assert_eq!(latest.kind, PriceKind::Quote);
    assert_eq!(latest.price, "42000.00");

    Ok(())
}

#[tokio::test]
async fn test_fx_close_fetches_and_caches() -> Result<()> {
    let store = Arc::new(MemoryMarketDataStore::new());
    let date = NaiveDate::from_ymd_opt(2024, 1, 2).unwrap();

    let rate = fx_rate_point("EUR", "USD", date, "1.10");
    let provider = MockMarketDataSource::new().with_fx_rate(rate.clone());
    let service = MarketDataService::new(store.clone(), Some(Arc::new(provider)));

    let fetched = service.fx_close("EUR", "USD", date).await?;
    assert_eq!(fetched.rate, "1.10");

    let cached = store
        .get_fx_rate("EUR", "USD", date, FxRateKind::Close)
        .await?
        .expect("fx rate should be cached");
    assert_eq!(cached.rate, "1.10");

    Ok(())
}

#[tokio::test]
async fn test_price_close_uses_lookback_from_store() -> Result<()> {
    let store = Arc::new(MemoryMarketDataStore::new());
    let asset = Asset::equity("AAPL");
    let date = NaiveDate::from_ymd_opt(2024, 1, 3).unwrap();
    let previous = NaiveDate::from_ymd_opt(2024, 1, 2).unwrap();

    let cached = price_point(&asset, previous, "187.25", "USD", PriceKind::Close);
    store.put_prices(std::slice::from_ref(&cached)).await?;

    let service = MarketDataService::new(store.clone(), None).with_lookback_days(3);
    let fetched = service.price_close(&asset, date).await?;

    assert_eq!(fetched.as_of_date, previous);
    assert_eq!(fetched.price, "187.25");

    Ok(())
}

#[tokio::test]
async fn test_price_latest_falls_back_to_close() -> Result<()> {
    let store = Arc::new(MemoryMarketDataStore::new());
    let asset = Asset::crypto("BTC");
    let date = NaiveDate::from_ymd_opt(2024, 1, 2).unwrap();

    let cached = price_point(&asset, date, "42000.00", "USD", PriceKind::Close);
    store.put_prices(std::slice::from_ref(&cached)).await?;

    let service = MarketDataService::new(store.clone(), None);
    let latest = service.price_latest(&asset, date).await?;

    assert_eq!(latest.kind, PriceKind::Close);
    assert_eq!(latest.price, "42000.00");

    Ok(())
}

#[tokio::test]
async fn test_fx_close_uses_lookback_from_store() -> Result<()> {
    let store = Arc::new(MemoryMarketDataStore::new());
    let date = NaiveDate::from_ymd_opt(2024, 1, 3).unwrap();
    let previous = NaiveDate::from_ymd_opt(2024, 1, 2).unwrap();

    let cached = fx_rate_point("EUR", "USD", previous, "1.09");
    store.put_fx_rates(std::slice::from_ref(&cached)).await?;

    let service = MarketDataService::new(store.clone(), None).with_lookback_days(5);
    let fetched = service.fx_close("EUR", "USD", date).await?;

    assert_eq!(fetched.as_of_date, previous);
    assert_eq!(fetched.rate, "1.09");

    Ok(())
}
