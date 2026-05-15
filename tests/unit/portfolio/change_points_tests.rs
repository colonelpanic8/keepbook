use super::*;
use chrono::{Datelike, NaiveDate, TimeZone, Timelike};
use std::sync::Arc;

use crate::market_data::{AssetId, MarketDataStore, MemoryMarketDataStore, PriceKind, PricePoint};
use crate::models::{
    Account, AccountConfig, AssetBalance, Connection, ConnectionConfig, ConnectionState,
};
use crate::storage::{MemoryStorage, Storage};

fn make_ts(year: i32, month: u32, day: u32, hour: u32, min: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(year, month, day, hour, min, 0)
        .unwrap()
}

fn date_to_timestamp(date: NaiveDate) -> DateTime<Utc> {
    date.and_hms_opt(23, 59, 59).expect("valid date").and_utc()
}

#[test]
fn collector_tracks_balance_changes() {
    let mut collector = ChangePointCollector::new();
    let ts = make_ts(2026, 1, 15, 10, 30);
    let account_id = Id::new();
    let asset = Asset::currency("USD");

    collector.add_balance_change(ts, account_id.clone(), asset.clone());

    let points = collector.into_change_points();
    assert_eq!(points.len(), 1);
    assert_eq!(points[0].timestamp, ts);
    assert_eq!(points[0].triggers.len(), 1);
}

#[test]
fn collector_merges_same_timestamp() {
    let mut collector = ChangePointCollector::new();
    let ts = make_ts(2026, 1, 15, 10, 30);
    let account_id = Id::new();

    collector.add_balance_change(ts, account_id.clone(), Asset::currency("USD"));
    collector.add_balance_change(ts, account_id.clone(), Asset::equity("AAPL"));

    let points = collector.into_change_points();
    assert_eq!(points.len(), 1);
    assert_eq!(points[0].triggers.len(), 2);
}

#[test]
fn date_to_timestamp_uses_end_of_day() {
    let date = chrono::NaiveDate::from_ymd_opt(2026, 2, 1).unwrap();
    let ts = date_to_timestamp(date);
    assert_eq!(ts.date_naive(), date);
    assert_eq!(ts.hour(), 23);
    assert_eq!(ts.minute(), 59);
    assert_eq!(ts.second(), 59);
}

#[test]
fn price_to_change_timestamp_uses_quote_timestamp() {
    let quote_ts = make_ts(2026, 2, 1, 14, 15);
    let price = PricePoint {
        asset_id: AssetId::from_asset(&Asset::equity("AAPL")),
        as_of_date: chrono::NaiveDate::from_ymd_opt(2026, 2, 1).unwrap(),
        timestamp: quote_ts,
        price: "200".to_string(),
        quote_currency: "USD".to_string(),
        kind: PriceKind::Quote,
        source: "test".to_string(),
    };

    assert_eq!(price_to_change_timestamp(&price), quote_ts);
}

#[test]
fn price_to_change_timestamp_ignores_kind() {
    let ts = make_ts(2026, 2, 5, 9, 0);
    let price = PricePoint {
        asset_id: AssetId::from_asset(&Asset::equity("AAPL")),
        as_of_date: chrono::NaiveDate::from_ymd_opt(2026, 2, 1).unwrap(),
        timestamp: ts,
        price: "200".to_string(),
        quote_currency: "USD".to_string(),
        kind: PriceKind::Close,
        source: "test".to_string(),
    };

    assert_eq!(price_to_change_timestamp(&price), ts);
}

#[test]
fn collector_sorts_by_timestamp() {
    let mut collector = ChangePointCollector::new();
    let account_id = Id::new();

    // Add out of order
    collector.add_balance_change(
        make_ts(2026, 1, 15, 12, 0),
        account_id.clone(),
        Asset::currency("USD"),
    );
    collector.add_balance_change(
        make_ts(2026, 1, 15, 10, 0),
        account_id.clone(),
        Asset::currency("USD"),
    );
    collector.add_balance_change(
        make_ts(2026, 1, 15, 11, 0),
        account_id.clone(),
        Asset::currency("USD"),
    );

    let points = collector.into_change_points();
    assert_eq!(points.len(), 3);
    assert!(points[0].timestamp < points[1].timestamp);
    assert!(points[1].timestamp < points[2].timestamp);
}

#[test]
fn filter_daily_granularity() {
    let mut collector = ChangePointCollector::new();
    let account_id = Id::new();

    // Multiple points on same day
    collector.add_balance_change(
        make_ts(2026, 1, 15, 10, 0),
        account_id.clone(),
        Asset::currency("USD"),
    );
    collector.add_balance_change(
        make_ts(2026, 1, 15, 14, 0),
        account_id.clone(),
        Asset::currency("USD"),
    );
    collector.add_balance_change(
        make_ts(2026, 1, 15, 18, 0),
        account_id.clone(),
        Asset::currency("USD"),
    );
    // One point on different day
    collector.add_balance_change(
        make_ts(2026, 1, 16, 9, 0),
        account_id.clone(),
        Asset::currency("USD"),
    );

    let points = collector.into_change_points();
    assert_eq!(points.len(), 4);

    // Filter to daily with "last" strategy
    let filtered = filter_by_granularity(points, Granularity::Daily, CoalesceStrategy::Last);
    assert_eq!(filtered.len(), 2);
    // Should keep 18:00 from Jan 15 and 9:00 from Jan 16
    assert_eq!(filtered[0].timestamp.hour(), 18);
    assert_eq!(filtered[1].timestamp.day(), 16);
}

#[test]
fn filter_date_range() {
    let mut collector = ChangePointCollector::new();
    let account_id = Id::new();

    collector.add_balance_change(
        make_ts(2026, 1, 10, 10, 0),
        account_id.clone(),
        Asset::currency("USD"),
    );
    collector.add_balance_change(
        make_ts(2026, 1, 15, 10, 0),
        account_id.clone(),
        Asset::currency("USD"),
    );
    collector.add_balance_change(
        make_ts(2026, 1, 20, 10, 0),
        account_id.clone(),
        Asset::currency("USD"),
    );

    let points = collector.into_change_points();

    let start = NaiveDate::from_ymd_opt(2026, 1, 12);
    let end = NaiveDate::from_ymd_opt(2026, 1, 18);

    let filtered = filter_by_date_range(points, start, end);
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].timestamp.day(), 15);
}

#[test]
fn filter_weekly_granularity() {
    let mut collector = ChangePointCollector::new();
    let account_id = Id::new();

    // Points across 3 weeks
    collector.add_balance_change(
        make_ts(2026, 1, 5, 10, 0), // Week 1
        account_id.clone(),
        Asset::currency("USD"),
    );
    collector.add_balance_change(
        make_ts(2026, 1, 6, 10, 0), // Week 1
        account_id.clone(),
        Asset::currency("USD"),
    );
    collector.add_balance_change(
        make_ts(2026, 1, 12, 10, 0), // Week 2
        account_id.clone(),
        Asset::currency("USD"),
    );
    collector.add_balance_change(
        make_ts(2026, 1, 20, 10, 0), // Week 3
        account_id.clone(),
        Asset::currency("USD"),
    );

    let points = collector.into_change_points();
    assert_eq!(points.len(), 4);

    let filtered = filter_by_granularity(points, Granularity::Weekly, CoalesceStrategy::Last);
    assert_eq!(filtered.len(), 3);
}

#[test]
fn filter_monthly_granularity() {
    let mut collector = ChangePointCollector::new();
    let account_id = Id::new();

    // Points across 3 months
    collector.add_balance_change(
        make_ts(2026, 1, 15, 10, 0),
        account_id.clone(),
        Asset::currency("USD"),
    );
    collector.add_balance_change(
        make_ts(2026, 1, 20, 10, 0),
        account_id.clone(),
        Asset::currency("USD"),
    );
    collector.add_balance_change(
        make_ts(2026, 2, 10, 10, 0),
        account_id.clone(),
        Asset::currency("USD"),
    );
    collector.add_balance_change(
        make_ts(2026, 3, 5, 10, 0),
        account_id.clone(),
        Asset::currency("USD"),
    );

    let points = collector.into_change_points();
    assert_eq!(points.len(), 4);

    let filtered = filter_by_granularity(points, Granularity::Monthly, CoalesceStrategy::Last);
    assert_eq!(filtered.len(), 3);
    // Should keep Jan 20, Feb 10, Mar 5
    assert_eq!(filtered[0].timestamp.day(), 20);
    assert_eq!(filtered[0].timestamp.month(), 1);
    assert_eq!(filtered[1].timestamp.month(), 2);
    assert_eq!(filtered[2].timestamp.month(), 3);
}

#[test]
fn filter_yearly_granularity() {
    let mut collector = ChangePointCollector::new();
    let account_id = Id::new();

    // Points across 2 years
    collector.add_balance_change(
        make_ts(2025, 6, 15, 10, 0),
        account_id.clone(),
        Asset::currency("USD"),
    );
    collector.add_balance_change(
        make_ts(2025, 12, 20, 10, 0),
        account_id.clone(),
        Asset::currency("USD"),
    );
    collector.add_balance_change(
        make_ts(2026, 3, 10, 10, 0),
        account_id.clone(),
        Asset::currency("USD"),
    );

    let points = collector.into_change_points();
    assert_eq!(points.len(), 3);

    let filtered = filter_by_granularity(points, Granularity::Yearly, CoalesceStrategy::Last);
    assert_eq!(filtered.len(), 2);
    // Should keep Dec 20 2025 and Mar 10 2026
    assert_eq!(filtered[0].timestamp.year(), 2025);
    assert_eq!(filtered[0].timestamp.month(), 12);
    assert_eq!(filtered[1].timestamp.year(), 2026);
}

#[test]
fn filter_custom_granularity_zero_duration_returns_input() {
    let mut collector = ChangePointCollector::new();
    let account_id = Id::new();

    collector.add_balance_change(
        make_ts(2026, 1, 15, 10, 0),
        account_id.clone(),
        Asset::currency("USD"),
    );
    collector.add_balance_change(
        make_ts(2026, 1, 15, 11, 0),
        account_id.clone(),
        Asset::currency("USD"),
    );

    let points = collector.into_change_points();
    let original_len = points.len();
    let first_timestamp = points.first().map(|p| p.timestamp);

    let filtered = filter_by_granularity(
        points,
        Granularity::Custom(Duration::zero()),
        CoalesceStrategy::Last,
    );

    assert_eq!(filtered.len(), original_len);
    assert_eq!(filtered.first().map(|p| p.timestamp), first_timestamp);
}

#[tokio::test]
async fn collect_change_points_excludes_accounts_marked_from_portfolio() -> Result<()> {
    let storage: Arc<dyn Storage> = Arc::new(MemoryStorage::new());
    let market_data: Arc<dyn MarketDataStore> = Arc::new(MemoryMarketDataStore::new());

    let conn_id = Id::from_string("conn-1");
    storage
        .save_connection(&Connection {
            config: ConnectionConfig {
                name: "Test".to_string(),
                synchronizer: "manual".to_string(),
                credentials: None,
                balance_staleness: None,
            },
            state: ConnectionState::new_with(conn_id.clone(), make_ts(2024, 1, 1, 0, 0)),
        })
        .await?;

    let included_id = Id::from_string("acct-1");
    let excluded_id = Id::from_string("acct-2");
    storage
        .save_account(&Account::new_with(
            included_id.clone(),
            make_ts(2024, 1, 1, 0, 0),
            "Checking",
            conn_id.clone(),
        ))
        .await?;
    storage
        .save_account(&Account::new_with(
            excluded_id.clone(),
            make_ts(2024, 1, 1, 0, 0),
            "Mortgage",
            conn_id,
        ))
        .await?;
    storage
        .save_account_config(
            &excluded_id,
            &AccountConfig {
                exclude_from_portfolio: Some(true),
                ..AccountConfig::default()
            },
        )
        .await?;

    storage
        .append_balance_snapshot(
            &included_id,
            &crate::models::BalanceSnapshot::new(
                make_ts(2024, 6, 15, 10, 0),
                vec![AssetBalance::new(Asset::currency("USD"), "1000")],
            ),
        )
        .await?;
    storage
        .append_balance_snapshot(
            &excluded_id,
            &crate::models::BalanceSnapshot::new(
                make_ts(2024, 6, 16, 10, 0),
                vec![AssetBalance::new(Asset::currency("USD"), "-500")],
            ),
        )
        .await?;

    let points = collect_change_points(&storage, &market_data, &CollectOptions::default()).await?;
    assert_eq!(points.len(), 1);
    assert_eq!(points[0].timestamp, make_ts(2024, 6, 15, 10, 0));

    Ok(())
}

#[tokio::test]
async fn collect_change_points_orders_same_timestamp_price_triggers_by_asset_id() -> Result<()> {
    let storage: Arc<dyn Storage> = Arc::new(MemoryStorage::new());
    let market_data: Arc<dyn MarketDataStore> = Arc::new(MemoryMarketDataStore::new());

    let conn_id = Id::from_string("conn-1");
    let account_id = Id::from_string("acct-1");

    storage
        .save_connection(&Connection {
            config: ConnectionConfig {
                name: "Test".to_string(),
                synchronizer: "manual".to_string(),
                credentials: None,
                balance_staleness: None,
            },
            state: ConnectionState::new_with(conn_id.clone(), make_ts(2024, 1, 1, 0, 0)),
        })
        .await?;

    storage
        .save_account(&Account::new_with(
            account_id.clone(),
            make_ts(2024, 1, 1, 0, 0),
            "Brokerage",
            conn_id,
        ))
        .await?;

    // Intentionally add holdings in reverse lexical order.
    storage
        .append_balance_snapshot(
            &account_id,
            &crate::models::BalanceSnapshot::new(
                make_ts(2024, 6, 15, 10, 0),
                vec![
                    AssetBalance::new(Asset::equity("VXUS"), "1"),
                    AssetBalance::new(Asset::equity("GOOGL"), "1"),
                ],
            ),
        )
        .await?;

    market_data
        .put_prices(&[
            PricePoint {
                asset_id: AssetId::from_asset(&Asset::equity("VXUS")),
                as_of_date: chrono::NaiveDate::from_ymd_opt(2024, 6, 15).unwrap(),
                timestamp: make_ts(2024, 6, 15, 16, 0),
                price: "60".to_string(),
                quote_currency: "USD".to_string(),
                kind: PriceKind::Close,
                source: "test".to_string(),
            },
            PricePoint {
                asset_id: AssetId::from_asset(&Asset::equity("GOOGL")),
                as_of_date: chrono::NaiveDate::from_ymd_opt(2024, 6, 15).unwrap(),
                timestamp: make_ts(2024, 6, 15, 16, 0),
                price: "170".to_string(),
                quote_currency: "USD".to_string(),
                kind: PriceKind::Close,
                source: "test".to_string(),
            },
        ])
        .await?;

    let points = collect_change_points(
        &storage,
        &market_data,
        &CollectOptions {
            account_ids: vec![account_id],
            include_prices: true,
            include_fx: false,
            target_currency: None,
        },
    )
    .await?;

    let expected_price_ts = make_ts(2024, 6, 15, 16, 0);
    let price_point = points
        .iter()
        .find(|p| p.timestamp == expected_price_ts)
        .cloned()
        .expect("price change point should exist");

    let price_ids: Vec<String> = price_point
        .triggers
        .iter()
        .filter_map(|trigger| match trigger {
            ChangeTrigger::Price { asset_id } => Some(asset_id.to_string()),
            _ => None,
        })
        .collect();

    assert_eq!(
        price_ids,
        vec!["equity/GOOGL".to_string(), "equity/VXUS".to_string()]
    );

    Ok(())
}

#[tokio::test]
async fn collect_change_points_preserves_intraday_quote_timestamps() -> Result<()> {
    let storage: Arc<dyn Storage> = Arc::new(MemoryStorage::new());
    let market_data: Arc<dyn MarketDataStore> = Arc::new(MemoryMarketDataStore::new());

    let conn_id = Id::from_string("conn-1");
    let account_id = Id::from_string("acct-1");

    storage
        .save_connection(&Connection {
            config: ConnectionConfig {
                name: "Test".to_string(),
                synchronizer: "manual".to_string(),
                credentials: None,
                balance_staleness: None,
            },
            state: ConnectionState::new_with(conn_id.clone(), make_ts(2024, 1, 1, 0, 0)),
        })
        .await?;

    storage
        .save_account(&Account::new_with(
            account_id.clone(),
            make_ts(2024, 1, 1, 0, 0),
            "Brokerage",
            conn_id,
        ))
        .await?;

    storage
        .append_balance_snapshot(
            &account_id,
            &crate::models::BalanceSnapshot::new(
                make_ts(2024, 6, 15, 10, 0),
                vec![AssetBalance::new(Asset::equity("AAPL"), "1")],
            ),
        )
        .await?;

    let quote_ts = make_ts(2024, 6, 15, 16, 0);
    market_data
        .put_prices(&[PricePoint {
            asset_id: AssetId::from_asset(&Asset::equity("AAPL")),
            as_of_date: chrono::NaiveDate::from_ymd_opt(2024, 6, 15).unwrap(),
            timestamp: quote_ts,
            price: "190".to_string(),
            quote_currency: "USD".to_string(),
            kind: PriceKind::Quote,
            source: "test".to_string(),
        }])
        .await?;

    let points = collect_change_points(
        &storage,
        &market_data,
        &CollectOptions {
            account_ids: vec![account_id],
            include_prices: true,
            include_fx: false,
            target_currency: None,
        },
    )
    .await?;

    let price_point = points
        .iter()
        .find(|p| {
            p.timestamp == quote_ts
                && p.triggers
                    .iter()
                    .any(|trigger| matches!(trigger, ChangeTrigger::Price { .. }))
        })
        .cloned()
        .expect("quote-triggered change point should exist");

    assert_eq!(price_point.timestamp, quote_ts);

    Ok(())
}
