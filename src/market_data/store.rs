use std::collections::HashMap;

use anyhow::Result;
use chrono::NaiveDate;

use super::{AssetId, AssetRegistryEntry, FxRateKind, FxRatePoint, PriceKind, PricePoint};

#[async_trait::async_trait]
pub trait MarketDataStore: Send + Sync {
    async fn get_price(
        &self,
        asset_id: &AssetId,
        date: NaiveDate,
        kind: PriceKind,
    ) -> Result<Option<PricePoint>>;

    /// Get all prices for an asset across all time.
    async fn get_all_prices(&self, asset_id: &AssetId) -> Result<Vec<PricePoint>>;

    async fn put_prices(&self, prices: &[PricePoint]) -> Result<()>;

    async fn get_fx_rate(
        &self,
        base: &str,
        quote: &str,
        date: NaiveDate,
        kind: FxRateKind,
    ) -> Result<Option<FxRatePoint>>;

    /// Get all FX rates for a currency pair across all time.
    async fn get_all_fx_rates(&self, base: &str, quote: &str) -> Result<Vec<FxRatePoint>>;

    async fn put_fx_rates(&self, rates: &[FxRatePoint]) -> Result<()>;

    async fn get_asset_entry(&self, asset_id: &AssetId) -> Result<Option<AssetRegistryEntry>>;

    async fn upsert_asset_entry(&self, entry: &AssetRegistryEntry) -> Result<()>;
}

pub struct NullMarketDataStore;

#[async_trait::async_trait]
impl MarketDataStore for NullMarketDataStore {
    async fn get_price(
        &self,
        _asset_id: &AssetId,
        _date: NaiveDate,
        _kind: PriceKind,
    ) -> Result<Option<PricePoint>> {
        Ok(None)
    }

    async fn get_all_prices(&self, _asset_id: &AssetId) -> Result<Vec<PricePoint>> {
        Ok(Vec::new())
    }

    async fn put_prices(&self, _prices: &[PricePoint]) -> Result<()> {
        Ok(())
    }

    async fn get_fx_rate(
        &self,
        _base: &str,
        _quote: &str,
        _date: NaiveDate,
        _kind: FxRateKind,
    ) -> Result<Option<FxRatePoint>> {
        Ok(None)
    }

    async fn get_all_fx_rates(&self, _base: &str, _quote: &str) -> Result<Vec<FxRatePoint>> {
        Ok(Vec::new())
    }

    async fn put_fx_rates(&self, _rates: &[FxRatePoint]) -> Result<()> {
        Ok(())
    }

    async fn get_asset_entry(&self, _asset_id: &AssetId) -> Result<Option<AssetRegistryEntry>> {
        Ok(None)
    }

    async fn upsert_asset_entry(&self, _entry: &AssetRegistryEntry) -> Result<()> {
        Ok(())
    }
}

#[derive(Default)]
pub struct MemoryMarketDataStore {
    prices: tokio::sync::Mutex<HashMap<(AssetId, NaiveDate, PriceKind), PricePoint>>,
    fx_rates: tokio::sync::Mutex<HashMap<(String, String, NaiveDate, FxRateKind), FxRatePoint>>,
    assets: tokio::sync::Mutex<HashMap<AssetId, AssetRegistryEntry>>,
}

impl MemoryMarketDataStore {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait::async_trait]
impl MarketDataStore for MemoryMarketDataStore {
    async fn get_price(
        &self,
        asset_id: &AssetId,
        date: NaiveDate,
        kind: PriceKind,
    ) -> Result<Option<PricePoint>> {
        let prices = self.prices.lock().await;
        Ok(prices.get(&(asset_id.clone(), date, kind)).cloned())
    }

    async fn get_all_prices(&self, asset_id: &AssetId) -> Result<Vec<PricePoint>> {
        let prices = self.prices.lock().await;
        Ok(prices
            .iter()
            .filter(|((id, _, _), _)| id == asset_id)
            .map(|(_, p)| p.clone())
            .collect())
    }

    async fn put_prices(&self, prices: &[PricePoint]) -> Result<()> {
        if prices.is_empty() {
            return Ok(());
        }
        let mut store = self.prices.lock().await;
        for price in prices {
            store.insert(
                (price.asset_id.clone(), price.as_of_date, price.kind),
                price.clone(),
            );
        }
        Ok(())
    }

    async fn get_fx_rate(
        &self,
        base: &str,
        quote: &str,
        date: NaiveDate,
        kind: FxRateKind,
    ) -> Result<Option<FxRatePoint>> {
        let fx_rates = self.fx_rates.lock().await;
        let base = base.trim().to_uppercase();
        let quote = quote.trim().to_uppercase();
        Ok(fx_rates
            .get(&(base, quote, date, kind))
            .cloned())
    }

    async fn get_all_fx_rates(&self, base: &str, quote: &str) -> Result<Vec<FxRatePoint>> {
        let fx_rates = self.fx_rates.lock().await;
        let base = base.trim().to_uppercase();
        let quote = quote.trim().to_uppercase();
        Ok(fx_rates
            .iter()
            .filter(|((b, q, _, _), _)| b == &base && q == &quote)
            .map(|(_, r)| r.clone())
            .collect())
    }

    async fn put_fx_rates(&self, rates: &[FxRatePoint]) -> Result<()> {
        if rates.is_empty() {
            return Ok(());
        }
        let mut store = self.fx_rates.lock().await;
        for rate in rates {
            let normalized = FxRatePoint {
                base: rate.base.trim().to_uppercase(),
                quote: rate.quote.trim().to_uppercase(),
                ..rate.clone()
            };
            store.insert(
                (
                    normalized.base.clone(),
                    normalized.quote.clone(),
                    normalized.as_of_date,
                    normalized.kind,
                ),
                normalized,
            );
        }
        Ok(())
    }

    async fn get_asset_entry(&self, asset_id: &AssetId) -> Result<Option<AssetRegistryEntry>> {
        let assets = self.assets.lock().await;
        Ok(assets.get(asset_id).cloned())
    }

    async fn upsert_asset_entry(&self, entry: &AssetRegistryEntry) -> Result<()> {
        let mut assets = self.assets.lock().await;
        assets.insert(entry.id.clone(), entry.clone());
        Ok(())
    }
}
