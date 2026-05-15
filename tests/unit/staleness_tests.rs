use super::*;
use crate::market_data::{AssetId, PriceKind, PricePoint};
use crate::models::Asset;
use crate::models::{ConnectionConfig, ConnectionState, LastSync, SyncStatus};
use chrono::TimeZone;

fn make_connection(last_sync_age_hours: Option<i64>) -> Connection {
    let mut state = ConnectionState::new();
    if let Some(hours) = last_sync_age_hours {
        state.last_sync = Some(LastSync {
            at: Utc::now() - chrono::Duration::hours(hours),
            status: SyncStatus::Success,
            error: None,
        });
    }
    Connection {
        config: ConnectionConfig {
            name: "Test".to_string(),
            synchronizer: "manual".to_string(),
            credentials: None,
            balance_staleness: None,
        },
        state,
    }
}

#[test]
fn test_balance_stale_when_old() {
    let connection = make_connection(Some(48));
    let threshold = Duration::from_secs(24 * 60 * 60);
    let check = check_balance_staleness(&connection, threshold);
    assert!(check.is_stale);
}

#[test]
fn test_balance_fresh_when_recent() {
    let connection = make_connection(Some(12));
    let threshold = Duration::from_secs(24 * 60 * 60);
    let check = check_balance_staleness(&connection, threshold);
    assert!(!check.is_stale);
}

#[test]
fn test_balance_stale_when_never_synced() {
    let connection = make_connection(None);
    let threshold = Duration::from_secs(24 * 60 * 60);
    let check = check_balance_staleness(&connection, threshold);
    assert!(check.is_stale);
    assert!(check.age.is_none());
}

#[test]
fn test_balance_future_timestamp_is_not_stale() {
    let mut connection = make_connection(Some(0));
    if let Some(last_sync) = &mut connection.state.last_sync {
        last_sync.at = Utc::now() + chrono::Duration::hours(1);
    }
    let threshold = Duration::from_secs(24 * 60 * 60);
    let check = check_balance_staleness(&connection, threshold);
    assert!(
        !check.is_stale,
        "future last_sync should be treated as fresh"
    );
}

#[test]
fn test_balance_stale_when_age_equals_threshold() {
    let last_sync_at = Utc.with_ymd_and_hms(2026, 2, 1, 0, 0, 0).unwrap();
    let now = last_sync_at + chrono::Duration::hours(1);
    let threshold = Duration::from_secs(60 * 60);

    let mut state = ConnectionState::new();
    state.last_sync = Some(LastSync {
        at: last_sync_at,
        status: SyncStatus::Success,
        error: None,
    });
    let connection = Connection {
        config: ConnectionConfig {
            name: "Test".to_string(),
            synchronizer: "manual".to_string(),
            credentials: None,
            balance_staleness: None,
        },
        state,
    };

    let check = check_balance_staleness_at(&connection, threshold, now);
    assert!(check.is_stale, "age == threshold should be stale");
}

#[test]
fn test_price_stale_when_age_equals_threshold() {
    let threshold = Duration::from_secs(60 * 60);
    let timestamp = Utc.with_ymd_and_hms(2026, 2, 1, 0, 0, 0).unwrap();
    let now = timestamp + chrono::Duration::hours(1);
    let price = PricePoint {
        asset_id: AssetId::from_asset(&Asset::equity("AAPL")),
        as_of_date: chrono::NaiveDate::from_ymd_opt(2026, 2, 1).unwrap(),
        timestamp,
        price: "1".to_string(),
        quote_currency: "USD".to_string(),
        kind: PriceKind::Close,
        source: "test".to_string(),
    };

    let check = check_price_staleness_at(Some(&price), threshold, now);
    assert!(check.is_stale, "age == threshold should be stale");
}

#[test]
fn test_resolve_account_override() {
    let account_config = AccountConfig {
        balance_staleness: Some(Duration::from_secs(7 * 24 * 60 * 60)),
        balance_backfill: None,
        exclude_from_portfolio: None,
    };
    let connection = make_connection(None);
    let global = RefreshConfig::default();
    let result = resolve_balance_staleness(Some(&account_config), &connection, &global);
    assert_eq!(result, Duration::from_secs(7 * 24 * 60 * 60));
}

#[test]
fn test_resolve_connection_override() {
    let mut connection = make_connection(None);
    connection.config.balance_staleness = Some(Duration::from_secs(3 * 24 * 60 * 60));
    let global = RefreshConfig::default();
    let result = resolve_balance_staleness(None, &connection, &global);
    assert_eq!(result, Duration::from_secs(3 * 24 * 60 * 60));
}

#[test]
fn test_resolve_global_default() {
    let connection = make_connection(None);
    let global = RefreshConfig::default();
    let result = resolve_balance_staleness(None, &connection, &global);
    assert_eq!(result, Duration::from_secs(14 * 24 * 60 * 60));
}

fn make_price_point(age_hours: i64) -> PricePoint {
    let asset = Asset::equity("AAPL");
    PricePoint {
        asset_id: AssetId::from_asset(&asset),
        as_of_date: Utc::now().date_naive(),
        timestamp: Utc::now() - chrono::Duration::hours(age_hours),
        price: "123.45".to_string(),
        quote_currency: "USD".to_string(),
        kind: PriceKind::Close,
        source: "test".to_string(),
    }
}

#[test]
fn test_price_stale_when_old() {
    let price = make_price_point(48);
    let threshold = Duration::from_secs(24 * 60 * 60);
    let check = check_price_staleness(Some(&price), threshold);
    assert!(check.is_stale);
}

#[test]
fn test_price_fresh_when_recent() {
    let price = make_price_point(1);
    let threshold = Duration::from_secs(24 * 60 * 60);
    let check = check_price_staleness(Some(&price), threshold);
    assert!(!check.is_stale);
}

#[test]
fn test_price_stale_when_missing() {
    let threshold = Duration::from_secs(24 * 60 * 60);
    let check = check_price_staleness(None, threshold);
    assert!(check.is_stale);
    assert!(check.age.is_none());
}

#[test]
fn test_price_future_timestamp_is_not_stale() {
    let mut price = make_price_point(0);
    price.timestamp = Utc::now() + chrono::Duration::hours(1);
    let threshold = Duration::from_secs(24 * 60 * 60);
    let check = check_price_staleness(Some(&price), threshold);
    assert!(
        !check.is_stale,
        "future price timestamps should be treated as fresh"
    );
}
