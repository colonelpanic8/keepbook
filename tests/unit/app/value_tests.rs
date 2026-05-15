use super::*;
use std::sync::Arc;

use chrono::{TimeZone, Utc};

use crate::market_data::{
    AssetId, MarketDataService, MarketDataStore, MemoryMarketDataStore, PriceKind, PricePoint,
};

fn quote_point(asset: &Asset, date: NaiveDate, price: &str) -> PricePoint {
    PricePoint {
        asset_id: AssetId::from_asset(asset),
        as_of_date: date,
        timestamp: Utc.with_ymd_and_hms(2026, 2, 19, 20, 0, 0).unwrap(),
        price: price.to_string(),
        quote_currency: "USD".to_string(),
        kind: PriceKind::Quote,
        source: "test".to_string(),
    }
}

fn close_point(asset: &Asset, date: NaiveDate, price: &str) -> PricePoint {
    PricePoint {
        asset_id: AssetId::from_asset(asset),
        as_of_date: date,
        timestamp: Utc.with_ymd_and_hms(2026, 2, 19, 21, 0, 0).unwrap(),
        price: price.to_string(),
        quote_currency: "USD".to_string(),
        kind: PriceKind::Close,
        source: "test".to_string(),
    }
}

#[tokio::test]
async fn valuation_uses_cached_quote() -> Result<()> {
    let store = Arc::new(MemoryMarketDataStore::new());
    let market_data = MarketDataService::new(store.clone(), None);
    let asset = Asset::equity("FXAIX");
    let date = NaiveDate::from_ymd_opt(2026, 2, 19).unwrap();
    store
        .put_prices(&[quote_point(&asset, date, "239.32")])
        .await?;

    let strict = value_in_reporting_currency_detailed(&market_data, &asset, "2", "USD", date, None)
        .await?
        .value;
    assert_eq!(strict.as_deref(), Some("478.64"));

    let best_effort =
        value_in_reporting_currency_best_effort(&market_data, &asset, "2", "USD", date, None)
            .await?;
    assert_eq!(best_effort.as_deref(), Some("478.64"));
    Ok(())
}

#[tokio::test]
async fn valuation_uses_latest_same_day_price_timestamp() -> Result<()> {
    let store = Arc::new(MemoryMarketDataStore::new());
    let market_data = MarketDataService::new(store.clone(), None);
    let asset = Asset::equity("FXAIX");
    let date = NaiveDate::from_ymd_opt(2026, 2, 19).unwrap();
    store
        .put_prices(&[
            quote_point(&asset, date, "237.99"),
            close_point(&asset, date, "239.32"),
        ])
        .await?;

    let best_effort =
        value_in_reporting_currency_best_effort(&market_data, &asset, "1", "USD", date, None)
            .await?;
    assert_eq!(best_effort.as_deref(), Some("239.32"));
    Ok(())
}
