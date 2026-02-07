use anyhow::Result;
use chrono::NaiveDate;
use keepbook::market_data::{
    AssetId, JsonlMarketDataStore, MarketDataService, MarketDataStore, PriceKind, PricePoint,
};
use keepbook::models::Asset;
use std::sync::Arc;
use tempfile::TempDir;

#[tokio::test]
async fn store_price_is_idempotent_for_jsonl_store() -> Result<()> {
    let dir = TempDir::new()?;
    let store = Arc::new(JsonlMarketDataStore::new(dir.path()));
    let svc = MarketDataService::new(store.clone(), None);

    let asset = Asset::equity("AAPL");
    let date = NaiveDate::from_ymd_opt(2024, 1, 2).unwrap();
    let price = PricePoint {
        asset_id: AssetId::from_asset(&asset),
        as_of_date: date,
        timestamp: chrono::Utc::now(),
        price: "189.50".to_string(),
        quote_currency: "USD".to_string(),
        kind: PriceKind::Close,
        source: "mock".to_string(),
    };

    svc.store_price(&price).await?;
    svc.store_price(&price).await?;

    let all = store.get_all_prices(&price.asset_id).await?;
    assert_eq!(all.len(), 1, "store_price should not append duplicates");
    Ok(())
}
