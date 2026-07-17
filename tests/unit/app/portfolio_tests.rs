use super::*;
use crate::app::*;
use crate::clock::{Clock, FixedClock};
use crate::config::{
    DisplayConfig, GitConfig, HistoryConfig, LatentCapitalGainsTaxConfig, PortfolioConfig,
    RefreshConfig, ResolvedConfig, SpendingConfig, TrayConfig,
};
use crate::market_data::PriceKind;
use crate::models::FixedIdGenerator;
use crate::models::{Account, AssetBalance, BalanceSnapshot, Connection, ConnectionConfig};
use crate::storage::JsonFileStorage;
use crate::storage::MemoryStorage;
use chrono::TimeZone;
use chrono::{DateTime, NaiveDate, Utc};
use std::collections::HashMap;
use std::path::PathBuf;
use tempfile::TempDir;

fn connection_config(name: &str) -> ConnectionConfig {
    ConnectionConfig {
        name: name.to_string(),
        synchronizer: "mock".to_string(),
        credentials: None,
        balance_staleness: None,
    }
}

async fn write_connection_config(
    storage: &JsonFileStorage,
    conn: &Connection,
) -> anyhow::Result<()> {
    storage
        .save_connection_config(conn.id(), &conn.config)
        .await?;
    Ok(())
}

fn sample_price(asset: &Asset, date: NaiveDate, timestamp: DateTime<Utc>) -> PricePoint {
    PricePoint {
        asset_id: AssetId::from_asset(asset),
        as_of_date: date,
        timestamp,
        price: "1.00".to_string(),
        quote_currency: "USD".to_string(),
        kind: PriceKind::Close,
        source: "test".to_string(),
    }
}

fn sample_fx_rate(
    base: &str,
    quote: &str,
    date: NaiveDate,
    timestamp: DateTime<Utc>,
) -> FxRatePoint {
    FxRatePoint {
        base: base.to_string(),
        quote: quote.to_string(),
        as_of_date: date,
        timestamp,
        rate: "1.25".to_string(),
        kind: FxRateKind::Close,
        source: "test".to_string(),
    }
}

#[test]
fn compute_percentage_change_from_previous_handles_expected_cases() {
    assert_eq!(
        compute_percentage_change_from_previous(None, Some(Decimal::ONE)),
        None
    );
    assert_eq!(
        compute_percentage_change_from_previous(Some(Decimal::ZERO), Some(Decimal::ONE)),
        Some("N/A".to_string())
    );
    assert_eq!(
        compute_percentage_change_from_previous(
            Some(Decimal::from_str("100").unwrap()),
            Some(Decimal::from_str("125.5").unwrap())
        ),
        Some("25.50".to_string())
    );
    assert_eq!(
        compute_percentage_change_from_previous(Some(Decimal::ONE), None),
        Some("N/A".to_string())
    );
}

#[test]
fn history_spec_dates_expand_default_recent_history_layout() -> anyhow::Result<()> {
    let dates = history_spec_dates(
        NaiveDate::from_ymd_opt(2025, 4, 19).unwrap(),
        &[
            "last 4 days".to_string(),
            "1 week ago".to_string(),
            "2 weeks ago".to_string(),
            "last 12 months".to_string(),
        ],
    )?;
    assert_eq!(
        dates,
        vec![
            NaiveDate::from_ymd_opt(2024, 5, 19).unwrap(),
            NaiveDate::from_ymd_opt(2024, 6, 19).unwrap(),
            NaiveDate::from_ymd_opt(2024, 7, 19).unwrap(),
            NaiveDate::from_ymd_opt(2024, 8, 19).unwrap(),
            NaiveDate::from_ymd_opt(2024, 9, 19).unwrap(),
            NaiveDate::from_ymd_opt(2024, 10, 19).unwrap(),
            NaiveDate::from_ymd_opt(2024, 11, 19).unwrap(),
            NaiveDate::from_ymd_opt(2024, 12, 19).unwrap(),
            NaiveDate::from_ymd_opt(2025, 1, 19).unwrap(),
            NaiveDate::from_ymd_opt(2025, 2, 19).unwrap(),
            NaiveDate::from_ymd_opt(2025, 3, 19).unwrap(),
            NaiveDate::from_ymd_opt(2025, 4, 5).unwrap(),
            NaiveDate::from_ymd_opt(2025, 4, 12).unwrap(),
            NaiveDate::from_ymd_opt(2025, 4, 16).unwrap(),
            NaiveDate::from_ymd_opt(2025, 4, 17).unwrap(),
            NaiveDate::from_ymd_opt(2025, 4, 18).unwrap(),
            NaiveDate::from_ymd_opt(2025, 4, 19).unwrap(),
        ]
    );
    Ok(())
}

#[test]
fn history_spec_dates_support_each_of_the_last_ranges() -> anyhow::Result<()> {
    let dates = history_spec_dates(
        NaiveDate::from_ymd_opt(2024, 2, 29).unwrap(),
        &["each of the last 3 months".to_string()],
    )?;
    assert_eq!(
        dates,
        vec![
            NaiveDate::from_ymd_opt(2023, 12, 29).unwrap(),
            NaiveDate::from_ymd_opt(2024, 1, 29).unwrap(),
            NaiveDate::from_ymd_opt(2024, 2, 29).unwrap(),
        ]
    );
    Ok(())
}

#[test]
fn parse_portfolio_date_bound_supports_partial_and_relative_dates() -> anyhow::Result<()> {
    let anchor = NaiveDate::from_ymd_opt(2026, 4, 26).unwrap();

    assert_eq!(
        parse_portfolio_date_bound("2025", DateRangeBound::Start, anchor)?,
        NaiveDate::from_ymd_opt(2025, 1, 1).unwrap()
    );
    assert_eq!(
        parse_portfolio_date_bound("2025", DateRangeBound::End, anchor)?,
        NaiveDate::from_ymd_opt(2025, 12, 31).unwrap()
    );
    assert_eq!(
        parse_portfolio_date_bound("2025-03", DateRangeBound::Start, anchor)?,
        NaiveDate::from_ymd_opt(2025, 3, 1).unwrap()
    );
    assert_eq!(
        parse_portfolio_date_bound("2025-03", DateRangeBound::End, anchor)?,
        NaiveDate::from_ymd_opt(2025, 3, 31).unwrap()
    );
    assert_eq!(
        parse_portfolio_date_bound("-1y", DateRangeBound::Start, anchor)?,
        NaiveDate::from_ymd_opt(2025, 4, 26).unwrap()
    );
    assert_eq!(
        parse_portfolio_date_bound("-3m", DateRangeBound::End, anchor)?,
        NaiveDate::from_ymd_opt(2026, 1, 26).unwrap()
    );

    Ok(())
}

#[tokio::test]
async fn portfolio_recent_history_uses_configured_history_spec() -> anyhow::Result<()> {
    let dir = TempDir::new()?;
    let tray = TrayConfig {
        history_spec: vec![
            "last 4 days".to_string(),
            "1 week ago".to_string(),
            "2 weeks ago".to_string(),
            "last 12 months".to_string(),
        ],
        ..Default::default()
    };
    let config = ResolvedConfig {
        data_dir: dir.path().to_path_buf(),
        reporting_currency: "USD".to_string(),
        display: DisplayConfig::default(),
        refresh: RefreshConfig::default(),
        history: HistoryConfig::default(),
        tray,
        spending: SpendingConfig::default(),
        tags: Default::default(),
        portfolio: crate::config::PortfolioConfig::default(),
        ignore: crate::config::IgnoreConfig::default(),
        ai: crate::config::AiConfig::default(),
        git: GitConfig::default(),
    };

    let storage = Arc::new(MemoryStorage::new());
    let connection = Connection::new(connection_config("Cash"));
    storage.save_connection(&connection).await?;

    let account = Account::new("Checking", connection.id().clone());
    storage.save_account(&account).await?;

    storage
        .append_balance_snapshot(
            &account.id,
            &BalanceSnapshot::new(
                Utc.with_ymd_and_hms(2024, 1, 1, 12, 0, 0).unwrap(),
                vec![AssetBalance::new(Asset::currency("USD"), "100")],
            ),
        )
        .await?;
    storage
        .append_balance_snapshot(
            &account.id,
            &BalanceSnapshot::new(
                Utc.with_ymd_and_hms(2025, 4, 15, 12, 0, 0).unwrap(),
                vec![AssetBalance::new(Asset::currency("USD"), "200")],
            ),
        )
        .await?;

    let output = portfolio_recent_history(
        storage,
        &config,
        None,
        false,
        NaiveDate::from_ymd_opt(2025, 4, 19).unwrap(),
    )
    .await?;

    assert_eq!(
        output
            .iter()
            .map(|point| point.date.as_str())
            .collect::<Vec<_>>(),
        vec![
            "2024-05-19",
            "2024-06-19",
            "2024-07-19",
            "2024-08-19",
            "2024-09-19",
            "2024-10-19",
            "2024-11-19",
            "2024-12-19",
            "2025-01-19",
            "2025-02-19",
            "2025-03-19",
            "2025-04-05",
            "2025-04-12",
            "2025-04-16",
            "2025-04-17",
            "2025-04-18",
            "2025-04-19",
        ]
    );
    assert_eq!(output[12].total_value, "100");
    assert_eq!(output[13].total_value, "200");
    assert_eq!(
        output[13].percentage_change_from_previous.as_deref(),
        Some("100")
    );

    Ok(())
}

#[tokio::test]
async fn portfolio_history_carries_forward_previous_valuation_when_price_missing(
) -> anyhow::Result<()> {
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
        portfolio: crate::config::PortfolioConfig::default(),
        ignore: crate::config::IgnoreConfig::default(),
        ai: crate::config::AiConfig::default(),
        git: GitConfig::default(),
    };

    let storage = Arc::new(MemoryStorage::new());
    let connection = Connection::new(connection_config("Test Broker"));
    storage.save_connection(&connection).await?;

    let account = Account::new("Trading", connection.id().clone());
    storage.save_account(&account).await?;

    let asset = Asset::equity("AAPL");
    storage
        .append_balance_snapshot(
            &account.id,
            &BalanceSnapshot::new(
                Utc.with_ymd_and_hms(2024, 1, 1, 12, 0, 0).unwrap(),
                vec![AssetBalance::new(asset.clone(), "10")],
            ),
        )
        .await?;
    storage
        .append_balance_snapshot(
            &account.id,
            &BalanceSnapshot::new(
                Utc.with_ymd_and_hms(2024, 2, 1, 12, 0, 0).unwrap(),
                vec![AssetBalance::new(asset.clone(), "10")],
            ),
        )
        .await?;

    // Only seed one early close price. By 2024-02-01 this is outside the default 7-day lookback.
    let store = JsonlMarketDataStore::new(&config.data_dir);
    store
        .put_prices(&[PricePoint {
            asset_id: AssetId::from_asset(&asset),
            as_of_date: NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
            timestamp: Utc.with_ymd_and_hms(2024, 1, 1, 23, 59, 59).unwrap(),
            price: "100".to_string(),
            quote_currency: "USD".to_string(),
            kind: PriceKind::Close,
            source: "test".to_string(),
        }])
        .await?;

    let output = portfolio_history(
        storage,
        &config,
        None,
        None,
        None,
        "none".to_string(),
        false,
    )
    .await?;

    assert_eq!(output.points.len(), 2);
    assert_eq!(output.points[0].total_value, "1000");
    assert_eq!(output.points[1].total_value, "1000");
    assert_eq!(
        output.points[1].percentage_change_from_previous.as_deref(),
        Some("0")
    );

    Ok(())
}

#[tokio::test]
async fn portfolio_history_subtracts_configured_latent_capital_gains_tax() -> anyhow::Result<()> {
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
        portfolio: PortfolioConfig {
            latent_capital_gains_tax: LatentCapitalGainsTaxConfig {
                enabled: true,
                rate: Some(0.23),
                account_name: "Latent Capital Gains Tax".to_string(),
            },
        },
        ignore: crate::config::IgnoreConfig::default(),
        ai: crate::config::AiConfig::default(),
        git: GitConfig::default(),
    };

    let storage = Arc::new(MemoryStorage::new());
    let connection = Connection::new(connection_config("Brokerage"));
    storage.save_connection(&connection).await?;

    let account = Account::new("Trading", connection.id().clone());
    storage.save_account(&account).await?;

    let asset = Asset::equity("AAPL");
    storage
        .append_balance_snapshot(
            &account.id,
            &BalanceSnapshot::new(
                Utc.with_ymd_and_hms(2024, 6, 14, 10, 0, 0).unwrap(),
                vec![AssetBalance::new(asset.clone(), "10").with_cost_basis("1500")],
            ),
        )
        .await?;

    let store = JsonlMarketDataStore::new(&config.data_dir);
    store
        .put_prices(&[PricePoint {
            asset_id: AssetId::from_asset(&asset),
            as_of_date: NaiveDate::from_ymd_opt(2024, 6, 14).unwrap(),
            timestamp: Utc.with_ymd_and_hms(2024, 6, 14, 23, 59, 59).unwrap(),
            price: "200".to_string(),
            quote_currency: "USD".to_string(),
            kind: PriceKind::Close,
            source: "test".to_string(),
        }])
        .await?;

    let output = portfolio_history(
        storage,
        &config,
        None,
        None,
        None,
        "none".to_string(),
        false,
    )
    .await?;

    assert_eq!(output.points.len(), 1);
    assert_eq!(output.points[0].total_value, "1885");

    Ok(())
}

#[tokio::test]
async fn portfolio_history_backfills_latest_cost_basis_for_latent_tax() -> anyhow::Result<()> {
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
        portfolio: PortfolioConfig {
            latent_capital_gains_tax: LatentCapitalGainsTaxConfig {
                enabled: true,
                rate: Some(0.23),
                account_name: "Latent Capital Gains Tax".to_string(),
            },
        },
        ignore: crate::config::IgnoreConfig::default(),
        ai: crate::config::AiConfig::default(),
        git: GitConfig::default(),
    };

    let storage = Arc::new(MemoryStorage::new());
    let connection = Connection::new(connection_config("Brokerage"));
    storage.save_connection(&connection).await?;

    let account = Account::new("Trading", connection.id().clone());
    storage.save_account(&account).await?;

    let asset = Asset::equity("AAPL");
    storage
        .append_balance_snapshot(
            &account.id,
            &BalanceSnapshot::new(
                Utc.with_ymd_and_hms(2024, 1, 1, 10, 0, 0).unwrap(),
                vec![AssetBalance::new(asset.clone(), "10")],
            ),
        )
        .await?;
    storage
        .append_balance_snapshot(
            &account.id,
            &BalanceSnapshot::new(
                Utc.with_ymd_and_hms(2024, 2, 1, 10, 0, 0).unwrap(),
                vec![AssetBalance::new(asset.clone(), "10").with_cost_basis("1500")],
            ),
        )
        .await?;

    let store = JsonlMarketDataStore::new(&config.data_dir);
    store
        .put_prices(&[
            PricePoint {
                asset_id: AssetId::from_asset(&asset),
                as_of_date: NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
                timestamp: Utc.with_ymd_and_hms(2024, 1, 1, 23, 59, 59).unwrap(),
                price: "200".to_string(),
                quote_currency: "USD".to_string(),
                kind: PriceKind::Close,
                source: "test".to_string(),
            },
            PricePoint {
                asset_id: AssetId::from_asset(&asset),
                as_of_date: NaiveDate::from_ymd_opt(2024, 2, 1).unwrap(),
                timestamp: Utc.with_ymd_and_hms(2024, 2, 1, 23, 59, 59).unwrap(),
                price: "200".to_string(),
                quote_currency: "USD".to_string(),
                kind: PriceKind::Close,
                source: "test".to_string(),
            },
        ])
        .await?;

    let output = portfolio_history(
        storage,
        &config,
        None,
        None,
        None,
        "none".to_string(),
        false,
    )
    .await?;

    assert_eq!(
        output
            .points
            .iter()
            .map(|point| point.total_value.as_str())
            .collect::<Vec<_>>(),
        vec!["1885", "1885"]
    );

    Ok(())
}

#[tokio::test]
async fn portfolio_history_projects_future_prices_when_configured() -> anyhow::Result<()> {
    let dir = TempDir::new()?;
    let config = ResolvedConfig {
        data_dir: dir.path().to_path_buf(),
        reporting_currency: "USD".to_string(),
        display: DisplayConfig::default(),
        refresh: RefreshConfig::default(),
        history: HistoryConfig {
            allow_future_projection: true,
            lookback_days: Some(7),
            ..Default::default()
        },
        tray: TrayConfig::default(),
        spending: SpendingConfig::default(),
        tags: Default::default(),
        portfolio: crate::config::PortfolioConfig::default(),
        ignore: crate::config::IgnoreConfig::default(),
        ai: crate::config::AiConfig::default(),
        git: GitConfig::default(),
    };

    let storage = Arc::new(MemoryStorage::new());
    let connection = Connection::new(connection_config("Test Broker"));
    storage.save_connection(&connection).await?;

    let account = Account::new("Trading", connection.id().clone());
    storage.save_account(&account).await?;

    let asset = Asset::equity("AAPL");
    storage
        .append_balance_snapshot(
            &account.id,
            &BalanceSnapshot::new(
                Utc.with_ymd_and_hms(2024, 1, 15, 12, 0, 0).unwrap(),
                vec![AssetBalance::new(asset.clone(), "10")],
            ),
        )
        .await?;

    let store = JsonlMarketDataStore::new(&config.data_dir);
    store
        .put_prices(&[
            PricePoint {
                asset_id: AssetId::from_asset(&asset),
                as_of_date: NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
                timestamp: Utc.with_ymd_and_hms(2024, 1, 1, 23, 59, 59).unwrap(),
                price: "100".to_string(),
                quote_currency: "USD".to_string(),
                kind: PriceKind::Close,
                source: "test".to_string(),
            },
            PricePoint {
                asset_id: AssetId::from_asset(&asset),
                as_of_date: NaiveDate::from_ymd_opt(2024, 1, 20).unwrap(),
                timestamp: Utc.with_ymd_and_hms(2024, 1, 20, 23, 59, 59).unwrap(),
                price: "120".to_string(),
                quote_currency: "USD".to_string(),
                kind: PriceKind::Close,
                source: "test".to_string(),
            },
        ])
        .await?;

    let output = portfolio_history(
        storage,
        &config,
        None,
        Some("2024-01-15".to_string()),
        Some("2024-01-15".to_string()),
        "none".to_string(),
        false,
    )
    .await?;

    assert_eq!(output.points.len(), 1);
    assert_eq!(output.points[0].total_value, "1200");

    Ok(())
}

#[tokio::test]
async fn fill_prices_at_date_wraps_daily_history_fetch() -> anyhow::Result<()> {
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
        portfolio: crate::config::PortfolioConfig::default(),
        ignore: crate::config::IgnoreConfig::default(),
        ai: crate::config::AiConfig::default(),
        git: GitConfig::default(),
    };

    let storage = Arc::new(MemoryStorage::new());
    let connection = Connection::new(connection_config("Brokerage"));
    storage.save_connection(&connection).await?;

    let account = Account::new("Main", connection.id().clone());
    storage.save_account(&account).await?;

    let asset = Asset::equity("AAPL");
    storage
        .append_balance_snapshot(
            &account.id,
            &BalanceSnapshot::new(
                Utc.with_ymd_and_hms(2024, 1, 15, 12, 0, 0).unwrap(),
                vec![AssetBalance::new(asset.clone(), "10")],
            ),
        )
        .await?;

    let store = JsonlMarketDataStore::new(&config.data_dir);
    store
        .put_prices(&[PricePoint {
            asset_id: AssetId::from_asset(&asset),
            as_of_date: NaiveDate::from_ymd_opt(2024, 1, 15).unwrap(),
            timestamp: Utc.with_ymd_and_hms(2024, 1, 15, 23, 59, 59).unwrap(),
            price: "182.5".to_string(),
            quote_currency: "USD".to_string(),
            kind: PriceKind::Close,
            source: "test".to_string(),
        }])
        .await?;

    let output = fill_prices_at_date(PriceHistoryRequest {
        storage: storage.as_ref(),
        config: &config,
        account: None,
        connection: None,
        start: Some("2024-01-15"),
        end: Some("2024-01-15"),
        interval: "monthly",
        lookback_days: 7,
        request_delay_ms: 0,
        currency: None,
        include_fx: false,
    })
    .await?;

    assert_eq!(output.interval, "daily");
    assert_eq!(output.start_date, "2024-01-15");
    assert_eq!(output.end_date, "2024-01-15");
    assert_eq!(output.points, 1);
    assert_eq!(output.prices.existing, 1);

    Ok(())
}

#[tokio::test]
async fn portfolio_history_prefers_same_day_quotes_over_older_closes() -> anyhow::Result<()> {
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
        portfolio: crate::config::PortfolioConfig::default(),
        ignore: crate::config::IgnoreConfig::default(),
        ai: crate::config::AiConfig::default(),
        git: GitConfig::default(),
    };

    let storage = Arc::new(MemoryStorage::new());
    let connection = Connection::new(connection_config("Test Broker"));
    storage.save_connection(&connection).await?;

    let account = Account::new("Trading", connection.id().clone());
    storage.save_account(&account).await?;

    let asset = Asset::equity("AAPL");
    storage
        .append_balance_snapshot(
            &account.id,
            &BalanceSnapshot::new(
                Utc.with_ymd_and_hms(2024, 1, 1, 12, 0, 0).unwrap(),
                vec![AssetBalance::new(asset.clone(), "10")],
            ),
        )
        .await?;

    let store = JsonlMarketDataStore::new(&config.data_dir);
    store
        .put_prices(&[
            PricePoint {
                asset_id: AssetId::from_asset(&asset),
                as_of_date: NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
                timestamp: Utc.with_ymd_and_hms(2024, 1, 1, 23, 59, 59).unwrap(),
                price: "100".to_string(),
                quote_currency: "USD".to_string(),
                kind: PriceKind::Close,
                source: "test".to_string(),
            },
            PricePoint {
                asset_id: AssetId::from_asset(&asset),
                as_of_date: NaiveDate::from_ymd_opt(2024, 1, 2).unwrap(),
                timestamp: Utc.with_ymd_and_hms(2024, 1, 2, 12, 0, 0).unwrap(),
                price: "110".to_string(),
                quote_currency: "USD".to_string(),
                kind: PriceKind::Quote,
                source: "test".to_string(),
            },
            PricePoint {
                asset_id: AssetId::from_asset(&asset),
                as_of_date: NaiveDate::from_ymd_opt(2024, 1, 3).unwrap(),
                timestamp: Utc.with_ymd_and_hms(2024, 1, 3, 12, 0, 0).unwrap(),
                price: "120".to_string(),
                quote_currency: "USD".to_string(),
                kind: PriceKind::Quote,
                source: "test".to_string(),
            },
        ])
        .await?;

    let output = portfolio_history(
        storage,
        &config,
        None,
        Some("2024-01-02".to_string()),
        Some("2024-01-03".to_string()),
        "none".to_string(),
        true,
    )
    .await?;

    assert_eq!(output.points.len(), 2);
    assert_eq!(output.points[0].date, "2024-01-02");
    assert_eq!(output.points[0].timestamp, "2024-01-02T12:00:00+00:00");
    assert_eq!(output.points[0].total_value, "1100");
    assert_eq!(output.points[1].date, "2024-01-03");
    assert_eq!(output.points[1].timestamp, "2024-01-03T12:00:00+00:00");
    assert_eq!(output.points[1].total_value, "1200");
    assert_eq!(
        output.points[1].percentage_change_from_previous.as_deref(),
        Some("9.09")
    );

    Ok(())
}

#[tokio::test]
async fn portfolio_history_can_jump_when_missing_asset_prices_arrive_late() -> anyhow::Result<()> {
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
        portfolio: crate::config::PortfolioConfig::default(),
        ignore: crate::config::IgnoreConfig::default(),
        ai: crate::config::AiConfig::default(),
        git: GitConfig::default(),
    };

    let storage = Arc::new(MemoryStorage::new());
    let connection = Connection::new(connection_config("Test Broker"));
    storage.save_connection(&connection).await?;

    let account = Account::new("Trading", connection.id().clone());
    storage.save_account(&account).await?;

    // Hold several crypto assets from the start of the year.
    storage
        .append_balance_snapshot(
            &account.id,
            &BalanceSnapshot::new(
                Utc.with_ymd_and_hms(2024, 1, 1, 12, 0, 0).unwrap(),
                vec![
                    AssetBalance::new(Asset::crypto("BTC"), "10"),
                    AssetBalance::new(Asset::crypto("ETH"), "100"),
                    AssetBalance::new(Asset::crypto("ICP"), "1000"),
                    AssetBalance::new(Asset::crypto("POL"), "1000"),
                ],
            ),
        )
        .await?;

    let store = JsonlMarketDataStore::new(&config.data_dir);
    let close = |asset: Asset, date: (i32, u32, u32), price: &str| PricePoint {
        asset_id: AssetId::from_asset(&asset),
        as_of_date: NaiveDate::from_ymd_opt(date.0, date.1, date.2).unwrap(),
        timestamp: Utc
            .with_ymd_and_hms(date.0, date.1, date.2, 23, 59, 59)
            .unwrap(),
        price: price.to_string(),
        quote_currency: "USD".to_string(),
        kind: PriceKind::Close,
        source: "test".to_string(),
    };

    // Only ICP has prices for Sep/Oct/Nov. Other assets are priced only in late Dec.
    store
        .put_prices(&[
            close(Asset::crypto("ICP"), (2024, 9, 22), "1"),
            close(Asset::crypto("ICP"), (2024, 10, 27), "1.1"),
            close(Asset::crypto("ICP"), (2024, 11, 24), "1.2"),
            close(Asset::crypto("ICP"), (2024, 12, 31), "1.3"),
            close(Asset::crypto("BTC"), (2024, 12, 31), "50000"),
            close(Asset::crypto("ETH"), (2024, 12, 31), "3000"),
            close(Asset::crypto("POL"), (2024, 12, 31), "2"),
        ])
        .await?;

    let output = portfolio_history(
        storage.clone(),
        &config,
        None,
        Some("2024-09-01".to_string()),
        Some("2025-01-10".to_string()),
        "monthly".to_string(),
        true,
    )
    .await?;

    assert_eq!(output.points.len(), 4);
    assert_eq!(output.points[0].total_value, "1000");
    let last_total = Decimal::from_str(&output.points[3].total_value)?;
    assert!(last_total > Decimal::from_str("800000")?);

    let last_change = Decimal::from_str(
        output.points[3]
            .percentage_change_from_previous
            .as_deref()
            .expect("last point should have previous change"),
    )?;
    assert!(
        last_change > Decimal::from_str("10000")?,
        "expected very large percentage change when late prices arrive"
    );

    // Without price triggers, there are no balance changes in this window.
    let no_price_output = portfolio_history(
        storage,
        &config,
        None,
        Some("2024-09-01".to_string()),
        Some("2025-01-10".to_string()),
        "monthly".to_string(),
        false,
    )
    .await?;
    assert!(no_price_output.points.is_empty());

    Ok(())
}

#[tokio::test]
async fn portfolio_change_points_includes_prices_when_enabled() -> anyhow::Result<()> {
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
        portfolio: crate::config::PortfolioConfig::default(),
        ignore: crate::config::IgnoreConfig::default(),
        ai: crate::config::AiConfig::default(),
        git: GitConfig::default(),
    };

    let storage = Arc::new(MemoryStorage::new());

    // Minimal account + balance snapshot so the collector considers this asset "held".
    let account = Account::new_with(
        Id::from_string("acct-1"),
        Utc.with_ymd_and_hms(2026, 2, 1, 0, 0, 0).unwrap(),
        "Test Account",
        Id::from_string("conn-1"),
    );
    storage.save_account(&account).await?;

    let aapl = Asset::equity("AAPL");
    storage
        .append_balance_snapshot(
            &account.id,
            &BalanceSnapshot::new(
                Utc.with_ymd_and_hms(2026, 2, 5, 7, 49, 58).unwrap(),
                vec![AssetBalance::new(aapl.clone(), "10")],
            ),
        )
        .await?;

    // Seed a cached price for the held asset on the next day.
    let store = JsonlMarketDataStore::new(&config.data_dir);
    store
        .put_prices(&[sample_price(
            &aapl,
            NaiveDate::from_ymd_opt(2026, 2, 6).unwrap(),
            Utc.with_ymd_and_hms(2026, 2, 6, 12, 0, 0).unwrap(),
        )])
        .await?;

    let output =
        portfolio_change_points(storage, &config, None, None, "none".to_string(), true).await?;

    assert!(
        output.points.iter().any(|p| p
            .triggers
            .iter()
            .any(|t| matches!(t, crate::portfolio::ChangeTrigger::Balance { .. }))),
        "expected at least one balance-triggered change point"
    );
    assert!(
        output.points.iter().any(|p| p
            .triggers
            .iter()
            .any(|t| matches!(t, crate::portfolio::ChangeTrigger::Price { .. }))),
        "expected at least one price-triggered change point"
    );

    Ok(())
}

#[test]
fn parse_asset_handles_prefixes() -> anyhow::Result<()> {
    let equity = parse_asset("Equity:AAPL")?;
    match equity {
        Asset::Equity { ticker, .. } => assert_eq!(ticker, "AAPL"),
        _ => anyhow::bail!("expected equity asset"),
    }

    let crypto = parse_asset("CRYPTO:BTC")?;
    match crypto {
        Asset::Crypto { symbol, .. } => assert_eq!(symbol, "BTC"),
        _ => anyhow::bail!("expected crypto asset"),
    }

    let currency = parse_asset(" currency:usd ")?;
    match currency {
        Asset::Currency { iso_code } => assert_eq!(iso_code, "usd"),
        _ => anyhow::bail!("expected currency asset"),
    }

    let manual_value = parse_asset("value:Expected Housing Value")?;
    match manual_value {
        Asset::ManualValue { name, currency } => {
            assert_eq!(name, "Expected Housing Value");
            assert_eq!(currency, "USD");
        }
        _ => anyhow::bail!("expected manual value asset"),
    }

    let manual_value = parse_asset("manual_value:eur:Foreign Property")?;
    match manual_value {
        Asset::ManualValue { name, currency } => {
            assert_eq!(name, "Foreign Property");
            assert_eq!(currency, "eur");
        }
        _ => anyhow::bail!("expected manual value asset"),
    }

    Ok(())
}

#[test]
fn parse_asset_rejects_empty_values() {
    assert!(parse_asset("").is_err());
    assert!(parse_asset("   ").is_err());
    assert!(parse_asset("equity:").is_err());
    assert!(parse_asset("crypto:   ").is_err());
    assert!(parse_asset("currency:").is_err());
    assert!(parse_asset("value:").is_err());
    assert!(parse_asset("value:USD:").is_err());
}

#[test]
fn align_start_date_monthly_uses_month_end() {
    let date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();
    let aligned = align_start_date(date, PriceHistoryInterval::Monthly);
    assert_eq!(aligned, NaiveDate::from_ymd_opt(2024, 1, 31).unwrap());
}

#[test]
fn align_start_date_yearly_uses_year_end() {
    let date = NaiveDate::from_ymd_opt(2024, 1, 14).unwrap();
    let aligned = align_start_date(date, PriceHistoryInterval::Yearly);
    assert_eq!(aligned, NaiveDate::from_ymd_opt(2024, 12, 31).unwrap());
}

#[test]
fn advance_interval_date_yearly_uses_next_year_end() {
    let date = NaiveDate::from_ymd_opt(2024, 12, 31).unwrap();
    let next = advance_interval_date(date, PriceHistoryInterval::Yearly);
    assert_eq!(next, NaiveDate::from_ymd_opt(2025, 12, 31).unwrap());
}

#[test]
fn resolve_cached_price_prefers_exact_then_lookback() {
    let asset = Asset::equity("AAPL");
    let date = NaiveDate::from_ymd_opt(2024, 1, 10).unwrap();
    let exact = sample_price(&asset, date, Utc::now());
    let mut cache = HashMap::new();
    cache.insert(date, exact.clone());

    let (found, exact_hit) = resolve_cached_price(&cache, date, 3).expect("exact price");
    assert!(exact_hit);
    assert_eq!(found.as_of_date, date);

    cache.remove(&date);
    let lookback_date = date - chrono::Duration::days(1);
    let lookback = sample_price(&asset, lookback_date, Utc::now());
    cache.insert(lookback_date, lookback.clone());

    let (found, exact_hit) = resolve_cached_price(&cache, date, 3).expect("lookback price");
    assert!(!exact_hit);
    assert_eq!(found.as_of_date, lookback_date);
}

#[test]
fn upsert_price_cache_prefers_newer_timestamp() {
    let asset = Asset::equity("AAPL");
    let date = NaiveDate::from_ymd_opt(2024, 1, 5).unwrap();
    let newer = sample_price(&asset, date, Utc::now());
    let older = sample_price(&asset, date, Utc::now() - chrono::Duration::minutes(5));

    let mut cache = HashMap::new();
    cache.insert(date, newer.clone());

    assert!(!upsert_price_cache(&mut cache, older));
    assert_eq!(cache.get(&date).unwrap().timestamp, newer.timestamp);

    let newest = sample_price(&asset, date, Utc::now() + chrono::Duration::minutes(1));
    assert!(upsert_price_cache(&mut cache, newest.clone()));
    assert_eq!(cache.get(&date).unwrap().timestamp, newest.timestamp);
}

#[test]
fn resolve_cached_fx_prefers_exact_then_lookback() {
    let date = NaiveDate::from_ymd_opt(2024, 1, 10).unwrap();
    let exact = sample_fx_rate("EUR", "USD", date, Utc::now());
    let mut cache = HashMap::new();
    cache.insert(date, exact.clone());

    let (found, exact_hit) = resolve_cached_fx(&cache, date, 3).expect("exact rate");
    assert!(exact_hit);
    assert_eq!(found.as_of_date, date);

    cache.remove(&date);
    let lookback_date = date - chrono::Duration::days(2);
    let lookback = sample_fx_rate("EUR", "USD", lookback_date, Utc::now());
    cache.insert(lookback_date, lookback.clone());

    let (found, exact_hit) = resolve_cached_fx(&cache, date, 3).expect("lookback rate");
    assert!(!exact_hit);
    assert_eq!(found.as_of_date, lookback_date);
}

#[tokio::test]
async fn add_connection_rejects_duplicate_names() -> anyhow::Result<()> {
    let dir = TempDir::new()?;
    let storage = JsonFileStorage::new(dir.path());
    let config = ResolvedConfig {
        data_dir: dir.path().to_path_buf(),
        reporting_currency: "USD".to_string(),
        display: DisplayConfig::default(),
        refresh: RefreshConfig::default(),
        history: HistoryConfig::default(),
        tray: TrayConfig::default(),
        spending: SpendingConfig::default(),
        tags: Default::default(),
        portfolio: crate::config::PortfolioConfig::default(),
        ignore: crate::config::IgnoreConfig::default(),
        ai: crate::config::AiConfig::default(),
        git: GitConfig::default(),
    };

    add_connection(&storage, &config, "Duplicate", "manual").await?;

    let err = add_connection(&storage, &config, "duplicate", "manual")
        .await
        .expect_err("expected duplicate connection name error");
    assert!(err.to_string().contains("Connection name already exists"));

    Ok(())
}

#[tokio::test]
async fn add_connection_and_account_use_injected_ids_and_clock() -> anyhow::Result<()> {
    let dir = TempDir::new()?;
    let storage = JsonFileStorage::new(dir.path());
    let config = ResolvedConfig {
        data_dir: dir.path().to_path_buf(),
        reporting_currency: "USD".to_string(),
        display: DisplayConfig::default(),
        refresh: RefreshConfig::default(),
        history: HistoryConfig::default(),
        tray: TrayConfig::default(),
        spending: SpendingConfig::default(),
        tags: Default::default(),
        portfolio: crate::config::PortfolioConfig::default(),
        ignore: crate::config::IgnoreConfig::default(),
        ai: crate::config::AiConfig::default(),
        git: GitConfig::default(),
    };

    let ids = FixedIdGenerator::new([Id::from_string("conn-id"), Id::from_string("acct-id")]);
    let clock = FixedClock::new(Utc.with_ymd_and_hms(2026, 2, 5, 12, 0, 0).unwrap());

    let out = add_connection_with(&storage, &config, "Test", "manual", &ids, &clock).await?;
    assert_eq!(out["connection"]["id"].as_str(), Some("conn-id"));

    let loaded = storage
        .get_connection(&Id::from_string("conn-id"))
        .await?
        .expect("connection should exist");
    assert_eq!(loaded.state.created_at, clock.now());

    let out = add_account_with(
        &storage,
        &config,
        "conn-id",
        "Checking",
        vec!["tag".to_string()],
        &ids,
        &clock,
    )
    .await?;
    assert_eq!(out["account"]["id"].as_str(), Some("acct-id"));

    let acct = storage
        .get_account(&Id::from_string("acct-id"))
        .await?
        .expect("account should exist");
    assert_eq!(acct.created_at, clock.now());
    assert_eq!(acct.tags, vec!["tag".to_string()]);

    Ok(())
}

#[cfg(unix)]
#[tokio::test]
async fn add_connection_creates_by_name_symlink() -> anyhow::Result<()> {
    let dir = TempDir::new()?;
    let storage = JsonFileStorage::new(dir.path());
    let config = ResolvedConfig {
        data_dir: dir.path().to_path_buf(),
        reporting_currency: "USD".to_string(),
        display: DisplayConfig::default(),
        refresh: RefreshConfig::default(),
        history: HistoryConfig::default(),
        tray: TrayConfig::default(),
        spending: SpendingConfig::default(),
        tags: Default::default(),
        portfolio: crate::config::PortfolioConfig::default(),
        ignore: crate::config::IgnoreConfig::default(),
        ai: crate::config::AiConfig::default(),
        git: GitConfig::default(),
    };

    let result = add_connection(&storage, &config, "Test Bank", "manual").await?;
    let id = result["connection"]["id"]
        .as_str()
        .expect("connection id missing");

    let link_path = dir
        .path()
        .join("connections")
        .join("by-name")
        .join("Test Bank");
    let metadata = std::fs::symlink_metadata(&link_path)?;
    assert!(metadata.file_type().is_symlink());

    let target = std::fs::read_link(&link_path)?;
    assert_eq!(target, PathBuf::from("..").join(id));

    Ok(())
}

#[tokio::test]
async fn set_balance_rejects_invalid_amount() -> anyhow::Result<()> {
    let dir = TempDir::new()?;
    let storage = JsonFileStorage::new(dir.path());
    let config = ResolvedConfig {
        data_dir: dir.path().to_path_buf(),
        reporting_currency: "USD".to_string(),
        display: DisplayConfig::default(),
        refresh: RefreshConfig::default(),
        history: HistoryConfig::default(),
        tray: TrayConfig::default(),
        spending: SpendingConfig::default(),
        tags: Default::default(),
        portfolio: crate::config::PortfolioConfig::default(),
        ignore: crate::config::IgnoreConfig::default(),
        ai: crate::config::AiConfig::default(),
        git: GitConfig::default(),
    };

    let account = Account::new("Checking", Id::new());
    storage.save_account(&account).await?;

    let err = set_balance(
        &storage,
        &config,
        account.id.as_str(),
        "USD",
        "not-a-number",
        None,
    )
    .await
    .expect_err("expected invalid amount error");
    assert!(err.to_string().contains("Invalid amount"));

    let snapshots = storage.get_balance_snapshots(&account.id).await?;
    assert!(snapshots.is_empty());

    Ok(())
}

#[tokio::test]
async fn set_account_config_updates_balance_backfill() -> anyhow::Result<()> {
    let dir = TempDir::new()?;
    let storage = JsonFileStorage::new(dir.path());
    let config = ResolvedConfig {
        data_dir: dir.path().to_path_buf(),
        reporting_currency: "USD".to_string(),
        display: DisplayConfig::default(),
        refresh: RefreshConfig::default(),
        history: HistoryConfig::default(),
        tray: TrayConfig::default(),
        spending: SpendingConfig::default(),
        tags: Default::default(),
        portfolio: crate::config::PortfolioConfig::default(),
        ignore: crate::config::IgnoreConfig::default(),
        ai: crate::config::AiConfig::default(),
        git: GitConfig::default(),
    };

    let account = Account::new("Checking", Id::new());
    storage.save_account(&account).await?;

    let out = set_account_config(
        &storage,
        &config,
        account.id.as_str(),
        Some("carry_earliest"),
        false,
    )
    .await?;
    assert_eq!(out["success"], serde_json::Value::Bool(true));
    assert_eq!(
        out["config"]["balance_backfill"],
        serde_json::Value::String("carry_earliest".to_string())
    );

    let stored = storage
        .get_account_config(&account.id)?
        .expect("account config should exist");
    assert_eq!(
        stored.balance_backfill,
        Some(crate::models::BalanceBackfillPolicy::CarryEarliest)
    );

    Ok(())
}

#[tokio::test]
async fn set_account_config_clears_balance_backfill() -> anyhow::Result<()> {
    let dir = TempDir::new()?;
    let storage = JsonFileStorage::new(dir.path());
    let config = ResolvedConfig {
        data_dir: dir.path().to_path_buf(),
        reporting_currency: "USD".to_string(),
        display: DisplayConfig::default(),
        refresh: RefreshConfig::default(),
        history: HistoryConfig::default(),
        tray: TrayConfig::default(),
        spending: SpendingConfig::default(),
        tags: Default::default(),
        portfolio: crate::config::PortfolioConfig::default(),
        ignore: crate::config::IgnoreConfig::default(),
        ai: crate::config::AiConfig::default(),
        git: GitConfig::default(),
    };

    let account = Account::new("Checking", Id::new());
    storage.save_account(&account).await?;
    storage
        .save_account_config(
            &account.id,
            &crate::models::AccountConfig {
                balance_backfill: Some(crate::models::BalanceBackfillPolicy::Zero),
                ..Default::default()
            },
        )
        .await?;

    let out = set_account_config(&storage, &config, "Checking", None, true).await?;
    assert_eq!(out["success"], serde_json::Value::Bool(true));
    assert_eq!(out["config"]["balance_backfill"], serde_json::Value::Null);

    let stored = storage
        .get_account_config(&account.id)?
        .expect("account config should exist");
    assert_eq!(stored.balance_backfill, None);

    Ok(())
}

#[tokio::test]
async fn resolve_scope_rejects_account_and_connection() -> anyhow::Result<()> {
    let dir = TempDir::new()?;
    let storage = JsonFileStorage::new(dir.path());

    let err = resolve_price_history_scope(&storage, Some("a"), Some("b"))
        .await
        .err()
        .expect("expected invalid scope error");
    assert!(err.to_string().contains("Specify only one"));

    Ok(())
}

#[tokio::test]
async fn resolve_scope_connection_requires_accounts() -> anyhow::Result<()> {
    let dir = TempDir::new()?;
    let storage = JsonFileStorage::new(dir.path());
    let mut conn = Connection::new(connection_config("Test Connection"));

    let missing_account = Account::new("Missing", conn.id().clone());
    conn.state.account_ids = vec![missing_account.id.clone()];

    write_connection_config(&storage, &conn).await?;
    storage.save_connection(&conn).await?;

    let conn_id = conn.id().to_string();
    let err = resolve_price_history_scope(&storage, None, Some(conn_id.as_str()))
        .await
        .err()
        .expect("expected missing accounts error");
    assert!(err.to_string().contains("No accounts found for connection"));

    Ok(())
}

#[tokio::test]
async fn resolve_scope_connection_uses_accounts_by_connection_id() -> anyhow::Result<()> {
    let dir = TempDir::new()?;
    let storage = JsonFileStorage::new(dir.path());
    let conn = Connection::new(connection_config("Test Connection"));

    write_connection_config(&storage, &conn).await?;
    storage.save_connection(&conn).await?;

    let account = Account::new("Checking", conn.id().clone());
    storage.save_account(&account).await?;

    let conn_id = conn.id().to_string();
    let (scope, accounts) =
        resolve_price_history_scope(&storage, None, Some(conn_id.as_str())).await?;
    assert_eq!(accounts.len(), 1);
    assert_eq!(accounts[0].id, account.id);
    match scope {
        PriceHistoryScopeOutput::Connection { id, .. } => {
            assert_eq!(id, conn_id);
        }
        _ => anyhow::bail!("expected connection scope"),
    }

    Ok(())
}

#[tokio::test]
async fn resolve_scope_connection_falls_back_when_state_ids_missing() -> anyhow::Result<()> {
    let dir = TempDir::new()?;
    let storage = JsonFileStorage::new(dir.path());
    let mut conn = Connection::new(connection_config("Test Connection"));

    conn.state.account_ids = vec![Id::from_string("missing-account")];

    write_connection_config(&storage, &conn).await?;
    storage.save_connection(&conn).await?;

    let account = Account::new("Checking", conn.id().clone());
    storage.save_account(&account).await?;

    let conn_id = conn.id().to_string();
    let (scope, accounts) =
        resolve_price_history_scope(&storage, None, Some(conn_id.as_str())).await?;
    assert_eq!(accounts.len(), 1);
    assert_eq!(accounts[0].id, account.id);
    match scope {
        PriceHistoryScopeOutput::Connection { id, .. } => {
            assert_eq!(id, conn_id);
        }
        _ => anyhow::bail!("expected connection scope"),
    }

    Ok(())
}

#[tokio::test]
async fn resolve_scope_connection_includes_accounts_missing_from_state() -> anyhow::Result<()> {
    let dir = TempDir::new()?;
    let storage = JsonFileStorage::new(dir.path());
    let mut conn = Connection::new(connection_config("Test Connection"));

    let account_a = Account::new("Checking", conn.id().clone());
    conn.state.account_ids = vec![account_a.id.clone()];

    write_connection_config(&storage, &conn).await?;
    storage.save_connection(&conn).await?;

    let account_b = Account::new("Savings", conn.id().clone());
    storage.save_account(&account_a).await?;
    storage.save_account(&account_b).await?;

    let conn_id = conn.id().to_string();
    let (_, accounts) = resolve_price_history_scope(&storage, None, Some(conn_id.as_str())).await?;
    assert_eq!(accounts.len(), 2);

    Ok(())
}

fn assets_test_config(data_dir: PathBuf) -> ResolvedConfig {
    ResolvedConfig {
        data_dir,
        reporting_currency: "USD".to_string(),
        display: DisplayConfig::default(),
        refresh: RefreshConfig::default(),
        history: HistoryConfig::default(),
        tray: TrayConfig::default(),
        spending: SpendingConfig::default(),
        tags: Default::default(),
        portfolio: PortfolioConfig::default(),
        ignore: crate::config::IgnoreConfig::default(),
        ai: crate::config::AiConfig::default(),
        git: GitConfig::default(),
    }
}

fn close_price(asset: &Asset, date: NaiveDate, price: &str) -> PricePoint {
    PricePoint {
        asset_id: AssetId::from_asset(asset),
        as_of_date: date,
        timestamp: Utc
            .with_ymd_and_hms(date.year(), date.month(), date.day(), 23, 59, 59)
            .unwrap(),
        price: price.to_string(),
        quote_currency: "USD".to_string(),
        kind: PriceKind::Close,
        source: "test".to_string(),
    }
}

#[tokio::test]
async fn portfolio_assets_computes_changes_across_dates() -> anyhow::Result<()> {
    let dir = TempDir::new()?;
    let config = assets_test_config(dir.path().to_path_buf());

    let storage = Arc::new(MemoryStorage::new());
    let connection = Connection::new(connection_config("Broker"));
    storage.save_connection(&connection).await?;

    let trading = Account::new("Trading", connection.id().clone());
    let growth = Account::new("Growth", connection.id().clone());
    storage.save_account(&trading).await?;
    storage.save_account(&growth).await?;

    let aapl = Asset::equity("AAPL");
    let msft = Asset::equity("MSFT");
    let nvda = Asset::equity("NVDA");

    // AAPL and NVDA held since well before all past dates.
    storage
        .append_balance_snapshot(
            &trading.id,
            &BalanceSnapshot::new(
                Utc.with_ymd_and_hms(2023, 1, 1, 10, 0, 0).unwrap(),
                vec![
                    AssetBalance::new(aapl.clone(), "10"),
                    AssetBalance::new(nvda.clone(), "3"),
                ],
            ),
        )
        .await?;
    // MSFT first held between the week-ago and day-ago dates.
    storage
        .append_balance_snapshot(
            &growth.id,
            &BalanceSnapshot::new(
                Utc.with_ymd_and_hms(2024, 6, 10, 10, 0, 0).unwrap(),
                vec![AssetBalance::new(msft.clone(), "5")],
            ),
        )
        .await?;

    // as_of 2024-06-15; past dates: 2024-06-14 (day), 2024-06-08 (week),
    // 2024-05-15 (month), 2023-06-15 (year).
    let store = JsonlMarketDataStore::new(&config.data_dir);
    store
        .put_prices(&[
            close_price(&aapl, NaiveDate::from_ymd_opt(2023, 6, 15).unwrap(), "100"),
            close_price(&aapl, NaiveDate::from_ymd_opt(2024, 5, 15).unwrap(), "160"),
            close_price(&aapl, NaiveDate::from_ymd_opt(2024, 6, 8).unwrap(), "180"),
            close_price(&aapl, NaiveDate::from_ymd_opt(2024, 6, 14).unwrap(), "190"),
            close_price(&aapl, NaiveDate::from_ymd_opt(2024, 6, 15).unwrap(), "200"),
            close_price(&msft, NaiveDate::from_ymd_opt(2024, 6, 14).unwrap(), "100"),
            close_price(&msft, NaiveDate::from_ymd_opt(2024, 6, 15).unwrap(), "110"),
            // NVDA has no price data at or before the week/month/year dates.
            close_price(&nvda, NaiveDate::from_ymd_opt(2024, 6, 14).unwrap(), "120"),
            close_price(&nvda, NaiveDate::from_ymd_opt(2024, 6, 15).unwrap(), "130"),
        ])
        .await?;

    let output = portfolio_assets(storage, &config, Some("2024-06-15".to_string())).await?;

    assert_eq!(
        output.as_of_date,
        NaiveDate::from_ymd_opt(2024, 6, 15).unwrap()
    );
    assert_eq!(output.currency, "USD");
    // 2000 (AAPL) + 550 (MSFT) + 390 (NVDA)
    assert_eq!(output.total_value, "2940");
    assert_eq!(
        output
            .assets
            .iter()
            .map(|entry| entry.asset_id.as_str())
            .collect::<Vec<_>>(),
        vec!["equity/AAPL", "equity/MSFT", "equity/NVDA"]
    );

    let pct = |change: &Option<AssetChange>| -> Option<Decimal> {
        change
            .as_ref()
            .and_then(|c| c.percentage.as_deref())
            .map(|p| Decimal::from_str(p).unwrap())
    };
    let abs = |change: &Option<AssetChange>| -> Option<String> {
        change.as_ref().map(|c| c.absolute.clone())
    };

    // AAPL: held with prices at every past date.
    let aapl_entry = &output.assets[0];
    assert!(!aapl_entry.liability);
    assert_eq!(aapl_entry.total_amount, "10");
    assert_eq!(aapl_entry.price.as_deref(), Some("200"));
    assert_eq!(aapl_entry.value_in_base.as_deref(), Some("2000"));
    assert_eq!(abs(&aapl_entry.changes.day).as_deref(), Some("100"));
    // Exact string mirrors compute_percentage_change_from_previous formatting.
    assert_eq!(
        aapl_entry
            .changes
            .day
            .as_ref()
            .and_then(|c| c.percentage.as_deref()),
        Some("5.26")
    );
    assert_eq!(abs(&aapl_entry.changes.week).as_deref(), Some("200"));
    assert_eq!(pct(&aapl_entry.changes.week), Some(Decimal::new(1111, 2)));
    assert_eq!(abs(&aapl_entry.changes.month).as_deref(), Some("400"));
    assert_eq!(pct(&aapl_entry.changes.month), Some(Decimal::from(25)));
    assert_eq!(abs(&aapl_entry.changes.year).as_deref(), Some("1000"));
    assert_eq!(pct(&aapl_entry.changes.year), Some(Decimal::from(100)));

    // MSFT: no holdings at the week/month/year dates -> change from zero with
    // percentage omitted.
    let msft_entry = &output.assets[1];
    assert_eq!(msft_entry.value_in_base.as_deref(), Some("550"));
    assert_eq!(abs(&msft_entry.changes.day).as_deref(), Some("50"));
    assert_eq!(pct(&msft_entry.changes.day), Some(Decimal::from(10)));
    assert_eq!(abs(&msft_entry.changes.week).as_deref(), Some("550"));
    assert_eq!(pct(&msft_entry.changes.week), None);
    assert_eq!(abs(&msft_entry.changes.month).as_deref(), Some("550"));
    assert_eq!(pct(&msft_entry.changes.month), None);
    assert_eq!(abs(&msft_entry.changes.year).as_deref(), Some("550"));
    assert_eq!(pct(&msft_entry.changes.year), None);

    // NVDA: held at the week/month/year dates but unpriceable then -> those
    // periods report no change at all.
    let nvda_entry = &output.assets[2];
    assert_eq!(nvda_entry.value_in_base.as_deref(), Some("390"));
    assert_eq!(abs(&nvda_entry.changes.day).as_deref(), Some("30"));
    assert_eq!(
        pct(&nvda_entry.changes.day),
        Some(Decimal::from_str("8.33")?)
    );
    assert!(nvda_entry.changes.week.is_none());
    assert!(nvda_entry.changes.month.is_none());
    assert!(nvda_entry.changes.year.is_none());

    Ok(())
}

#[tokio::test]
async fn portfolio_assets_sorts_by_absolute_value_descending() -> anyhow::Result<()> {
    let dir = TempDir::new()?;
    let config = assets_test_config(dir.path().to_path_buf());

    let storage = Arc::new(MemoryStorage::new());
    let connection = Connection::new(connection_config("Bank"));
    storage.save_connection(&connection).await?;

    let checking = Account::new("Checking", connection.id().clone());
    let mortgage = Account::new("Mortgage", connection.id().clone());
    let brokerage = Account::new("Brokerage", connection.id().clone());
    storage.save_account(&checking).await?;
    storage.save_account(&mortgage).await?;
    storage.save_account(&brokerage).await?;

    let timestamp = Utc.with_ymd_and_hms(2024, 1, 15, 10, 0, 0).unwrap();
    storage
        .append_balance_snapshot(
            &checking.id,
            &BalanceSnapshot::new(
                timestamp,
                vec![AssetBalance::new(Asset::currency("USD"), "100")],
            ),
        )
        .await?;
    storage
        .append_balance_snapshot(
            &mortgage.id,
            &BalanceSnapshot::new(
                timestamp,
                vec![AssetBalance::new(Asset::currency("USD"), "-5000")],
            ),
        )
        .await?;
    storage
        .append_balance_snapshot(
            &brokerage.id,
            &BalanceSnapshot::new(
                timestamp,
                vec![
                    AssetBalance::new(Asset::equity("AAPL"), "10"),
                    // No price data exists for this asset.
                    AssetBalance::new(Asset::crypto("XYZ"), "1"),
                ],
            ),
        )
        .await?;

    let store = JsonlMarketDataStore::new(&config.data_dir);
    store
        .put_prices(&[close_price(
            &Asset::equity("AAPL"),
            NaiveDate::from_ymd_opt(2024, 6, 15).unwrap(),
            "200",
        )])
        .await?;

    let output = portfolio_assets(storage, &config, Some("2024-06-15".to_string())).await?;

    // |-5000| > |2000| > |100|; unpriceable rows sort last.
    assert_eq!(
        output
            .assets
            .iter()
            .map(|entry| (entry.asset_id.as_str(), entry.liability))
            .collect::<Vec<_>>(),
        vec![
            ("currency/USD", true),
            ("equity/AAPL", false),
            ("currency/USD", false),
            ("crypto/XYZ", false),
        ]
    );

    let unpriced = &output.assets[3];
    assert_eq!(unpriced.value_in_base, None);
    assert!(unpriced.changes.day.is_none());
    assert!(unpriced.changes.week.is_none());
    assert!(unpriced.changes.month.is_none());
    assert!(unpriced.changes.year.is_none());

    // total_value counts only priced rows: 100 - 5000 + 2000.
    assert_eq!(output.total_value, "-2900");

    Ok(())
}
