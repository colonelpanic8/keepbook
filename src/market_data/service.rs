use std::sync::Arc;

use anyhow::{Context, Result};
use chrono::{Duration, NaiveDate};
use tracing::{debug, info};

use crate::clock::{Clock, SystemClock};

use super::{
    AssetId, CryptoPriceRouter, EquityPriceRouter, FxRateKind, FxRatePoint, FxRateRouter,
    MarketDataSource, MarketDataStore, PricePoint,
};
use crate::models::Asset;

pub struct MarketDataService {
    store: Arc<dyn MarketDataStore>,
    provider: Option<Arc<dyn MarketDataSource>>,
    equity_router: Option<Arc<EquityPriceRouter>>,
    crypto_router: Option<Arc<CryptoPriceRouter>>,
    fx_router: Option<Arc<FxRateRouter>>,
    // When set, bounds store lookups to N days back from query date.
    // When None, store lookups are unbounded and return the latest price <= query date.
    store_lookback_days: Option<u32>,
    // Bounds external fetch attempts when an exact close is unavailable.
    fetch_lookback_days: u32,
    // When true, historical store lookups may project the earliest later cached
    // reading backward when no acceptable earlier reading exists.
    allow_future_projection: bool,
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
            store_lookback_days: None,
            fetch_lookback_days: 7,
            allow_future_projection: false,
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
        self.store_lookback_days = Some(days);
        self.fetch_lookback_days = days;
        self
    }

    pub fn with_quote_staleness(mut self, staleness: std::time::Duration) -> Self {
        self.quote_staleness = Some(staleness);
        self
    }

    pub fn with_future_projection(mut self, enabled: bool) -> Self {
        self.allow_future_projection = enabled;
        self
    }

    pub fn with_clock(mut self, clock: Arc<dyn Clock>) -> Self {
        self.clock = clock;
        self
    }

    /// Get price from store only, no external fetching.
    /// Returns the latest price on or before `date`, regardless of price kind.
    ///
    /// If `store_lookback_days` is set, limits lookup to that range.
    /// Otherwise (default), lookup is unbounded.
    pub async fn price_from_store(
        &self,
        asset: &Asset,
        date: NaiveDate,
    ) -> Result<Option<PricePoint>> {
        let asset = asset.normalized();
        let asset_id = AssetId::from_asset(&asset);
        debug!(asset_id = %asset_id, date = %date, "looking up price from store only");

        let prices = self.store.get_all_prices(&asset_id).await?;

        if let Some(days) = self.store_lookback_days {
            let start = date - Duration::days(days as i64);
            if let Some(price) = select_latest_price_in_range(prices.clone(), start, date) {
                debug!(
                    asset_id = %asset_id,
                    date = %price.as_of_date,
                    price = %price.price,
                    source = %price.source,
                    "price found in bounded store lookup"
                );
                return Ok(Some(price));
            }
            if !self.allow_future_projection {
                return Ok(None);
            }

            return Ok(select_earliest_price_on_or_after(prices, date));
        }

        if let Some(price) = select_latest_price_on_or_before(prices.clone(), date) {
            return Ok(Some(price));
        }

        if self.allow_future_projection {
            return Ok(select_earliest_price_on_or_after(prices, date));
        }

        Ok(None)
    }

    /// Get a valuation price from store only, no external fetching.
    ///
    /// Price kind is ignored; the latest price observation on or before `date` wins.
    pub async fn valuation_price_from_store(
        &self,
        asset: &Asset,
        date: NaiveDate,
    ) -> Result<Option<PricePoint>> {
        self.price_from_store(asset, date).await
    }

    pub async fn price_close(&self, asset: &Asset, date: NaiveDate) -> Result<PricePoint> {
        let asset = asset.normalized();
        let asset_id = AssetId::from_asset(&asset);
        debug!(asset_id = %asset_id, date = %date, "looking up historical price");

        if let Some(price) = self.price_from_store(&asset, date).await? {
            debug!(
                asset_id = %asset_id,
                date = %price.as_of_date,
                price = %price.price,
                "price found in cache"
            );
            return Ok(price);
        }

        for offset in 0..=self.fetch_lookback_days {
            let target_date = date - Duration::days(offset as i64);
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
            "No price found for asset {asset_id} on or before {date}"
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

        for offset in 0..=self.fetch_lookback_days {
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

    /// Fetch and store historical prices for an asset over a date range.
    ///
    /// This bypasses per-date lookback loops and lets providers use native
    /// historical range endpoints when available.
    pub async fn price_closes_range(
        &self,
        asset: &Asset,
        start: NaiveDate,
        end: NaiveDate,
    ) -> Result<Vec<PricePoint>> {
        if start > end {
            return Ok(Vec::new());
        }

        let asset = asset.normalized();
        let asset_id = AssetId::from_asset(&asset);
        let prices = self
            .fetch_price_range_from_sources(&asset, &asset_id, start, end)
            .await?;

        if !prices.is_empty() {
            self.store.put_prices(&prices).await?;
        }

        Ok(prices)
    }

    /// Get the latest available price for an asset.
    /// Tries real-time quote first, falls back to the latest cached/fetched historical price.
    /// If quote_staleness is set, returns a same-day cached price if it's fresh enough.
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

    /// Like [`Self::price_latest`] but always tries to fetch a new quote first (ignores cached
    /// freshness), then falls back to cached/fetched historical prices. Returns whether a new point
    /// was fetched/stored.
    pub async fn price_latest_force(
        &self,
        asset: &Asset,
        date: NaiveDate,
    ) -> Result<(PricePoint, bool)> {
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

        let mut stale_cached_price = None;

        // Check for a cached same-day price first if staleness is configured (unless forced).
        if !force {
            if let Some(staleness) = self.quote_staleness {
                if let Some(cached) = self
                    .latest_price_on_date_from_store(&asset_id, date)
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
                    stale_cached_price = Some(cached);
                    debug!(
                        asset_id = %asset_id,
                        age_secs = age.as_secs(),
                        staleness_secs = staleness.as_secs(),
                        "cached price is stale, fetching new quote"
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

        if let Some(cached) = stale_cached_price {
            debug!(
                asset_id = %asset_id,
                price = %cached.price,
                "returning stale cached price after live quote fetch failed"
            );
            return Ok((cached, false));
        }

        debug!(asset_id = %asset_id, "no live quote available, falling back to cached/fetched price");
        // Fall back to cached/fetched historical price, but track whether we had to fetch.
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

    async fn latest_price_on_date_from_store(
        &self,
        asset_id: &AssetId,
        date: NaiveDate,
    ) -> Result<Option<PricePoint>> {
        let prices = self.store.get_all_prices(asset_id).await?;
        Ok(select_latest_price_on_date(prices, date))
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

        if let Some(rate) = self.fx_from_store(&base, &quote, date).await? {
            debug!(
                base = %base,
                quote = %quote,
                date = %rate.as_of_date,
                rate = %rate.rate,
                "FX rate found in cache"
            );
            return Ok(rate);
        }

        for offset in 0..=self.fetch_lookback_days {
            let target_date = date - Duration::days(offset as i64);
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

        for offset in 0..=self.fetch_lookback_days {
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
    /// Returns the latest close on or before `date`.
    ///
    /// If `store_lookback_days` is set, limits lookup to that range.
    /// Otherwise (default), lookup is unbounded.
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

        if let Some(days) = self.store_lookback_days {
            for offset in 0..=days {
                let target_date = date - Duration::days(offset as i64);
                if let Some(rate) = self
                    .store
                    .get_fx_rate(&base, &quote, target_date, FxRateKind::Close)
                    .await?
                {
                    return Ok(Some(rate));
                }
            }
            if !self.allow_future_projection {
                return Ok(None);
            }

            let rates = self.store.get_all_fx_rates(&base, &quote).await?;
            return Ok(select_earliest_fx_rate_on_or_after(rates, date));
        }

        let rates = self.store.get_all_fx_rates(&base, &quote).await?;
        if let Some(rate) = select_latest_fx_rate_on_or_before(rates.clone(), date) {
            return Ok(Some(rate));
        }

        if self.allow_future_projection {
            return Ok(select_earliest_fx_rate_on_or_after(rates, date));
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

    async fn fetch_price_range_from_sources(
        &self,
        asset: &Asset,
        asset_id: &AssetId,
        start: NaiveDate,
        end: NaiveDate,
    ) -> Result<Vec<PricePoint>> {
        match asset {
            Asset::Equity { .. } => {
                if let Some(router) = &self.equity_router {
                    let prices = router.fetch_closes(asset, asset_id, start, end).await?;
                    if !prices.is_empty() {
                        return Ok(prices);
                    }
                }
            }
            Asset::Crypto { .. } => {
                if let Some(router) = &self.crypto_router {
                    let prices = router.fetch_closes(asset, asset_id, start, end).await?;
                    if !prices.is_empty() {
                        return Ok(prices);
                    }
                }
            }
            _ => {}
        }

        Ok(Vec::new())
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

fn select_latest_price_on_or_before(
    prices: Vec<PricePoint>,
    date: NaiveDate,
) -> Option<PricePoint> {
    prices
        .into_iter()
        .filter(|p| p.as_of_date <= date)
        .max_by(|a, b| {
            a.as_of_date
                .cmp(&b.as_of_date)
                .then_with(|| a.timestamp.cmp(&b.timestamp))
        })
}

fn select_latest_price_on_date(prices: Vec<PricePoint>, date: NaiveDate) -> Option<PricePoint> {
    prices
        .into_iter()
        .filter(|p| p.as_of_date == date)
        .max_by_key(|p| p.timestamp)
}

fn select_latest_price_in_range(
    prices: Vec<PricePoint>,
    start: NaiveDate,
    end: NaiveDate,
) -> Option<PricePoint> {
    prices
        .into_iter()
        .filter(|p| p.as_of_date >= start && p.as_of_date <= end)
        .max_by(|a, b| {
            a.as_of_date
                .cmp(&b.as_of_date)
                .then_with(|| a.timestamp.cmp(&b.timestamp))
        })
}

fn select_earliest_price_on_or_after(
    prices: Vec<PricePoint>,
    date: NaiveDate,
) -> Option<PricePoint> {
    prices
        .into_iter()
        .filter(|p| p.as_of_date >= date)
        .min_by(|a, b| {
            a.as_of_date
                .cmp(&b.as_of_date)
                .then_with(|| b.timestamp.cmp(&a.timestamp))
        })
}

fn select_latest_fx_rate_on_or_before(
    rates: Vec<FxRatePoint>,
    date: NaiveDate,
) -> Option<FxRatePoint> {
    rates
        .into_iter()
        .filter(|r| r.kind == FxRateKind::Close && r.as_of_date <= date)
        .max_by(|a, b| {
            a.as_of_date
                .cmp(&b.as_of_date)
                .then_with(|| a.timestamp.cmp(&b.timestamp))
        })
}

fn select_earliest_fx_rate_on_or_after(
    rates: Vec<FxRatePoint>,
    date: NaiveDate,
) -> Option<FxRatePoint> {
    rates
        .into_iter()
        .filter(|r| r.kind == FxRateKind::Close && r.as_of_date >= date)
        .min_by(|a, b| {
            a.as_of_date
                .cmp(&b.as_of_date)
                .then_with(|| a.timestamp.cmp(&b.timestamp))
        })
}

#[cfg(test)]
#[path = "../../tests/unit/market_data/service_tests.rs"]
mod service_tests;
