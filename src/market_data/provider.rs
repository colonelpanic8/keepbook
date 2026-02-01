use anyhow::Result;
use chrono::NaiveDate;

use super::{AssetId, FxRatePoint, PricePoint};
use crate::models::Asset;

#[async_trait::async_trait]
pub trait MarketDataProvider: Send + Sync {
    async fn fetch_price(
        &self,
        asset: &Asset,
        asset_id: &AssetId,
        date: NaiveDate,
    ) -> Result<Option<PricePoint>>;

    async fn fetch_fx_rate(&self, base: &str, quote: &str, date: NaiveDate)
        -> Result<Option<FxRatePoint>>;

    fn name(&self) -> &str;
}

pub struct NoopProvider;

#[async_trait::async_trait]
impl MarketDataProvider for NoopProvider {
    async fn fetch_price(
        &self,
        _asset: &Asset,
        _asset_id: &AssetId,
        _date: NaiveDate,
    ) -> Result<Option<PricePoint>> {
        Ok(None)
    }

    async fn fetch_fx_rate(
        &self,
        _base: &str,
        _quote: &str,
        _date: NaiveDate,
    ) -> Result<Option<FxRatePoint>> {
        Ok(None)
    }

    fn name(&self) -> &str {
        "noop"
    }
}
