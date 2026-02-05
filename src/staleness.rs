//! Staleness detection and resolution for balances and prices.

use std::time::Duration;

use chrono::Utc;
use tracing::info;

use crate::config::RefreshConfig;
use crate::market_data::PricePoint;
use crate::models::{AccountConfig, Connection};

/// Result of a staleness check.
#[derive(Debug, Clone)]
pub struct StalenessCheck {
    pub is_stale: bool,
    pub age: Option<Duration>,
    pub threshold: Duration,
}

impl StalenessCheck {
    pub fn stale(age: Duration, threshold: Duration) -> Self {
        Self {
            is_stale: true,
            age: Some(age),
            threshold,
        }
    }

    pub fn fresh(age: Duration, threshold: Duration) -> Self {
        Self {
            is_stale: false,
            age: Some(age),
            threshold,
        }
    }

    pub fn missing(threshold: Duration) -> Self {
        Self {
            is_stale: true,
            age: None,
            threshold,
        }
    }
}

/// Resolve the effective balance staleness threshold for an account.
/// Resolution order: account config -> connection config -> global config.
pub fn resolve_balance_staleness(
    account_config: Option<&AccountConfig>,
    connection: &Connection,
    global_config: &RefreshConfig,
) -> Duration {
    if let Some(config) = account_config {
        if let Some(staleness) = config.balance_staleness {
            return staleness;
        }
    }
    if let Some(staleness) = connection.config.balance_staleness {
        return staleness;
    }
    global_config.balance_staleness
}

/// Check if a connection's balances are stale.
pub fn check_balance_staleness(connection: &Connection, threshold: Duration) -> StalenessCheck {
    let now = Utc::now();
    match &connection.state.last_sync {
        Some(last_sync) => {
            let age = (now - last_sync.at).to_std().unwrap_or(Duration::ZERO);
            if age > threshold {
                StalenessCheck::stale(age, threshold)
            } else {
                StalenessCheck::fresh(age, threshold)
            }
        }
        None => StalenessCheck::missing(threshold),
    }
}

/// Check if a price is stale.
pub fn check_price_staleness(price: Option<&PricePoint>, threshold: Duration) -> StalenessCheck {
    let now = Utc::now();
    match price {
        Some(p) => {
            let age = (now - p.timestamp).to_std().unwrap_or(Duration::ZERO);
            if age > threshold {
                StalenessCheck::stale(age, threshold)
            } else {
                StalenessCheck::fresh(age, threshold)
            }
        }
        None => StalenessCheck::missing(threshold),
    }
}

/// Log staleness check results for a connection's balances.
pub fn log_balance_staleness(connection_name: &str, check: &StalenessCheck) {
    let status = if check.is_stale { "stale" } else { "fresh" };
    let age_str = check
        .age
        .map(crate::duration::format_duration)
        .unwrap_or_else(|| "never".to_string());
    let threshold_str = crate::duration::format_duration(check.threshold);

    info!(
        connection = connection_name,
        age = %age_str,
        threshold = %threshold_str,
        status = status,
        "balance staleness check"
    );
}

/// Log price staleness check results.
pub fn log_price_staleness(asset_id: &str, check: &StalenessCheck) {
    let status = if check.is_stale { "stale" } else { "fresh" };
    let age_str = check
        .age
        .map(crate::duration::format_duration)
        .unwrap_or_else(|| "never".to_string());
    let threshold_str = crate::duration::format_duration(check.threshold);

    info!(
        asset = asset_id,
        age = %age_str,
        threshold = %threshold_str,
        status = status,
        "price staleness check"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::market_data::{AssetId, PriceKind, PricePoint};
    use crate::models::Asset;
    use crate::models::{ConnectionConfig, ConnectionState, LastSync, SyncStatus};

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
    fn test_resolve_account_override() {
        let account_config = AccountConfig {
            balance_staleness: Some(Duration::from_secs(7 * 24 * 60 * 60)),
            balance_backfill: None,
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
}
