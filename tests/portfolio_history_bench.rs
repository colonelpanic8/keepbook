//! Timing harness for portfolio history over a representative dataset.
//!
//! Ignored by default; run it when changing how history loads balances or
//! prices:
//!
//! ```sh
//! cargo test --test portfolio_history_bench -- --ignored --nocapture
//! ```
use std::sync::Arc;
use std::time::Instant;

use anyhow::Result;
use chrono::{Duration, TimeZone, Utc};
use keepbook::app::portfolio_history;
use keepbook::config::{
    AiConfig, DisplayConfig, GitConfig, HistoryConfig, IgnoreConfig, PortfolioConfig,
    RefreshConfig, ResolvedConfig, SpendingConfig, TrayConfig,
};
use keepbook::market_data::{
    AssetId, JsonlMarketDataStore, MarketDataStore, PriceKind, PricePoint,
};
use keepbook::models::{
    Account, Asset, AssetBalance, BalanceSnapshot, Connection, ConnectionConfig,
};
use keepbook::storage::{JsonFileStorage, Storage};
use tempfile::TempDir;

const ACCOUNTS: usize = 20;
const SNAPSHOTS_PER_ACCOUNT: usize = 120;
const ASSETS: usize = 10;
const PRICE_DAYS: usize = 1000;

#[tokio::test]
#[ignore = "timing harness, not a pass/fail test"]
async fn time_portfolio_history_over_a_representative_dataset() -> Result<()> {
    let dir = TempDir::new()?;
    let config = ResolvedConfig {
        data_dir: dir.path().to_path_buf(),
        reporting_currency: "USD".to_string(),
        display: DisplayConfig::default(),
        refresh: RefreshConfig::default(),
        history: HistoryConfig::default(),
        tray: TrayConfig::default(),
        spending: SpendingConfig::default(),
        tags: Default::default(),
        portfolio: PortfolioConfig::default(),
        ignore: IgnoreConfig::default(),
        ai: AiConfig::default(),
        git: GitConfig::default(),
    };

    let storage = JsonFileStorage::new(dir.path());
    let connection = Connection::new(ConnectionConfig {
        name: "Bench Broker".into(),
        synchronizer: "manual".into(),
        credentials: None,
        balance_staleness: None,
    });
    storage.save_connection(&connection).await?;

    let assets: Vec<Asset> = (0..ASSETS)
        .map(|index| Asset::equity(format!("SYM{index}")))
        .collect();
    let start = Utc.with_ymd_and_hms(2022, 1, 1, 12, 0, 0).unwrap();

    for account_index in 0..ACCOUNTS {
        let account = Account::new(format!("Account {account_index}"), connection.id().clone());
        storage.save_account(&account).await?;
        for snapshot_index in 0..SNAPSHOTS_PER_ACCOUNT {
            let balances: Vec<AssetBalance> = assets
                .iter()
                .map(|asset| AssetBalance::new(asset.clone(), (snapshot_index + 1).to_string()))
                .collect();
            storage
                .append_balance_snapshot(
                    &account.id,
                    &BalanceSnapshot::new(
                        start + Duration::days(snapshot_index as i64 * 3),
                        balances,
                    ),
                )
                .await?;
        }
    }

    let store = JsonlMarketDataStore::new(dir.path());
    for asset in &assets {
        let prices: Vec<PricePoint> = (0..PRICE_DAYS)
            .map(|day| {
                let timestamp = start + Duration::days(day as i64);
                PricePoint {
                    asset_id: AssetId::from_asset(asset),
                    as_of_date: timestamp.date_naive(),
                    timestamp,
                    price: (100 + day % 50).to_string(),
                    quote_currency: "USD".to_string(),
                    kind: PriceKind::Close,
                    source: "bench".to_string(),
                }
            })
            .collect();
        store.put_prices(&prices).await?;
    }

    let storage: Arc<dyn Storage> = Arc::new(storage);
    for granularity in ["monthly", "weekly", "daily"] {
        let began = Instant::now();
        let output = portfolio_history(
            storage.clone(),
            &config,
            None,
            None,
            None,
            granularity.to_string(),
            false,
        )
        .await?;
        let elapsed = began.elapsed();
        println!(
            "{granularity}: {} points over {ACCOUNTS} accounts x {ASSETS} assets in {elapsed:.2?} ({:.1?}/point)",
            output.points.len(),
            elapsed / output.points.len().max(1) as u32,
        );
    }

    Ok(())
}
