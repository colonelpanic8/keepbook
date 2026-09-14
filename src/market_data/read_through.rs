//! Request-scoped memoization for market data reads.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use chrono::NaiveDate;

use super::{
    AssetId, AssetRegistryEntry, FxRateKind, FxRatePoint, MarketDataStore, PriceKind, PricePoint,
};

#[derive(Default)]
struct ReadThroughCache {
    prices: HashMap<AssetId, Arc<Vec<PricePoint>>>,
    fx_rates: HashMap<(String, String), Arc<Vec<FxRatePoint>>>,
}

/// Reads each asset's price history and each pair's FX history at most once.
///
/// Valuing a series of history points asks for the same histories once per
/// point, and the underlying stores answer by cloning their whole cached file
/// contents each time. Wrap a store in this for the duration of one such
/// request; a write clears what was memoized, and the wrapper is dropped when
/// the request ends, so there is no cache to invalidate across requests.
pub struct ReadThroughMarketDataStore {
    inner: Arc<dyn MarketDataStore>,
    cache: Mutex<ReadThroughCache>,
}

impl ReadThroughMarketDataStore {
    pub fn new(inner: Arc<dyn MarketDataStore>) -> Self {
        Self {
            inner,
            cache: Mutex::new(ReadThroughCache::default()),
        }
    }

    fn cache(&self) -> std::sync::MutexGuard<'_, ReadThroughCache> {
        self.cache
            .lock()
            .expect("market data read-through poisoned")
    }

    fn clear(&self) {
        *self.cache() = ReadThroughCache::default();
    }
}

#[async_trait::async_trait]
impl MarketDataStore for ReadThroughMarketDataStore {
    async fn get_price(
        &self,
        asset_id: &AssetId,
        date: NaiveDate,
        kind: PriceKind,
    ) -> Result<Option<PricePoint>> {
        self.inner.get_price(asset_id, date, kind).await
    }

    async fn get_all_prices(&self, asset_id: &AssetId) -> Result<Vec<PricePoint>> {
        Ok(self.all_prices_shared(asset_id).await?.as_ref().clone())
    }

    async fn all_prices_shared(&self, asset_id: &AssetId) -> Result<Arc<Vec<PricePoint>>> {
        if let Some(prices) = self.cache().prices.get(asset_id) {
            return Ok(Arc::clone(prices));
        }

        let prices = Arc::new(self.inner.get_all_prices(asset_id).await?);
        self.cache()
            .prices
            .insert(asset_id.clone(), Arc::clone(&prices));
        Ok(prices)
    }

    async fn put_prices(&self, prices: &[PricePoint]) -> Result<()> {
        self.inner.put_prices(prices).await?;
        self.clear();
        Ok(())
    }

    async fn get_fx_rate(
        &self,
        base: &str,
        quote: &str,
        date: NaiveDate,
        kind: FxRateKind,
    ) -> Result<Option<FxRatePoint>> {
        self.inner.get_fx_rate(base, quote, date, kind).await
    }

    async fn get_all_fx_rates(&self, base: &str, quote: &str) -> Result<Vec<FxRatePoint>> {
        Ok(self
            .all_fx_rates_shared(base, quote)
            .await?
            .as_ref()
            .clone())
    }

    async fn all_fx_rates_shared(&self, base: &str, quote: &str) -> Result<Arc<Vec<FxRatePoint>>> {
        let key = (base.to_string(), quote.to_string());
        if let Some(rates) = self.cache().fx_rates.get(&key) {
            return Ok(Arc::clone(rates));
        }

        let rates = Arc::new(self.inner.get_all_fx_rates(base, quote).await?);
        self.cache().fx_rates.insert(key, Arc::clone(&rates));
        Ok(rates)
    }

    async fn put_fx_rates(&self, rates: &[FxRatePoint]) -> Result<()> {
        self.inner.put_fx_rates(rates).await?;
        self.clear();
        Ok(())
    }

    async fn get_asset_entry(&self, asset_id: &AssetId) -> Result<Option<AssetRegistryEntry>> {
        self.inner.get_asset_entry(asset_id).await
    }

    async fn upsert_asset_entry(&self, entry: &AssetRegistryEntry) -> Result<()> {
        self.inner.upsert_asset_entry(entry).await
    }
}

#[cfg(test)]
#[path = "../../tests/unit/market_data/read_through_tests.rs"]
mod read_through_tests;
