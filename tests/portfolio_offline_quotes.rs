use anyhow::Result;
use chrono::Utc;
use keepbook::app::portfolio_snapshot;
use keepbook::config::{GitConfig, RefreshConfig, ResolvedConfig};
use keepbook::market_data::{AssetId, JsonlMarketDataStore, MarketDataStore, PriceKind, PricePoint};
use keepbook::models::{Account, Asset, AssetBalance, BalanceSnapshot, Connection, ConnectionConfig};
use keepbook::storage::{JsonFileStorage, Storage};
use tempfile::TempDir;

async fn write_connection_config(
    storage: &JsonFileStorage,
    connection: &Connection,
) -> Result<()> {
    storage
        .save_connection_config(connection.id(), &connection.config)
        .await?;
    Ok(())
}

#[tokio::test]
async fn portfolio_snapshot_offline_uses_cached_quote() -> Result<()> {
    let dir = TempDir::new()?;
    let storage = JsonFileStorage::new(dir.path());

    let config = ResolvedConfig {
        data_dir: dir.path().to_path_buf(),
        reporting_currency: "USD".to_string(),
        refresh: RefreshConfig::default(),
        git: GitConfig::default(),
    };

    let connection = Connection::new(ConnectionConfig {
        name: "Test Bank".to_string(),
        synchronizer: "manual".to_string(),
        credentials: None,
        balance_staleness: None,
    });

    write_connection_config(&storage, &connection).await?;
    storage.save_connection(&connection).await?;

    let account = Account::new("Brokerage", connection.id().clone());
    storage.save_account(&account).await?;

    let asset = Asset::equity("AAPL");
    let snapshot = BalanceSnapshot::now(vec![AssetBalance::new(asset.clone(), "2")]);
    storage.append_balance_snapshot(&account.id, &snapshot).await?;

    let price = PricePoint {
        asset_id: AssetId::from_asset(&asset),
        as_of_date: Utc::now().date_naive(),
        timestamp: Utc::now(),
        price: "100".to_string(),
        quote_currency: "USD".to_string(),
        kind: PriceKind::Quote,
        source: "test".to_string(),
    };

    let market_data_store = JsonlMarketDataStore::new(dir.path());
    market_data_store
        .put_prices(std::slice::from_ref(&price))
        .await?;

    let snapshot = portfolio_snapshot(
        &storage,
        &config,
        None,
        Some(Utc::now().date_naive().to_string()),
        "asset".to_string(),
        false,
        false,
        true,
        false,
        false,
    )
    .await?;

    assert_eq!(snapshot.total_value, "200");

    Ok(())
}
