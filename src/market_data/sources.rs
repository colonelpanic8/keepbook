use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use chrono::NaiveDate;

use super::{AssetId, FxRatePoint, PricePoint};
use crate::models::Asset;

#[async_trait::async_trait]
pub trait EquityPriceSource: Send + Sync {
    async fn fetch_close(
        &self,
        asset: &Asset,
        asset_id: &AssetId,
        date: NaiveDate,
    ) -> Result<Option<PricePoint>>;

    fn name(&self) -> &str;
}

#[async_trait::async_trait]
pub trait CryptoPriceSource: Send + Sync {
    async fn fetch_close(
        &self,
        asset: &Asset,
        asset_id: &AssetId,
        date: NaiveDate,
    ) -> Result<Option<PricePoint>>;

    fn name(&self) -> &str;
}

#[async_trait::async_trait]
pub trait FxRateSource: Send + Sync {
    async fn fetch_close(&self, base: &str, quote: &str, date: NaiveDate)
        -> Result<Option<FxRatePoint>>;

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
        for source in &self.sources {
            let _limit = self.rate_limits.get(source.name());
            if let Some(price) = source.fetch_close(asset, asset_id, date).await? {
                return Ok(Some(price));
            }
        }
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
        for source in &self.sources {
            let _limit = self.rate_limits.get(source.name());
            if let Some(price) = source.fetch_close(asset, asset_id, date).await? {
                return Ok(Some(price));
            }
        }
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
        for source in &self.sources {
            let _limit = self.rate_limits.get(source.name());
            if let Some(rate) = source.fetch_close(base, quote, date).await? {
                return Ok(Some(rate));
            }
        }
        Ok(None)
    }
}
