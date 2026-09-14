// src/portfolio/change_points.rs
//! Primitives for identifying all points in time where portfolio value could have changed.

use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;

use anyhow::Result;
use chrono::{DateTime, Duration, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

use crate::market_data::PricePoint;
use crate::market_data::{AssetId, MarketDataStore};
use crate::models::{Account, Asset, Id};
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
///
/// The date range and granularity a caller asked for are applied as
/// observations arrive, so a bounded or coarse query never materializes the
/// triggers it would immediately discard.
#[derive(Debug, Default)]
pub struct ChangePointCollector {
    /// Map from coalescing bucket to the point kept for it. Bucket order is
    /// timestamp order for a fixed granularity, so this keeps points sorted.
    points: BTreeMap<BucketKey, ChangePoint>,
    /// Track which assets have been held (to know which prices matter).
    held_assets: HashSet<AssetId>,
    start: Option<NaiveDate>,
    end: Option<NaiveDate>,
    granularity: Granularity,
    strategy: CoalesceStrategy,
    /// The last timestamp classified and where it landed.
    classified: Option<(DateTime<Utc>, Option<BucketKey>)>,
}

impl ChangePointCollector {
    pub fn new() -> Self {
        Self::default()
    }

    /// Drop observations outside `[start, end]` (inclusive, by date).
    pub fn within(mut self, start: Option<NaiveDate>, end: Option<NaiveDate>) -> Self {
        self.start = start;
        self.end = end;
        self
    }

    /// Keep at most one point per `granularity` bucket, chosen by `strategy`.
    pub fn coalesced(mut self, granularity: Granularity, strategy: CoalesceStrategy) -> Self {
        self.granularity = granularity;
        self.strategy = strategy;
        self
    }

    /// Add a balance change point.
    pub fn add_balance_change(&mut self, timestamp: DateTime<Utc>, account_id: Id, asset: Asset) {
        // Track that this asset was held. A price outside the window can still
        // be the last one before it, so this is independent of the range.
        self.held_assets.insert(AssetId::from_asset(&asset));

        self.add_trigger(timestamp, || ChangeTrigger::Balance { account_id, asset });
    }

    /// Add a price change point.
    pub fn add_price_change(&mut self, timestamp: DateTime<Utc>, asset_id: AssetId) {
        self.add_trigger(timestamp, || ChangeTrigger::Price { asset_id });
    }

    /// Add an FX rate change point.
    pub fn add_fx_change(&mut self, timestamp: DateTime<Utc>, base: String, quote: String) {
        self.add_trigger(timestamp, || ChangeTrigger::FxRate { base, quote });
    }

    /// The bucket an observation lands in, or `None` if the range excludes it.
    ///
    /// Every balance in a snapshot shares its timestamp, so remembering the
    /// last answer skips most of the calendar arithmetic.
    fn classify(&mut self, timestamp: DateTime<Utc>) -> Option<BucketKey> {
        if let Some((cached, bucket)) = self.classified {
            if cached == timestamp {
                return bucket;
            }
        }

        let bucket = in_date_range(timestamp, self.start, self.end)
            .then(|| bucket_key(timestamp, self.granularity));
        self.classified = Some((timestamp, bucket));
        bucket
    }

    /// Record a trigger, building it only if its point survives filtering.
    fn add_trigger(&mut self, timestamp: DateTime<Utc>, trigger: impl FnOnce() -> ChangeTrigger) {
        let Some(bucket) = self.classify(timestamp) else {
            return;
        };

        match self.points.entry(bucket) {
            Entry::Vacant(entry) => {
                entry.insert(ChangePoint {
                    timestamp,
                    triggers: vec![trigger()],
                });
            }
            Entry::Occupied(mut entry) => {
                let kept = entry.get_mut();
                if timestamp == kept.timestamp {
                    kept.triggers.push(trigger());
                } else if wins_bucket(self.strategy, timestamp, kept.timestamp) {
                    *kept = ChangePoint {
                        timestamp,
                        triggers: vec![trigger()],
                    };
                }
            }
        }
    }

    /// Get the set of assets that have been held.
    pub fn held_assets(&self) -> &HashSet<AssetId> {
        &self.held_assets
    }

    /// Consume the collector and return sorted change points.
    pub fn into_change_points(self) -> Vec<ChangePoint> {
        self.points.into_values().collect()
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

/// Whether `candidate` should replace `kept` within one bucket.
fn wins_bucket(strategy: CoalesceStrategy, candidate: DateTime<Utc>, kept: DateTime<Utc>) -> bool {
    match strategy {
        CoalesceStrategy::First => candidate < kept,
        CoalesceStrategy::Last => candidate > kept,
    }
}

fn in_date_range(
    timestamp: DateTime<Utc>,
    start: Option<NaiveDate>,
    end: Option<NaiveDate>,
) -> bool {
    if start.is_none() && end.is_none() {
        return true;
    }

    let date = timestamp.date_naive();
    if let Some(start) = start {
        if date < start {
            return false;
        }
    }
    if let Some(end) = end {
        if date > end {
            return false;
        }
    }
    true
}

fn price_to_change_timestamp(price: &PricePoint) -> DateTime<Utc> {
    price.timestamp
}

/// Granularity for filtering change points.
#[derive(Debug, Clone, Copy, Default)]
pub enum Granularity {
    /// Keep all change points (no filtering).
    #[default]
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
    /// One bucket per instant, for granularities that coalesce nothing.
    Instant(DateTime<Utc>),
}

/// The bucket a timestamp coalesces into. Within one granularity, bucket order
/// is timestamp order.
fn bucket_key(timestamp: DateTime<Utc>, granularity: Granularity) -> BucketKey {
    use chrono::Datelike;

    match granularity {
        Granularity::Full => BucketKey::Instant(timestamp),
        Granularity::Hourly => BucketKey::Duration(timestamp.timestamp() / 3600),
        Granularity::Daily => BucketKey::Duration(timestamp.timestamp() / 86400),
        Granularity::Weekly => BucketKey::Duration(timestamp.timestamp() / (86400 * 7)),
        Granularity::Monthly => {
            let date = timestamp.date_naive();
            BucketKey::Month(date.year(), date.month())
        }
        Granularity::Yearly => BucketKey::Year(timestamp.date_naive().year()),
        Granularity::Custom(duration) => match duration.num_seconds() {
            seconds if seconds > 0 => BucketKey::Duration(timestamp.timestamp() / seconds),
            _ => BucketKey::Instant(timestamp),
        },
    }
}

/// Whether a granularity puts every instant in its own bucket.
fn coalesces_nothing(granularity: Granularity) -> bool {
    match granularity {
        Granularity::Full => true,
        Granularity::Custom(duration) => duration.num_seconds() <= 0,
        _ => false,
    }
}

/// Filter change points to a desired granularity.
///
/// Prefer bounding the collection itself through [`CollectOptions`]; this
/// exists for callers that already hold materialized points.
pub fn filter_by_granularity(
    points: Vec<ChangePoint>,
    granularity: Granularity,
    strategy: CoalesceStrategy,
) -> Vec<ChangePoint> {
    if points.is_empty() || coalesces_nothing(granularity) {
        return points;
    }

    let mut buckets: BTreeMap<BucketKey, Vec<ChangePoint>> = BTreeMap::new();
    for point in points {
        buckets
            .entry(bucket_key(point.timestamp, granularity))
            .or_default()
            .push(point);
    }

    buckets
        .into_values()
        .filter_map(|mut bucket_points| match strategy {
            CoalesceStrategy::First => bucket_points.into_iter().next(),
            CoalesceStrategy::Last => bucket_points.pop(),
        })
        .collect()
}

/// Filter change points to only include those within a date range.
///
/// Prefer bounding the collection itself through [`CollectOptions`]; this
/// exists for callers that already hold materialized points.
pub fn filter_by_date_range(
    points: Vec<ChangePoint>,
    start: Option<NaiveDate>,
    end: Option<NaiveDate>,
) -> Vec<ChangePoint> {
    points
        .into_iter()
        .filter(|point| in_date_range(point.timestamp, start, end))
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
    /// Earliest date to report, inclusive. Unbounded when absent.
    pub start: Option<NaiveDate>,
    /// Latest date to report, inclusive. Unbounded when absent.
    pub end: Option<NaiveDate>,
    /// Coalesce reported points to this granularity.
    pub granularity: Granularity,
    /// Which point in a granularity bucket to keep.
    pub strategy: CoalesceStrategy,
}

/// The accounts a collection covers: the requested ones, or every account, less
/// the ones excluded from the portfolio.
async fn selected_accounts(
    storage: &Arc<dyn Storage>,
    options: &CollectOptions,
) -> Result<Vec<Account>> {
    let candidates = if options.account_ids.is_empty() {
        storage.list_accounts().await?
    } else {
        let mut candidates = Vec::new();
        for id in &options.account_ids {
            if let Some(account) = storage.get_account(id).await? {
                candidates.push(account);
            }
        }
        candidates
    };

    let mut accounts = Vec::with_capacity(candidates.len());
    for account in candidates {
        let excluded = storage
            .get_account_config(&account.id)?
            .and_then(|config| config.exclude_from_portfolio)
            .unwrap_or(false);
        if !excluded {
            accounts.push(account);
        }
    }
    Ok(accounts)
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
    let mut collector = ChangePointCollector::new()
        .within(options.start, options.end)
        .coalesced(options.granularity, options.strategy);

    // Collect balance change points from all accounts
    for account in selected_accounts(storage, options).await? {
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
        let mut held_assets: Vec<AssetId> = collector.held_assets().iter().cloned().collect();
        held_assets.sort_by_key(|asset_id| asset_id.to_string());
        for asset_id in held_assets {
            // Prices are ordered by as-of date rather than strictly by
            // timestamp, so the range has to be checked per price.
            for price in market_data.all_prices_shared(&asset_id).await?.iter() {
                collector.add_price_change(price_to_change_timestamp(price), asset_id.clone());
            }
        }
    }

    // TODO: FX rate tracking would require knowing:
    // 1. Which currencies are held (non-target currency holdings)
    // 2. Which quote currencies prices are in (for assets needing FX conversion)
    // For now, this is left as a placeholder for future enhancement.

    Ok(collector.into_change_points())
}

/// The timestamp of the first change point [`collect_change_points`] would
/// report at full granularity, without materializing any of them.
///
/// Coalescing can move a bucket's kept point later, so this ignores
/// `options.granularity` and `options.strategy`.
pub async fn earliest_change_point(
    storage: &Arc<dyn Storage>,
    market_data: &Arc<dyn MarketDataStore>,
    options: &CollectOptions,
) -> Result<Option<DateTime<Utc>>> {
    let mut earliest: Option<DateTime<Utc>> = None;
    let mut held_assets: HashSet<AssetId> = HashSet::new();

    let observe = |timestamp: DateTime<Utc>, earliest: &mut Option<DateTime<Utc>>| {
        if in_date_range(timestamp, options.start, options.end)
            && earliest.is_none_or(|current| timestamp < current)
        {
            *earliest = Some(timestamp);
        }
    };

    for account in selected_accounts(storage, options).await? {
        for snapshot in storage.get_balance_snapshots(&account.id).await? {
            for balance in &snapshot.balances {
                held_assets.insert(AssetId::from_asset(&balance.asset));
            }
            if !snapshot.balances.is_empty() {
                observe(snapshot.timestamp, &mut earliest);
            }
        }
    }

    if options.include_prices {
        for asset_id in &held_assets {
            for price in market_data.all_prices_shared(asset_id).await?.iter() {
                observe(price_to_change_timestamp(price), &mut earliest);
            }
        }
    }

    Ok(earliest)
}

#[cfg(test)]
#[path = "../../tests/unit/portfolio/change_points_tests.rs"]
mod change_points_tests;
