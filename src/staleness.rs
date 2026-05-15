//! Staleness detection and resolution for balances and prices.

use std::time::Duration;

use chrono::{DateTime, Utc};
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
pub fn check_balance_staleness_at(
    connection: &Connection,
    threshold: Duration,
    now: DateTime<Utc>,
) -> StalenessCheck {
    match &connection.state.last_sync {
        Some(last_sync) => {
            let age = (now - last_sync.at).to_std().unwrap_or(Duration::ZERO);
            if age >= threshold {
                StalenessCheck::stale(age, threshold)
            } else {
                StalenessCheck::fresh(age, threshold)
            }
        }
        None => StalenessCheck::missing(threshold),
    }
}

/// Check if a price is stale.
pub fn check_price_staleness_at(
    price: Option<&PricePoint>,
    threshold: Duration,
    now: DateTime<Utc>,
) -> StalenessCheck {
    match price {
        Some(p) => {
            let age = (now - p.timestamp).to_std().unwrap_or(Duration::ZERO);
            if age >= threshold {
                StalenessCheck::stale(age, threshold)
            } else {
                StalenessCheck::fresh(age, threshold)
            }
        }
        None => StalenessCheck::missing(threshold),
    }
}

/// Convenience wrapper that checks staleness relative to `Utc::now()`.
pub fn check_balance_staleness(connection: &Connection, threshold: Duration) -> StalenessCheck {
    check_balance_staleness_at(connection, threshold, Utc::now())
}

/// Convenience wrapper that checks staleness relative to `Utc::now()`.
pub fn check_price_staleness(price: Option<&PricePoint>, threshold: Duration) -> StalenessCheck {
    check_price_staleness_at(price, threshold, Utc::now())
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
#[path = "../tests/unit/staleness_tests.rs"]
mod staleness_tests;
