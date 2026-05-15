use super::super::Grouping;
use super::*;
use crate::market_data::{AssetId, EquityPriceRouter, EquityPriceSource, PriceKind, PricePoint};
use crate::market_data::{MarketDataStore, MemoryMarketDataStore};
use crate::models::{
    Account, AccountConfig, Asset, AssetBalance, BalanceBackfillPolicy, BalanceSnapshot,
    Connection, ConnectionConfig,
};
use crate::storage::MemoryStorage;
use chrono::{TimeZone, Utc};
use rust_decimal::Decimal;
use std::sync::Arc;

#[test]
fn build_asset_summaries_errors_on_missing_price_cache_entry() {
    let storage = Arc::new(MemoryStorage::new());
    let store = Arc::new(MemoryMarketDataStore::new());
    let market_data = Arc::new(MarketDataService::new(store, None));
    let service = PortfolioService::new(storage, market_data);

    let asset = Asset::equity("AAPL");
    let mut by_asset: std::collections::HashMap<Asset, super::AssetAggregate> =
        std::collections::HashMap::new();
    by_asset.insert(
        asset.clone(),
        super::AssetAggregate {
            total_amount: Decimal::ONE,
            amount_with_cost_basis: Decimal::ZERO,
            total_cost_basis: None,
            latest_balance_date: chrono::NaiveDate::from_ymd_opt(2026, 2, 1).unwrap(),
            holdings: Vec::new(),
        },
    );

    let price_cache: std::collections::HashMap<Asset, super::AssetValuation> =
        std::collections::HashMap::new();
    let account_map: std::collections::HashMap<Id, Account> = std::collections::HashMap::new();

    let err = service
        .build_asset_summaries(&by_asset, &price_cache, &account_map, false, None, None)
        .unwrap_err();
    assert!(err.to_string().contains("missing valuation"));
}

#[test]
fn build_account_summaries_errors_on_missing_price_cache_entry() {
    let asset = Asset::equity("AAPL");
    let snapshot = BalanceSnapshot::new(
        Utc.with_ymd_and_hms(2026, 2, 1, 12, 0, 0).unwrap(),
        vec![AssetBalance::new(asset.clone(), "1")],
    );

    let snapshots = vec![(Id::from_string("acct-1"), snapshot)];
    let price_cache: std::collections::HashMap<Asset, super::AssetValuation> =
        std::collections::HashMap::new();
    let account_map: std::collections::HashMap<Id, Account> = std::collections::HashMap::new();
    let connection_map: std::collections::HashMap<Id, Connection> =
        std::collections::HashMap::new();

    let err = PortfolioService::build_account_summaries(
        &snapshots,
        &[],
        &price_cache,
        &account_map,
        &connection_map,
        None,
    )
    .unwrap_err();
    assert!(err.to_string().contains("missing valuation"));
}

#[tokio::test]
async fn calculate_single_currency_holding() -> Result<()> {
    // Setup storage with one account holding USD
    let storage = Arc::new(MemoryStorage::new());
    let connection = Connection::new(ConnectionConfig {
        name: "Test Bank".to_string(),
        synchronizer: "manual".to_string(),
        credentials: None,
        balance_staleness: None,
    });
    storage.save_connection(&connection).await?;

    let account = Account::new("Checking", connection.id().clone());
    storage.save_account(&account).await?;

    let snapshot = BalanceSnapshot::new(
        Utc.with_ymd_and_hms(2026, 2, 1, 12, 0, 0).unwrap(),
        vec![AssetBalance::new(Asset::currency("USD"), "1000.00")],
    );
    storage
        .append_balance_snapshot(&account.id, &snapshot)
        .await?;

    // Setup market data (no prices needed for USD->USD)
    let store = Arc::new(MemoryMarketDataStore::new());
    let market_data = Arc::new(MarketDataService::new(store, None));

    // Calculate
    let service = PortfolioService::new(storage, market_data);
    let query = PortfolioQuery {
        as_of_date: chrono::NaiveDate::from_ymd_opt(2026, 2, 2).unwrap(),
        currency: "USD".to_string(),
        currency_decimals: None,
        grouping: Grouping::Both,
        include_detail: false,
        capital_gains_tax_rate: None,
        equity_valuation_adjustment: None,
        account_ids: Vec::new(),
    };
    let result = service.calculate(&query).await?;

    // Decimal::normalize() removes trailing zeros, so "1000.00" becomes "1000"
    assert_eq!(result.total_value, "1000");
    assert_eq!(result.currency, "USD");
    Ok(())
}

#[tokio::test]
async fn calculate_with_equity_and_fx() -> Result<()> {
    use crate::market_data::{AssetId, FxRateKind, FxRatePoint, PriceKind, PricePoint};

    let storage = Arc::new(MemoryStorage::new());
    let connection = Connection::new(ConnectionConfig {
        name: "Broker".to_string(),
        synchronizer: "manual".to_string(),
        credentials: None,
        balance_staleness: None,
    });
    storage.save_connection(&connection).await?;

    let account = Account::new("Brokerage", connection.id().clone());
    storage.save_account(&account).await?;

    // 10 shares of AAPL
    let snapshot = BalanceSnapshot::new(
        Utc.with_ymd_and_hms(2026, 2, 1, 12, 0, 0).unwrap(),
        vec![AssetBalance::new(Asset::equity("AAPL"), "10")],
    );
    storage
        .append_balance_snapshot(&account.id, &snapshot)
        .await?;

    // Setup market data with AAPL at $200 and USD/EUR at 0.91
    let store = Arc::new(MemoryMarketDataStore::new());
    let as_of_date = chrono::NaiveDate::from_ymd_opt(2026, 2, 1).unwrap();

    // Store AAPL price
    let aapl_price = PricePoint {
        asset_id: AssetId::from_asset(&Asset::equity("AAPL")),
        as_of_date,
        timestamp: Utc::now(),
        price: "200".to_string(),
        quote_currency: "USD".to_string(),
        kind: PriceKind::Close,
        source: "test".to_string(),
    };
    store.put_prices(&[aapl_price]).await?;

    // Store USD->EUR FX rate
    let fx_rate = FxRatePoint {
        base: "USD".to_string(),
        quote: "EUR".to_string(),
        as_of_date,
        timestamp: Utc::now(),
        rate: "0.91".to_string(),
        kind: FxRateKind::Close,
        source: "test".to_string(),
    };
    store.put_fx_rates(&[fx_rate]).await?;

    let market_data = Arc::new(MarketDataService::new(store, None));

    // Calculate in EUR
    let service = PortfolioService::new(storage, market_data);
    let query = PortfolioQuery {
        as_of_date: chrono::NaiveDate::from_ymd_opt(2026, 2, 2).unwrap(),
        currency: "EUR".to_string(),
        currency_decimals: None,
        grouping: Grouping::Asset,
        include_detail: false,
        capital_gains_tax_rate: None,
        equity_valuation_adjustment: None,
        account_ids: Vec::new(),
    };
    let result = service.calculate(&query).await?;

    // 10 shares * $200 = $2000 * 0.91 = 1820 EUR
    assert_eq!(result.total_value, "1820");
    assert_eq!(result.currency, "EUR");

    // Check asset summary
    let by_asset = result.by_asset.unwrap();
    assert_eq!(by_asset.len(), 1);
    assert_eq!(by_asset[0].total_amount, "10");
    assert_eq!(by_asset[0].price, Some("200".to_string()));
    assert_eq!(by_asset[0].fx_rate, Some("0.91".to_string()));
    assert_eq!(by_asset[0].value_in_base, Some("1820".to_string()));

    Ok(())
}

#[tokio::test]
async fn calculate_reports_unrealized_gain_and_tax_from_cost_basis() -> Result<()> {
    let storage = Arc::new(MemoryStorage::new());
    let connection = Connection::new(ConnectionConfig {
        name: "Broker".to_string(),
        synchronizer: "manual".to_string(),
        credentials: None,
        balance_staleness: None,
    });
    storage.save_connection(&connection).await?;

    let account = Account::new("Brokerage", connection.id().clone());
    storage.save_account(&account).await?;

    let snapshot = BalanceSnapshot::new(
        Utc.with_ymd_and_hms(2026, 2, 1, 12, 0, 0).unwrap(),
        vec![AssetBalance::new(Asset::equity("AAPL"), "10").with_cost_basis("1500")],
    );
    storage
        .append_balance_snapshot(&account.id, &snapshot)
        .await?;

    let store = Arc::new(MemoryMarketDataStore::new());
    let as_of_date = chrono::NaiveDate::from_ymd_opt(2026, 2, 1).unwrap();
    store
        .put_prices(&[PricePoint {
            asset_id: AssetId::from_asset(&Asset::equity("AAPL")),
            as_of_date,
            timestamp: Utc::now(),
            price: "200".to_string(),
            quote_currency: "USD".to_string(),
            kind: PriceKind::Close,
            source: "test".to_string(),
        }])
        .await?;

    let market_data = Arc::new(MarketDataService::new(store, None));
    let service = PortfolioService::new(storage, market_data);
    let query = PortfolioQuery {
        as_of_date: chrono::NaiveDate::from_ymd_opt(2026, 2, 2).unwrap(),
        currency: "USD".to_string(),
        currency_decimals: None,
        grouping: Grouping::Asset,
        include_detail: true,
        capital_gains_tax_rate: Some(Decimal::new(238, 3)),
        equity_valuation_adjustment: None,
        account_ids: Vec::new(),
    };
    let result = service.calculate(&query).await?;

    assert_eq!(result.total_value, "2000");
    assert_eq!(result.total_cost_basis, Some("1500".to_string()));
    assert_eq!(result.total_unrealized_gain, Some("500".to_string()));
    assert_eq!(
        result.prospective_capital_gains_tax,
        Some("119".to_string())
    );

    let asset = &result.by_asset.unwrap()[0];
    assert_eq!(asset.cost_basis, Some("1500".to_string()));
    assert_eq!(asset.unrealized_gain, Some("500".to_string()));
    assert_eq!(asset.prospective_capital_gains_tax, Some("119".to_string()));
    assert_eq!(
        asset.holdings.as_ref().unwrap()[0].unrealized_gain,
        Some("500".to_string())
    );

    Ok(())
}

#[tokio::test]
async fn calculate_can_scale_equities_to_target_pre_tax_total_value() -> Result<()> {
    let storage = Arc::new(MemoryStorage::new());
    let connection = Connection::new(ConnectionConfig {
        name: "Broker".to_string(),
        synchronizer: "manual".to_string(),
        credentials: None,
        balance_staleness: None,
    });
    storage.save_connection(&connection).await?;

    let account = Account::new("Brokerage", connection.id().clone());
    storage.save_account(&account).await?;

    let snapshot = BalanceSnapshot::new(
        Utc.with_ymd_and_hms(2026, 2, 1, 12, 0, 0).unwrap(),
        vec![
            AssetBalance::new(Asset::currency("USD"), "1000"),
            AssetBalance::new(Asset::equity("AAPL"), "10").with_cost_basis("1500"),
        ],
    );
    storage
        .append_balance_snapshot(&account.id, &snapshot)
        .await?;

    let store = Arc::new(MemoryMarketDataStore::new());
    let as_of_date = chrono::NaiveDate::from_ymd_opt(2026, 2, 1).unwrap();
    store
        .put_prices(&[PricePoint {
            asset_id: AssetId::from_asset(&Asset::equity("AAPL")),
            as_of_date,
            timestamp: Utc::now(),
            price: "200".to_string(),
            quote_currency: "USD".to_string(),
            kind: PriceKind::Close,
            source: "test".to_string(),
        }])
        .await?;

    let market_data = Arc::new(MarketDataService::new(store, None));
    let service = PortfolioService::new(storage, market_data);
    let query = PortfolioQuery {
        as_of_date: chrono::NaiveDate::from_ymd_opt(2026, 2, 2).unwrap(),
        currency: "USD".to_string(),
        currency_decimals: None,
        grouping: Grouping::Both,
        include_detail: true,
        capital_gains_tax_rate: Some(Decimal::new(23, 2)),
        equity_valuation_adjustment: Some(EquityValuationAdjustment::TargetPreTaxTotalValue(
            Decimal::from(2600),
        )),
        account_ids: Vec::new(),
    };
    let result = service.calculate(&query).await?;

    assert_eq!(result.total_value, "2600");
    assert_eq!(result.total_cost_basis, Some("1500".to_string()));
    assert_eq!(result.total_unrealized_gain, Some("100".to_string()));
    assert_eq!(result.prospective_capital_gains_tax, Some("23".to_string()));
    assert_eq!(
        result.valuation_scenario.as_ref().map(|s| (
            s.equity_multiplier.as_str(),
            s.equity_change_percent.as_str(),
            s.pre_tax_total_value.as_str(),
            s.equity_value_before.as_str(),
            s.equity_value_after.as_str(),
            s.target_pre_tax_total_value.as_deref(),
        )),
        Some(("0.8", "-20", "2600", "2000", "1600", Some("2600")))
    );

    let by_asset = result.by_asset.unwrap();
    let equity = by_asset
        .iter()
        .find(|summary| matches!(summary.asset, Asset::Equity { .. }))
        .expect("equity summary");
    assert_eq!(equity.price, Some("160".to_string()));
    assert_eq!(equity.value_in_base, Some("1600".to_string()));
    assert_eq!(equity.unrealized_gain, Some("100".to_string()));
    assert_eq!(
        result.by_account.unwrap()[0].value_in_base,
        Some("2600".to_string())
    );

    Ok(())
}

#[tokio::test]
async fn calculate_with_detail() -> Result<()> {
    let storage = Arc::new(MemoryStorage::new());
    let connection = Connection::new(ConnectionConfig {
        name: "Bank".to_string(),
        synchronizer: "manual".to_string(),
        credentials: None,
        balance_staleness: None,
    });
    storage.save_connection(&connection).await?;

    // Create two accounts
    let account1 = Account::new("Checking", connection.id().clone());
    let account2 = Account::new("Savings", connection.id().clone());
    storage.save_account(&account1).await?;
    storage.save_account(&account2).await?;

    // Add USD balances to both accounts
    let snapshot1 = BalanceSnapshot::new(
        Utc.with_ymd_and_hms(2026, 2, 1, 12, 0, 0).unwrap(),
        vec![AssetBalance::new(Asset::currency("USD"), "1000")],
    );
    let snapshot2 = BalanceSnapshot::new(
        Utc.with_ymd_and_hms(2026, 2, 1, 14, 0, 0).unwrap(),
        vec![AssetBalance::new(Asset::currency("USD"), "2000")],
    );
    storage
        .append_balance_snapshot(&account1.id, &snapshot1)
        .await?;
    storage
        .append_balance_snapshot(&account2.id, &snapshot2)
        .await?;

    let store = Arc::new(MemoryMarketDataStore::new());
    let market_data = Arc::new(MarketDataService::new(store, None));

    let service = PortfolioService::new(storage, market_data);
    let query = PortfolioQuery {
        as_of_date: chrono::NaiveDate::from_ymd_opt(2026, 2, 2).unwrap(),
        currency: "USD".to_string(),
        currency_decimals: None,
        grouping: Grouping::Asset,
        include_detail: true,
        capital_gains_tax_rate: None,
        equity_valuation_adjustment: None,
        account_ids: Vec::new(),
    };
    let result = service.calculate(&query).await?;

    // Total should be 3000
    assert_eq!(result.total_value, "3000");

    // Check asset summary with holdings detail
    let by_asset = result.by_asset.unwrap();
    assert_eq!(by_asset.len(), 1);
    assert_eq!(by_asset[0].total_amount, "3000");

    // Check holdings detail
    let holdings = by_asset[0].holdings.as_ref().unwrap();
    assert_eq!(holdings.len(), 2);

    // Find the checking and savings holdings
    let checking_holding = holdings.iter().find(|h| h.account_name == "Checking");
    let savings_holding = holdings.iter().find(|h| h.account_name == "Savings");

    assert!(checking_holding.is_some());
    assert!(savings_holding.is_some());
    assert_eq!(checking_holding.unwrap().amount, "1000");
    assert_eq!(savings_holding.unwrap().amount, "2000");

    Ok(())
}

#[tokio::test]
async fn calculate_merges_case_insensitive_assets() -> Result<()> {
    let storage = Arc::new(MemoryStorage::new());
    let connection = Connection::new(ConnectionConfig {
        name: "Bank".to_string(),
        synchronizer: "manual".to_string(),
        credentials: None,
        balance_staleness: None,
    });
    storage.save_connection(&connection).await?;

    let account1 = Account::new("Checking", connection.id().clone());
    let account2 = Account::new("Savings", connection.id().clone());
    storage.save_account(&account1).await?;
    storage.save_account(&account2).await?;

    let snapshot1 = BalanceSnapshot::new(
        Utc.with_ymd_and_hms(2026, 2, 1, 12, 0, 0).unwrap(),
        vec![AssetBalance::new(Asset::currency("USD"), "1000")],
    );
    let snapshot2 = BalanceSnapshot::new(
        Utc.with_ymd_and_hms(2026, 2, 1, 14, 0, 0).unwrap(),
        vec![AssetBalance::new(Asset::currency(" usd "), "2000")],
    );
    storage
        .append_balance_snapshot(&account1.id, &snapshot1)
        .await?;
    storage
        .append_balance_snapshot(&account2.id, &snapshot2)
        .await?;

    let store = Arc::new(MemoryMarketDataStore::new());
    let market_data = Arc::new(MarketDataService::new(store, None));
    let service = PortfolioService::new(storage, market_data);

    let query = PortfolioQuery {
        as_of_date: chrono::NaiveDate::from_ymd_opt(2026, 2, 2).unwrap(),
        currency: "USD".to_string(),
        currency_decimals: None,
        grouping: Grouping::Asset,
        include_detail: false,
        capital_gains_tax_rate: None,
        equity_valuation_adjustment: None,
        account_ids: Vec::new(),
    };
    let result = service.calculate(&query).await?;

    let by_asset = result.by_asset.unwrap();
    assert_eq!(by_asset.len(), 1);
    assert_eq!(by_asset[0].total_amount, "3000");
    match &by_asset[0].asset {
        Asset::Currency { iso_code } => assert_eq!(iso_code, "USD"),
        _ => panic!("expected currency asset"),
    }

    Ok(())
}

#[tokio::test]
async fn calculate_uses_latest_snapshot_before_date() -> Result<()> {
    let storage = Arc::new(MemoryStorage::new());
    let connection = Connection::new(ConnectionConfig {
        name: "Test Bank".to_string(),
        synchronizer: "manual".to_string(),
        credentials: None,
        balance_staleness: None,
    });
    storage.save_connection(&connection).await?;

    let account = Account::new("Checking", connection.id().clone());
    storage.save_account(&account).await?;

    let older_snapshot = BalanceSnapshot::new(
        Utc.with_ymd_and_hms(2026, 2, 1, 12, 0, 0).unwrap(),
        vec![AssetBalance::new(Asset::currency("USD"), "1000")],
    );
    let newer_snapshot = BalanceSnapshot::new(
        Utc.with_ymd_and_hms(2026, 2, 3, 12, 0, 0).unwrap(),
        vec![AssetBalance::new(Asset::currency("USD"), "2000")],
    );
    storage
        .append_balance_snapshot(&account.id, &older_snapshot)
        .await?;
    storage
        .append_balance_snapshot(&account.id, &newer_snapshot)
        .await?;

    let store = Arc::new(MemoryMarketDataStore::new());
    let market_data = Arc::new(MarketDataService::new(store, None));
    let service = PortfolioService::new(storage, market_data);

    let query = PortfolioQuery {
        as_of_date: chrono::NaiveDate::from_ymd_opt(2026, 2, 2).unwrap(),
        currency: "USD".to_string(),
        currency_decimals: None,
        grouping: Grouping::Both,
        include_detail: false,
        capital_gains_tax_rate: None,
        equity_valuation_adjustment: None,
        account_ids: Vec::new(),
    };

    let result = service.calculate(&query).await?;
    assert_eq!(result.total_value, "1000");
    Ok(())
}

#[tokio::test]
async fn calculate_zero_backfill() -> Result<()> {
    let storage = Arc::new(MemoryStorage::new());
    let connection = Connection::new(ConnectionConfig {
        name: "Test Bank".to_string(),
        synchronizer: "manual".to_string(),
        credentials: None,
        balance_staleness: None,
    });
    storage.save_connection(&connection).await?;

    let account = Account::new("Checking", connection.id().clone());
    storage.save_account(&account).await?;
    storage
        .set_account_config(
            &account.id,
            AccountConfig {
                balance_backfill: Some(BalanceBackfillPolicy::Zero),
                ..AccountConfig::default()
            },
        )
        .await;

    let future_snapshot = BalanceSnapshot::new(
        Utc.with_ymd_and_hms(2026, 2, 3, 12, 0, 0).unwrap(),
        vec![AssetBalance::new(Asset::currency("USD"), "1000")],
    );
    storage
        .append_balance_snapshot(&account.id, &future_snapshot)
        .await?;

    let store = Arc::new(MemoryMarketDataStore::new());
    let market_data = Arc::new(MarketDataService::new(store, None));
    let service = PortfolioService::new(storage, market_data);

    let query = PortfolioQuery {
        as_of_date: chrono::NaiveDate::from_ymd_opt(2026, 2, 1).unwrap(),
        currency: "USD".to_string(),
        currency_decimals: None,
        grouping: Grouping::Account,
        include_detail: false,
        capital_gains_tax_rate: None,
        equity_valuation_adjustment: None,
        account_ids: Vec::new(),
    };

    let result = service.calculate(&query).await?;
    assert_eq!(result.total_value, "0");

    let by_account = result.by_account.expect("account summaries");
    assert_eq!(by_account.len(), 1);
    assert_eq!(by_account[0].value_in_base.as_deref(), Some("0"));
    Ok(())
}

#[tokio::test]
async fn calculate_excludes_accounts_marked_from_portfolio() -> Result<()> {
    let storage = Arc::new(MemoryStorage::new());
    let connection = Connection::new(ConnectionConfig {
        name: "Test Bank".to_string(),
        synchronizer: "manual".to_string(),
        credentials: None,
        balance_staleness: None,
    });
    storage.save_connection(&connection).await?;

    let included = Account::new("Checking", connection.id().clone());
    let excluded = Account::new("Mortgage", connection.id().clone());
    storage.save_account(&included).await?;
    storage.save_account(&excluded).await?;
    storage
        .set_account_config(
            &excluded.id,
            AccountConfig {
                exclude_from_portfolio: Some(true),
                ..AccountConfig::default()
            },
        )
        .await;

    let included_snapshot = BalanceSnapshot::new(
        Utc.with_ymd_and_hms(2026, 2, 1, 12, 0, 0).unwrap(),
        vec![AssetBalance::new(Asset::currency("USD"), "1000")],
    );
    let excluded_snapshot = BalanceSnapshot::new(
        Utc.with_ymd_and_hms(2026, 2, 1, 12, 0, 0).unwrap(),
        vec![AssetBalance::new(Asset::currency("USD"), "-500")],
    );
    storage
        .append_balance_snapshot(&included.id, &included_snapshot)
        .await?;
    storage
        .append_balance_snapshot(&excluded.id, &excluded_snapshot)
        .await?;

    let store = Arc::new(MemoryMarketDataStore::new());
    let market_data = Arc::new(MarketDataService::new(store, None));
    let service = PortfolioService::new(storage, market_data);

    let query = PortfolioQuery {
        as_of_date: chrono::NaiveDate::from_ymd_opt(2026, 2, 2).unwrap(),
        currency: "USD".to_string(),
        currency_decimals: None,
        grouping: Grouping::Both,
        include_detail: false,
        capital_gains_tax_rate: None,
        equity_valuation_adjustment: None,
        account_ids: Vec::new(),
    };

    let result = service.calculate(&query).await?;
    assert_eq!(result.total_value, "1000");

    let by_account = result.by_account.expect("account summaries");
    assert_eq!(by_account.len(), 1);
    assert_eq!(by_account[0].account_name, "Checking");

    Ok(())
}

#[tokio::test]
async fn calculate_carry_back_earliest_balance() -> Result<()> {
    let storage = Arc::new(MemoryStorage::new());
    let connection = Connection::new(ConnectionConfig {
        name: "Test Bank".to_string(),
        synchronizer: "manual".to_string(),
        credentials: None,
        balance_staleness: None,
    });
    storage.save_connection(&connection).await?;

    let account = Account::new("Checking", connection.id().clone());
    storage.save_account(&account).await?;
    storage
        .set_account_config(
            &account.id,
            AccountConfig {
                balance_backfill: Some(BalanceBackfillPolicy::CarryEarliest),
                ..AccountConfig::default()
            },
        )
        .await;

    let earliest_snapshot = BalanceSnapshot::new(
        Utc.with_ymd_and_hms(2026, 2, 3, 12, 0, 0).unwrap(),
        vec![AssetBalance::new(Asset::currency("USD"), "1000")],
    );
    storage
        .append_balance_snapshot(&account.id, &earliest_snapshot)
        .await?;

    let store = Arc::new(MemoryMarketDataStore::new());
    let market_data = Arc::new(MarketDataService::new(store, None));
    let service = PortfolioService::new(storage, market_data);

    let query = PortfolioQuery {
        as_of_date: chrono::NaiveDate::from_ymd_opt(2026, 2, 1).unwrap(),
        currency: "USD".to_string(),
        currency_decimals: None,
        grouping: Grouping::Both,
        include_detail: false,
        capital_gains_tax_rate: None,
        equity_valuation_adjustment: None,
        account_ids: Vec::new(),
    };

    let result = service.calculate(&query).await?;
    assert_eq!(result.total_value, "1000");
    Ok(())
}

#[tokio::test]
async fn manual_value_carries_back_without_becoming_currency() -> Result<()> {
    let storage = Arc::new(MemoryStorage::new());
    let connection = Connection::new(ConnectionConfig {
        name: "Manual".to_string(),
        synchronizer: "manual".to_string(),
        credentials: None,
        balance_staleness: None,
    });
    storage.save_connection(&connection).await?;

    let account = Account::new("Expected Housing Value", connection.id().clone());
    storage.save_account(&account).await?;
    storage
        .set_account_config(
            &account.id,
            AccountConfig {
                balance_backfill: Some(BalanceBackfillPolicy::CarryEarliest),
                ..AccountConfig::default()
            },
        )
        .await;

    let asset = Asset::manual_value("Expected Housing Value", "USD");
    let future_snapshot = BalanceSnapshot::new(
        Utc.with_ymd_and_hms(2026, 5, 2, 12, 0, 0).unwrap(),
        vec![AssetBalance::new(asset.clone(), "600000")],
    );
    storage
        .append_balance_snapshot(&account.id, &future_snapshot)
        .await?;

    let store = Arc::new(MemoryMarketDataStore::new());
    let market_data = Arc::new(MarketDataService::new(store, None));
    let service = PortfolioService::new(storage, market_data);

    let query = PortfolioQuery {
        as_of_date: chrono::NaiveDate::from_ymd_opt(2021, 10, 31).unwrap(),
        currency: "USD".to_string(),
        currency_decimals: None,
        grouping: Grouping::Asset,
        include_detail: true,
        capital_gains_tax_rate: None,
        equity_valuation_adjustment: None,
        account_ids: Vec::new(),
    };

    let result = service.calculate(&query).await?;
    assert_eq!(result.total_value, "600000");

    let by_asset = result.by_asset.expect("asset summaries");
    assert_eq!(by_asset.len(), 1);
    assert_eq!(by_asset[0].asset, asset.normalized());
    assert_eq!(by_asset[0].value_in_base.as_deref(), Some("600000"));
    assert_eq!(by_asset[0].price, None);
    assert_eq!(
        by_asset[0].holdings.as_ref().unwrap()[0]
            .balance_date
            .to_string(),
        "2026-05-02"
    );

    Ok(())
}

#[tokio::test]
async fn historical_snapshot_does_not_fetch_live_quote_for_past_date() -> Result<()> {
    #[derive(Clone)]
    struct QuoteOnlySource {
        quote: PricePoint,
    }

    #[async_trait::async_trait]
    impl EquityPriceSource for QuoteOnlySource {
        async fn fetch_close(
            &self,
            _asset: &Asset,
            _asset_id: &AssetId,
            _date: chrono::NaiveDate,
        ) -> Result<Option<PricePoint>> {
            Ok(None)
        }

        async fn fetch_quote(
            &self,
            _asset: &Asset,
            _asset_id: &AssetId,
        ) -> Result<Option<PricePoint>> {
            Ok(Some(self.quote.clone()))
        }

        fn name(&self) -> &str {
            "quote-only"
        }
    }

    let asset = Asset::equity("AAPL");
    let asset_id = AssetId::from_asset(&asset);
    let as_of_date = chrono::NaiveDate::from_ymd_opt(2024, 1, 2).unwrap();

    let store = Arc::new(MemoryMarketDataStore::new());
    let close_price = PricePoint {
        asset_id: asset_id.clone(),
        as_of_date,
        timestamp: Utc::now(),
        price: "100".to_string(),
        quote_currency: "USD".to_string(),
        kind: PriceKind::Close,
        source: "close".to_string(),
    };
    store.put_prices(&[close_price]).await?;

    let quote_price = PricePoint {
        asset_id,
        as_of_date: Utc::now().date_naive(),
        timestamp: Utc::now(),
        price: "200".to_string(),
        quote_currency: "USD".to_string(),
        kind: PriceKind::Quote,
        source: "quote".to_string(),
    };

    let router = EquityPriceRouter::new(vec![Arc::new(QuoteOnlySource { quote: quote_price })]);
    let market_data =
        Arc::new(MarketDataService::new(store, None).with_equity_router(Arc::new(router)));

    let service = PortfolioService::new(Arc::new(MemoryStorage::new()), market_data);
    let valuation = service
        .value_asset(&asset, Decimal::ONE, "USD", as_of_date)
        .await?;

    assert_eq!(valuation.price.as_deref(), Some("100"));
    assert_eq!(valuation.price_date, Some(as_of_date));

    Ok(())
}

#[tokio::test]
async fn historical_snapshot_prefers_same_day_quote_over_older_close() -> Result<()> {
    let storage = Arc::new(MemoryStorage::new());
    let store = Arc::new(MemoryMarketDataStore::new());
    let asset = Asset::equity("AAPL");
    let asset_id = AssetId::from_asset(&asset);
    let as_of_date = chrono::NaiveDate::from_ymd_opt(2024, 1, 2).unwrap();
    let quote_timestamp = Utc.with_ymd_and_hms(2024, 1, 2, 12, 0, 0).unwrap();

    store
        .put_prices(&[
            PricePoint {
                asset_id: asset_id.clone(),
                as_of_date: chrono::NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
                timestamp: Utc.with_ymd_and_hms(2024, 1, 1, 23, 59, 59).unwrap(),
                price: "100".to_string(),
                quote_currency: "USD".to_string(),
                kind: PriceKind::Close,
                source: "close".to_string(),
            },
            PricePoint {
                asset_id,
                as_of_date,
                timestamp: quote_timestamp,
                price: "110".to_string(),
                quote_currency: "USD".to_string(),
                kind: PriceKind::Quote,
                source: "quote".to_string(),
            },
        ])
        .await?;

    let market_data = Arc::new(MarketDataService::new(store, None));
    let service = PortfolioService::new(storage, market_data);
    let valuation = service
        .value_asset(&asset, Decimal::ONE, "USD", as_of_date)
        .await?;

    assert_eq!(valuation.price.as_deref(), Some("110"));
    assert_eq!(valuation.price_date, Some(as_of_date));
    assert_eq!(valuation.price_timestamp, Some(quote_timestamp));

    Ok(())
}
