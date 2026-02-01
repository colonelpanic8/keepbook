use std::sync::Arc;

use anyhow::{Context, Result};
use chrono::{Duration, NaiveDate};

use super::{
    AssetId, CryptoPriceRouter, EquityPriceRouter, FxRateKind, FxRatePoint, FxRateRouter,
    MarketDataProvider, MarketDataStore, PriceKind, PricePoint,
};
use crate::models::Asset;

pub struct MarketDataService {
    store: Arc<dyn MarketDataStore>,
    provider: Option<Arc<dyn MarketDataProvider>>,
    equity_router: Option<Arc<EquityPriceRouter>>,
    crypto_router: Option<Arc<CryptoPriceRouter>>,
    fx_router: Option<Arc<FxRateRouter>>,
    lookback_days: u32,
}

impl MarketDataService {
    pub fn new(
        store: Arc<dyn MarketDataStore>,
        provider: Option<Arc<dyn MarketDataProvider>>,
    ) -> Self {
        Self {
            store,
            provider,
            equity_router: None,
            crypto_router: None,
            fx_router: None,
            lookback_days: 7,
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

    pub async fn price_close(&self, asset: &Asset, date: NaiveDate) -> Result<PricePoint> {
        let asset_id = AssetId::from_asset(asset);
        for offset in 0..=self.lookback_days {
            let target_date = date - Duration::days(offset as i64);
            if let Some(price) = self
                .store
                .get_price(&asset_id, target_date, PriceKind::Close)
                .await?
            {
                return Ok(price);
            }

            if let Some(price) = self.fetch_price_from_sources(asset, &asset_id, target_date).await?
            {
                self.store.put_prices(&[price.clone()]).await?;
                return Ok(price);
            }
        }

        Err(anyhow::anyhow!(
            "No close price found for asset {} on or before {}",
            asset_id,
            date
        ))
    }

    pub async fn fx_close(&self, base: &str, quote: &str, date: NaiveDate) -> Result<FxRatePoint> {
        for offset in 0..=self.lookback_days {
            let target_date = date - Duration::days(offset as i64);
            if let Some(rate) = self
                .store
                .get_fx_rate(base, quote, target_date, FxRateKind::Close)
                .await?
            {
                return Ok(rate);
            }

            if let Some(rate) = self.fetch_fx_from_sources(base, quote, target_date).await? {
                self.store.put_fx_rates(&[rate.clone()]).await?;
                return Ok(rate);
            }
        }

        Err(anyhow::anyhow!(
            "No FX rate found for {}->{} on or before {}",
            base,
            quote,
            date
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
