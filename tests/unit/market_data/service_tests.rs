use super::*;
use crate::clock::FixedClock;
use crate::market_data::{AssetId, EquityPriceRouter, EquityPriceSource};
use crate::market_data::{MemoryMarketDataStore, PriceKind, PricePoint};
use chrono::{TimeZone, Utc};
use std::sync::Arc;

struct FixedEquityQuoteSource {
    point: PricePoint,
}

#[async_trait::async_trait]
impl EquityPriceSource for FixedEquityQuoteSource {
    async fn fetch_close(
        &self,
        _asset: &Asset,
        _asset_id: &AssetId,
        _date: NaiveDate,
    ) -> Result<Option<PricePoint>> {
        Ok(None)
    }

    async fn fetch_quote(&self, _asset: &Asset, _asset_id: &AssetId) -> Result<Option<PricePoint>> {
        Ok(Some(self.point.clone()))
    }

    fn name(&self) -> &str {
        "fixed"
    }
}

fn make_quote(
    asset_id: &AssetId,
    as_of_date: NaiveDate,
    ts: chrono::DateTime<Utc>,
    px: &str,
) -> PricePoint {
    PricePoint {
        asset_id: asset_id.clone(),
        as_of_date,
        timestamp: ts,
        kind: PriceKind::Quote,
        price: px.to_string(),
        quote_currency: "USD".to_string(),
        source: "fixed".to_string(),
    }
}

fn make_close(
    asset_id: &AssetId,
    as_of_date: NaiveDate,
    ts: chrono::DateTime<Utc>,
    px: &str,
) -> PricePoint {
    PricePoint {
        asset_id: asset_id.clone(),
        as_of_date,
        timestamp: ts,
        kind: PriceKind::Close,
        price: px.to_string(),
        quote_currency: "USD".to_string(),
        source: "fixed".to_string(),
    }
}

fn make_fx_close(
    base: &str,
    quote: &str,
    as_of_date: NaiveDate,
    ts: chrono::DateTime<Utc>,
    rate: &str,
) -> FxRatePoint {
    FxRatePoint {
        base: base.to_string(),
        quote: quote.to_string(),
        as_of_date,
        timestamp: ts,
        rate: rate.to_string(),
        kind: FxRateKind::Close,
        source: "fixed".to_string(),
    }
}

#[tokio::test]
async fn price_latest_with_status_uses_fresh_cached_quote() -> Result<()> {
    let now = Utc.with_ymd_and_hms(2026, 2, 6, 12, 0, 0).unwrap();
    let clock = Arc::new(FixedClock::new(now));

    let store = Arc::new(MemoryMarketDataStore::default());
    let mut svc = MarketDataService::new(store.clone(), None)
        .with_quote_staleness(std::time::Duration::from_secs(3600))
        .with_clock(clock);

    let asset = Asset::Equity {
        ticker: "AAPL".to_string(),
        exchange: Some("NASDAQ".to_string()),
    };
    let asset_id = AssetId::from_asset(&asset.normalized());
    let today = now.date_naive();

    let cached = make_quote(&asset_id, today, now - chrono::Duration::minutes(30), "100");
    store.put_prices(std::slice::from_ref(&cached)).await?;

    // Router exists but should not be used due to fresh cache.
    let src_quote = make_quote(&asset_id, today, now, "200");
    let router = Arc::new(EquityPriceRouter::new(vec![Arc::new(
        FixedEquityQuoteSource { point: src_quote },
    )]));
    svc = svc.with_equity_router(router);

    let (p, fetched) = svc.price_latest_with_status(&asset, today).await?;
    assert!(!fetched);
    assert_eq!(p.price, "100");
    Ok(())
}

#[tokio::test]
async fn price_latest_with_status_fetches_when_cached_quote_is_stale() -> Result<()> {
    let now = Utc.with_ymd_and_hms(2026, 2, 6, 12, 0, 0).unwrap();
    let clock = Arc::new(FixedClock::new(now));

    let store = Arc::new(MemoryMarketDataStore::default());
    let mut svc = MarketDataService::new(store.clone(), None)
        .with_quote_staleness(std::time::Duration::from_secs(3600))
        .with_clock(clock);

    let asset = Asset::Equity {
        ticker: "AAPL".to_string(),
        exchange: Some("NASDAQ".to_string()),
    };
    let asset_id = AssetId::from_asset(&asset.normalized());
    let today = now.date_naive();

    let cached = make_quote(&asset_id, today, now - chrono::Duration::hours(2), "100");
    store.put_prices(std::slice::from_ref(&cached)).await?;

    let src_quote = make_quote(&asset_id, today, now, "200");
    let router = Arc::new(EquityPriceRouter::new(vec![Arc::new(
        FixedEquityQuoteSource {
            point: src_quote.clone(),
        },
    )]));
    svc = svc.with_equity_router(router);

    let (p, fetched) = svc.price_latest_with_status(&asset, today).await?;
    assert!(fetched);
    assert_eq!(p.price, "200");
    Ok(())
}

#[tokio::test]
async fn price_latest_with_status_uses_stale_quote_before_older_close_when_fetch_unavailable(
) -> Result<()> {
    let now = Utc.with_ymd_and_hms(2026, 2, 7, 12, 0, 0).unwrap();
    let clock = Arc::new(FixedClock::new(now));

    let store = Arc::new(MemoryMarketDataStore::default());
    let svc = MarketDataService::new(store.clone(), None)
        .with_quote_staleness(std::time::Duration::from_secs(3600))
        .with_clock(clock);

    let asset = Asset::equity("AAPL");
    let asset_id = AssetId::from_asset(&asset);
    let today = now.date_naive();

    let older_close = make_close(
        &asset_id,
        today - chrono::Duration::days(4),
        now - chrono::Duration::days(4),
        "100",
    );
    let stale_quote = make_quote(&asset_id, today, now - chrono::Duration::hours(2), "120");
    store.put_prices(&[older_close, stale_quote]).await?;

    let (p, fetched) = svc.price_latest_with_status(&asset, today).await?;
    assert!(!fetched);
    assert_eq!(p.kind, PriceKind::Quote);
    assert_eq!(p.as_of_date, today);
    assert_eq!(p.price, "120");
    Ok(())
}

#[tokio::test]
async fn price_latest_force_ignores_fresh_cached_quote() -> Result<()> {
    let now = Utc.with_ymd_and_hms(2026, 2, 6, 12, 0, 0).unwrap();
    let clock = Arc::new(FixedClock::new(now));

    let store = Arc::new(MemoryMarketDataStore::default());
    let mut svc = MarketDataService::new(store.clone(), None)
        .with_quote_staleness(std::time::Duration::from_secs(3600))
        .with_clock(clock);

    let asset = Asset::Equity {
        ticker: "AAPL".to_string(),
        exchange: Some("NASDAQ".to_string()),
    };
    let asset_id = AssetId::from_asset(&asset.normalized());
    let today = now.date_naive();

    let cached = make_quote(&asset_id, today, now - chrono::Duration::minutes(5), "100");
    store.put_prices(std::slice::from_ref(&cached)).await?;

    let src_quote = make_quote(&asset_id, today, now, "200");
    let router = Arc::new(EquityPriceRouter::new(vec![Arc::new(
        FixedEquityQuoteSource {
            point: src_quote.clone(),
        },
    )]));
    svc = svc.with_equity_router(router);

    let (p, fetched) = svc.price_latest_force(&asset, today).await?;
    assert!(fetched);
    assert_eq!(p.price, "200");
    Ok(())
}

#[tokio::test]
async fn price_from_store_is_unbounded_by_default() -> Result<()> {
    let store = Arc::new(MemoryMarketDataStore::default());
    let svc = MarketDataService::new(store.clone(), None);
    let asset = Asset::equity("AAPL");
    let asset_id = AssetId::from_asset(&asset);
    let query_date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();

    store
        .put_prices(std::slice::from_ref(&make_close(
            &asset_id,
            NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
            Utc.with_ymd_and_hms(2024, 1, 1, 23, 59, 59).unwrap(),
            "100",
        )))
        .await?;

    let found = svc.price_from_store(&asset, query_date).await?;
    assert!(found.is_some());
    assert_eq!(
        found.unwrap().as_of_date,
        NaiveDate::from_ymd_opt(2024, 1, 1).unwrap()
    );
    Ok(())
}

#[tokio::test]
async fn with_lookback_days_bounds_store_lookup() -> Result<()> {
    let store = Arc::new(MemoryMarketDataStore::default());
    let svc = MarketDataService::new(store.clone(), None).with_lookback_days(7);
    let asset = Asset::equity("AAPL");
    let asset_id = AssetId::from_asset(&asset);
    let query_date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();

    store
        .put_prices(std::slice::from_ref(&make_close(
            &asset_id,
            NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
            Utc.with_ymd_and_hms(2024, 1, 1, 23, 59, 59).unwrap(),
            "100",
        )))
        .await?;

    let found = svc.price_from_store(&asset, query_date).await?;
    assert!(found.is_none());
    Ok(())
}

#[tokio::test]
async fn future_projection_uses_earliest_later_price_when_bounded_lookback_misses() -> Result<()> {
    let store = Arc::new(MemoryMarketDataStore::default());
    let svc = MarketDataService::new(store.clone(), None)
        .with_lookback_days(7)
        .with_future_projection(true);
    let asset = Asset::equity("AAPL");
    let asset_id = AssetId::from_asset(&asset);
    let query_date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();

    store
        .put_prices(&[
            make_close(
                &asset_id,
                NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
                Utc.with_ymd_and_hms(2024, 1, 1, 23, 59, 59).unwrap(),
                "100",
            ),
            make_close(
                &asset_id,
                NaiveDate::from_ymd_opt(2024, 1, 20).unwrap(),
                Utc.with_ymd_and_hms(2024, 1, 20, 23, 59, 59).unwrap(),
                "110",
            ),
        ])
        .await?;

    let found = svc.price_from_store(&asset, query_date).await?;
    assert!(found.is_some());
    assert_eq!(
        found.unwrap().as_of_date,
        NaiveDate::from_ymd_opt(2024, 1, 20).unwrap()
    );
    Ok(())
}

#[tokio::test]
async fn valuation_price_from_store_prefers_same_day_quote_over_older_close() -> Result<()> {
    let store = Arc::new(MemoryMarketDataStore::default());
    let svc = MarketDataService::new(store.clone(), None);
    let asset = Asset::equity("AAPL");
    let asset_id = AssetId::from_asset(&asset);
    let query_date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();

    store
        .put_prices(&[
            make_close(
                &asset_id,
                NaiveDate::from_ymd_opt(2024, 1, 14).unwrap(),
                Utc.with_ymd_and_hms(2024, 1, 14, 23, 59, 59).unwrap(),
                "100",
            ),
            make_quote(
                &asset_id,
                query_date,
                Utc.with_ymd_and_hms(2024, 1, 15, 12, 0, 0).unwrap(),
                "110",
            ),
        ])
        .await?;

    let found = svc.valuation_price_from_store(&asset, query_date).await?;
    assert!(found.is_some());
    let found = found.unwrap();
    assert_eq!(found.kind, PriceKind::Quote);
    assert_eq!(found.as_of_date, query_date);
    assert_eq!(found.price, "110");
    Ok(())
}

#[tokio::test]
async fn fx_from_store_is_unbounded_by_default() -> Result<()> {
    let store = Arc::new(MemoryMarketDataStore::default());
    let svc = MarketDataService::new(store.clone(), None);
    let query_date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();

    store
        .put_fx_rates(std::slice::from_ref(&make_fx_close(
            "USD",
            "EUR",
            NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
            Utc.with_ymd_and_hms(2024, 1, 1, 23, 59, 59).unwrap(),
            "0.91",
        )))
        .await?;

    let found = svc.fx_from_store("USD", "EUR", query_date).await?;
    assert!(found.is_some());
    assert_eq!(
        found.unwrap().as_of_date,
        NaiveDate::from_ymd_opt(2024, 1, 1).unwrap()
    );
    Ok(())
}

#[tokio::test]
async fn future_projection_uses_earliest_later_fx_rate_when_bounded_lookback_misses() -> Result<()>
{
    let store = Arc::new(MemoryMarketDataStore::default());
    let svc = MarketDataService::new(store.clone(), None)
        .with_lookback_days(7)
        .with_future_projection(true);
    let query_date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();

    store
        .put_fx_rates(&[
            make_fx_close(
                "USD",
                "EUR",
                NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
                Utc.with_ymd_and_hms(2024, 1, 1, 23, 59, 59).unwrap(),
                "0.91",
            ),
            make_fx_close(
                "USD",
                "EUR",
                NaiveDate::from_ymd_opt(2024, 1, 20).unwrap(),
                Utc.with_ymd_and_hms(2024, 1, 20, 23, 59, 59).unwrap(),
                "0.93",
            ),
        ])
        .await?;

    let found = svc.fx_from_store("USD", "EUR", query_date).await?;
    assert!(found.is_some());
    assert_eq!(
        found.unwrap().as_of_date,
        NaiveDate::from_ymd_opt(2024, 1, 20).unwrap()
    );
    Ok(())
}
