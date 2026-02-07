use std::sync::Arc;

use anyhow::{Context, Result};
use chrono::{Duration, NaiveDate};
use tracing::{debug, info};

use crate::clock::{Clock, SystemClock};

use super::{
    AssetId, CryptoPriceRouter, EquityPriceRouter, FxRateKind, FxRatePoint, FxRateRouter,
    MarketDataSource, MarketDataStore, PriceKind, PricePoint,
};
use crate::models::Asset;

pub struct MarketDataService {
    store: Arc<dyn MarketDataStore>,
    provider: Option<Arc<dyn MarketDataSource>>,
    equity_router: Option<Arc<EquityPriceRouter>>,
    crypto_router: Option<Arc<CryptoPriceRouter>>,
    fx_router: Option<Arc<FxRateRouter>>,
    lookback_days: u32,
    /// How old a quote can be before we fetch a new one. None means always fetch.
    quote_staleness: Option<std::time::Duration>,
    clock: Arc<dyn Clock>,
}

impl MarketDataService {
    pub fn new(
        store: Arc<dyn MarketDataStore>,
        provider: Option<Arc<dyn MarketDataSource>>,
    ) -> Self {
        Self {
            store,
            provider,
            equity_router: None,
            crypto_router: None,
            fx_router: None,
            lookback_days: 7,
            quote_staleness: None,
            clock: Arc::new(SystemClock),
        }
    }

    pub fn with_equity_router(mut self, router: Arc<EquityPriceRouter>) -> Self {
        self.equity_router = Some(router);
        self
    }

    pub fn with_crypto_router(mut self, router: Arc<CryptoPriceRouter>) -> Self {
        self.crypto_router = Some(router);
        self
    }

    pub fn with_fx_router(mut self, router: Arc<FxRateRouter>) -> Self {
        self.fx_router = Some(router);
        self
    }

    pub fn with_lookback_days(mut self, days: u32) -> Self {
        self.lookback_days = days;
        self
    }

    pub fn with_quote_staleness(mut self, staleness: std::time::Duration) -> Self {
        self.quote_staleness = Some(staleness);
        self
    }

    pub fn with_clock(mut self, clock: Arc<dyn Clock>) -> Self {
        self.clock = clock;
        self
    }

    /// Get price from store only, no external fetching.
    /// Returns the most recent price by timestamp for the given date (with lookback).
    pub async fn price_from_store(
        &self,
        asset: &Asset,
        date: NaiveDate,
    ) -> Result<Option<PricePoint>> {
        let asset = asset.normalized();
        let asset_id = AssetId::from_asset(&asset);
        debug!(asset_id = %asset_id, date = %date, "looking up price from store only");

        for offset in 0..=self.lookback_days {
            let target_date = date - Duration::days(offset as i64);
            if let Some(price) = self
                .store
                .get_price(&asset_id, target_date, PriceKind::Close)
                .await?
            {
                debug!(
                    asset_id = %asset_id,
                    date = %target_date,
                    price = %price.price,
                    source = %price.source,
                    "price found in store"
                );
                return Ok(Some(price));
            }
        }

        Ok(None)
    }

    pub async fn price_close(&self, asset: &Asset, date: NaiveDate) -> Result<PricePoint> {
        let asset = asset.normalized();
        let asset_id = AssetId::from_asset(&asset);
        debug!(asset_id = %asset_id, date = %date, "looking up close price");

        for offset in 0..=self.lookback_days {
            let target_date = date - Duration::days(offset as i64);
            if let Some(price) = self
                .store
                .get_price(&asset_id, target_date, PriceKind::Close)
                .await?
            {
                debug!(
                    asset_id = %asset_id,
                    date = %target_date,
                    price = %price.price,
                    "price found in cache"
                );
                return Ok(price);
            }

            if let Some(price) = self
                .fetch_price_from_sources(&asset, &asset_id, target_date)
                .await?
            {
                info!(
                    asset_id = %asset_id,
                    date = %target_date,
                    price = %price.price,
                    source = %price.source,
                    "price fetched and stored"
                );
                self.store.put_prices(std::slice::from_ref(&price)).await?;
                return Ok(price);
            }
        }

        Err(anyhow::anyhow!(
            "No close price found for asset {asset_id} on or before {date}"
        ))
    }

    /// Like [`Self::price_close`] but tries to fetch from sources first, even if the store already
    /// has data. Falls back to the cached result if sources don't return anything.
    ///
    /// Returns `(price, fetched)` where `fetched` indicates whether a new point was fetched and stored.
    pub async fn price_close_force(
        &self,
        asset: &Asset,
        date: NaiveDate,
    ) -> Result<(PricePoint, bool)> {
        let asset = asset.normalized();
        let asset_id = AssetId::from_asset(&asset);

        let had_cached = self.price_from_store(&asset, date).await?.is_some();

        for offset in 0..=self.lookback_days {
            let target_date = date - Duration::days(offset as i64);
            if let Some(price) = self
                .fetch_price_from_sources(&asset, &asset_id, target_date)
                .await?
            {
                self.store.put_prices(std::slice::from_ref(&price)).await?;
                return Ok((price, true));
            }
        }

        let price = self.price_close(&asset, date).await?;
        Ok((price, !had_cached))
    }

    /// Get the latest available price for an asset.
    /// Tries real-time quote first, falls back to historical close.
    /// If quote_staleness is set, returns cached quote if it's fresh enough.
    pub async fn price_latest(&self, asset: &Asset, date: NaiveDate) -> Result<PricePoint> {
        Ok(self.price_latest_with_status(asset, date).await?.0)
    }

    /// Like [`Self::price_latest`] but returns whether a new point was fetched and stored.
    pub async fn price_latest_with_status(
        &self,
        asset: &Asset,
        date: NaiveDate,
    ) -> Result<(PricePoint, bool)> {
        self.price_latest_inner(asset, date, false).await
    }

    /// Like [`Self::price_latest`] but always tries to fetch a new quote first (ignores cached quote
    /// freshness), then falls back to close prices. Returns whether a new point was fetched/stored.
    pub async fn price_latest_force(&self, asset: &Asset, date: NaiveDate) -> Result<(PricePoint, bool)> {
        self.price_latest_inner(asset, date, true).await
    }

    async fn price_latest_inner(
        &self,
        asset: &Asset,
        date: NaiveDate,
        force: bool,
    ) -> Result<(PricePoint, bool)> {
        let asset = asset.normalized();
        let asset_id = AssetId::from_asset(&asset);
        debug!(asset_id = %asset_id, "looking up latest price (quote or close)");

        // Check for a cached quote first if staleness is configured (unless forced).
        if !force {
            if let Some(staleness) = self.quote_staleness {
                if let Some(cached) = self
                    .store
                    .get_price(&asset_id, date, PriceKind::Quote)
                    .await?
                {
                    let age = (self.clock.now() - cached.timestamp)
                        .to_std()
                        .unwrap_or(std::time::Duration::ZERO);
                    if age < staleness {
                        debug!(
                            asset_id = %asset_id,
                            price = %cached.price,
                            age_secs = age.as_secs(),
                            "returning cached quote (still fresh)"
                        );
                        return Ok((cached, false));
                    }
                    debug!(
                        asset_id = %asset_id,
                        age_secs = age.as_secs(),
                        staleness_secs = staleness.as_secs(),
                        "cached quote is stale, fetching new one"
                    );
                }
            }
        }

        // Try to get a live quote
        if let Some(price) = self.fetch_quote_from_sources(&asset, &asset_id).await? {
            info!(
                asset_id = %asset_id,
                price = %price.price,
                source = %price.source,
                kind = ?price.kind,
                "live quote fetched and stored"
            );
            self.store.put_prices(std::slice::from_ref(&price)).await?;
            return Ok((price, true));
        }

        debug!(asset_id = %asset_id, "no live quote available, falling back to close price");
        // Fall back to close price, but track whether we had to fetch.
        if force {
            let (price, fetched) = self.price_close_force(&asset, date).await?;
            return Ok((price, fetched));
        }

        if let Some(price) = self.price_from_store(&asset, date).await? {
            return Ok((price, false));
        }

        let price = self.price_close(&asset, date).await?;
        Ok((price, true))
    }

    pub async fn fx_close(&self, base: &str, quote: &str, date: NaiveDate) -> Result<FxRatePoint> {
        let base = base.trim().to_uppercase();
        let quote = quote.trim().to_uppercase();
        debug!(base = %base, quote = %quote, date = %date, "looking up FX rate");

        if base == quote {
            return Ok(FxRatePoint {
                base,
                quote,
                as_of_date: date,
                timestamp: self.clock.now(),
                rate: "1".to_string(),
                kind: FxRateKind::Close,
                source: "identity".to_string(),
            });
        }

        for offset in 0..=self.lookback_days {
            let target_date = date - Duration::days(offset as i64);
            if let Some(rate) = self
                .store
                .get_fx_rate(&base, &quote, target_date, FxRateKind::Close)
                .await?
            {
                debug!(
                    base = %base,
                    quote = %quote,
                    date = %target_date,
                    rate = %rate.rate,
                    "FX rate found in cache"
                );
                return Ok(rate);
            }

            if let Some(rate) = self
                .fetch_fx_from_sources(&base, &quote, target_date)
                .await?
            {
                info!(
                    base = %base,
                    quote = %quote,
                    date = %target_date,
                    rate = %rate.rate,
                    source = %rate.source,
                    "FX rate fetched and stored"
                );
                self.store.put_fx_rates(std::slice::from_ref(&rate)).await?;
                return Ok(rate);
            }
        }

        Err(anyhow::anyhow!(
            "No FX rate found for {base}->{quote} on or before {date}"
        ))
    }

    /// Like [`Self::fx_close`] but tries to fetch from sources first, even if the store already
    /// has data. Falls back to the cached result if sources don't return anything.
    ///
    /// Returns `(rate, fetched)` where `fetched` indicates whether a new point was fetched and stored.
    pub async fn fx_close_force(
        &self,
        base: &str,
        quote: &str,
        date: NaiveDate,
    ) -> Result<(FxRatePoint, bool)> {
        let base = base.trim().to_uppercase();
        let quote = quote.trim().to_uppercase();

        if base == quote {
            return Ok((
                FxRatePoint {
                    base,
                    quote,
                    as_of_date: date,
                    timestamp: self.clock.now(),
                    rate: "1".to_string(),
                    kind: FxRateKind::Close,
                    source: "identity".to_string(),
                },
                false,
            ));
        }

        let had_cached = self.fx_from_store(&base, &quote, date).await?.is_some();

        for offset in 0..=self.lookback_days {
            let target_date = date - Duration::days(offset as i64);
            if let Some(rate) = self
                .fetch_fx_from_sources(&base, &quote, target_date)
                .await?
            {
                self.store.put_fx_rates(std::slice::from_ref(&rate)).await?;
                return Ok((rate, true));
            }
        }

        let rate = self.fx_close(&base, &quote, date).await?;
        Ok((rate, !had_cached))
    }

    /// Get FX rate from store only, no external fetching.
    /// Returns the most recent close rate by date (with lookback).
    pub async fn fx_from_store(
        &self,
        base: &str,
        quote: &str,
        date: NaiveDate,
    ) -> Result<Option<FxRatePoint>> {
        let base = base.trim().to_uppercase();
        let quote = quote.trim().to_uppercase();
        debug!(base = %base, quote = %quote, date = %date, "looking up FX rate from store only");

        if base == quote {
            return Ok(Some(FxRatePoint {
                base,
                quote,
                as_of_date: date,
                timestamp: self.clock.now(),
                rate: "1".to_string(),
                kind: FxRateKind::Close,
                source: "identity".to_string(),
            }));
        }

        for offset in 0..=self.lookback_days {
            let target_date = date - Duration::days(offset as i64);
            if let Some(rate) = self
                .store
                .get_fx_rate(&base, &quote, target_date, FxRateKind::Close)
                .await?
            {
                return Ok(Some(rate));
            }
        }

        Ok(None)
    }

    pub async fn register_asset(&self, asset: &Asset) -> Result<()> {
        let entry = super::AssetRegistryEntry::new(asset.normalized());
        if self.store.get_asset_entry(&entry.id).await?.is_none() {
            self.store
                .upsert_asset_entry(&entry)
                .await
                .context("Failed to write asset registry entry")?;
        }
        Ok(())
    }

    /// Store a price point directly (e.g., from a synchronizer).
    pub async fn store_price(&self, price: &PricePoint) -> Result<()> {
        // Be idempotent: don't append duplicates (JsonlMarketDataStore is append-only).
        if let Some(existing) = self
            .store
            .get_price(&price.asset_id, price.as_of_date, price.kind)
            .await?
        {
            if existing.timestamp >= price.timestamp {
                debug!(
                    asset_id = %price.asset_id,
                    date = %price.as_of_date,
                    kind = ?price.kind,
                    "skipping store_price: existing price is newer-or-equal"
                );
                return Ok(());
            }
        }

        self.store.put_prices(std::slice::from_ref(price)).await
    }

    async fn fetch_quote_from_sources(
        &self,
        asset: &Asset,
        asset_id: &AssetId,
    ) -> Result<Option<PricePoint>> {
        match asset {
            Asset::Equity { .. } => {
                if let Some(router) = &self.equity_router {
                    if let Some(price) = router.fetch_quote(asset, asset_id).await? {
                        return Ok(Some(price));
                    }
                }
            }
            Asset::Crypto { .. } => {
                if let Some(router) = &self.crypto_router {
                    if let Some(price) = router.fetch_quote(asset, asset_id).await? {
                        return Ok(Some(price));
                    }
                }
            }
            _ => {}
        }

        Ok(None)
    }

    async fn fetch_price_from_sources(
        &self,
        asset: &Asset,
        asset_id: &AssetId,
        date: NaiveDate,
    ) -> Result<Option<PricePoint>> {
        match asset {
            Asset::Equity { .. } => {
                if let Some(router) = &self.equity_router {
                    if let Some(price) = router.fetch_close(asset, asset_id, date).await? {
                        return Ok(Some(price));
                    }
                }
            }
            Asset::Crypto { .. } => {
                if let Some(router) = &self.crypto_router {
                    if let Some(price) = router.fetch_close(asset, asset_id, date).await? {
                        return Ok(Some(price));
                    }
                }
            }
            _ => {}
        }

        if let Some(provider) = &self.provider {
            return provider.fetch_price(asset, asset_id, date).await;
        }

        Ok(None)
    }

    async fn fetch_fx_from_sources(
        &self,
        base: &str,
        quote: &str,
        date: NaiveDate,
    ) -> Result<Option<FxRatePoint>> {
        if let Some(router) = &self.fx_router {
            if let Some(rate) = router.fetch_close(base, quote, date).await? {
                return Ok(Some(rate));
            }
        }

        if let Some(provider) = &self.provider {
            return provider.fetch_fx_rate(base, quote, date).await;
        }

        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::FixedClock;
    use crate::market_data::{MemoryMarketDataStore, PriceKind, PricePoint};
    use crate::market_data::{AssetId, EquityPriceRouter, EquityPriceSource};
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

    fn make_quote(asset_id: &AssetId, as_of_date: NaiveDate, ts: chrono::DateTime<Utc>, px: &str) -> PricePoint {
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
        let router = Arc::new(EquityPriceRouter::new(vec![Arc::new(FixedEquityQuoteSource {
            point: src_quote,
        })]));
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
        let router = Arc::new(EquityPriceRouter::new(vec![Arc::new(FixedEquityQuoteSource {
            point: src_quote.clone(),
        })]));
        svc = svc.with_equity_router(router);

        let (p, fetched) = svc.price_latest_with_status(&asset, today).await?;
        assert!(fetched);
        assert_eq!(p.price, "200");
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
        let router = Arc::new(EquityPriceRouter::new(vec![Arc::new(FixedEquityQuoteSource {
            point: src_quote.clone(),
        })]));
        svc = svc.with_equity_router(router);

        let (p, fetched) = svc.price_latest_force(&asset, today).await?;
        assert!(fetched);
        assert_eq!(p.price, "200");
        Ok(())
    }
}
