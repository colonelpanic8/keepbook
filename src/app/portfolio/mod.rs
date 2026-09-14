mod backfill;
mod history;
mod intervals;

use std::collections::{HashMap, HashSet};
use std::str::FromStr;
use std::sync::Arc;

use anyhow::{Context, Result};
use chrono::{Days, Duration, Months, NaiveDate, Utc};
use rust_decimal::Decimal;
#[cfg(not(feature = "sync"))]
use tracing::warn;

use crate::config::ResolvedConfig;
use crate::format::format_base_currency_value;
use crate::market_data::{
    AssetId, JsonlMarketDataStore, MarketDataServiceBuilder, MarketDataStore,
    ReadThroughMarketDataStore,
};
use crate::models::{Asset, Id};
use crate::portfolio::{
    collect_change_points, earliest_change_point, AccountSummary, CoalesceStrategy, CollectOptions,
    EquityValuationAdjustment, Granularity, Grouping, PortfolioQuery, PortfolioService,
};
use crate::staleness::{
    check_balance_staleness, check_price_staleness, log_balance_staleness, log_price_staleness,
    resolve_balance_staleness,
};
use crate::storage::{ReadThroughStorage, Storage};

#[cfg(feature = "sync")]
use super::sync::build_sync_service;
use super::{
    maybe_auto_commit, AssetBreakdownEntry, AssetBreakdownHolding, AssetBreakdownOutput,
    AssetChange, AssetChanges, ChangePointsOutput, HistoryOutput, HistoryPoint,
    StackedHistoryOutput, StackedHistorySeries, TaxImpactOutput, TaxImpactPoint,
};
pub use backfill::{fetch_historical_prices, fill_prices_at_date, PriceHistoryRequest};

use backfill::resolve_price_history_scope;
use history::{
    build_history_point_for_date, build_stacked_history_point_for_date, calculate_history_summary,
    collect_history_cost_basis_backfill, configure_history_market_data, HistoryPointInput,
    HistoryValueMode, StackedHistoryPointInput,
};
use intervals::{history_spec_dates, parse_portfolio_date_bound, DateRangeBound};

/// Options for `portfolio_snapshot`. Defaults mirror the CLI defaults:
/// today, grouped by both asset and account, auto-refreshing stale data.
#[derive(Debug, Clone)]
pub struct PortfolioSnapshotRequest {
    pub currency: Option<String>,
    /// `YYYY-MM-DD`; today when absent.
    pub date: Option<String>,
    /// `asset`, `account`, or `both`.
    pub group_by: String,
    pub detail: bool,
    pub capital_gains_tax_rate: Option<String>,
    pub equity_change_percent: Option<String>,
    pub target_pre_tax_total_value: Option<String>,
    /// Explicit form of the default refresh behavior; has no extra effect.
    pub auto: bool,
    /// Use cached data only.
    pub offline: bool,
    /// Log staleness without refreshing.
    pub dry_run: bool,
    /// Refresh everything regardless of staleness.
    pub force_refresh: bool,
}

impl Default for PortfolioSnapshotRequest {
    fn default() -> Self {
        Self {
            currency: None,
            date: None,
            group_by: "both".to_string(),
            detail: false,
            capital_gains_tax_rate: None,
            equity_change_percent: None,
            target_pre_tax_total_value: None,
            auto: false,
            offline: false,
            dry_run: false,
            force_refresh: false,
        }
    }
}

/// Options for `portfolio_tax_impact`.
#[derive(Debug, Clone)]
pub struct PortfolioTaxImpactRequest {
    pub currency: Option<String>,
    /// `YYYY-MM-DD`; today when absent.
    pub date: Option<String>,
    pub capital_gains_tax_rate: Option<String>,
    /// Minimum nominal pre-tax net worth for the curve; half of current when absent.
    pub min: Option<String>,
    /// Maximum nominal pre-tax net worth for the curve; current when absent.
    pub max: Option<String>,
    /// Number of curve points; must be at least 1.
    pub points: usize,
}

impl Default for PortfolioTaxImpactRequest {
    fn default() -> Self {
        Self {
            currency: None,
            date: None,
            capital_gains_tax_rate: None,
            min: None,
            max: None,
            points: 25,
        }
    }
}

pub const DEFAULT_PORTFOLIO_HISTORY_GRANULARITY: &str =
    crate::config::DEFAULT_HISTORY_PORTFOLIO_GRANULARITY;
pub const DEFAULT_PORTFOLIO_CHANGE_POINTS_GRANULARITY: &str =
    crate::config::DEFAULT_HISTORY_CHANGE_POINTS_GRANULARITY;
pub const DEFAULT_PORTFOLIO_INCLUDE_PRICES: bool = crate::config::DEFAULT_HISTORY_INCLUDE_PRICES;
pub const LATENT_CAPITAL_GAINS_TAX_ACCOUNT_ID: &str = "virtual:latent_capital_gains_tax";

pub enum PortfolioHistorySelection {
    Portfolio,
    Accounts(Vec<Id>),
    LatentCapitalGainsTax,
}

pub fn default_portfolio_history_granularity() -> String {
    crate::config::default_history_portfolio_granularity()
}

pub fn default_portfolio_change_points_granularity() -> String {
    crate::config::default_history_change_points_granularity()
}

pub fn default_portfolio_include_prices() -> bool {
    crate::config::default_history_include_prices()
}

/// True for accounts the portfolio contributes rather than storage, such as
/// the latent capital gains tax account.
pub fn is_virtual_account_id(account_id: &str) -> bool {
    account_id.starts_with("virtual:")
}

fn is_latent_capital_gains_tax_account(config: &ResolvedConfig, id_or_name: &str) -> bool {
    id_or_name == LATENT_CAPITAL_GAINS_TAX_ACCOUNT_ID
        || id_or_name.eq_ignore_ascii_case(&config.portfolio.latent_capital_gains_tax.account_name)
}

pub async fn resolve_portfolio_history_selection(
    storage: &dyn Storage,
    config: &ResolvedConfig,
    account: Option<&str>,
    connection: Option<&str>,
) -> Result<PortfolioHistorySelection> {
    if account.is_some() && connection.is_some() {
        anyhow::bail!("Specify only one of --account or --connection");
    }

    if let Some(id_or_name) = account {
        if is_latent_capital_gains_tax_account(config, id_or_name) {
            return Ok(PortfolioHistorySelection::LatentCapitalGainsTax);
        }
    }

    if account.is_none() && connection.is_none() {
        return Ok(PortfolioHistorySelection::Portfolio);
    }

    let (_, accounts) = resolve_price_history_scope(storage, account, connection).await?;
    Ok(PortfolioHistorySelection::Accounts(
        accounts.into_iter().map(|account| account.id).collect(),
    ))
}

fn parse_tax_rate_fraction(rate: &str, context: &str) -> Result<Decimal> {
    Decimal::from_str(rate)
        .with_context(|| format!("Invalid {context}: {rate}"))
        .map(|rate| rate / Decimal::from(100))
}

fn parse_decimal_arg(value: &str, context: &str) -> Result<Decimal> {
    Decimal::from_str(value).with_context(|| format!("Invalid {context}: {value}"))
}

fn resolve_equity_valuation_adjustment(
    equity_change_percent: Option<String>,
    target_pre_tax_total_value: Option<String>,
) -> Result<Option<EquityValuationAdjustment>> {
    match (equity_change_percent, target_pre_tax_total_value) {
        (Some(_), Some(_)) => anyhow::bail!(
            "--equity-change-percent and --target-pre-tax-total-value cannot be used together"
        ),
        (Some(percent), None) => Ok(Some(EquityValuationAdjustment::PercentChange(
            parse_decimal_arg(&percent, "equity change percent")?,
        ))),
        (None, Some(target)) => Ok(Some(EquityValuationAdjustment::TargetPreTaxTotalValue(
            parse_decimal_arg(&target, "target pre-tax total value")?,
        ))),
        (None, None) => Ok(None),
    }
}

fn decimal_from_f64(value: f64, context: &str) -> Result<Decimal> {
    Decimal::from_str(&value.to_string()).with_context(|| format!("Invalid {context}: {value}"))
}

fn resolve_capital_gains_tax_rate(
    config: &ResolvedConfig,
    cli_percent_rate: Option<String>,
) -> Result<(Option<Decimal>, bool)> {
    let latent_tax = &config.portfolio.latent_capital_gains_tax;
    if let Some(rate) = cli_percent_rate {
        return Ok((
            Some(parse_tax_rate_fraction(&rate, "capital gains tax rate")?),
            latent_tax.enabled,
        ));
    }

    if !latent_tax.enabled {
        return Ok((None, false));
    }

    let rate = latent_tax
        .rate
        .context("portfolio.latent_capital_gains_tax.enabled requires a rate")?;
    Ok((
        Some(decimal_from_f64(
            rate,
            "portfolio.latent_capital_gains_tax.rate",
        )?),
        true,
    ))
}

fn apply_latent_tax_virtual_account(
    snapshot: &mut crate::portfolio::PortfolioSnapshot,
    config: &ResolvedConfig,
) -> Result<()> {
    let Some(tax_str) = &snapshot.prospective_capital_gains_tax else {
        return Ok(());
    };
    let tax = Decimal::from_str(tax_str)
        .with_context(|| format!("Invalid prospective_capital_gains_tax: {tax_str}"))?;
    if tax <= Decimal::ZERO {
        return Ok(());
    }

    let total_value = Decimal::from_str(&snapshot.total_value)
        .with_context(|| format!("Invalid total_value: {}", snapshot.total_value))?;
    snapshot.total_value =
        format_base_currency_value(total_value - tax, config.display.currency_decimals);

    if let Some(by_account) = snapshot.by_account.as_mut() {
        by_account.push(AccountSummary {
            account_id: "virtual:latent_capital_gains_tax".to_string(),
            account_name: config
                .portfolio
                .latent_capital_gains_tax
                .account_name
                .clone(),
            connection_name: "Virtual".to_string(),
            value_in_base: Some(format_base_currency_value(
                -tax,
                config.display.currency_decimals,
            )),
        });
    }

    Ok(())
}

pub async fn portfolio_snapshot(
    storage: Arc<dyn Storage>,
    config: &ResolvedConfig,
    request: PortfolioSnapshotRequest,
) -> Result<crate::portfolio::PortfolioSnapshot> {
    let PortfolioSnapshotRequest {
        currency,
        date,
        group_by,
        detail,
        capital_gains_tax_rate,
        equity_change_percent,
        target_pre_tax_total_value,
        auto,
        offline,
        dry_run,
        force_refresh,
    } = request;

    // Parse date
    let as_of_date = match date {
        Some(d) => NaiveDate::parse_from_str(&d, "%Y-%m-%d")
            .with_context(|| format!("Invalid date format: {d}"))?,
        None => Utc::now().date_naive(),
    };

    // Parse grouping
    let grouping = match group_by.as_str() {
        "asset" => Grouping::Asset,
        "account" => Grouping::Account,
        "both" => Grouping::Both,
        _ => anyhow::bail!("Invalid grouping: {group_by}. Use: asset, account, both"),
    };

    let (capital_gains_tax_rate, include_latent_tax_virtual_account) =
        resolve_capital_gains_tax_rate(config, capital_gains_tax_rate)?;
    let equity_valuation_adjustment =
        resolve_equity_valuation_adjustment(equity_change_percent, target_pre_tax_total_value)?;

    // Determine what to refresh based on flags
    // Default (no flags or --auto): auto-refresh stale data
    // --offline: no refresh
    // --dry-run: log staleness but no refresh
    // --force-refresh: refresh everything
    let should_refresh_balances = !offline && !dry_run;
    let should_refresh_prices = !offline && !dry_run;
    let ignore_staleness = force_refresh;

    // Explicit --auto flag has same behavior as default
    let _ = auto;

    // Build query
    let query = PortfolioQuery {
        as_of_date,
        as_of_timestamp: None,
        currency: currency.unwrap_or_else(|| config.reporting_currency.clone()),
        currency_decimals: config.display.currency_decimals,
        grouping,
        include_detail: detail,
        capital_gains_tax_rate,
        equity_valuation_adjustment,
        account_ids: Vec::new(),
    };

    // Setup market data store
    let store = Arc::new(JsonlMarketDataStore::new(&config.data_dir));

    // Check which connections need syncing based on staleness
    let connections = storage.list_connections().await?;
    let mut connections_to_sync = Vec::new();

    for connection in &connections {
        let threshold = resolve_balance_staleness(None, connection, &config.refresh);
        let check = check_balance_staleness(connection, threshold);

        // Log if dry_run
        if dry_run {
            log_balance_staleness(&connection.config.name, &check);
        }

        // Add to sync list if stale (or force)
        if should_refresh_balances && (ignore_staleness || check.is_stale) {
            connections_to_sync.push(connection.clone());
        }
    }

    // Check price staleness for dry-run
    if dry_run {
        use std::collections::HashSet;

        // Load balances to find unique assets that need prices
        let snapshots = storage.get_latest_balances().await?;
        let mut seen_assets: HashSet<String> = HashSet::new();

        for (_, snapshot) in &snapshots {
            for asset_balance in &snapshot.balances {
                match &asset_balance.asset {
                    Asset::Equity { .. } | Asset::Crypto { .. } => {
                        let asset_id = AssetId::from_asset(&asset_balance.asset);
                        let asset_key = asset_id.to_string();

                        if seen_assets.contains(&asset_key) {
                            continue;
                        }
                        seen_assets.insert(asset_key.clone());

                        let prices = store.get_all_prices(&asset_id).await?;
                        let cached_price = prices
                            .into_iter()
                            .filter(|price| {
                                price.as_of_date <= query.as_of_date
                                    && price.as_of_date >= query.as_of_date - Duration::days(7)
                            })
                            .max_by(|a, b| {
                                a.as_of_date
                                    .cmp(&b.as_of_date)
                                    .then_with(|| a.timestamp.cmp(&b.timestamp))
                            });

                        let check = check_price_staleness(
                            cached_price.as_ref(),
                            config.refresh.price_staleness,
                        );
                        log_price_staleness(&asset_key, &check);
                    }
                    Asset::Currency { .. } | Asset::ManualValue { .. } => {
                        // Currency and manual value assets don't need price lookup (only FX).
                    }
                }
            }
        }
    }

    // Sync stale connections
    if !connections_to_sync.is_empty() {
        #[cfg(feature = "sync")]
        {
            let sync_service = build_sync_service(storage.clone(), config).await;
            for connection in &connections_to_sync {
                let _ = sync_service.sync_connection(connection.id().as_ref()).await;
            }
        }

        #[cfg(not(feature = "sync"))]
        warn!(
            count = connections_to_sync.len(),
            "Skipping stale connection sync because keepbook was built without sync support"
        );
    }

    // Setup market data service with or without configured providers.
    let market_data = if should_refresh_prices {
        Arc::new(
            MarketDataServiceBuilder::new(store.clone(), config.data_dir.clone())
                .with_quote_staleness(config.refresh.price_staleness)
                .build()
                .await,
        )
    } else {
        Arc::new(
            MarketDataServiceBuilder::new(store.clone(), config.data_dir.clone())
                .with_quote_staleness(config.refresh.price_staleness)
                .offline_only()
                .build()
                .await,
        )
    };

    // Calculate and output
    let service = PortfolioService::new(storage.clone(), market_data);
    let mut snapshot = service.calculate(&query).await?;
    if include_latent_tax_virtual_account {
        apply_latent_tax_virtual_account(&mut snapshot, config)?;
    }

    maybe_auto_commit(config, "portfolio snapshot");

    Ok(snapshot)
}

/// Per-asset portfolio breakdown at `date` (default today) with
/// day/week/month/year changes computed against the breakdown at the
/// corresponding past dates. Uses cached market data only (no staleness-driven
/// sync or price refresh), mirroring the portfolio history commands.
pub async fn portfolio_assets(
    storage: Arc<dyn Storage>,
    config: &ResolvedConfig,
    date: Option<String>,
    include_amount_changes: bool,
) -> Result<AssetBreakdownOutput> {
    let as_of_date = match date {
        Some(d) => NaiveDate::parse_from_str(&d, "%Y-%m-%d")
            .with_context(|| format!("Invalid date format: {d}"))?,
        None => Utc::now().date_naive(),
    };
    let currency = config.reporting_currency.clone();
    let currency_decimals = config.display.currency_decimals;

    let store: Arc<dyn MarketDataStore> = Arc::new(JsonlMarketDataStore::new(&config.data_dir));
    let market_data = Arc::new(configure_history_market_data(
        MarketDataServiceBuilder::new(store, config.data_dir.clone())
            .with_quote_staleness(config.refresh.price_staleness)
            .offline_only()
            .build()
            .await,
        config,
    ));
    let service = PortfolioService::new(storage, market_data);

    let breakdown_query = |as_of: NaiveDate| PortfolioQuery {
        as_of_date: as_of,
        as_of_timestamp: None,
        currency: currency.clone(),
        currency_decimals,
        grouping: Grouping::Asset,
        include_detail: false,
        capital_gains_tax_rate: None,
        equity_valuation_adjustment: None,
        account_ids: Vec::new(),
    };

    let current_rows = service
        .asset_breakdown(&breakdown_query(as_of_date))
        .await?;
    let current_assets: HashSet<Asset> = current_rows.iter().map(|row| row.asset.clone()).collect();

    // Rows at each past date, keyed by (asset_id, liability). A key that is
    // absent had no holdings at that date; a key mapped to None was held but
    // could not be priced.
    let mut past_values: Vec<HashMap<(String, bool), Option<Decimal>>> = Vec::with_capacity(4);
    for past_date in [
        as_of_date.checked_sub_days(Days::new(1)),
        as_of_date.checked_sub_days(Days::new(7)),
        as_of_date.checked_sub_months(Months::new(1)),
        as_of_date.checked_sub_months(Months::new(12)),
    ] {
        let values = match past_date {
            Some(date) if include_amount_changes => service
                .asset_breakdown(&breakdown_query(date))
                .await?
                .into_iter()
                .map(|row| {
                    (
                        (AssetId::from_asset(&row.asset).to_string(), row.liability),
                        row.value_in_base,
                    )
                })
                .collect(),
            Some(date) => {
                let unit_values = service
                    .asset_unit_values(&current_assets, &currency, date)
                    .await?;
                current_rows
                    .iter()
                    .map(|row| {
                        let value = unit_values
                            .get(&row.asset)
                            .copied()
                            .flatten()
                            .map(|unit_value| unit_value * row.total_amount);
                        (
                            (AssetId::from_asset(&row.asset).to_string(), row.liability),
                            value,
                        )
                    })
                    .collect()
            }
            None => HashMap::new(),
        };
        past_values.push(values);
    }

    let mut total_value = Decimal::ZERO;
    let mut entries: Vec<(Option<Decimal>, AssetBreakdownEntry)> =
        Vec::with_capacity(current_rows.len());
    for row in current_rows {
        let asset_id = AssetId::from_asset(&row.asset).to_string();
        let key = (asset_id.clone(), row.liability);
        if let Some(value) = row.value_in_base {
            total_value += value;
        }

        let changes = AssetChanges {
            day: compute_asset_change(
                row.value_in_base,
                past_values[0].get(&key),
                currency_decimals,
            ),
            week: compute_asset_change(
                row.value_in_base,
                past_values[1].get(&key),
                currency_decimals,
            ),
            month: compute_asset_change(
                row.value_in_base,
                past_values[2].get(&key),
                currency_decimals,
            ),
            year: compute_asset_change(
                row.value_in_base,
                past_values[3].get(&key),
                currency_decimals,
            ),
        };

        let holdings = row
            .holdings
            .into_iter()
            .map(|holding| AssetBreakdownHolding {
                account_id: holding.account_id,
                account_name: holding.account_name,
                connection_name: holding.connection_name,
                amount: holding.amount.normalize().to_string(),
                balance_date: holding.balance_date,
                value_in_base: holding
                    .value_in_base
                    .map(|value| format_base_currency_value(value, currency_decimals)),
            })
            .collect();

        entries.push((
            row.value_in_base,
            AssetBreakdownEntry {
                asset: row.asset,
                asset_id,
                liability: row.liability,
                total_amount: row.total_amount.normalize().to_string(),
                price: row.price,
                price_date: row.price_date,
                price_updated_at: row.price_timestamp.map(|timestamp| timestamp.to_rfc3339()),
                amount_last_checked_at: row
                    .amount_last_checked_at
                    .map(|timestamp| timestamp.to_rfc3339()),
                amount_last_changed_at: row
                    .amount_last_changed_at
                    .map(|timestamp| timestamp.to_rfc3339()),
                value_in_base: row
                    .value_in_base
                    .map(|value| format_base_currency_value(value, currency_decimals)),
                value_issue: row.value_issue,
                changes,
                holdings,
            },
        ));
    }

    // Sort by absolute value descending; rows without a value last; break ties
    // by asset_id, then liability.
    entries.sort_by(|(a_value, a), (b_value, b)| {
        match (
            a_value.map(|value| value.abs()),
            b_value.map(|value| value.abs()),
        ) {
            (Some(a_abs), Some(b_abs)) => b_abs.cmp(&a_abs),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        }
        .then_with(|| a.asset_id.cmp(&b.asset_id))
        .then_with(|| a.liability.cmp(&b.liability))
    });

    Ok(AssetBreakdownOutput {
        as_of_date,
        currency,
        change_mode: if include_amount_changes {
            "price_and_amount".to_string()
        } else {
            "price_only".to_string()
        },
        total_value: format_base_currency_value(total_value, currency_decimals),
        asset_count: entries.iter().filter(|(_, entry)| !entry.liability).count(),
        liability_count: entries.iter().filter(|(_, entry)| entry.liability).count(),
        assets: entries.into_iter().map(|(_, entry)| entry).collect(),
    })
}

/// Change of a row's value versus a past date.
///
/// - Current value unknown: no change is reported.
/// - Row absent at the past date: change from zero, percentage omitted.
/// - Row held at the past date but unpriceable: no change is reported (rather
///   than pretending the past value was zero).
/// - Past value zero: percentage omitted.
fn compute_asset_change(
    current: Option<Decimal>,
    past: Option<&Option<Decimal>>,
    currency_decimals: Option<u32>,
) -> Option<AssetChange> {
    let current = current?;
    let past_value = match past {
        Some(None) => return None,
        Some(Some(value)) => *value,
        None => Decimal::ZERO,
    };
    let absolute = current - past_value;
    let percentage = if past_value == Decimal::ZERO {
        None
    } else {
        Some(
            (absolute / past_value.abs() * Decimal::from(100))
                .round_dp(2)
                .to_string(),
        )
    };
    Some(AssetChange {
        absolute: format_base_currency_value(absolute, currency_decimals),
        percentage,
    })
}

pub async fn portfolio_tax_impact(
    storage: Arc<dyn Storage>,
    config: &ResolvedConfig,
    request: PortfolioTaxImpactRequest,
) -> Result<TaxImpactOutput> {
    let PortfolioTaxImpactRequest {
        currency,
        date,
        capital_gains_tax_rate,
        min,
        max,
        points,
    } = request;

    let as_of_date = match date {
        Some(d) => NaiveDate::parse_from_str(&d, "%Y-%m-%d")
            .with_context(|| format!("Invalid date format: {d}"))?,
        None => Utc::now().date_naive(),
    };
    let currency = currency.unwrap_or_else(|| config.reporting_currency.clone());
    let (rate, _) = resolve_capital_gains_tax_rate(config, capital_gains_tax_rate)?;
    let rate = rate.context(
        "portfolio tax-impact requires --capital-gains-tax-rate or enabled portfolio.latent_capital_gains_tax.rate",
    )?;
    if points == 0 {
        anyhow::bail!("points must be at least 1");
    }

    let market_data = Arc::new(
        MarketDataServiceBuilder::new(
            Arc::new(JsonlMarketDataStore::new(&config.data_dir)),
            config.data_dir.clone(),
        )
        .with_quote_staleness(config.refresh.price_staleness)
        .offline_only()
        .build()
        .await,
    );
    let service = PortfolioService::new(storage, market_data);
    let base_query = PortfolioQuery {
        as_of_date,
        as_of_timestamp: None,
        currency: currency.clone(),
        currency_decimals: config.display.currency_decimals,
        grouping: Grouping::Asset,
        include_detail: false,
        capital_gains_tax_rate: Some(rate),
        equity_valuation_adjustment: None,
        account_ids: Vec::new(),
    };
    let base = service.calculate(&base_query).await?;
    let current_nominal = parse_decimal_arg(&base.total_value, "current nominal net worth")?;
    let current_tax = base
        .prospective_capital_gains_tax
        .as_deref()
        .map(|value| parse_decimal_arg(value, "current tax liability"))
        .transpose()?
        .unwrap_or(Decimal::ZERO);
    let current_after_tax = current_nominal - current_tax;

    let min_value = match min {
        Some(value) => parse_decimal_arg(&value, "minimum nominal net worth")?,
        None => current_nominal * Decimal::new(5, 1),
    };
    let max_value = match max {
        Some(value) => parse_decimal_arg(&value, "maximum nominal net worth")?,
        None => current_nominal,
    };
    if min_value > max_value {
        anyhow::bail!("min must be less than or equal to max");
    }

    let steps = if points <= 1 {
        vec![min_value]
    } else {
        let step = (max_value - min_value) / Decimal::from((points - 1) as u64);
        (0..points)
            .map(|idx| min_value + step * Decimal::from(idx as u64))
            .collect()
    };

    let mut curve_points = Vec::with_capacity(steps.len());
    for target in steps {
        let query = PortfolioQuery {
            equity_valuation_adjustment: Some(EquityValuationAdjustment::TargetPreTaxTotalValue(
                target,
            )),
            ..base_query.clone()
        };
        let snapshot = service.calculate(&query).await?;
        let nominal = parse_decimal_arg(&snapshot.total_value, "scenario nominal net worth")?;
        let tax = snapshot
            .prospective_capital_gains_tax
            .as_deref()
            .map(|value| parse_decimal_arg(value, "scenario tax liability"))
            .transpose()?
            .unwrap_or(Decimal::ZERO);
        let scenario = snapshot
            .valuation_scenario
            .context("missing valuation_scenario for tax impact point")?;
        curve_points.push(TaxImpactPoint {
            nominal_net_worth: format_base_currency_value(
                nominal,
                config.display.currency_decimals,
            ),
            tax_liability: format_base_currency_value(tax, config.display.currency_decimals),
            after_tax_net_worth: format_base_currency_value(
                nominal - tax,
                config.display.currency_decimals,
            ),
            equity_multiplier: scenario.equity_multiplier,
            equity_change_percent: scenario.equity_change_percent,
        });
    }

    Ok(TaxImpactOutput {
        currency,
        as_of_date: as_of_date.to_string(),
        capital_gains_tax_rate: rate.normalize().to_string(),
        current_nominal_net_worth: format_base_currency_value(
            current_nominal,
            config.display.currency_decimals,
        ),
        current_tax_liability: format_base_currency_value(
            current_tax,
            config.display.currency_decimals,
        ),
        current_after_tax_net_worth: format_base_currency_value(
            current_after_tax,
            config.display.currency_decimals,
        ),
        points: curve_points,
    })
}

pub async fn portfolio_history(
    storage: Arc<dyn Storage>,
    config: &ResolvedConfig,
    currency: Option<String>,
    start: Option<String>,
    end: Option<String>,
    granularity: String,
    include_prices: bool,
    include_current: bool,
) -> Result<HistoryOutput> {
    portfolio_history_scoped(
        storage,
        config,
        currency,
        start,
        end,
        granularity,
        include_prices,
        include_current,
        Vec::new(),
        HistoryValueMode::Portfolio {
            include_latent_tax_adjustment: resolve_capital_gains_tax_rate(config, None)?.1,
        },
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub async fn portfolio_history_for_accounts(
    storage: Arc<dyn Storage>,
    config: &ResolvedConfig,
    currency: Option<String>,
    start: Option<String>,
    end: Option<String>,
    granularity: String,
    include_prices: bool,
    include_current: bool,
    account_ids: Vec<Id>,
) -> Result<HistoryOutput> {
    portfolio_history_scoped(
        storage,
        config,
        currency,
        start,
        end,
        granularity,
        include_prices,
        include_current,
        account_ids,
        HistoryValueMode::Portfolio {
            include_latent_tax_adjustment: false,
        },
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub async fn portfolio_stacked_history(
    storage: Arc<dyn Storage>,
    config: &ResolvedConfig,
    currency: Option<String>,
    start: Option<String>,
    end: Option<String>,
    granularity: String,
    include_prices: bool,
    include_current: bool,
) -> Result<StackedHistoryOutput> {
    let today = Utc::now().date_naive();
    let start_date = start
        .as_ref()
        .map(|s| parse_portfolio_date_bound(s, DateRangeBound::Start, today))
        .transpose()?;
    let end_date = end
        .as_ref()
        .map(|s| parse_portfolio_date_bound(s, DateRangeBound::End, today))
        .transpose()?;
    let start_date_output = start_date.map(|date| date.to_string());
    let end_date_output = end_date.map(|date| date.to_string());

    let granularity_enum = match granularity.as_str() {
        "none" | "full" => Granularity::Full,
        "hourly" => Granularity::Hourly,
        "daily" => Granularity::Daily,
        "weekly" => Granularity::Weekly,
        "monthly" => Granularity::Monthly,
        "yearly" => Granularity::Yearly,
        _ => anyhow::bail!(
            "Invalid granularity: {granularity}. Use: none, full, hourly, daily, weekly, monthly, yearly"
        ),
    };

    // One history request values many points from the same accounts, balances,
    // and price histories. These read each of them once for the request and are
    // dropped with it.
    let store: Arc<dyn MarketDataStore> = Arc::new(ReadThroughMarketDataStore::new(Arc::new(
        JsonlMarketDataStore::new(&config.data_dir),
    )));
    let storage_arc: Arc<dyn Storage> = Arc::new(ReadThroughStorage::new(storage));
    let options = CollectOptions {
        account_ids: Vec::new(),
        include_prices,
        include_fx: false,
        target_currency: currency.clone(),
        start: start_date,
        end: end_date,
        granularity: granularity_enum,
        strategy: CoalesceStrategy::Last,
    };
    let filtered = collect_change_points(&storage_arc, &store, &options).await?;
    let target_currency = currency.unwrap_or_else(|| config.reporting_currency.clone());

    // A current point still needs valuing when the range holds no change
    // points, so only bail out early when the caller did not ask for one.
    let want_current = include_current && !end_date.is_some_and(|end| end < today);
    if filtered.is_empty() && !want_current {
        return Ok(StackedHistoryOutput {
            currency: target_currency,
            start_date: start_date_output,
            end_date: end_date_output,
            granularity,
            series: Vec::new(),
            points: Vec::new(),
            current: None,
            summary: None,
        });
    }

    let market_data = Arc::new(configure_history_market_data(
        MarketDataServiceBuilder::new(store, config.data_dir.clone())
            .with_quote_staleness(config.refresh.price_staleness)
            .offline_only()
            .build()
            .await,
        config,
    ));
    let service = PortfolioService::new(storage_arc.clone(), market_data);
    let account_ids = Vec::new();
    let valuation_history = service.load_valuation_history(&account_ids).await?;
    let cost_basis_backfill =
        collect_history_cost_basis_backfill(storage_arc.as_ref(), &account_ids).await?;
    let (capital_gains_tax_rate, include_latent_tax_adjustment) =
        resolve_capital_gains_tax_rate(config, None)?;

    let mut series_by_key = HashMap::<String, StackedHistorySeries>::new();
    let mut series_order = Vec::<String>::new();
    let mut points = Vec::with_capacity(filtered.len());
    let mut carry_forward_unit_values: HashMap<String, Decimal> = HashMap::new();

    for change_point in &filtered {
        let point = build_stacked_history_point_for_date(
            &service,
            &valuation_history,
            config,
            StackedHistoryPointInput {
                target_currency: &target_currency,
                as_of_date: change_point.timestamp.date_naive(),
                as_of_timestamp: Some(change_point.timestamp),
                timestamp: change_point.timestamp.to_rfc3339(),
                capital_gains_tax_rate,
                include_latent_tax_adjustment,
                cost_basis_backfill: &cost_basis_backfill,
            },
            &mut carry_forward_unit_values,
            &mut series_by_key,
            &mut series_order,
        )
        .await?;
        points.push(point);
    }

    let current = if want_current {
        let now = Utc::now();
        Some(
            build_stacked_history_point_for_date(
                &service,
                &valuation_history,
                config,
                StackedHistoryPointInput {
                    target_currency: &target_currency,
                    as_of_date: now.date_naive(),
                    as_of_timestamp: Some(now),
                    timestamp: now.to_rfc3339(),
                    capital_gains_tax_rate,
                    include_latent_tax_adjustment,
                    cost_basis_backfill: &cost_basis_backfill,
                },
                &mut carry_forward_unit_values,
                &mut series_by_key,
                &mut series_order,
            )
            .await?,
        )
    } else {
        None
    };

    let summary = calculate_history_summary(
        points
            .iter()
            .chain(current.iter())
            .map(|point| point.total_value.as_str()),
    );

    Ok(StackedHistoryOutput {
        currency: target_currency,
        start_date: start_date_output,
        end_date: end_date_output,
        granularity,
        series: series_order
            .into_iter()
            .filter_map(|key| series_by_key.remove(&key))
            .collect(),
        points,
        current,
        summary,
    })
}

pub async fn latent_capital_gains_tax_history(
    storage: Arc<dyn Storage>,
    config: &ResolvedConfig,
    currency: Option<String>,
    start: Option<String>,
    end: Option<String>,
    granularity: String,
    include_prices: bool,
    include_current: bool,
) -> Result<HistoryOutput> {
    portfolio_history_scoped(
        storage,
        config,
        currency,
        start,
        end,
        granularity,
        include_prices,
        include_current,
        Vec::new(),
        HistoryValueMode::LatentCapitalGainsTax,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn portfolio_history_scoped(
    storage: Arc<dyn Storage>,
    config: &ResolvedConfig,
    currency: Option<String>,
    start: Option<String>,
    end: Option<String>,
    granularity: String,
    include_prices: bool,
    include_current: bool,
    account_ids: Vec<Id>,
    value_mode: HistoryValueMode,
) -> Result<HistoryOutput> {
    // Parse date range
    let today = Utc::now().date_naive();
    let start_date = start
        .as_ref()
        .map(|s| parse_portfolio_date_bound(s, DateRangeBound::Start, today))
        .transpose()?;
    let end_date = end
        .as_ref()
        .map(|s| parse_portfolio_date_bound(s, DateRangeBound::End, today))
        .transpose()?;
    let start_date_output = start_date.map(|date| date.to_string());
    let end_date_output = end_date.map(|date| date.to_string());

    // Parse granularity
    let granularity_enum = match granularity.as_str() {
        "none" | "full" => Granularity::Full,
        "hourly" => Granularity::Hourly,
        "daily" => Granularity::Daily,
        "weekly" => Granularity::Weekly,
        "monthly" => Granularity::Monthly,
        "yearly" => Granularity::Yearly,
        _ => anyhow::bail!(
            "Invalid granularity: {granularity}. Use: none, full, hourly, daily, weekly, monthly, yearly"
        ),
    };

    // Setup storage and market data store. These read each account, balance
    // log, and price history once for the whole request and are dropped with it.
    let store: Arc<dyn MarketDataStore> = Arc::new(ReadThroughMarketDataStore::new(Arc::new(
        JsonlMarketDataStore::new(&config.data_dir),
    )));
    let storage_arc: Arc<dyn Storage> = Arc::new(ReadThroughStorage::new(storage));

    // Collect change points already bounded to the requested window and
    // granularity, so out-of-range observations are never materialized.
    let options = CollectOptions {
        account_ids: account_ids.clone(),
        include_prices,
        include_fx: false,
        target_currency: currency.clone(),
        start: start_date,
        end: end_date,
        granularity: granularity_enum,
        strategy: CoalesceStrategy::Last,
    };

    let filtered = collect_change_points(&storage_arc, &store, &options).await?;

    // A current point still needs valuing when the range holds no change
    // points, so only bail out early when the caller did not ask for one.
    let want_current = include_current && !end_date.is_some_and(|end| end < today);
    if filtered.is_empty() && !want_current {
        return Ok(HistoryOutput {
            currency: currency.unwrap_or_else(|| config.reporting_currency.clone()),
            start_date: start_date_output,
            end_date: end_date_output,
            granularity,
            points: Vec::new(),
            current: None,
            summary: None,
        });
    }

    // Setup market data service (offline mode - use cached data only)
    let market_data = Arc::new(configure_history_market_data(
        MarketDataServiceBuilder::new(store, config.data_dir.clone())
            .with_quote_staleness(config.refresh.price_staleness)
            .offline_only()
            .build()
            .await,
        config,
    ));

    let cost_basis_backfill =
        collect_history_cost_basis_backfill(storage_arc.as_ref(), &account_ids).await?;

    // Create portfolio service
    let service = PortfolioService::new(storage_arc, market_data);
    let valuation_history = service.load_valuation_history(&account_ids).await?;
    let capital_gains_tax_rate = match value_mode {
        HistoryValueMode::LatentCapitalGainsTax => {
            let rate = config
                .portfolio
                .latent_capital_gains_tax
                .rate
                .context("portfolio.latent_capital_gains_tax.rate is required for the latent capital gains tax account")?;
            Some(decimal_from_f64(
                rate,
                "portfolio.latent_capital_gains_tax.rate",
            )?)
        }
        HistoryValueMode::Portfolio { .. } => resolve_capital_gains_tax_rate(config, None)?.0,
    };

    // Calculate portfolio value at each change point
    let target_currency = currency
        .clone()
        .unwrap_or_else(|| config.reporting_currency.clone());
    let mut history_points = Vec::with_capacity(filtered.len());
    let mut previous_total_value: Option<Decimal> = None;
    let mut carry_forward_unit_values: HashMap<String, Decimal> = HashMap::new();

    for change_point in &filtered {
        let as_of_date = change_point.timestamp.date_naive();

        // Format trigger descriptions
        let trigger_descriptions: Vec<String> = change_point
            .triggers
            .iter()
            .map(|t| match t {
                crate::portfolio::ChangeTrigger::Balance { account_id, asset } => {
                    format!(
                        "balance:{}:{}",
                        account_id,
                        serde_json::to_string(asset).unwrap_or_default()
                    )
                }
                crate::portfolio::ChangeTrigger::Price { asset_id } => {
                    format!("price:{asset_id}")
                }
                crate::portfolio::ChangeTrigger::FxRate { base, quote } => {
                    format!("fx:{base}/{quote}")
                }
            })
            .collect();

        let (history_point, current_total_value) = build_history_point_for_date(
            &service,
            &valuation_history,
            config,
            HistoryPointInput {
                target_currency: &target_currency,
                as_of_date,
                as_of_timestamp: Some(change_point.timestamp),
                timestamp: change_point.timestamp.to_rfc3339(),
                change_triggers: if trigger_descriptions.is_empty() {
                    None
                } else {
                    Some(trigger_descriptions)
                },
                previous_total_value,
                capital_gains_tax_rate,
                value_mode,
                cost_basis_backfill: &cost_basis_backfill,
                account_ids: &account_ids,
            },
            &mut carry_forward_unit_values,
        )
        .await?;
        history_points.push(history_point);
        previous_total_value = current_total_value;
    }

    let current = if want_current {
        let now = Utc::now();
        let (point, _) = build_history_point_for_date(
            &service,
            &valuation_history,
            config,
            HistoryPointInput {
                target_currency: &target_currency,
                as_of_date: now.date_naive(),
                as_of_timestamp: Some(now),
                timestamp: now.to_rfc3339(),
                change_triggers: None,
                previous_total_value,
                capital_gains_tax_rate,
                value_mode,
                cost_basis_backfill: &cost_basis_backfill,
                account_ids: &account_ids,
            },
            &mut carry_forward_unit_values,
        )
        .await?;
        Some(point)
    } else {
        None
    };

    let summary = calculate_history_summary(
        history_points
            .iter()
            .chain(current.iter())
            .map(|point| point.total_value.as_str()),
    );

    Ok(HistoryOutput {
        currency: target_currency,
        start_date: start_date_output,
        end_date: end_date_output,
        granularity,
        points: history_points,
        current,
        summary,
    })
}

pub async fn portfolio_recent_history(
    storage: Arc<dyn Storage>,
    config: &ResolvedConfig,
    currency: Option<String>,
    include_prices: bool,
    anchor_date: NaiveDate,
) -> Result<Vec<HistoryPoint>> {
    let store: Arc<dyn MarketDataStore> = Arc::new(ReadThroughMarketDataStore::new(Arc::new(
        JsonlMarketDataStore::new(&config.data_dir),
    )));
    let storage_arc: Arc<dyn Storage> = Arc::new(ReadThroughStorage::new(storage));

    let options = CollectOptions {
        account_ids: Vec::new(),
        include_prices,
        include_fx: false,
        target_currency: currency.clone(),
        ..Default::default()
    };
    // Only the first change point's date matters here, so don't build the rest.
    let Some(earliest) = earliest_change_point(&storage_arc, &store, &options).await? else {
        return Ok(Vec::new());
    };
    let earliest_date = earliest.date_naive();

    let sample_dates: Vec<NaiveDate> = history_spec_dates(anchor_date, &config.tray.history_spec)?
        .into_iter()
        .filter(|date| *date >= earliest_date)
        .collect();
    if sample_dates.is_empty() {
        return Ok(Vec::new());
    }

    let market_data = Arc::new(configure_history_market_data(
        MarketDataServiceBuilder::new(store, config.data_dir.clone())
            .with_quote_staleness(config.refresh.price_staleness)
            .offline_only()
            .build()
            .await,
        config,
    ));
    let account_ids = Vec::new();
    let cost_basis_backfill =
        collect_history_cost_basis_backfill(storage_arc.as_ref(), &account_ids).await?;
    let service = PortfolioService::new(storage_arc, market_data);
    let valuation_history = service.load_valuation_history(&account_ids).await?;
    let target_currency = currency.unwrap_or_else(|| config.reporting_currency.clone());
    let (capital_gains_tax_rate, include_latent_tax_adjustment) =
        resolve_capital_gains_tax_rate(config, None)?;

    let mut history_points = Vec::with_capacity(sample_dates.len());
    let mut previous_total_value: Option<Decimal> = None;
    let mut carry_forward_unit_values: HashMap<String, Decimal> = HashMap::new();

    for as_of_date in sample_dates {
        let timestamp = as_of_date
            .and_hms_opt(0, 0, 0)
            .expect("valid start of day")
            .and_utc()
            .to_rfc3339();
        let (history_point, current_total_value) = build_history_point_for_date(
            &service,
            &valuation_history,
            config,
            HistoryPointInput {
                target_currency: &target_currency,
                as_of_date,
                as_of_timestamp: None,
                timestamp,
                change_triggers: None,
                previous_total_value,
                capital_gains_tax_rate,
                value_mode: HistoryValueMode::Portfolio {
                    include_latent_tax_adjustment,
                },
                cost_basis_backfill: &cost_basis_backfill,
                account_ids: &account_ids,
            },
            &mut carry_forward_unit_values,
        )
        .await?;
        history_points.push(history_point);
        previous_total_value = current_total_value;
    }

    Ok(history_points)
}

pub async fn portfolio_change_points(
    storage: Arc<dyn Storage>,
    config: &ResolvedConfig,
    start: Option<String>,
    end: Option<String>,
    granularity: String,
    include_prices: bool,
) -> Result<ChangePointsOutput> {
    // Parse date range
    let today = Utc::now().date_naive();
    let start_date = start
        .as_ref()
        .map(|s| parse_portfolio_date_bound(s, DateRangeBound::Start, today))
        .transpose()?;
    let end_date = end
        .as_ref()
        .map(|s| parse_portfolio_date_bound(s, DateRangeBound::End, today))
        .transpose()?;
    let start_date_output = start_date.map(|date| date.to_string());
    let end_date_output = end_date.map(|date| date.to_string());

    // Parse granularity
    let granularity_enum = match granularity.as_str() {
        "none" | "full" => Granularity::Full,
        "hourly" => Granularity::Hourly,
        "daily" => Granularity::Daily,
        "weekly" => Granularity::Weekly,
        "monthly" => Granularity::Monthly,
        "yearly" => Granularity::Yearly,
        _ => anyhow::bail!(
            "Invalid granularity: {granularity}. Use: none, full, hourly, daily, weekly, monthly, yearly"
        ),
    };

    // Setup storage and market data store
    let store: Arc<dyn MarketDataStore> = Arc::new(JsonlMarketDataStore::new(&config.data_dir));
    let storage_arc: Arc<dyn Storage> = storage;

    // Collect change points
    let options = CollectOptions {
        account_ids: Vec::new(), // All accounts
        include_prices,
        include_fx: false,
        target_currency: None,
        start: start_date,
        end: end_date,
        granularity: granularity_enum,
        strategy: CoalesceStrategy::Last,
    };

    let filtered = collect_change_points(&storage_arc, &store, &options).await?;

    Ok(ChangePointsOutput {
        start_date: start_date_output,
        end_date: end_date_output,
        granularity,
        include_prices,
        points: filtered,
    })
}

#[cfg(test)]
#[path = "../../../tests/unit/app/portfolio_tests.rs"]
mod portfolio_tests;
