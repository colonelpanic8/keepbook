use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use chrono::NaiveDate;
use tracing::{debug, info, warn};

use super::{AssetId, FxRatePoint, PricePoint};
use crate::models::Asset;

#[async_trait::async_trait]
pub trait EquityPriceSource: Send + Sync {
    /// Fetch end-of-day closing price for a specific date.
    async fn fetch_close(
        &self,
        asset: &Asset,
        asset_id: &AssetId,
        date: NaiveDate,
    ) -> Result<Option<PricePoint>>;

    /// Fetch real-time or delayed quote (current price).
    /// Default implementation returns None (not supported).
    async fn fetch_quote(&self, _asset: &Asset, _asset_id: &AssetId) -> Result<Option<PricePoint>> {
        Ok(None)
    }

    fn name(&self) -> &str;
}

#[async_trait::async_trait]
pub trait CryptoPriceSource: Send + Sync {
    /// Fetch end-of-day closing price for a specific date.
    async fn fetch_close(
        &self,
        asset: &Asset,
        asset_id: &AssetId,
        date: NaiveDate,
    ) -> Result<Option<PricePoint>>;

    /// Fetch real-time or delayed quote (current price).
    /// Default implementation returns None (not supported).
    async fn fetch_quote(&self, _asset: &Asset, _asset_id: &AssetId) -> Result<Option<PricePoint>> {
        Ok(None)
    }

    fn name(&self) -> &str;
}

#[async_trait::async_trait]
pub trait FxRateSource: Send + Sync {
    async fn fetch_close(
        &self,
        base: &str,
        quote: &str,
        date: NaiveDate,
    ) -> Result<Option<FxRatePoint>>;

    fn name(&self) -> &str;
}

#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    pub max_requests: u32,
    pub window_seconds: u64,
}

impl RateLimitConfig {
    pub fn new(max_requests: u32, window_seconds: u64) -> Self {
        Self {
            max_requests,
            window_seconds,
        }
    }
}

pub struct EquityPriceRouter {
    sources: Vec<Arc<dyn EquityPriceSource>>,
    rate_limits: HashMap<String, RateLimitConfig>,
}

impl EquityPriceRouter {
    pub fn new(sources: Vec<Arc<dyn EquityPriceSource>>) -> Self {
        Self {
            sources,
            rate_limits: HashMap::new(),
        }
    }

    pub fn with_rate_limits(mut self, limits: HashMap<String, RateLimitConfig>) -> Self {
        self.rate_limits = limits;
        self
    }

    pub async fn fetch_close(
        &self,
        asset: &Asset,
        asset_id: &AssetId,
        date: NaiveDate,
    ) -> Result<Option<PricePoint>> {
        debug!(asset_id = %asset_id, date = %date, "fetching equity close price");
        for source in &self.sources {
            let _limit = self.rate_limits.get(source.name());
            match source.fetch_close(asset, asset_id, date).await {
                Ok(Some(price)) => {
                    info!(
                        source = source.name(),
                        asset_id = %asset_id,
                        date = %date,
                        price = %price.price,
                        currency = %price.quote_currency,
                        "equity price fetched"
                    );
                    return Ok(Some(price));
                }
                Ok(None) => {
                    debug!(source = source.name(), asset_id = %asset_id, "no price from source");
                    continue;
                }
                Err(e) => {
                    warn!(
                        source = source.name(),
                        asset_id = %asset_id,
                        error = %e,
                        "equity price fetch failed"
                    );
                    continue;
                }
            }
        }
        warn!(asset_id = %asset_id, date = %date, "no equity close price found from any source");
        Ok(None)
    }

    pub async fn fetch_quote(
        &self,
        asset: &Asset,
        asset_id: &AssetId,
    ) -> Result<Option<PricePoint>> {
        debug!(asset_id = %asset_id, "fetching equity quote");
        for source in &self.sources {
            let _limit = self.rate_limits.get(source.name());
            match source.fetch_quote(asset, asset_id).await {
                Ok(Some(price)) => {
                    info!(
                        source = source.name(),
                        asset_id = %asset_id,
                        price = %price.price,
                        currency = %price.quote_currency,
                        "equity quote fetched"
                    );
                    return Ok(Some(price));
                }
                Ok(None) => {
                    debug!(source = source.name(), asset_id = %asset_id, "no quote from source");
                    continue;
                }
                Err(e) => {
                    warn!(
                        source = source.name(),
                        asset_id = %asset_id,
                        error = %e,
                        "equity quote fetch failed"
                    );
                    continue;
                }
            }
        }
        debug!(asset_id = %asset_id, "no equity quote found from any source");
        Ok(None)
    }
}

pub struct CryptoPriceRouter {
    sources: Vec<Arc<dyn CryptoPriceSource>>,
    rate_limits: HashMap<String, RateLimitConfig>,
}

impl CryptoPriceRouter {
    pub fn new(sources: Vec<Arc<dyn CryptoPriceSource>>) -> Self {
        Self {
            sources,
            rate_limits: HashMap::new(),
        }
    }

    pub fn with_rate_limits(mut self, limits: HashMap<String, RateLimitConfig>) -> Self {
        self.rate_limits = limits;
        self
    }

    pub async fn fetch_close(
        &self,
        asset: &Asset,
        asset_id: &AssetId,
        date: NaiveDate,
    ) -> Result<Option<PricePoint>> {
        debug!(asset_id = %asset_id, date = %date, "fetching crypto close price");
        for source in &self.sources {
            let _limit = self.rate_limits.get(source.name());
            match source.fetch_close(asset, asset_id, date).await {
                Ok(Some(price)) => {
                    info!(
                        source = source.name(),
                        asset_id = %asset_id,
                        date = %date,
                        price = %price.price,
                        currency = %price.quote_currency,
                        "crypto price fetched"
                    );
                    return Ok(Some(price));
                }
                Ok(None) => {
                    debug!(source = source.name(), asset_id = %asset_id, "no price from source");
                    continue;
                }
                Err(e) => {
                    warn!(
                        source = source.name(),
                        asset_id = %asset_id,
                        error = %e,
                        "crypto price fetch failed"
                    );
                    continue;
                }
            }
        }
        warn!(asset_id = %asset_id, date = %date, "no crypto close price found from any source");
        Ok(None)
    }

    pub async fn fetch_quote(
        &self,
        asset: &Asset,
        asset_id: &AssetId,
    ) -> Result<Option<PricePoint>> {
        debug!(asset_id = %asset_id, "fetching crypto quote");
        for source in &self.sources {
            let _limit = self.rate_limits.get(source.name());
            match source.fetch_quote(asset, asset_id).await {
                Ok(Some(price)) => {
                    info!(
                        source = source.name(),
                        asset_id = %asset_id,
                        price = %price.price,
                        currency = %price.quote_currency,
                        "crypto quote fetched"
                    );
                    return Ok(Some(price));
                }
                Ok(None) => {
                    debug!(source = source.name(), asset_id = %asset_id, "no quote from source");
                    continue;
                }
                Err(e) => {
                    warn!(
                        source = source.name(),
                        asset_id = %asset_id,
                        error = %e,
                        "crypto quote fetch failed"
                    );
                    continue;
                }
            }
        }
        debug!(asset_id = %asset_id, "no crypto quote found from any source");
        Ok(None)
    }
}

pub struct FxRateRouter {
    sources: Vec<Arc<dyn FxRateSource>>,
    rate_limits: HashMap<String, RateLimitConfig>,
}

impl FxRateRouter {
    pub fn new(sources: Vec<Arc<dyn FxRateSource>>) -> Self {
        Self {
            sources,
            rate_limits: HashMap::new(),
        }
    }

    pub fn with_rate_limits(mut self, limits: HashMap<String, RateLimitConfig>) -> Self {
        self.rate_limits = limits;
        self
    }

    pub async fn fetch_close(
        &self,
        base: &str,
        quote: &str,
        date: NaiveDate,
    ) -> Result<Option<FxRatePoint>> {
        debug!(base = base, quote = quote, date = %date, "fetching FX rate");
        for source in &self.sources {
            let _limit = self.rate_limits.get(source.name());
            match source.fetch_close(base, quote, date).await {
                Ok(Some(rate)) => {
                    info!(
                        source = source.name(),
                        base = base,
                        quote = quote,
                        date = %date,
                        rate = %rate.rate,
                        "FX rate fetched"
                    );
                    return Ok(Some(rate));
                }
                Ok(None) => {
                    debug!(
                        source = source.name(),
                        base = base,
                        quote = quote,
                        "no rate from source"
                    );
                    continue;
                }
                Err(e) => {
                    warn!(
                        source = source.name(),
                        base = base,
                        quote = quote,
                        error = %e,
                        "FX rate fetch failed"
                    );
                    continue;
                }
            }
        }
        warn!(base = base, quote = quote, date = %date, "no FX rate found from any source");
        Ok(None)
    }
}
