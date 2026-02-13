use std::sync::Arc;

use anyhow::Result;
use chrono::NaiveDate;
use keepbook::market_data::{MarketDataService, NullMarketDataStore};
use keepbook::models::{
    Account, AccountConfig, BalanceBackfillPolicy, Connection, ConnectionConfig,
};
use keepbook::portfolio::{Grouping, PortfolioQuery, PortfolioService};
use keepbook::storage::{JsonFileStorage, Storage};

#[tokio::test]
async fn json_storage_reads_account_config_for_zero_backfill() -> Result<()> {
    let dir = tempfile::TempDir::new()?;
    let storage = Arc::new(JsonFileStorage::new(dir.path()));

    let connection = Connection::new(ConnectionConfig {
        name: "Test".to_string(),
        synchronizer: "manual".to_string(),
        credentials: None,
        balance_staleness: None,
    });
    storage
        .save_connection_config(connection.id(), &connection.config)
        .await?;
    storage.save_connection(&connection).await?;

    let account = Account::new("Checking", connection.id().clone());
    storage.save_account(&account).await?;
    storage
        .save_account_config(
            &account.id,
            &AccountConfig {
                balance_staleness: None,
                balance_backfill: Some(BalanceBackfillPolicy::Zero),
            },
        )
        .await?;

    let market_data = Arc::new(MarketDataService::new(Arc::new(NullMarketDataStore), None));
    let service = PortfolioService::new(storage, market_data);

    let query = PortfolioQuery {
        as_of_date: NaiveDate::from_ymd_opt(2026, 2, 5).unwrap(),
        currency: "USD".to_string(),
        currency_decimals: None,
        grouping: Grouping::Account,
        include_detail: false,
    };

    let snapshot = service.calculate(&query).await?;
    assert_eq!(snapshot.total_value, "0");

    let by_account = snapshot.by_account.expect("by_account should be present");
    assert_eq!(by_account.len(), 1);
    assert_eq!(by_account[0].account_name, "Checking");
    assert_eq!(by_account[0].value_in_base.as_deref(), Some("0"));

    Ok(())
}
