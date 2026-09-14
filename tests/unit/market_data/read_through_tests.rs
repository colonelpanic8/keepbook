use super::*;
use crate::market_data::{MemoryMarketDataStore, PriceKind};
use chrono::{TimeZone, Utc};
use std::sync::atomic::{AtomicUsize, Ordering};

/// Counts how often the whole price history is read.
struct CountingStore {
    inner: MemoryMarketDataStore,
    reads: AtomicUsize,
}

#[async_trait::async_trait]
impl MarketDataStore for CountingStore {
    async fn get_price(
        &self,
        asset_id: &AssetId,
        date: chrono::NaiveDate,
        kind: PriceKind,
    ) -> Result<Option<PricePoint>> {
        self.inner.get_price(asset_id, date, kind).await
    }

    async fn get_all_prices(&self, asset_id: &AssetId) -> Result<Vec<PricePoint>> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        self.inner.get_all_prices(asset_id).await
    }

    async fn put_prices(&self, prices: &[PricePoint]) -> Result<()> {
        self.inner.put_prices(prices).await
    }

    async fn get_fx_rate(
        &self,
        base: &str,
        quote: &str,
        date: chrono::NaiveDate,
        kind: FxRateKind,
    ) -> Result<Option<FxRatePoint>> {
        self.inner.get_fx_rate(base, quote, date, kind).await
    }

    async fn get_all_fx_rates(&self, base: &str, quote: &str) -> Result<Vec<FxRatePoint>> {
        self.inner.get_all_fx_rates(base, quote).await
    }

    async fn put_fx_rates(&self, rates: &[FxRatePoint]) -> Result<()> {
        self.inner.put_fx_rates(rates).await
    }

    async fn get_asset_entry(&self, asset_id: &AssetId) -> Result<Option<AssetRegistryEntry>> {
        self.inner.get_asset_entry(asset_id).await
    }

    async fn upsert_asset_entry(&self, entry: &AssetRegistryEntry) -> Result<()> {
        self.inner.upsert_asset_entry(entry).await
    }
}

fn price(value: &str) -> PricePoint {
    PricePoint {
        asset_id: AssetId::from_asset(&crate::models::Asset::equity("AAPL")),
        as_of_date: Utc
            .with_ymd_and_hms(2026, 2, 1, 0, 0, 0)
            .unwrap()
            .date_naive(),
        timestamp: Utc.with_ymd_and_hms(2026, 2, 1, 12, 0, 0).unwrap(),
        price: value.to_string(),
        quote_currency: "USD".to_string(),
        kind: PriceKind::Close,
        source: "test".to_string(),
    }
}

#[tokio::test]
async fn repeated_reads_of_a_price_history_hit_the_store_once() -> Result<()> {
    let counting = Arc::new(CountingStore {
        inner: MemoryMarketDataStore::new(),
        reads: AtomicUsize::new(0),
    });
    counting.put_prices(&[price("100")]).await?;
    let store = ReadThroughMarketDataStore::new(counting.clone());
    let asset_id = price("100").asset_id;

    for _ in 0..5 {
        assert_eq!(store.get_all_prices(&asset_id).await?.len(), 1);
    }
    assert_eq!(counting.reads.load(Ordering::SeqCst), 1);

    // A write invalidates what was memoized.
    store.put_prices(&[price("150")]).await?;
    let prices = store.get_all_prices(&asset_id).await?;
    assert_eq!(prices.len(), 1);
    assert_eq!(prices[0].price, "150");
    assert_eq!(counting.reads.load(Ordering::SeqCst), 2);

    Ok(())
}
