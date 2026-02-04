// src/portfolio/change_points.rs
//! Primitives for identifying all points in time where portfolio value could have changed.

use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;

use anyhow::Result;
use chrono::{DateTime, Duration, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

use crate::market_data::{AssetId, MarketDataStore};
use crate::models::{Asset, Id};
use crate::storage::Storage;

/// A point in time where portfolio value could have changed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangePoint {
    /// The timestamp of the change.
    pub timestamp: DateTime<Utc>,
    /// What triggered this change point.
    pub triggers: Vec<ChangeTrigger>,
}

/// What caused a change point.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ChangeTrigger {
    /// A balance changed for an account.
    Balance { account_id: Id, asset: Asset },
    /// A price changed for an asset.
    Price { asset_id: AssetId },
    /// An FX rate changed.
    FxRate { base: String, quote: String },
}

/// Builder for collecting change points from various sources.
#[derive(Debug, Default)]
pub struct ChangePointCollector {
    /// Map from timestamp to triggers at that timestamp.
    /// Using BTreeMap to keep timestamps sorted.
    points: BTreeMap<DateTime<Utc>, Vec<ChangeTrigger>>,
    /// Track which assets have been held (to know which prices matter).
    held_assets: HashSet<AssetId>,
}

impl ChangePointCollector {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a balance change point.
    pub fn add_balance_change(&mut self, timestamp: DateTime<Utc>, account_id: Id, asset: Asset) {
        // Track that this asset was held
        self.held_assets.insert(AssetId::from_asset(&asset));

        let trigger = ChangeTrigger::Balance { account_id, asset };
        self.points.entry(timestamp).or_default().push(trigger);
    }

    /// Add a price change point.
    pub fn add_price_change(&mut self, timestamp: DateTime<Utc>, asset_id: AssetId) {
        let trigger = ChangeTrigger::Price { asset_id };
        self.points.entry(timestamp).or_default().push(trigger);
    }

    /// Add an FX rate change point.
    pub fn add_fx_change(&mut self, timestamp: DateTime<Utc>, base: String, quote: String) {
        let trigger = ChangeTrigger::FxRate { base, quote };
        self.points.entry(timestamp).or_default().push(trigger);
    }

    /// Get the set of assets that have been held.
    pub fn held_assets(&self) -> &HashSet<AssetId> {
        &self.held_assets
    }

    /// Consume the collector and return sorted change points.
    pub fn into_change_points(self) -> Vec<ChangePoint> {
        self.points
            .into_iter()
            .map(|(timestamp, triggers)| ChangePoint {
                timestamp,
                triggers,
            })
            .collect()
    }

    /// Get the number of change points collected.
    pub fn len(&self) -> usize {
        self.points.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }
}

/// Granularity for filtering change points.
#[derive(Debug, Clone, Copy)]
pub enum Granularity {
    /// Keep all change points (no filtering).
    Full,
    /// At most one change point per hour.
    Hourly,
    /// At most one change point per day.
    Daily,
    /// At most one change point per week.
    Weekly,
    /// At most one change point per month.
    Monthly,
    /// At most one change point per year.
    Yearly,
    /// Custom duration bucket.
    Custom(Duration),
}

/// Strategy for selecting which point to keep when coalescing.
#[derive(Debug, Clone, Copy, Default)]
pub enum CoalesceStrategy {
    /// Keep the first point in each bucket.
    First,
    /// Keep the last point in each bucket (default).
    #[default]
    Last,
}

/// Bucket key for calendar-based granularities.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum BucketKey {
    /// Duration-based bucket (timestamp / bucket_seconds)
    Duration(i64),
    /// Monthly bucket (year, month)
    Month(i32, u32),
    /// Yearly bucket
    Year(i32),
}

/// Filter change points to a desired granularity.
pub fn filter_by_granularity(
    points: Vec<ChangePoint>,
    granularity: Granularity,
    strategy: CoalesceStrategy,
) -> Vec<ChangePoint> {
    use chrono::Datelike;

    if points.is_empty() {
        return points;
    }

    // Full granularity - return as-is
    if matches!(granularity, Granularity::Full) {
        return points;
    }

    // Group points into buckets
    let mut buckets: BTreeMap<BucketKey, Vec<ChangePoint>> = BTreeMap::new();

    for point in points {
        let bucket_key = match granularity {
            Granularity::Full => unreachable!(),
            Granularity::Hourly => BucketKey::Duration(point.timestamp.timestamp() / 3600),
            Granularity::Daily => BucketKey::Duration(point.timestamp.timestamp() / 86400),
            Granularity::Weekly => {
                // Bucket by week (7 days from epoch)
                BucketKey::Duration(point.timestamp.timestamp() / (86400 * 7))
            }
            Granularity::Monthly => {
                let date = point.timestamp.date_naive();
                BucketKey::Month(date.year(), date.month())
            }
            Granularity::Yearly => {
                let date = point.timestamp.date_naive();
                BucketKey::Year(date.year())
            }
            Granularity::Custom(duration) => {
                let bucket_seconds = duration.num_seconds();
                BucketKey::Duration(point.timestamp.timestamp() / bucket_seconds)
            }
        };
        buckets.entry(bucket_key).or_default().push(point);
    }

    // Select one point per bucket based on strategy
    buckets
        .into_values()
        .filter_map(|mut bucket_points| {
            if bucket_points.is_empty() {
                return None;
            }
            match strategy {
                CoalesceStrategy::First => Some(bucket_points.remove(0)),
                CoalesceStrategy::Last => Some(bucket_points.pop().unwrap()),
            }
        })
        .collect()
}

/// Filter change points to only include those within a date range.
pub fn filter_by_date_range(
    points: Vec<ChangePoint>,
    start: Option<NaiveDate>,
    end: Option<NaiveDate>,
) -> Vec<ChangePoint> {
    points
        .into_iter()
        .filter(|p| {
            let date = p.timestamp.date_naive();
            if let Some(s) = start {
                if date < s {
                    return false;
                }
            }
            if let Some(e) = end {
                if date > e {
                    return false;
                }
            }
            true
        })
        .collect()
}

/// Options for collecting change points.
#[derive(Debug, Clone, Default)]
pub struct CollectOptions {
    /// Only collect from these account IDs. If empty, collect from all accounts.
    pub account_ids: Vec<Id>,
    /// Include price change points (requires loading price history).
    pub include_prices: bool,
    /// Include FX rate change points (requires loading FX history).
    /// Note: Currently we don't track which FX pairs are needed, so this is a placeholder.
    pub include_fx: bool,
    /// Target currency for FX tracking (needed to know which FX pairs matter).
    pub target_currency: Option<String>,
}

/// Collect all change points from storage and market data.
///
/// This is the main entry point for gathering all timestamps where
/// portfolio value could have changed.
pub async fn collect_change_points(
    storage: &Arc<dyn Storage>,
    market_data: &Arc<dyn MarketDataStore>,
    options: &CollectOptions,
) -> Result<Vec<ChangePoint>> {
    let mut collector = ChangePointCollector::new();

    // Determine which accounts to process
    let accounts = if options.account_ids.is_empty() {
        storage.list_accounts().await?
    } else {
        let mut accounts = Vec::new();
        for id in &options.account_ids {
            if let Some(account) = storage.get_account(id).await? {
                accounts.push(account);
            }
        }
        accounts
    };

    // Collect balance change points from all accounts
    for account in &accounts {
        let snapshots = storage.get_balance_snapshots(&account.id).await?;
        for snapshot in snapshots {
            for balance in &snapshot.balances {
                collector.add_balance_change(
                    snapshot.timestamp,
                    account.id.clone(),
                    balance.asset.clone(),
                );
            }
        }
    }

    // Collect price change points for held assets
    if options.include_prices {
        let held_assets = collector.held_assets().clone();
        for asset_id in held_assets {
            let prices = market_data.get_all_prices(&asset_id).await?;
            for price in prices {
                collector.add_price_change(price.timestamp, asset_id.clone());
            }
        }
    }

    // TODO: FX rate tracking would require knowing:
    // 1. Which currencies are held (non-target currency holdings)
    // 2. Which quote currencies prices are in (for assets needing FX conversion)
    // For now, this is left as a placeholder for future enhancement.

    Ok(collector.into_change_points())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Datelike, TimeZone, Timelike};

    fn make_ts(year: i32, month: u32, day: u32, hour: u32, min: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(year, month, day, hour, min, 0)
            .unwrap()
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
}
