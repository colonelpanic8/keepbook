use anyhow::Result;
use chrono::NaiveDate;
use keepbook::app::store_sync_prices;
use keepbook::config::{GitConfig, RefreshConfig, ResolvedConfig};
use keepbook::market_data::{AssetId, MarketDataStore, PriceKind, PricePoint};
use keepbook::models::{Account, Asset, Connection, ConnectionConfig};
use keepbook::market_data::JsonlMarketDataStore;
use keepbook::sync::{SyncResult, SyncedAssetBalance};
use tempfile::TempDir;

#[tokio::test]
async fn store_sync_prices_persists_price_points() -> Result<()> {
    let dir = TempDir::new()?;
    let config = ResolvedConfig {
        data_dir: dir.path().to_path_buf(),
        reporting_currency: "USD".to_string(),
        refresh: RefreshConfig::default(),
        git: GitConfig::default(),
    };

    let mut connection = Connection::new(ConnectionConfig {
        name: "Test".to_string(),
        synchronizer: "mock".to_string(),
        credentials: None,
        balance_staleness: None,
    });
    let account = Account::new("Checking", connection.id().clone());
    connection.state.account_ids = vec![account.id.clone()];

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

    let balance = SyncedAssetBalance::new(keepbook::models::AssetBalance::new(
        asset,
        "1",
    ))
    .with_price(price.clone());

    let result = SyncResult {
        connection: connection.clone(),
        accounts: vec![account.clone()],
        balances: vec![(account.id.clone(), vec![balance])],
        transactions: Vec::new(),
    };

    let stored = store_sync_prices(&result, &config).await?;
    assert_eq!(stored, 1);

    let store = JsonlMarketDataStore::new(&config.data_dir);
    let loaded = store
        .get_price(&price.asset_id, date, PriceKind::Close)
        .await?
        .expect("price should be stored");
    assert_eq!(loaded.price, "189.50");
    assert_eq!(loaded.source, "mock");

    Ok(())
}
