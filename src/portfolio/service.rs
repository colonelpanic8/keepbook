// src/portfolio/service.rs
use std::collections::{HashMap, HashSet};
use std::str::FromStr;
use std::sync::Arc;

use anyhow::{Context, Result};
use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;

use crate::clock::{Clock, SystemClock};
use crate::format::format_base_currency_value;
use crate::market_data::{is_market_data_missing, AssetId, MarketDataService};
use crate::models::{Account, Asset, BalanceBackfillPolicy, BalanceSnapshot, Connection, Id};
use crate::storage::Storage;

use super::{
    AccountHolding, AccountSummary, AssetBreakdownAccountHolding, AssetBreakdownRow, AssetSummary,
    EquityValuationAdjustment, Grouping, PortfolioQuery, PortfolioSnapshot,
    PortfolioValuationScenario, ValuationIssue, ValuationIssueReason,
};

pub struct PortfolioService {
    storage: Arc<dyn Storage>,
    market_data: Arc<MarketDataService>,
    clock: Arc<dyn Clock>,
}

/// Valuation result for an asset.
#[derive(Clone)]
struct AssetValuation {
    /// The value in target currency. None if price data unavailable.
    value: Option<Decimal>,
    /// Why `value` is absent, when it is.
    issue: Option<ValuationIssueReason>,
    /// The underlying failure behind a lookup-failed issue.
    issue_message: Option<String>,
    price: Option<String>,
    price_date: Option<NaiveDate>,
    price_timestamp: Option<DateTime<Utc>>,
    fx_rate: Option<String>,
    fx_date: Option<NaiveDate>,
}

impl AssetValuation {
    fn issue(&self, asset: &Asset) -> Option<ValuationIssue> {
        self.issue.map(|reason| ValuationIssue {
            asset: asset.clone(),
            reason,
            message: self.issue_message.clone(),
        })
    }
}

/// Represents a single asset holding from a snapshot.
struct AssetHolding {
    account_id: Id,
    #[allow(dead_code)]
    asset: Asset,
    amount: String,
    cost_basis: Option<String>,
    timestamp: DateTime<Utc>,
}

/// Aggregated data for a single asset across all accounts.
struct AssetAggregate {
    total_amount: Decimal,
    amount_with_cost_basis: Decimal,
    total_cost_basis: Option<Decimal>,
    latest_balance_date: NaiveDate,
    holdings: Vec<AssetHolding>,
}

#[derive(Debug, Default)]
struct GainsTotals {
    total_cost_basis: Option<Decimal>,
    total_unrealized_gain: Option<Decimal>,
    prospective_capital_gains_tax: Option<Decimal>,
}

struct ResolvedValuationScenario {
    multiplier: Decimal,
    output: PortfolioValuationScenario,
}

/// One point in time's view of a [`ValuationHistory`].
struct CalculationContext<'a> {
    account_map: &'a HashMap<Id, Account>,
    connection_map: &'a HashMap<Id, Connection>,
    filtered_snapshots: Vec<(Id, BalanceSnapshot)>,
    zero_accounts: Vec<Id>,
}

/// One account's complete balance history, ordered oldest first.
struct AccountHistory {
    account_id: Id,
    backfill: BalanceBackfillPolicy,
    snapshots: Vec<BalanceSnapshot>,
}

/// Accounts, connections, and balance history read once and reused across a
/// series of valuations.
///
/// Valuing a history point selects one balance snapshot per account. Loading
/// that per point re-reads and re-copies every account's whole balance log,
/// which is most of the work in a long history. Prepared once, each point is a
/// search over sorted snapshots and a copy of only the ones it selects.
pub struct ValuationHistory {
    account_map: HashMap<Id, Account>,
    connection_map: HashMap<Id, Connection>,
    accounts: Vec<AccountHistory>,
}

#[derive(Clone, Copy, Debug, Default)]
struct AssetUpdateMetadata {
    amount_last_checked_at: Option<DateTime<Utc>>,
    amount_last_changed_at: Option<DateTime<Utc>>,
}

impl PortfolioService {
    pub fn new(storage: Arc<dyn Storage>, market_data: Arc<MarketDataService>) -> Self {
        Self {
            storage,
            market_data,
            clock: Arc::new(SystemClock),
        }
    }

    pub fn with_clock(mut self, clock: Arc<dyn Clock>) -> Self {
        self.clock = clock;
        self
    }

    pub async fn calculate(&self, query: &PortfolioQuery) -> Result<PortfolioSnapshot> {
        let history = self.load_valuation_history(&query.account_ids).await?;
        self.calculate_with_history(&history, query).await
    }

    /// Like [`Self::calculate`] but reuses balance history already read for
    /// another point in time. `query.account_ids` must match the history's
    /// scope.
    pub async fn calculate_with_history(
        &self,
        history: &ValuationHistory,
        query: &PortfolioQuery,
    ) -> Result<PortfolioSnapshot> {
        let ctx = Self::calculation_context(history, query.as_of_date, query.as_of_timestamp);

        // Aggregate balances by asset
        let by_asset_agg = Self::aggregate_by_asset(&ctx.filtered_snapshots)?;

        // Fetch valuations for all unique assets (cached)
        let price_cache = self
            .fetch_asset_valuations(
                &by_asset_agg,
                &query.currency,
                query.as_of_date,
                query.as_of_timestamp,
            )
            .await?;

        let valuation_scenario = Self::resolve_equity_valuation_scenario(
            &by_asset_agg,
            &price_cache,
            query.equity_valuation_adjustment.as_ref(),
            query.currency_decimals,
        )?;
        let effective_price_cache = match valuation_scenario.as_ref() {
            Some(scenario) => {
                Self::apply_equity_valuation_multiplier(&price_cache, scenario.multiplier)
            }
            None => price_cache.clone(),
        };

        // Build asset summaries and calculate total value
        let (mut asset_summaries, total_value, gains_totals) = self.build_asset_summaries(
            &by_asset_agg,
            &effective_price_cache,
            ctx.account_map,
            query.include_detail,
            query.currency_decimals,
            query.capital_gains_tax_rate,
        )?;

        // Build account summaries
        let mut account_summaries = Self::build_account_summaries(
            &ctx.filtered_snapshots,
            &ctx.zero_accounts,
            &effective_price_cache,
            ctx.account_map,
            ctx.connection_map,
            query.currency_decimals,
        )?;

        // Sort for consistent output
        account_summaries.sort_by(|a, b| a.account_name.cmp(&b.account_name));
        asset_summaries.sort_by(|a, b| {
            let a_id = AssetId::from_asset(&a.asset);
            let b_id = AssetId::from_asset(&b.asset);
            a_id.as_str().cmp(b_id.as_str())
        });

        let valuation_issues = Self::collect_valuation_issues(&effective_price_cache);

        // Build snapshot based on grouping
        let (by_asset, by_account) = match query.grouping {
            Grouping::Asset => (Some(asset_summaries), None),
            Grouping::Account => (None, Some(account_summaries)),
            Grouping::Both => (Some(asset_summaries), Some(account_summaries)),
        };

        Ok(PortfolioSnapshot {
            as_of_date: query.as_of_date,
            currency: query.currency.clone(),
            total_value: format_base_currency_value(total_value, query.currency_decimals),
            total_cost_basis: gains_totals
                .total_cost_basis
                .map(|v| format_base_currency_value(v, query.currency_decimals)),
            total_unrealized_gain: gains_totals
                .total_unrealized_gain
                .map(|v| format_base_currency_value(v, query.currency_decimals)),
            prospective_capital_gains_tax: gains_totals
                .prospective_capital_gains_tax
                .map(|v| format_base_currency_value(v, query.currency_decimals)),
            valuation_scenario: valuation_scenario.map(|s| s.output),
            by_asset,
            by_account,
            valuation_issues,
        })
    }

    /// Assets left out of the total, sorted for stable output.
    fn collect_valuation_issues(
        price_cache: &HashMap<Asset, AssetValuation>,
    ) -> Vec<ValuationIssue> {
        let mut issues: Vec<ValuationIssue> = price_cache
            .iter()
            .filter_map(|(asset, valuation)| valuation.issue(asset))
            .collect();
        issues.sort_by_key(|issue| AssetId::from_asset(&issue.asset).to_string());
        issues
    }

    /// Per-asset portfolio breakdown, split into liability and non-liability
    /// rows. Positive (and zero) holdings of an asset aggregate into one row
    /// and negative holdings into a separate liability row, with no netting
    /// between the two. Reuses the same context loading (exclusions, latest
    /// snapshot at or before the date, backfill policy) and valuation
    /// machinery as `calculate`. `grouping` and `include_detail` on the query
    /// are ignored.
    pub async fn asset_breakdown(&self, query: &PortfolioQuery) -> Result<Vec<AssetBreakdownRow>> {
        let history = self.load_valuation_history(&query.account_ids).await?;
        let ctx = Self::calculation_context(&history, query.as_of_date, query.as_of_timestamp);

        let by_key = Self::aggregate_by_asset_liability(&ctx.filtered_snapshots)?;
        let update_metadata = self.asset_update_metadata(query).await?;

        // Value each unique asset only once; both partitions of the same
        // asset share the valuation.
        let mut price_cache: HashMap<Asset, AssetValuation> = HashMap::new();
        for (asset, _liability) in by_key.keys() {
            if !price_cache.contains_key(asset) {
                let valuation = self
                    .value_asset(
                        asset,
                        Decimal::ONE,
                        &query.currency,
                        query.as_of_date,
                        query.as_of_timestamp,
                    )
                    .await?;
                price_cache.insert(asset.clone(), valuation);
            }
        }

        let mut rows = Vec::with_capacity(by_key.len());
        for ((asset, liability), agg) in &by_key {
            let valuation = price_cache.get(asset).with_context(|| {
                format!("missing valuation for asset {}", AssetId::from_asset(asset))
            })?;
            let unit_value = valuation.value;

            let mut holdings = Vec::with_capacity(agg.holdings.len());
            for holding in &agg.holdings {
                let amount = Decimal::from_str(&holding.amount)?;
                let account = ctx.account_map.get(&holding.account_id);
                let account_name = account.map(|a| a.name.clone()).unwrap_or_default();
                let connection_name = account
                    .and_then(|a| ctx.connection_map.get(&a.connection_id))
                    .map(|c| c.name().to_string());
                holdings.push(AssetBreakdownAccountHolding {
                    account_id: holding.account_id.to_string(),
                    account_name,
                    connection_name,
                    amount,
                    balance_date: holding.timestamp.date_naive(),
                    value_in_base: unit_value.map(|price| price * amount),
                });
            }
            holdings.sort_by(|a, b| {
                a.account_name
                    .cmp(&b.account_name)
                    .then_with(|| a.account_id.cmp(&b.account_id))
            });

            rows.push(AssetBreakdownRow {
                asset: asset.clone(),
                liability: *liability,
                total_amount: agg.total_amount,
                value_in_base: unit_value.map(|price| price * agg.total_amount),
                price: valuation.price.clone(),
                price_date: valuation.price_date,
                price_timestamp: valuation.price_timestamp,
                amount_last_checked_at: update_metadata
                    .get(&(asset.clone(), *liability))
                    .and_then(|metadata| metadata.amount_last_checked_at),
                amount_last_changed_at: update_metadata
                    .get(&(asset.clone(), *liability))
                    .and_then(|metadata| metadata.amount_last_changed_at),
                fx_rate: valuation.fx_rate.clone(),
                fx_date: valuation.fx_date,
                value_issue: valuation.issue(asset),
                holdings,
            });
        }

        rows.sort_by_key(|row| (AssetId::from_asset(&row.asset).to_string(), row.liability));

        Ok(rows)
    }

    /// Value one unit of each asset at a historical date using the same
    /// price/FX selection rules as portfolio calculations.
    pub async fn asset_unit_values(
        &self,
        assets: &HashSet<Asset>,
        target_currency: &str,
        as_of_date: NaiveDate,
    ) -> Result<HashMap<Asset, Option<Decimal>>> {
        let mut values = HashMap::with_capacity(assets.len());
        for asset in assets {
            let valuation = self
                .value_asset(asset, Decimal::ONE, target_currency, as_of_date, None)
                .await?;
            values.insert(asset.clone(), valuation.value);
        }
        Ok(values)
    }

    /// Derive amount freshness from complete account balance snapshots. Each
    /// snapshot checks every asset previously seen in that account (an omitted
    /// asset is a zero balance), while change timestamps reflect changes to the
    /// aggregate amount across all included accounts.
    async fn asset_update_metadata(
        &self,
        query: &PortfolioQuery,
    ) -> Result<HashMap<(Asset, bool), AssetUpdateMetadata>> {
        type AssetKey = (Asset, bool);
        type AccountAmounts = HashMap<AssetKey, Decimal>;

        let scoped_account_ids: HashSet<&Id> = query.account_ids.iter().collect();
        let mut events: Vec<(DateTime<Utc>, Id, AccountAmounts)> = Vec::new();

        for account in self.storage.list_accounts().await? {
            if !scoped_account_ids.is_empty() && !scoped_account_ids.contains(&account.id) {
                continue;
            }

            let account_config = self.storage.get_account_config(&account.id)?;
            if account_config
                .as_ref()
                .and_then(|config| config.exclude_from_portfolio)
                .unwrap_or(false)
            {
                continue;
            }

            let mut snapshots = self.storage.get_balance_snapshots(&account.id).await?;
            snapshots.sort_by_key(|snapshot| snapshot.timestamp);
            let mut eligible: Vec<BalanceSnapshot> = snapshots
                .iter()
                .filter(|snapshot| {
                    snapshot_at_or_before(snapshot, query.as_of_date, query.as_of_timestamp)
                })
                .cloned()
                .collect();
            if eligible.is_empty()
                && matches!(
                    account_config.and_then(|config| config.balance_backfill),
                    Some(BalanceBackfillPolicy::CarryEarliest)
                )
            {
                if let Some(earliest) = snapshots.first().cloned() {
                    eligible.push(earliest);
                }
            }

            for snapshot in eligible {
                let mut amounts = AccountAmounts::new();
                for balance in snapshot.balances {
                    let asset = balance.asset.normalized();
                    let amount = Decimal::from_str(&balance.amount)?;
                    let key = (asset, amount < Decimal::ZERO);
                    *amounts.entry(key).or_insert(Decimal::ZERO) += amount;
                }
                events.push((snapshot.timestamp, account.id.clone(), amounts));
            }
        }

        events.sort_by(|(a_timestamp, a_account, _), (b_timestamp, b_account, _)| {
            a_timestamp
                .cmp(b_timestamp)
                .then_with(|| a_account.as_str().cmp(b_account.as_str()))
        });

        let mut metadata: HashMap<AssetKey, AssetUpdateMetadata> = HashMap::new();
        let mut account_amounts: HashMap<Id, AccountAmounts> = HashMap::new();
        let mut account_known_assets: HashMap<Id, HashSet<AssetKey>> = HashMap::new();
        let mut totals: AccountAmounts = HashMap::new();
        let mut index = 0;

        while index < events.len() {
            let timestamp = events[index].0;
            let group_end = events[index..]
                .iter()
                .position(|(candidate, _, _)| *candidate != timestamp)
                .map(|offset| index + offset)
                .unwrap_or(events.len());
            let totals_before = totals.clone();
            let mut touched_keys = HashSet::new();

            for (_, account_id, current_amounts) in &events[index..group_end] {
                let previous_amounts = account_amounts
                    .insert(account_id.clone(), current_amounts.clone())
                    .unwrap_or_default();
                let known_assets = account_known_assets.entry(account_id.clone()).or_default();

                for key in previous_amounts.keys().chain(current_amounts.keys()) {
                    known_assets.insert(key.clone());
                    touched_keys.insert(key.clone());
                }
                for key in known_assets.iter() {
                    metadata
                        .entry(key.clone())
                        .or_default()
                        .amount_last_checked_at = Some(timestamp);
                }

                for (key, amount) in previous_amounts {
                    *totals.entry(key).or_insert(Decimal::ZERO) -= amount;
                }
                for (key, amount) in current_amounts {
                    *totals.entry(key.clone()).or_insert(Decimal::ZERO) += *amount;
                }
            }

            touched_keys.extend(totals_before.keys().cloned());
            touched_keys.extend(totals.keys().cloned());
            for key in touched_keys {
                let before = totals_before.get(&key).copied().unwrap_or(Decimal::ZERO);
                let after = totals.get(&key).copied().unwrap_or(Decimal::ZERO);
                if before != after {
                    metadata.entry(key).or_default().amount_last_changed_at = Some(timestamp);
                }
            }

            index = group_end;
        }

        Ok(metadata)
    }

    /// Read accounts, connections, and balance history once, for reuse across
    /// a series of valuations at different instants.
    pub async fn load_valuation_history(&self, account_ids: &[Id]) -> Result<ValuationHistory> {
        let accounts = self.storage.list_accounts().await?;
        let connections = self.storage.list_connections().await?;

        let account_map: HashMap<Id, Account> =
            accounts.into_iter().map(|a| (a.id.clone(), a)).collect();
        let connection_map: HashMap<Id, Connection> = connections
            .into_iter()
            .map(|c| (c.id().clone(), c))
            .collect();

        let scoped_account_ids: HashSet<&Id> = account_ids.iter().collect();
        let mut histories = Vec::new();

        for account in account_map.values() {
            if !scoped_account_ids.is_empty() && !scoped_account_ids.contains(&account.id) {
                continue;
            }

            let account_config = self.storage.get_account_config(&account.id)?;
            let excluded = account_config
                .as_ref()
                .and_then(|config| config.exclude_from_portfolio)
                .unwrap_or(false);
            if excluded {
                continue;
            }

            let mut snapshots = self.storage.get_balance_snapshots(&account.id).await?;
            snapshots.sort_by_key(|snapshot| snapshot.timestamp);

            histories.push(AccountHistory {
                account_id: account.id.clone(),
                backfill: account_config
                    .and_then(|config| config.balance_backfill)
                    .unwrap_or(BalanceBackfillPolicy::None),
                snapshots,
            });
        }

        histories.sort_by(|a, b| a.account_id.as_str().cmp(b.account_id.as_str()));

        Ok(ValuationHistory {
            account_map,
            connection_map,
            accounts: histories,
        })
    }

    /// Select each account's balances for one point in time.
    fn calculation_context<'a>(
        history: &'a ValuationHistory,
        as_of_date: NaiveDate,
        as_of_timestamp: Option<DateTime<Utc>>,
    ) -> CalculationContext<'a> {
        let mut filtered_snapshots = Vec::new();
        let mut zero_accounts = Vec::new();

        for account in &history.accounts {
            if account.snapshots.is_empty() {
                if matches!(account.backfill, BalanceBackfillPolicy::Zero) {
                    zero_accounts.push(account.account_id.clone());
                }
                continue;
            }

            // Snapshots are ordered by timestamp and both eligibility rules are
            // monotone in it, so everything eligible is a prefix.
            let eligible = account.snapshots.partition_point(|snapshot| {
                snapshot_at_or_before(snapshot, as_of_date, as_of_timestamp)
            });

            if let Some(snapshot) = account.snapshots[..eligible].last() {
                filtered_snapshots.push((account.account_id.clone(), snapshot.clone()));
                continue;
            }

            match account.backfill {
                BalanceBackfillPolicy::CarryEarliest => {
                    if let Some(earliest) = account.snapshots.first() {
                        filtered_snapshots.push((account.account_id.clone(), earliest.clone()));
                    }
                }
                BalanceBackfillPolicy::Zero => zero_accounts.push(account.account_id.clone()),
                BalanceBackfillPolicy::None => {}
            }
        }

        CalculationContext {
            account_map: &history.account_map,
            connection_map: &history.connection_map,
            filtered_snapshots,
            zero_accounts,
        }
    }

    /// Aggregate balances by asset, tracking totals and holdings.
    fn aggregate_by_asset(
        snapshots: &[(Id, BalanceSnapshot)],
    ) -> Result<HashMap<Asset, AssetAggregate>> {
        let mut by_asset: HashMap<Asset, AssetAggregate> = HashMap::new();

        for (account_id, snapshot) in snapshots {
            for asset_balance in &snapshot.balances {
                let asset_key = asset_balance.asset.normalized();
                let amount = Decimal::from_str(&asset_balance.amount)?;
                let balance_date = snapshot.timestamp.date_naive();

                let entry = by_asset
                    .entry(asset_key.clone())
                    .or_insert_with(|| AssetAggregate {
                        total_amount: Decimal::ZERO,
                        amount_with_cost_basis: Decimal::ZERO,
                        total_cost_basis: None,
                        latest_balance_date: balance_date,
                        holdings: Vec::new(),
                    });

                entry.total_amount += amount;
                if let Some(cost_basis) = &asset_balance.cost_basis {
                    let cost_basis = Decimal::from_str(cost_basis)?;
                    entry.amount_with_cost_basis += amount;
                    entry.total_cost_basis =
                        Some(entry.total_cost_basis.unwrap_or(Decimal::ZERO) + cost_basis);
                }
                if balance_date > entry.latest_balance_date {
                    entry.latest_balance_date = balance_date;
                }
                entry.holdings.push(AssetHolding {
                    account_id: account_id.clone(),
                    asset: asset_key.clone(),
                    amount: asset_balance.amount.clone(),
                    cost_basis: asset_balance.cost_basis.clone(),
                    timestamp: snapshot.timestamp,
                });
            }
        }

        Ok(by_asset)
    }

    /// Aggregate balances by (asset, liability), where a holding is a
    /// liability when its amount is negative. Zero-amount holdings count as
    /// non-liability. Mirrors `aggregate_by_asset` otherwise.
    fn aggregate_by_asset_liability(
        snapshots: &[(Id, BalanceSnapshot)],
    ) -> Result<HashMap<(Asset, bool), AssetAggregate>> {
        let mut by_key: HashMap<(Asset, bool), AssetAggregate> = HashMap::new();

        for (account_id, snapshot) in snapshots {
            for asset_balance in &snapshot.balances {
                let asset_key = asset_balance.asset.normalized();
                let amount = Decimal::from_str(&asset_balance.amount)?;
                let liability = amount < Decimal::ZERO;
                let balance_date = snapshot.timestamp.date_naive();

                let entry = by_key
                    .entry((asset_key.clone(), liability))
                    .or_insert_with(|| AssetAggregate {
                        total_amount: Decimal::ZERO,
                        amount_with_cost_basis: Decimal::ZERO,
                        total_cost_basis: None,
                        latest_balance_date: balance_date,
                        holdings: Vec::new(),
                    });

                entry.total_amount += amount;
                if let Some(cost_basis) = &asset_balance.cost_basis {
                    let cost_basis = Decimal::from_str(cost_basis)?;
                    entry.amount_with_cost_basis += amount;
                    entry.total_cost_basis =
                        Some(entry.total_cost_basis.unwrap_or(Decimal::ZERO) + cost_basis);
                }
                if balance_date > entry.latest_balance_date {
                    entry.latest_balance_date = balance_date;
                }
                entry.holdings.push(AssetHolding {
                    account_id: account_id.clone(),
                    asset: asset_key.clone(),
                    amount: asset_balance.amount.clone(),
                    cost_basis: asset_balance.cost_basis.clone(),
                    timestamp: snapshot.timestamp,
                });
            }
        }

        Ok(by_key)
    }

    /// Fetch valuations for all unique assets, caching to avoid duplicate API calls.
    async fn fetch_asset_valuations(
        &self,
        by_asset: &HashMap<Asset, AssetAggregate>,
        target_currency: &str,
        as_of_date: NaiveDate,
        as_of_timestamp: Option<DateTime<Utc>>,
    ) -> Result<HashMap<Asset, AssetValuation>> {
        let mut cache = HashMap::new();

        for asset in by_asset.keys() {
            let valuation = self
                .value_asset(
                    asset,
                    Decimal::ONE,
                    target_currency,
                    as_of_date,
                    as_of_timestamp,
                )
                .await?;
            cache.insert(asset.clone(), valuation);
        }

        Ok(cache)
    }

    fn asset_value(agg: &AssetAggregate, valuation: &AssetValuation) -> Option<Decimal> {
        valuation
            .value
            .map(|unit_price| unit_price * agg.total_amount)
    }

    fn equity_value_totals(
        by_asset: &HashMap<Asset, AssetAggregate>,
        price_cache: &HashMap<Asset, AssetValuation>,
    ) -> Result<(Decimal, Decimal)> {
        let mut equity_value = Decimal::ZERO;
        let mut non_equity_value = Decimal::ZERO;

        for (asset, agg) in by_asset {
            let valuation = price_cache.get(asset).with_context(|| {
                format!("missing valuation for asset {}", AssetId::from_asset(asset))
            })?;
            let Some(value) = Self::asset_value(agg, valuation) else {
                continue;
            };
            match asset {
                Asset::Equity { .. } => equity_value += value,
                _ => non_equity_value += value,
            }
        }

        Ok((equity_value, non_equity_value))
    }

    fn resolve_equity_valuation_scenario(
        by_asset: &HashMap<Asset, AssetAggregate>,
        price_cache: &HashMap<Asset, AssetValuation>,
        adjustment: Option<&EquityValuationAdjustment>,
        currency_decimals: Option<u32>,
    ) -> Result<Option<ResolvedValuationScenario>> {
        let Some(adjustment) = adjustment else {
            return Ok(None);
        };

        let (equity_value_before, non_equity_value) =
            Self::equity_value_totals(by_asset, price_cache)?;
        if equity_value_before <= Decimal::ZERO {
            anyhow::bail!("equity valuation scenario requires positive priced equity holdings");
        }

        let (multiplier, target_pre_tax_total_value) = match adjustment {
            EquityValuationAdjustment::PercentChange(percent) => {
                (Decimal::ONE + (*percent / Decimal::from(100)), None)
            }
            EquityValuationAdjustment::TargetPreTaxTotalValue(target) => {
                let required_equity_value = *target - non_equity_value;
                if required_equity_value < Decimal::ZERO {
                    anyhow::bail!(
                        "target pre-tax total value {target} is below non-equity portfolio value {non_equity_value}"
                    );
                }
                (required_equity_value / equity_value_before, Some(*target))
            }
        };

        if multiplier < Decimal::ZERO {
            anyhow::bail!("equity valuation scenario would produce negative equity prices");
        }

        let equity_value_after = equity_value_before * multiplier;
        let pre_tax_total_value = non_equity_value + equity_value_after;
        let equity_change_percent = (multiplier - Decimal::ONE) * Decimal::from(100);

        Ok(Some(ResolvedValuationScenario {
            multiplier,
            output: PortfolioValuationScenario {
                equity_multiplier: multiplier.normalize().to_string(),
                equity_change_percent: equity_change_percent.normalize().to_string(),
                pre_tax_total_value: format_base_currency_value(
                    pre_tax_total_value,
                    currency_decimals,
                ),
                equity_value_before: format_base_currency_value(
                    equity_value_before,
                    currency_decimals,
                ),
                equity_value_after: format_base_currency_value(
                    equity_value_after,
                    currency_decimals,
                ),
                target_pre_tax_total_value: target_pre_tax_total_value
                    .map(|v| format_base_currency_value(v, currency_decimals)),
            },
        }))
    }

    fn apply_equity_valuation_multiplier(
        price_cache: &HashMap<Asset, AssetValuation>,
        multiplier: Decimal,
    ) -> HashMap<Asset, AssetValuation> {
        price_cache
            .iter()
            .map(|(asset, valuation)| {
                if !matches!(asset, Asset::Equity { .. }) {
                    return (asset.clone(), valuation.clone());
                }

                let mut adjusted = valuation.clone();
                adjusted.value = adjusted.value.map(|value| value * multiplier);
                adjusted.price = adjusted
                    .price
                    .as_ref()
                    .and_then(|price| Decimal::from_str(price).ok())
                    .map(|price| (price * multiplier).normalize().to_string())
                    .or_else(|| valuation.price.clone());
                (asset.clone(), adjusted)
            })
            .collect()
    }

    /// Build asset summaries from aggregated data and cached valuations.
    fn build_asset_summaries(
        &self,
        by_asset: &HashMap<Asset, AssetAggregate>,
        price_cache: &HashMap<Asset, AssetValuation>,
        account_map: &HashMap<Id, Account>,
        include_detail: bool,
        currency_decimals: Option<u32>,
        capital_gains_tax_rate: Option<Decimal>,
    ) -> Result<(Vec<AssetSummary>, Decimal, GainsTotals)> {
        let mut summaries = Vec::new();
        let mut total_value = Decimal::ZERO;
        let mut gains_totals = GainsTotals::default();

        for (asset, agg) in by_asset {
            let valuation = price_cache.get(asset).with_context(|| {
                format!("missing valuation for asset {}", AssetId::from_asset(asset))
            })?;

            let asset_value = valuation
                .value
                .map(|unit_price| unit_price * agg.total_amount);
            if let Some(v) = asset_value {
                total_value += v;
            }

            let cost_basis = agg.total_cost_basis;
            let unrealized_gain = match (valuation.value, cost_basis) {
                (Some(unit_price), Some(total_basis)) => {
                    Some((unit_price * agg.amount_with_cost_basis) - total_basis)
                }
                _ => None,
            };
            let prospective_tax = unrealized_gain
                .filter(|gain| *gain > Decimal::ZERO)
                .and_then(|gain| capital_gains_tax_rate.map(|rate| gain * rate));

            if let Some(total_basis) = cost_basis {
                gains_totals.total_cost_basis =
                    Some(gains_totals.total_cost_basis.unwrap_or(Decimal::ZERO) + total_basis);
            }
            if let Some(gain) = unrealized_gain {
                gains_totals.total_unrealized_gain =
                    Some(gains_totals.total_unrealized_gain.unwrap_or(Decimal::ZERO) + gain);
            }
            if let Some(tax) = prospective_tax {
                gains_totals.prospective_capital_gains_tax = Some(
                    gains_totals
                        .prospective_capital_gains_tax
                        .unwrap_or(Decimal::ZERO)
                        + tax,
                );
            }

            let holdings_detail = if include_detail {
                Some(Self::build_holdings_detail(
                    &agg.holdings,
                    account_map,
                    valuation.value,
                    currency_decimals,
                )?)
            } else {
                None
            };

            summaries.push(AssetSummary {
                asset: asset.clone(),
                total_amount: agg.total_amount.normalize().to_string(),
                amount_date: agg.latest_balance_date,
                price: valuation.price.clone(),
                price_date: valuation.price_date,
                price_timestamp: valuation.price_timestamp,
                fx_rate: valuation.fx_rate.clone(),
                fx_date: valuation.fx_date,
                value_in_base: asset_value
                    .map(|v| format_base_currency_value(v, currency_decimals)),
                cost_basis: cost_basis.map(|v| format_base_currency_value(v, currency_decimals)),
                unrealized_gain: unrealized_gain
                    .map(|v| format_base_currency_value(v, currency_decimals)),
                prospective_capital_gains_tax: prospective_tax
                    .map(|v| format_base_currency_value(v, currency_decimals)),
                holdings: holdings_detail,
            });
        }

        Ok((summaries, total_value, gains_totals))
    }

    /// Build holdings detail for an asset.
    fn build_holdings_detail(
        holdings: &[AssetHolding],
        account_map: &HashMap<Id, Account>,
        unit_value: Option<Decimal>,
        currency_decimals: Option<u32>,
    ) -> Result<Vec<AccountHolding>> {
        let mut detail = Vec::new();

        for holding in holdings {
            let account_name = account_map
                .get(&holding.account_id)
                .map(|a| a.name.clone())
                .unwrap_or_default();

            detail.push(AccountHolding {
                account_id: holding.account_id.to_string(),
                account_name,
                amount: Decimal::from_str(&holding.amount)?.normalize().to_string(),
                balance_date: holding.timestamp.date_naive(),
                cost_basis: holding
                    .cost_basis
                    .as_ref()
                    .map(|cost_basis| {
                        Decimal::from_str(cost_basis)
                            .map(|v| format_base_currency_value(v, currency_decimals))
                    })
                    .transpose()?,
                unrealized_gain: match (&holding.cost_basis, unit_value) {
                    (Some(cost_basis), Some(unit_value)) => {
                        let amount = Decimal::from_str(&holding.amount)?;
                        let cost_basis = Decimal::from_str(cost_basis)?;
                        Some(format_base_currency_value(
                            unit_value * amount - cost_basis,
                            currency_decimals,
                        ))
                    }
                    _ => None,
                },
            });
        }

        Ok(detail)
    }

    /// Build account summaries by aggregating values across assets.
    fn build_account_summaries(
        snapshots: &[(Id, BalanceSnapshot)],
        zero_accounts: &[Id],
        price_cache: &HashMap<Asset, AssetValuation>,
        account_map: &HashMap<Id, Account>,
        connection_map: &HashMap<Id, Connection>,
        currency_decimals: Option<u32>,
    ) -> Result<Vec<AccountSummary>> {
        // Track (sum, has_missing_values) per account
        let mut by_account: HashMap<Id, (Decimal, bool)> = HashMap::new();

        for (account_id, snapshot) in snapshots {
            for asset_balance in &snapshot.balances {
                let asset_key = asset_balance.asset.normalized();
                let amount = Decimal::from_str(&asset_balance.amount)?;
                let valuation = price_cache.get(&asset_key).with_context(|| {
                    format!(
                        "missing valuation for asset {}",
                        AssetId::from_asset(&asset_key)
                    )
                })?;

                let entry = by_account
                    .entry(account_id.clone())
                    .or_insert((Decimal::ZERO, false));

                match valuation.value {
                    Some(unit_price) => entry.0 += unit_price * amount,
                    None => entry.1 = true,
                }
            }
        }

        let mut summaries: Vec<AccountSummary> = by_account
            .into_iter()
            .filter_map(|(account_id, (value, has_missing))| {
                let account = account_map.get(&account_id)?;
                let connection = connection_map.get(&account.connection_id)?;
                Some(AccountSummary {
                    account_id: account_id.to_string(),
                    account_name: account.name.clone(),
                    connection_name: connection.name().to_string(),
                    value_in_base: if has_missing {
                        None
                    } else {
                        Some(format_base_currency_value(value, currency_decimals))
                    },
                })
            })
            .collect();

        for account_id in zero_accounts {
            if summaries
                .iter()
                .any(|s| s.account_id == account_id.to_string())
            {
                continue;
            }
            let account = match account_map.get(account_id) {
                Some(account) => account,
                None => continue,
            };
            let connection = match connection_map.get(&account.connection_id) {
                Some(connection) => connection,
                None => continue,
            };
            summaries.push(AccountSummary {
                account_id: account_id.to_string(),
                account_name: account.name.clone(),
                connection_name: connection.name().to_string(),
                value_in_base: Some(format_base_currency_value(Decimal::ZERO, currency_decimals)),
            });
        }

        Ok(summaries)
    }

    /// Value an asset in the target currency.
    /// Uses live quotes when available, falls back to cached or fetched historical prices.
    async fn value_asset(
        &self,
        asset: &Asset,
        amount: Decimal,
        target_currency: &str,
        as_of_date: NaiveDate,
        as_of_timestamp: Option<DateTime<Utc>>,
    ) -> Result<AssetValuation> {
        match asset {
            Asset::Currency { iso_code } => {
                if iso_code.eq_ignore_ascii_case(target_currency) {
                    // Same currency, no conversion needed
                    Ok(AssetValuation {
                        value: Some(amount),
                        issue: None,
                        issue_message: None,
                        price: None,
                        price_date: None,
                        price_timestamp: None,
                        fx_rate: None,
                        fx_date: None,
                    })
                } else {
                    // Need FX conversion
                    match self
                        .market_data
                        .fx_close_at(iso_code, target_currency, as_of_date, as_of_timestamp)
                        .await
                    {
                        Ok(rate) => {
                            let fx_rate = Decimal::from_str(&rate.rate)?;
                            Ok(AssetValuation {
                                value: Some(amount * fx_rate),
                                issue: None,
                                issue_message: None,
                                price: None,
                                price_date: None,
                                price_timestamp: None,
                                fx_rate: Some(fx_rate.normalize().to_string()),
                                fx_date: Some(rate.as_of_date),
                            })
                        }
                        Err(error) => {
                            let (issue, issue_message) = fx_issue(asset, &error);
                            Ok(AssetValuation {
                                value: None,
                                issue: Some(issue),
                                issue_message,
                                price: None,
                                price_date: None,
                                price_timestamp: None,
                                fx_rate: None,
                                fx_date: None,
                            })
                        }
                    }
                }
            }
            Asset::ManualValue { currency, .. } => {
                if currency.eq_ignore_ascii_case(target_currency) {
                    Ok(AssetValuation {
                        value: Some(amount),
                        issue: None,
                        issue_message: None,
                        price: None,
                        price_date: None,
                        price_timestamp: None,
                        fx_rate: None,
                        fx_date: None,
                    })
                } else {
                    match self
                        .market_data
                        .fx_close_at(currency, target_currency, as_of_date, as_of_timestamp)
                        .await
                    {
                        Ok(rate) => {
                            let fx_rate = Decimal::from_str(&rate.rate)?;
                            Ok(AssetValuation {
                                value: Some(amount * fx_rate),
                                issue: None,
                                issue_message: None,
                                price: None,
                                price_date: None,
                                price_timestamp: None,
                                fx_rate: Some(fx_rate.normalize().to_string()),
                                fx_date: Some(rate.as_of_date),
                            })
                        }
                        Err(error) => {
                            let (issue, issue_message) = fx_issue(asset, &error);
                            Ok(AssetValuation {
                                value: None,
                                issue: Some(issue),
                                issue_message,
                                price: None,
                                price_date: None,
                                price_timestamp: None,
                                fx_rate: None,
                                fx_date: None,
                            })
                        }
                    }
                }
            }
            Asset::Equity { .. } | Asset::Crypto { .. } => {
                // Use live pricing for today. Historical valuation uses cached/fetched prices
                // at or before the requested date without special-casing price kind.
                // A live quote is the price *now*, so it can only stand in for
                // a valuation that is not bounded to an earlier instant.
                let price_result = if as_of_timestamp.is_none() && as_of_date == self.clock.today()
                {
                    self.market_data.price_latest(asset, as_of_date).await
                } else {
                    // Not `?`: a failed read is reported as a valuation issue
                    // rather than failing every other asset's valuation too.
                    match self
                        .market_data
                        .valuation_price_from_store_at(asset, as_of_date, as_of_timestamp)
                        .await
                    {
                        Ok(Some(price)) => Ok(price),
                        Ok(None) => {
                            self.market_data
                                .price_close_at(asset, as_of_date, as_of_timestamp)
                                .await
                        }
                        Err(error) => Err(error),
                    }
                };
                let price_point = match price_result {
                    Ok(p) => p,
                    Err(error) => {
                        let (issue, issue_message) = price_issue(asset, &error);
                        return Ok(AssetValuation {
                            value: None,
                            issue: Some(issue),
                            issue_message,
                            price: None,
                            price_date: None,
                            price_timestamp: None,
                            fx_rate: None,
                            fx_date: None,
                        });
                    }
                };
                let price = Decimal::from_str(&price_point.price)?;
                let value_in_quote = amount * price;

                // Convert to target currency if needed
                if price_point
                    .quote_currency
                    .eq_ignore_ascii_case(target_currency)
                {
                    Ok(AssetValuation {
                        value: Some(value_in_quote),
                        issue: None,
                        issue_message: None,
                        price: Some(price.normalize().to_string()),
                        price_date: Some(price_point.as_of_date),
                        price_timestamp: Some(price_point.timestamp),
                        fx_rate: None,
                        fx_date: None,
                    })
                } else {
                    match self
                        .market_data
                        .fx_close_at(
                            &price_point.quote_currency,
                            target_currency,
                            as_of_date,
                            as_of_timestamp,
                        )
                        .await
                    {
                        Ok(rate) => {
                            let fx_rate = Decimal::from_str(&rate.rate)?;
                            Ok(AssetValuation {
                                value: Some(value_in_quote * fx_rate),
                                issue: None,
                                issue_message: None,
                                price: Some(price.normalize().to_string()),
                                price_date: Some(price_point.as_of_date),
                                price_timestamp: Some(price_point.timestamp),
                                fx_rate: Some(fx_rate.normalize().to_string()),
                                fx_date: Some(rate.as_of_date),
                            })
                        }
                        Err(error) => {
                            // Have price but no FX rate
                            let (issue, issue_message) = fx_issue(asset, &error);
                            Ok(AssetValuation {
                                value: None,
                                issue: Some(issue),
                                issue_message,
                                price: Some(price.normalize().to_string()),
                                price_date: Some(price_point.as_of_date),
                                price_timestamp: Some(price_point.timestamp),
                                fx_rate: None,
                                fx_date: None,
                            })
                        }
                    }
                }
            }
        }
    }
}

/// Classifies a failed lookup as absent data or an operational failure, and
/// logs the latter: a total that quietly omits an asset because the store or a
/// provider failed is indistinguishable from one nobody has priced.
fn classify_lookup_error(
    asset: &Asset,
    error: &anyhow::Error,
    missing: ValuationIssueReason,
    failed: ValuationIssueReason,
) -> (ValuationIssueReason, Option<String>) {
    if is_market_data_missing(error) {
        return (missing, None);
    }

    let message = format!("{error:#}");
    tracing::warn!(
        asset_id = %AssetId::from_asset(asset),
        error = %message,
        "valuation lookup failed; asset left out of the total"
    );
    (failed, Some(message))
}

fn price_issue(asset: &Asset, error: &anyhow::Error) -> (ValuationIssueReason, Option<String>) {
    classify_lookup_error(
        asset,
        error,
        ValuationIssueReason::MissingPrice,
        ValuationIssueReason::PriceLookupFailed,
    )
}

fn fx_issue(asset: &Asset, error: &anyhow::Error) -> (ValuationIssueReason, Option<String>) {
    classify_lookup_error(
        asset,
        error,
        ValuationIssueReason::MissingFxRate,
        ValuationIssueReason::FxLookupFailed,
    )
}

/// Whether a balance snapshot counts towards a valuation.
///
/// Without a cutoff this is every snapshot recorded on or before `as_of_date`,
/// end of day included. With one, only snapshots recorded at or before that
/// instant, so a point earlier in the day cannot see a later change.
fn snapshot_at_or_before(
    snapshot: &BalanceSnapshot,
    as_of_date: NaiveDate,
    as_of_timestamp: Option<DateTime<Utc>>,
) -> bool {
    match as_of_timestamp {
        Some(cutoff) => snapshot.timestamp <= cutoff,
        None => snapshot.timestamp.date_naive() <= as_of_date,
    }
}

#[cfg(test)]
#[path = "../../tests/unit/portfolio/service_tests.rs"]
mod service_tests;
