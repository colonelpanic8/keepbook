use std::sync::Arc;

use anyhow::{Context, Result};
use chrono::{Duration, NaiveDate};
use tracing::{debug, info};

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

    pub async fn price_close(&self, asset: &Asset, date: NaiveDate) -> Result<PricePoint> {
        let asset_id = AssetId::from_asset(asset);
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

            if let Some(price) = self.fetch_price_from_sources(asset, &asset_id, target_date).await?
            {
                info!(
                    asset_id = %asset_id,
                    date = %target_date,
                    price = %price.price,
                    source = %price.source,
                    "price fetched and stored"
                );
                self.store.put_prices(&[price.clone()]).await?;
                return Ok(price);
            }
        }

        Err(anyhow::anyhow!(
            "No close price found for asset {asset_id} on or before {date}"
        ))
    }

    /// Get the latest available price for an asset.
    /// Tries real-time quote first, falls back to historical close.
    /// If quote_staleness is set, returns cached quote if it's fresh enough.
    pub async fn price_latest(&self, asset: &Asset, date: NaiveDate) -> Result<PricePoint> {
        let asset_id = AssetId::from_asset(asset);
        debug!(asset_id = %asset_id, "looking up latest price (quote or close)");

        // Check for a cached quote first if staleness is configured
        if let Some(staleness) = self.quote_staleness {
            if let Some(cached) = self
                .store
                .get_price(&asset_id, date, PriceKind::Quote)
                .await?
            {
                let age = (chrono::Utc::now() - cached.timestamp)
                    .to_std()
                    .unwrap_or(std::time::Duration::MAX);
                if age < staleness {
                    debug!(
                        asset_id = %asset_id,
                        price = %cached.price,
                        age_secs = age.as_secs(),
                        "returning cached quote (still fresh)"
                    );
                    return Ok(cached);
                }
                debug!(
                    asset_id = %asset_id,
                    age_secs = age.as_secs(),
                    staleness_secs = staleness.as_secs(),
                    "cached quote is stale, fetching new one"
                );
            }
        }

        // Try to get a live quote
        if let Some(price) = self.fetch_quote_from_sources(asset, &asset_id).await? {
            info!(
                asset_id = %asset_id,
                price = %price.price,
                source = %price.source,
                kind = ?price.kind,
                "live quote fetched and stored"
            );
            self.store.put_prices(&[price.clone()]).await?;
            return Ok(price);
        }

        debug!(asset_id = %asset_id, "no live quote available, falling back to close price");
        // Fall back to close price
        self.price_close(asset, date).await
    }

    pub async fn fx_close(&self, base: &str, quote: &str, date: NaiveDate) -> Result<FxRatePoint> {
        debug!(base = base, quote = quote, date = %date, "looking up FX rate");

        for offset in 0..=self.lookback_days {
            let target_date = date - Duration::days(offset as i64);
            if let Some(rate) = self
                .store
                .get_fx_rate(base, quote, target_date, FxRateKind::Close)
                .await?
            {
                debug!(
                    base = base,
                    quote = quote,
                    date = %target_date,
                    rate = %rate.rate,
                    "FX rate found in cache"
                );
                return Ok(rate);
            }

            if let Some(rate) = self.fetch_fx_from_sources(base, quote, target_date).await? {
                info!(
                    base = base,
                    quote = quote,
                    date = %target_date,
                    rate = %rate.rate,
                    source = %rate.source,
                    "FX rate fetched and stored"
                );
                self.store.put_fx_rates(&[rate.clone()]).await?;
                return Ok(rate);
            }
        }

        Err(anyhow::anyhow!(
            "No FX rate found for {base}->{quote} on or before {date}"
        ))
    }

    pub async fn register_asset(&self, asset: &Asset) -> Result<()> {
        let entry = super::AssetRegistryEntry::new(asset.clone());
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
        self.store.put_prices(&[price.clone()]).await
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
