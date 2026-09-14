use anyhow::Result;
use chrono::{TimeZone, Utc};
use keepbook::market_data::{
    AssetId, JsonlMarketDataStore, MarketDataStore, PriceKind, PricePoint,
};
use tempfile::TempDir;

#[tokio::test]
async fn jsonl_store_rejects_unsafe_asset_paths() -> Result<()> {
    let dir = TempDir::new()?;
    let store = JsonlMarketDataStore::new(dir.path().join("market-data"));
    let timestamp = Utc.with_ymd_and_hms(2026, 2, 6, 18, 0, 0).unwrap();
    let outside = dir.path().join("outside");
    let outside = outside.to_str().unwrap();
    for raw in [
        "",
        ".",
        "..",
        "../../outside",
        "equity/../../outside",
        outside,
        "equity/./AAPL",
        "equity//AAPL",
        "equity/AAPL/",
        "equity\\AAPL",
        "equity/AA\0PL",
    ] {
        let asset_id: AssetId = serde_json::from_value(serde_json::json!(raw))?;
        assert!(store.get_all_prices(&asset_id).await.is_err(), "{raw:?}");
        assert!(
            store
                .get_price(&asset_id, timestamp.date_naive(), PriceKind::Close)
                .await
                .is_err(),
            "{raw:?}"
        );
        let price = PricePoint {
            asset_id,
            as_of_date: timestamp.date_naive(),
            timestamp,
            price: "200".into(),
            quote_currency: "USD".into(),
            kind: PriceKind::Close,
            source: "test".into(),
        };
        assert!(store.put_prices(&[price]).await.is_err(), "{raw:?}");
    }
    assert!(!dir.path().join("outside").exists());
    Ok(())
}
