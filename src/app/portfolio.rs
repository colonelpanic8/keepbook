use std::collections::{HashMap, HashSet};
use std::str::FromStr;
use std::sync::Arc;

use anyhow::{Context, Result};
use chrono::{Datelike, Days, Duration, Months, NaiveDate, Utc};
use rust_decimal::Decimal;
use tracing::warn;

use crate::config::ResolvedConfig;
use crate::format::format_base_currency_value;
use crate::market_data::{
    AssetId, FxRateKind, FxRatePoint, JsonlMarketDataStore, MarketDataService,
    MarketDataServiceBuilder, MarketDataStore, PricePoint,
};
use crate::models::{Account, Asset, Id};
use crate::portfolio::{
    collect_change_points, filter_by_date_range, filter_by_granularity, AccountSummary,
    CoalesceStrategy, CollectOptions, EquityValuationAdjustment, Granularity, Grouping,
    PortfolioQuery, PortfolioService,
};
use crate::staleness::{
    check_balance_staleness, check_price_staleness, log_balance_staleness, log_price_staleness,
    resolve_balance_staleness,
};
use crate::storage::{find_account, find_connection, Storage};

#[cfg(feature = "sync")]
use super::sync::build_sync_service;
use super::{
    maybe_auto_commit, AssetBreakdownEntry, AssetBreakdownHolding, AssetBreakdownOutput,
    AssetChange, AssetChanges, AssetInfoOutput, ChangePointsOutput, HistoryOutput, HistoryPoint,
    HistorySummary, PriceHistoryFailure, PriceHistoryOutput, PriceHistoryScopeOutput,
    PriceHistoryStats, StackedHistoryComponent, StackedHistoryOutput, StackedHistoryPoint,
    StackedHistorySeries, TaxImpactOutput, TaxImpactPoint,
};

pub struct PriceHistoryRequest<'a> {
    pub storage: &'a dyn Storage,
    pub config: &'a ResolvedConfig,
    pub account: Option<&'a str>,
    pub connection: Option<&'a str>,
    pub start: Option<&'a str>,
    pub end: Option<&'a str>,
    pub interval: &'a str,
    pub lookback_days: u32,
    pub request_delay_ms: u64,
    pub currency: Option<String>,
    pub include_fx: bool,
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

#[derive(Debug, Clone, Copy)]
enum PriceHistoryInterval {
    Daily,
    Weekly,
    Monthly,
    Yearly,
}

impl PriceHistoryInterval {
    fn parse(value: &str) -> Result<Self> {
        match value.to_lowercase().as_str() {
            "daily" => Ok(Self::Daily),
            "weekly" => Ok(Self::Weekly),
            "monthly" => Ok(Self::Monthly),
            "yearly" | "annual" | "annually" => Ok(Self::Yearly),
            _ => anyhow::bail!(
                "Invalid interval: {value}. Use: daily, weekly, monthly, yearly, annual"
            ),
        }
    }

    fn as_str(&self) -> &'static str {
        match self {
            Self::Daily => "daily",
            Self::Weekly => "weekly",
            Self::Monthly => "monthly",
            Self::Yearly => "yearly",
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum DateRangeBound {
    Start,
    End,
}

impl DateRangeBound {
    fn label(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::End => "end",
        }
    }
}

fn parse_portfolio_date_bound(
    value: &str,
    bound: DateRangeBound,
    anchor_date: NaiveDate,
) -> Result<NaiveDate> {
    let value = value.trim();
    if value.eq_ignore_ascii_case("today") {
        return Ok(anchor_date);
    }

    if let Ok(date) = NaiveDate::parse_from_str(value, "%Y-%m-%d") {
        return Ok(date);
    }

    if value.len() == 4 && value.chars().all(|c| c.is_ascii_digit()) {
        let year = value.parse::<i32>()?;
        return match bound {
            DateRangeBound::Start => NaiveDate::from_ymd_opt(year, 1, 1)
                .with_context(|| format!("Invalid {} date: {value}", bound.label())),
            DateRangeBound::End => Ok(year_end(year)),
        };
    }

    if value.len() == 7
        && value.as_bytes()[4] == b'-'
        && value[..4].chars().all(|c| c.is_ascii_digit())
        && value[5..].chars().all(|c| c.is_ascii_digit())
    {
        let year = value[..4].parse::<i32>()?;
        let month = value[5..].parse::<u32>()?;
        let first_day = NaiveDate::from_ymd_opt(year, month, 1)
            .with_context(|| format!("Invalid {} date: {value}", bound.label()))?;
        return match bound {
            DateRangeBound::Start => Ok(first_day),
            DateRangeBound::End => Ok(month_end(first_day)),
        };
    }

    if let Some(date) = parse_relative_portfolio_date(value, anchor_date)? {
        return Ok(date);
    }

    anyhow::bail!(
        "Invalid {} date: {value}. Use YYYY-MM-DD, YYYY-MM, YYYY, today, or relative offsets like -1y, -3m, -2w, -10d",
        bound.label()
    )
}

fn parse_relative_portfolio_date(value: &str, anchor_date: NaiveDate) -> Result<Option<NaiveDate>> {
    let Some(sign) = value.chars().next().filter(|c| *c == '-' || *c == '+') else {
        return Ok(None);
    };
    if value.len() < 3 {
        return Ok(None);
    }

    let unit = value
        .chars()
        .last()
        .map(|c| c.to_ascii_lowercase())
        .unwrap_or_default();
    let amount = &value[1..value.len() - unit.len_utf8()];
    if amount.is_empty() || !amount.chars().all(|c| c.is_ascii_digit()) {
        return Ok(None);
    }

    let magnitude = amount.parse::<i32>()?;
    let signed = if sign == '-' { -magnitude } else { magnitude };

    let date = match unit {
        'd' => anchor_date
            .checked_add_signed(Duration::days(signed as i64))
            .with_context(|| format!("Relative date out of range: {value}"))?,
        'w' => anchor_date
            .checked_add_signed(Duration::weeks(signed as i64))
            .with_context(|| format!("Relative date out of range: {value}"))?,
        'm' => shift_months_clamped(anchor_date, signed),
        'y' => {
            let months = signed
                .checked_mul(12)
                .with_context(|| format!("Relative date out of range: {value}"))?;
            shift_months_clamped(anchor_date, months)
        }
        _ => return Ok(None),
    };

    Ok(Some(date))
}

fn compute_percentage_change_from_previous(
    previous_total: Option<Decimal>,
    current_total: Option<Decimal>,
) -> Option<String> {
    match (previous_total, current_total) {
        (None, _) => None,
        (Some(previous), Some(current)) => {
            if previous == Decimal::ZERO {
                Some("N/A".to_string())
            } else {
                Some(
                    ((current - previous) / previous * Decimal::from(100))
                        .round_dp(2)
                        .to_string(),
                )
            }
        }
        (Some(_), None) => Some("N/A".to_string()),
    }
}

#[derive(Debug, Clone, Copy)]
struct HistoryCostBasisBackfill {
    unit_cost_basis: Decimal,
}

#[derive(Debug, Clone, Copy)]
struct HistoryValuation {
    total_value: Decimal,
    prospective_capital_gains_tax: Option<Decimal>,
}

#[derive(Debug, Clone)]
struct HistoryAssetValuation {
    asset_id: String,
    value: Decimal,
}

struct HistoryPointValue {
    total_value: String,
    prospective_capital_gains_tax: Option<String>,
}

#[derive(Debug, Clone, Copy)]
enum HistoryValueMode {
    Portfolio { include_latent_tax_adjustment: bool },
    LatentCapitalGainsTax,
}

fn add_optional_decimal(total: &mut Option<Decimal>, value: Decimal) {
    *total = Some(total.unwrap_or(Decimal::ZERO) + value);
}

fn compute_history_valuation_with_carry_forward(
    by_asset: &[crate::portfolio::AssetSummary],
    carry_forward_unit_values: &mut HashMap<String, Decimal>,
    cost_basis_backfill: &HashMap<String, HistoryCostBasisBackfill>,
    capital_gains_tax_rate: Option<Decimal>,
) -> Option<HistoryValuation> {
    let mut total_value = Decimal::ZERO;
    let mut prospective_capital_gains_tax = None;
    let asset_valuations =
        compute_history_asset_valuations_with_carry_forward(by_asset, carry_forward_unit_values)?;

    for (asset_summary, asset_valuation) in by_asset.iter().zip(asset_valuations.iter()) {
        let asset_id = AssetId::from_asset(&asset_summary.asset).to_string();
        let total_amount = Decimal::from_str(&asset_summary.total_amount).ok()?;
        let asset_value = asset_valuation.value;

        total_value += asset_value;

        if let Some(tax) = asset_summary
            .prospective_capital_gains_tax
            .as_deref()
            .and_then(|value| Decimal::from_str(value).ok())
        {
            if tax > Decimal::ZERO {
                add_optional_decimal(&mut prospective_capital_gains_tax, tax);
            }
            continue;
        }

        let Some(rate) = capital_gains_tax_rate else {
            continue;
        };
        let cost_basis = asset_summary
            .cost_basis
            .as_deref()
            .and_then(|value| Decimal::from_str(value).ok())
            .or_else(|| {
                cost_basis_backfill
                    .get(&asset_id)
                    .map(|basis| basis.unit_cost_basis * total_amount)
            });
        let Some(cost_basis) = cost_basis else {
            continue;
        };

        let gain = asset_value - cost_basis;
        if gain > Decimal::ZERO {
            add_optional_decimal(&mut prospective_capital_gains_tax, gain * rate);
        }
    }

    Some(HistoryValuation {
        total_value,
        prospective_capital_gains_tax,
    })
}

fn compute_history_asset_valuations_with_carry_forward(
    by_asset: &[crate::portfolio::AssetSummary],
    carry_forward_unit_values: &mut HashMap<String, Decimal>,
) -> Option<Vec<HistoryAssetValuation>> {
    by_asset
        .iter()
        .map(|asset_summary| {
            let asset_id = AssetId::from_asset(&asset_summary.asset).to_string();
            let total_amount = Decimal::from_str(&asset_summary.total_amount).ok()?;
            let value = match &asset_summary.value_in_base {
                Some(value_str) => {
                    let value = Decimal::from_str(value_str).ok()?;
                    if total_amount != Decimal::ZERO {
                        carry_forward_unit_values.insert(asset_id.clone(), value / total_amount);
                    }
                    value
                }
                None => {
                    if total_amount == Decimal::ZERO {
                        Decimal::ZERO
                    } else {
                        carry_forward_unit_values
                            .get(&asset_id)
                            .copied()
                            .map(|unit_value| unit_value * total_amount)
                            .unwrap_or(Decimal::ZERO)
                    }
                }
            };

            Some(HistoryAssetValuation { asset_id, value })
        })
        .collect()
}

async fn collect_history_cost_basis_backfill(
    storage: &dyn Storage,
    account_ids: &[Id],
) -> Result<HashMap<String, HistoryCostBasisBackfill>> {
    let mut totals: HashMap<String, (Decimal, Decimal)> = HashMap::new();
    let scoped_account_ids: HashSet<&Id> = account_ids.iter().collect();

    for (account_id, snapshot) in storage.get_latest_balances().await? {
        if !scoped_account_ids.is_empty() && !scoped_account_ids.contains(&account_id) {
            continue;
        }

        let excluded = storage
            .get_account_config(&account_id)?
            .and_then(|config| config.exclude_from_portfolio)
            .unwrap_or(false);
        if excluded {
            continue;
        }

        for balance in snapshot.balances {
            let Some(cost_basis) = balance.cost_basis else {
                continue;
            };
            let amount = Decimal::from_str(&balance.amount)?;
            if amount == Decimal::ZERO {
                continue;
            }
            let cost_basis = Decimal::from_str(&cost_basis)?;
            let asset_id = AssetId::from_asset(&balance.asset).to_string();
            let entry = totals
                .entry(asset_id)
                .or_insert((Decimal::ZERO, Decimal::ZERO));
            entry.0 += amount;
            entry.1 += cost_basis;
        }
    }

    Ok(totals
        .into_iter()
        .filter_map(|(asset_id, (amount, cost_basis))| {
            if amount == Decimal::ZERO {
                None
            } else {
                Some((
                    asset_id,
                    HistoryCostBasisBackfill {
                        unit_cost_basis: cost_basis / amount,
                    },
                ))
            }
        })
        .collect())
}

fn history_total_value_from_snapshot(
    snapshot: &crate::portfolio::PortfolioSnapshot,
    config: &ResolvedConfig,
    mode: HistoryValueMode,
    capital_gains_tax_rate: Option<Decimal>,
    cost_basis_backfill: &HashMap<String, HistoryCostBasisBackfill>,
    carry_forward_unit_values: &mut HashMap<String, Decimal>,
) -> Result<HistoryPointValue> {
    let history_valuation = snapshot.by_asset.as_ref().and_then(|assets| {
        compute_history_valuation_with_carry_forward(
            assets,
            carry_forward_unit_values,
            cost_basis_backfill,
            capital_gains_tax_rate,
        )
    });
    let total_value = history_valuation
        .as_ref()
        .map(|valuation| {
            format_base_currency_value(valuation.total_value, config.display.currency_decimals)
        })
        .unwrap_or_else(|| snapshot.total_value.clone());
    let prospective_capital_gains_tax = history_valuation
        .as_ref()
        .and_then(|valuation| valuation.prospective_capital_gains_tax)
        .map(|tax| format_base_currency_value(tax, config.display.currency_decimals))
        .or_else(|| snapshot.prospective_capital_gains_tax.clone());

    if matches!(mode, HistoryValueMode::LatentCapitalGainsTax) {
        return Ok(HistoryPointValue {
            total_value: prospective_capital_gains_tax.clone().unwrap_or_else(|| {
                format_base_currency_value(Decimal::ZERO, config.display.currency_decimals)
            }),
            prospective_capital_gains_tax,
        });
    }

    let HistoryValueMode::Portfolio {
        include_latent_tax_adjustment,
    } = mode
    else {
        unreachable!("handled latent capital gains tax history mode above");
    };

    if !include_latent_tax_adjustment {
        return Ok(HistoryPointValue {
            total_value,
            prospective_capital_gains_tax,
        });
    }

    let tax = if let Some(tax) = history_valuation
        .as_ref()
        .and_then(|valuation| valuation.prospective_capital_gains_tax)
    {
        tax
    } else {
        let Some(tax_str) = &snapshot.prospective_capital_gains_tax else {
            return Ok(HistoryPointValue {
                total_value,
                prospective_capital_gains_tax,
            });
        };
        Decimal::from_str(tax_str)
            .with_context(|| format!("Invalid prospective_capital_gains_tax: {tax_str}"))?
    };
    if tax <= Decimal::ZERO {
        return Ok(HistoryPointValue {
            total_value,
            prospective_capital_gains_tax,
        });
    }

    let total_value = Decimal::from_str(&total_value)
        .with_context(|| format!("Invalid total_value: {total_value}"))?;
    Ok(HistoryPointValue {
        total_value: format_base_currency_value(
            total_value - tax,
            config.display.currency_decimals,
        ),
        prospective_capital_gains_tax,
    })
}

fn configure_history_market_data(
    mut market_data: MarketDataService,
    config: &ResolvedConfig,
) -> MarketDataService {
    if let Some(days) = config.history.lookback_days {
        market_data = market_data.with_lookback_days(days);
    }

    market_data.with_future_projection(config.history.allow_future_projection)
}

fn calculate_history_summary(history_points: &[HistoryPoint]) -> Option<HistorySummary> {
    if history_points.len() < 2 {
        return None;
    }

    let initial = Decimal::from_str(&history_points[0].total_value).unwrap_or(Decimal::ZERO);
    let final_val = Decimal::from_str(&history_points[history_points.len() - 1].total_value)
        .unwrap_or(Decimal::ZERO);
    let absolute_change = final_val - initial;
    let percentage_change = if initial != Decimal::ZERO {
        ((final_val - initial) / initial * Decimal::from(100))
            .round_dp(2)
            .to_string()
    } else {
        "N/A".to_string()
    };

    Some(HistorySummary {
        initial_value: initial.normalize().to_string(),
        final_value: final_val.normalize().to_string(),
        absolute_change: absolute_change.normalize().to_string(),
        percentage_change,
    })
}

struct HistoryPointInput<'a> {
    target_currency: &'a str,
    as_of_date: NaiveDate,
    timestamp: String,
    change_triggers: Option<Vec<String>>,
    previous_total_value: Option<Decimal>,
    capital_gains_tax_rate: Option<Decimal>,
    value_mode: HistoryValueMode,
    cost_basis_backfill: &'a HashMap<String, HistoryCostBasisBackfill>,
    account_ids: &'a [Id],
}

struct StackedHistoryPointInput<'a> {
    target_currency: &'a str,
    as_of_date: NaiveDate,
    timestamp: String,
    capital_gains_tax_rate: Option<Decimal>,
    include_latent_tax_adjustment: bool,
    cost_basis_backfill: &'a HashMap<String, HistoryCostBasisBackfill>,
}

async fn build_history_point_for_date(
    service: &PortfolioService,
    config: &ResolvedConfig,
    input: HistoryPointInput<'_>,
    carry_forward_unit_values: &mut HashMap<String, Decimal>,
) -> Result<(HistoryPoint, Option<Decimal>)> {
    let query = PortfolioQuery {
        as_of_date: input.as_of_date,
        currency: input.target_currency.to_string(),
        currency_decimals: config.display.currency_decimals,
        grouping: Grouping::Asset,
        include_detail: false,
        capital_gains_tax_rate: input.capital_gains_tax_rate,
        equity_valuation_adjustment: None,
        account_ids: input.account_ids.to_vec(),
    };

    let snapshot = service.calculate(&query).await?;
    let history_point_value = history_total_value_from_snapshot(
        &snapshot,
        config,
        input.value_mode,
        input.capital_gains_tax_rate,
        input.cost_basis_backfill,
        carry_forward_unit_values,
    )?;
    let current_total_value = Decimal::from_str(&history_point_value.total_value).ok();
    let percentage_change_from_previous =
        compute_percentage_change_from_previous(input.previous_total_value, current_total_value);

    Ok((
        HistoryPoint {
            timestamp: input.timestamp,
            date: input.as_of_date.to_string(),
            total_value: history_point_value.total_value,
            prospective_capital_gains_tax: history_point_value.prospective_capital_gains_tax,
            percentage_change_from_previous,
            change_triggers: input.change_triggers,
        },
        current_total_value,
    ))
}

async fn build_stacked_history_point_for_date(
    service: &PortfolioService,
    config: &ResolvedConfig,
    input: StackedHistoryPointInput<'_>,
    carry_forward_unit_values: &mut HashMap<String, Decimal>,
    series_by_key: &mut HashMap<String, StackedHistorySeries>,
    series_order: &mut Vec<String>,
) -> Result<StackedHistoryPoint> {
    let query = PortfolioQuery {
        as_of_date: input.as_of_date,
        currency: input.target_currency.to_string(),
        currency_decimals: config.display.currency_decimals,
        grouping: Grouping::Both,
        include_detail: true,
        capital_gains_tax_rate: input.capital_gains_tax_rate,
        equity_valuation_adjustment: None,
        account_ids: Vec::new(),
    };

    let snapshot = service.calculate(&query).await?;
    let history_point_value = history_total_value_from_snapshot(
        &snapshot,
        config,
        HistoryValueMode::Portfolio {
            include_latent_tax_adjustment: input.include_latent_tax_adjustment,
        },
        input.capital_gains_tax_rate,
        input.cost_basis_backfill,
        carry_forward_unit_values,
    )?;

    let mut components = Vec::new();
    let mut account_component_values = HashMap::<String, Decimal>::new();

    if let Some(accounts) = snapshot.by_account.as_ref() {
        for account in accounts {
            let key = account_series_key(&account.account_id);
            remember_series(
                series_by_key,
                series_order,
                StackedHistorySeries {
                    key: key.clone(),
                    label: format!("{} / {}", account.connection_name, account.account_name),
                    series_type: "account".to_string(),
                    account_id: Some(account.account_id.clone()),
                    account_name: Some(account.account_name.clone()),
                    connection_name: Some(account.connection_name.clone()),
                    parent_key: None,
                    asset: None,
                },
            );
        }
    }

    if let Some(assets) = snapshot.by_asset.as_ref() {
        let asset_valuations =
            compute_history_asset_valuations_with_carry_forward(assets, carry_forward_unit_values)
                .unwrap_or_default();
        for (asset_summary, asset_valuation) in assets.iter().zip(asset_valuations.iter()) {
            let total_amount = Decimal::from_str(&asset_summary.total_amount).unwrap_or_default();
            let Some(holdings) = asset_summary.holdings.as_ref() else {
                continue;
            };
            for holding in holdings {
                let holding_amount = Decimal::from_str(&holding.amount).unwrap_or_default();
                let value = if total_amount == Decimal::ZERO {
                    Decimal::ZERO
                } else {
                    asset_valuation.value * holding_amount / total_amount
                };
                let account_entry = account_component_values
                    .entry(holding.account_id.clone())
                    .or_insert(Decimal::ZERO);
                *account_entry += value;
                let key = account_asset_series_key(&holding.account_id, &asset_valuation.asset_id);
                remember_series(
                    series_by_key,
                    series_order,
                    StackedHistorySeries {
                        key: key.clone(),
                        label: format!(
                            "{} / {}",
                            holding.account_name,
                            asset_display_label(&asset_summary.asset)
                        ),
                        series_type: "account_asset".to_string(),
                        account_id: Some(holding.account_id.clone()),
                        account_name: Some(holding.account_name.clone()),
                        connection_name: None,
                        parent_key: Some(account_series_key(&holding.account_id)),
                        asset: Some(serde_json::to_value(&asset_summary.asset)?),
                    },
                );
                components.push(StackedHistoryComponent {
                    series_key: key,
                    value: format_base_currency_value(value, config.display.currency_decimals),
                });
            }
        }
    }

    if let Some(accounts) = snapshot.by_account.as_ref() {
        for account in accounts {
            let value = account_component_values
                .get(&account.account_id)
                .copied()
                .or_else(|| {
                    account
                        .value_in_base
                        .as_deref()
                        .and_then(|value| Decimal::from_str(value).ok())
                })
                .unwrap_or(Decimal::ZERO);
            components.push(StackedHistoryComponent {
                series_key: account_series_key(&account.account_id),
                value: format_base_currency_value(value, config.display.currency_decimals),
            });
        }
    }

    if input.include_latent_tax_adjustment {
        if let Some(tax) = history_point_value
            .prospective_capital_gains_tax
            .as_deref()
            .and_then(|value| Decimal::from_str(value).ok())
            .filter(|tax| *tax > Decimal::ZERO)
        {
            let key = account_series_key(LATENT_CAPITAL_GAINS_TAX_ACCOUNT_ID);
            remember_series(
                series_by_key,
                series_order,
                StackedHistorySeries {
                    key: key.clone(),
                    label: config
                        .portfolio
                        .latent_capital_gains_tax
                        .account_name
                        .clone(),
                    series_type: "account".to_string(),
                    account_id: Some(LATENT_CAPITAL_GAINS_TAX_ACCOUNT_ID.to_string()),
                    account_name: Some(
                        config
                            .portfolio
                            .latent_capital_gains_tax
                            .account_name
                            .clone(),
                    ),
                    connection_name: Some("Virtual".to_string()),
                    parent_key: None,
                    asset: None,
                },
            );
            components.push(StackedHistoryComponent {
                series_key: key,
                value: format_base_currency_value(-tax, config.display.currency_decimals),
            });
        }
    }

    Ok(StackedHistoryPoint {
        timestamp: input.timestamp,
        date: input.as_of_date.to_string(),
        total_value: history_point_value.total_value,
        components,
    })
}

fn remember_series(
    series_by_key: &mut HashMap<String, StackedHistorySeries>,
    series_order: &mut Vec<String>,
    series: StackedHistorySeries,
) {
    if series_by_key.contains_key(&series.key) {
        return;
    }
    series_order.push(series.key.clone());
    series_by_key.insert(series.key.clone(), series);
}

fn account_series_key(account_id: &str) -> String {
    format!("account:{account_id}")
}

fn account_asset_series_key(account_id: &str, asset_id: &str) -> String {
    format!("account_asset:{account_id}:{asset_id}")
}

fn asset_display_label(asset: &Asset) -> String {
    match asset {
        Asset::Currency { iso_code } => iso_code.clone(),
        Asset::ManualValue { name, .. } => name.clone(),
        Asset::Equity { ticker, exchange } => match exchange.as_deref() {
            Some(exchange) if !exchange.is_empty() => format!("{ticker}.{exchange}"),
            _ => ticker.clone(),
        },
        Asset::Crypto { symbol, network } => match network.as_deref() {
            Some(network) if !network.is_empty() => format!("{symbol} ({network})"),
            _ => symbol.clone(),
        },
    }
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

struct AssetPriceCache {
    asset: Asset,
    asset_id: AssetId,
    prices: HashMap<NaiveDate, PricePoint>,
    fetched_dates: HashSet<NaiveDate>,
}

pub async fn fetch_historical_prices(
    request: PriceHistoryRequest<'_>,
) -> Result<PriceHistoryOutput> {
    let PriceHistoryRequest {
        storage,
        config,
        account,
        connection,
        start,
        end,
        interval,
        lookback_days,
        request_delay_ms,
        currency,
        include_fx,
    } = request;

    let (scope, accounts) = resolve_price_history_scope(storage, account, connection).await?;

    let mut assets: HashSet<Asset> = HashSet::new();
    let mut earliest_balance_date: Option<NaiveDate> = None;

    for account in &accounts {
        let snapshots = storage.get_balance_snapshots(&account.id).await?;
        for snapshot in snapshots {
            let date = snapshot.timestamp.date_naive();
            earliest_balance_date = Some(match earliest_balance_date {
                Some(current) => current.min(date),
                None => date,
            });
            for balance in snapshot.balances {
                assets.insert(balance.asset.normalized());
            }
        }
    }

    if assets.is_empty() {
        anyhow::bail!("No balances found for selected scope");
    }

    let start_date = match start {
        Some(value) => NaiveDate::parse_from_str(value, "%Y-%m-%d")
            .with_context(|| format!("Invalid start date: {value}"))?,
        None => earliest_balance_date.context("No balances found to infer start date")?,
    };

    let end_date = match end {
        Some(value) => NaiveDate::parse_from_str(value, "%Y-%m-%d")
            .with_context(|| format!("Invalid end date: {value}"))?,
        None => Utc::now().date_naive(),
    };

    if start_date > end_date {
        anyhow::bail!("Start date must be on or before end date");
    }

    let interval = PriceHistoryInterval::parse(interval)?;
    let aligned_start = align_start_date(start_date, interval);

    let target_currency = currency.unwrap_or_else(|| config.reporting_currency.clone());
    let target_currency_upper = target_currency.to_uppercase();

    let store: Arc<dyn MarketDataStore> = Arc::new(JsonlMarketDataStore::new(&config.data_dir));
    let market_data = MarketDataServiceBuilder::new(store.clone(), config.data_dir.clone())
        .with_lookback_days(lookback_days)
        .build()
        .await;

    let mut asset_caches = Vec::new();
    for asset in assets {
        let asset_id = AssetId::from_asset(&asset);
        let prices = load_price_cache(&store, &asset_id).await?;
        asset_caches.push(AssetPriceCache {
            asset,
            asset_id,
            prices,
            fetched_dates: HashSet::new(),
        });
    }

    asset_caches.sort_by_key(|cache| cache.asset_id.to_string());

    let mut failures = Vec::new();
    let mut failure_count = 0usize;
    let failure_limit = 50usize;
    let fetch_start = aligned_start - Duration::days(lookback_days as i64);
    let request_delay = if request_delay_ms > 0 {
        Some(std::time::Duration::from_millis(request_delay_ms))
    } else {
        None
    };

    for asset_cache in asset_caches.iter_mut() {
        match &asset_cache.asset {
            Asset::Equity { .. } | Asset::Crypto { .. } => {
                let mut needs_fetch = false;
                let mut current = aligned_start;
                while current <= end_date {
                    if resolve_cached_price(&asset_cache.prices, current, lookback_days).is_none() {
                        needs_fetch = true;
                        break;
                    }
                    current = advance_interval_date(current, interval);
                }

                if !needs_fetch {
                    continue;
                }

                match market_data
                    .price_closes_range(&asset_cache.asset, fetch_start, end_date)
                    .await
                {
                    Ok(fetched_prices) => {
                        for price in fetched_prices {
                            let as_of_date = price.as_of_date;
                            if upsert_price_cache(&mut asset_cache.prices, price) {
                                asset_cache.fetched_dates.insert(as_of_date);
                            }
                        }
                    }
                    Err(e) => {
                        failure_count += 1;
                        if failures.len() < failure_limit {
                            failures.push(PriceHistoryFailure {
                                kind: "price_range".to_string(),
                                date: format!("{fetch_start}/{end_date}"),
                                error: e.to_string(),
                                asset_id: Some(asset_cache.asset_id.to_string()),
                                asset: Some(asset_cache.asset.clone()),
                                base: None,
                                quote: None,
                            });
                        }
                    }
                }

                if let Some(delay) = request_delay {
                    tokio::time::sleep(delay).await;
                }
            }
            Asset::Currency { .. } | Asset::ManualValue { .. } => {}
        }
    }

    let mut fx_cache: HashMap<(String, String), HashMap<NaiveDate, FxRatePoint>> = HashMap::new();

    if include_fx {
        for asset_cache in &asset_caches {
            let base_currency = match &asset_cache.asset {
                Asset::Currency { iso_code } => Some(iso_code),
                Asset::ManualValue { currency, .. } => Some(currency),
                Asset::Equity { .. } | Asset::Crypto { .. } => None,
            };
            if let Some(base_currency) = base_currency {
                let base = base_currency.to_uppercase();
                if base != target_currency_upper {
                    let key = (base.clone(), target_currency_upper.clone());
                    if !fx_cache.contains_key(&key) {
                        fx_cache.insert(key.clone(), load_fx_cache(&store, &key.0, &key.1).await?);
                    }
                }
            }
        }
    }

    let mut price_stats = PriceHistoryStats::default();
    let mut fx_stats = PriceHistoryStats::default();

    let mut current = aligned_start;
    let mut points = 0usize;
    {
        let mut fx_ctx = FxRateContext {
            market_data: &market_data,
            store: &store,
            fx_cache: &mut fx_cache,
            stats: &mut fx_stats,
            failures: &mut failures,
            failure_count: &mut failure_count,
            failure_limit,
            lookback_days,
        };

        while current <= end_date {
            points += 1;
            for asset_cache in asset_caches.iter_mut() {
                let mut should_delay = false;
                match &asset_cache.asset {
                    Asset::Currency { iso_code } => {
                        if include_fx {
                            let base = iso_code.to_uppercase();
                            if base != target_currency_upper {
                                ensure_fx_rate(&mut fx_ctx, &base, &target_currency_upper, current)
                                    .await?;
                            }
                        }
                    }
                    Asset::ManualValue { currency, .. } => {
                        if include_fx {
                            let base = currency.to_uppercase();
                            if base != target_currency_upper {
                                ensure_fx_rate(&mut fx_ctx, &base, &target_currency_upper, current)
                                    .await?;
                            }
                        }
                    }
                    Asset::Equity { .. } | Asset::Crypto { .. } => {
                        price_stats.attempted += 1;
                        if let Some((price, exact)) =
                            resolve_cached_price(&asset_cache.prices, current, lookback_days)
                        {
                            if exact {
                                if asset_cache.fetched_dates.contains(&price.as_of_date) {
                                    price_stats.fetched += 1;
                                } else {
                                    price_stats.existing += 1;
                                }
                            } else {
                                price_stats.lookback += 1;
                            }

                            if include_fx
                                && price.quote_currency.to_uppercase() != target_currency_upper
                            {
                                ensure_fx_rate(
                                    &mut fx_ctx,
                                    &price.quote_currency.to_uppercase(),
                                    &target_currency_upper,
                                    current,
                                )
                                .await?;
                            }
                            continue;
                        }

                        price_stats.missing += 1;
                        *fx_ctx.failure_count += 1;
                        if fx_ctx.failures.len() < fx_ctx.failure_limit {
                            fx_ctx.failures.push(PriceHistoryFailure {
                                kind: "price".to_string(),
                                date: current.to_string(),
                                error: format!(
                                    "No price found for asset {} on or before {}",
                                    asset_cache.asset_id, current
                                ),
                                asset_id: Some(asset_cache.asset_id.to_string()),
                                asset: Some(asset_cache.asset.clone()),
                                base: None,
                                quote: None,
                            });
                        }
                        should_delay = request_delay.is_some();
                    }
                }

                if should_delay {
                    if let Some(delay) = request_delay {
                        tokio::time::sleep(delay).await;
                    }
                }
            }

            current = advance_interval_date(current, interval);
        }
    }

    let days = (end_date - start_date).num_days() as usize + 1;

    let assets_output = asset_caches
        .iter()
        .map(|cache| AssetInfoOutput {
            asset: cache.asset.clone(),
            asset_id: cache.asset_id.to_string(),
        })
        .collect();

    let output = PriceHistoryOutput {
        scope,
        currency: target_currency,
        interval: interval.as_str().to_string(),
        start_date: start_date.to_string(),
        end_date: end_date.to_string(),
        earliest_balance_date: earliest_balance_date.map(|d| d.to_string()),
        days,
        points,
        assets: assets_output,
        prices: price_stats,
        fx: if include_fx { Some(fx_stats) } else { None },
        failure_count,
        failures,
    };

    maybe_auto_commit(config, "market data fetch");

    Ok(output)
}

pub async fn fill_prices_at_date(request: PriceHistoryRequest<'_>) -> Result<PriceHistoryOutput> {
    let date = request
        .start
        .context("fill_prices_at_date requires a start date")?;
    if let Some(end) = request.end {
        anyhow::ensure!(
            end == date,
            "fill_prices_at_date requires start and end to match"
        );
    }

    fetch_historical_prices(PriceHistoryRequest {
        start: Some(date),
        end: Some(date),
        interval: "daily",
        ..request
    })
    .await
}

fn advance_interval_date(date: NaiveDate, interval: PriceHistoryInterval) -> NaiveDate {
    match interval {
        PriceHistoryInterval::Daily => date + Duration::days(1),
        PriceHistoryInterval::Weekly => date + Duration::days(7),
        PriceHistoryInterval::Monthly => next_month_end(date),
        PriceHistoryInterval::Yearly => next_year_end(date),
    }
}

fn align_start_date(date: NaiveDate, interval: PriceHistoryInterval) -> NaiveDate {
    match interval {
        PriceHistoryInterval::Monthly => month_end(date),
        PriceHistoryInterval::Yearly => year_end(date.year()),
        _ => date,
    }
}

fn next_year_end(date: NaiveDate) -> NaiveDate {
    year_end(date.year() + 1)
}

fn year_end(year: i32) -> NaiveDate {
    NaiveDate::from_ymd_opt(year, 12, 31).expect("valid year end")
}

fn next_month_end(date: NaiveDate) -> NaiveDate {
    let (year, month) = if date.month() == 12 {
        (date.year() + 1, 1)
    } else {
        (date.year(), date.month() + 1)
    };
    let day = days_in_month(year, month);
    NaiveDate::from_ymd_opt(year, month, day).expect("valid next month end")
}

fn month_end(date: NaiveDate) -> NaiveDate {
    let day = days_in_month(date.year(), date.month());
    NaiveDate::from_ymd_opt(date.year(), date.month(), day).expect("valid month end")
}

fn days_in_month(year: i32, month: u32) -> u32 {
    let (next_year, next_month) = if month == 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    };
    let first_next = NaiveDate::from_ymd_opt(next_year, next_month, 1).expect("valid next month");
    let last = first_next - Duration::days(1);
    last.day()
}

fn shift_months_clamped(date: NaiveDate, months: i32) -> NaiveDate {
    let month_index = date.year() * 12 + date.month0() as i32 + months;
    let year = month_index.div_euclid(12);
    let month0 = month_index.rem_euclid(12) as u32;
    let month = month0 + 1;
    let day = date.day().min(days_in_month(year, month));
    NaiveDate::from_ymd_opt(year, month, day).expect("valid shifted month")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RelativeDateUnit {
    Day,
    Week,
    Month,
    Year,
}

impl RelativeDateUnit {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "d" | "day" | "days" => Some(Self::Day),
            "w" | "week" | "weeks" => Some(Self::Week),
            "m" | "mo" | "mon" | "month" | "months" => Some(Self::Month),
            "y" | "yr" | "year" | "years" => Some(Self::Year),
            _ => None,
        }
    }

    fn shift(self, anchor_date: NaiveDate, count: i32) -> NaiveDate {
        match self {
            Self::Day => anchor_date + Duration::days(count as i64),
            Self::Week => anchor_date + Duration::days((count * 7) as i64),
            Self::Month => shift_months_clamped(anchor_date, count),
            Self::Year => shift_months_clamped(anchor_date, count * 12),
        }
    }
}

fn parse_count_and_unit(value: &str) -> Option<(usize, RelativeDateUnit)> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }

    let digit_count = trimmed.chars().take_while(|c| c.is_ascii_digit()).count();
    if digit_count > 0 {
        let count = trimmed[..digit_count].parse::<usize>().ok()?;
        let unit = RelativeDateUnit::parse(trimmed[digit_count..].trim())?;
        return Some((count, unit));
    }

    let mut parts = trimmed.split_whitespace();
    let count = parts.next()?.parse::<usize>().ok()?;
    let unit = RelativeDateUnit::parse(parts.next()?)?;
    if parts.next().is_some() {
        return None;
    }
    Some((count, unit))
}

fn parse_history_spec_entry(anchor_date: NaiveDate, spec: &str) -> Result<Vec<NaiveDate>> {
    let normalized = spec.trim().to_lowercase();
    if normalized.is_empty() {
        anyhow::bail!("history spec entry must not be empty");
    }
    if normalized == "today" {
        return Ok(vec![anchor_date]);
    }

    for prefix in ["each of the last ", "last "] {
        if let Some(rest) = normalized.strip_prefix(prefix) {
            let (count, unit) = parse_count_and_unit(rest)
                .with_context(|| format!("Invalid history spec entry: {spec}"))?;
            if count == 0 {
                anyhow::bail!("history spec entry must use a positive count: {spec}");
            }
            return Ok((0..count)
                .map(|offset| unit.shift(anchor_date, -(offset as i32)))
                .collect());
        }
    }

    let rest = normalized.strip_suffix(" ago").unwrap_or(&normalized);
    let (count, unit) = parse_count_and_unit(rest)
        .with_context(|| format!("Invalid history spec entry: {spec}"))?;
    if count == 0 {
        anyhow::bail!("history spec entry must use a positive count: {spec}");
    }
    Ok(vec![unit.shift(anchor_date, -(count as i32))])
}

fn history_spec_dates(anchor_date: NaiveDate, history_spec: &[String]) -> Result<Vec<NaiveDate>> {
    let mut dates = Vec::new();
    for spec in history_spec {
        dates.extend(parse_history_spec_entry(anchor_date, spec)?);
    }
    dates.sort_unstable();
    dates.dedup();
    Ok(dates)
}

async fn resolve_price_history_scope(
    storage: &dyn Storage,
    account: Option<&str>,
    connection: Option<&str>,
) -> Result<(PriceHistoryScopeOutput, Vec<Account>)> {
    if account.is_some() && connection.is_some() {
        anyhow::bail!("Specify only one of --account or --connection");
    }

    if let Some(id_or_name) = account {
        let account = find_account(storage, id_or_name)
            .await?
            .context(format!("Account not found: {id_or_name}"))?;
        return Ok((
            PriceHistoryScopeOutput::Account {
                id: account.id.to_string(),
                name: account.name.clone(),
            },
            vec![account],
        ));
    }

    if let Some(id_or_name) = connection {
        let connection = find_connection(storage, id_or_name)
            .await?
            .context(format!("Connection not found: {id_or_name}"))?;
        let mut accounts = Vec::new();
        let mut seen_ids: HashSet<Id> = HashSet::new();

        if !connection.state.account_ids.is_empty() {
            for account_id in &connection.state.account_ids {
                if !seen_ids.insert(account_id.clone()) {
                    continue;
                }
                if !Id::is_path_safe(account_id.as_str()) {
                    warn!(
                        connection_id = %connection.id(),
                        account_id = %account_id,
                        "skipping account with unsafe id referenced by connection"
                    );
                    continue;
                }
                match storage.get_account(account_id).await? {
                    Some(account) => {
                        if account.connection_id != *connection.id() {
                            warn!(
                                connection_id = %connection.id(),
                                account_id = %account_id,
                                account_connection_id = %account.connection_id,
                                "account referenced by connection belongs to different connection"
                            );
                        } else {
                            accounts.push(account);
                        }
                    }
                    None => {
                        warn!(
                            connection_id = %connection.id(),
                            account_id = %account_id,
                            "account referenced by connection not found"
                        );
                    }
                }
            }
        }

        let extra_accounts: Vec<Account> = storage
            .list_accounts()
            .await?
            .into_iter()
            .filter(|a| a.connection_id == *connection.id() && !seen_ids.contains(&a.id))
            .collect();

        for account in extra_accounts {
            seen_ids.insert(account.id.clone());
            accounts.push(account);
        }

        if accounts.is_empty() {
            anyhow::bail!("No accounts found for connection {}", connection.name());
        }

        return Ok((
            PriceHistoryScopeOutput::Connection {
                id: connection.id().to_string(),
                name: connection.name().to_string(),
            },
            accounts,
        ));
    }

    let accounts = storage.list_accounts().await?;
    if accounts.is_empty() {
        anyhow::bail!("No accounts found");
    }

    Ok((PriceHistoryScopeOutput::Portfolio, accounts))
}

async fn load_price_cache(
    store: &Arc<dyn MarketDataStore>,
    asset_id: &AssetId,
) -> Result<HashMap<NaiveDate, PricePoint>> {
    let prices = store.get_all_prices(asset_id).await?;
    let mut map: HashMap<NaiveDate, PricePoint> = HashMap::new();

    for price in prices {
        match map.get(&price.as_of_date) {
            Some(existing) if existing.timestamp >= price.timestamp => {}
            _ => {
                map.insert(price.as_of_date, price);
            }
        }
    }

    Ok(map)
}

async fn load_fx_cache(
    store: &Arc<dyn MarketDataStore>,
    base: &str,
    quote: &str,
) -> Result<HashMap<NaiveDate, FxRatePoint>> {
    let rates = store.get_all_fx_rates(base, quote).await?;
    let mut map: HashMap<NaiveDate, FxRatePoint> = HashMap::new();

    for rate in rates {
        if rate.kind != FxRateKind::Close {
            continue;
        }
        match map.get(&rate.as_of_date) {
            Some(existing) if existing.timestamp >= rate.timestamp => {}
            _ => {
                map.insert(rate.as_of_date, rate);
            }
        }
    }

    Ok(map)
}

fn resolve_cached_price(
    cache: &HashMap<NaiveDate, PricePoint>,
    date: NaiveDate,
    lookback_days: u32,
) -> Option<(PricePoint, bool)> {
    if let Some(price) = cache.get(&date) {
        return Some((price.clone(), true));
    }

    for offset in 1..=lookback_days {
        let target = date - Duration::days(offset as i64);
        if let Some(price) = cache.get(&target) {
            return Some((price.clone(), false));
        }
    }

    None
}

fn resolve_cached_fx(
    cache: &HashMap<NaiveDate, FxRatePoint>,
    date: NaiveDate,
    lookback_days: u32,
) -> Option<(FxRatePoint, bool)> {
    if let Some(rate) = cache.get(&date) {
        return Some((rate.clone(), true));
    }

    for offset in 1..=lookback_days {
        let target = date - Duration::days(offset as i64);
        if let Some(rate) = cache.get(&target) {
            return Some((rate.clone(), false));
        }
    }

    None
}

fn upsert_price_cache(cache: &mut HashMap<NaiveDate, PricePoint>, price: PricePoint) -> bool {
    match cache.get(&price.as_of_date) {
        Some(existing) if existing.timestamp >= price.timestamp => false,
        _ => {
            cache.insert(price.as_of_date, price);
            true
        }
    }
}

fn upsert_fx_cache(cache: &mut HashMap<NaiveDate, FxRatePoint>, rate: FxRatePoint) -> bool {
    match cache.get(&rate.as_of_date) {
        Some(existing) if existing.timestamp >= rate.timestamp => false,
        _ => {
            cache.insert(rate.as_of_date, rate);
            true
        }
    }
}

struct FxRateContext<'a> {
    market_data: &'a MarketDataService,
    store: &'a Arc<dyn MarketDataStore>,
    fx_cache: &'a mut HashMap<(String, String), HashMap<NaiveDate, FxRatePoint>>,
    stats: &'a mut PriceHistoryStats,
    failures: &'a mut Vec<PriceHistoryFailure>,
    failure_count: &'a mut usize,
    failure_limit: usize,
    lookback_days: u32,
}

async fn ensure_fx_rate(
    ctx: &mut FxRateContext<'_>,
    base: &str,
    quote: &str,
    date: NaiveDate,
) -> Result<()> {
    ctx.stats.attempted += 1;

    let base_upper = base.to_uppercase();
    let quote_upper = quote.to_uppercase();
    let key = (base_upper.clone(), quote_upper.clone());

    if !ctx.fx_cache.contains_key(&key) {
        ctx.fx_cache.insert(
            key.clone(),
            load_fx_cache(ctx.store, &base_upper, &quote_upper).await?,
        );
    }

    let cache = ctx
        .fx_cache
        .get(&key)
        .expect("fx cache should be initialized");

    if let Some((_, exact)) = resolve_cached_fx(cache, date, ctx.lookback_days) {
        if exact {
            ctx.stats.existing += 1;
        } else {
            ctx.stats.lookback += 1;
        }
        return Ok(());
    }

    match ctx
        .market_data
        .fx_close(&base_upper, &quote_upper, date)
        .await
    {
        Ok(rate) => {
            if rate.as_of_date == date {
                ctx.stats.fetched += 1;
            } else {
                ctx.stats.lookback += 1;
            }
            if let Some(cache) = ctx.fx_cache.get_mut(&key) {
                upsert_fx_cache(cache, rate);
            }
        }
        Err(e) => {
            ctx.stats.missing += 1;
            *ctx.failure_count += 1;
            if ctx.failures.len() < ctx.failure_limit {
                ctx.failures.push(PriceHistoryFailure {
                    kind: "fx".to_string(),
                    date: date.to_string(),
                    error: e.to_string(),
                    asset_id: None,
                    asset: None,
                    base: Some(base_upper),
                    quote: Some(quote_upper),
                });
            }
        }
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn portfolio_snapshot(
    storage: Arc<dyn Storage>,
    config: &ResolvedConfig,
    currency: Option<String>,
    date: Option<String>,
    group_by: String,
    detail: bool,
    capital_gains_tax_rate: Option<String>,
    equity_change_percent: Option<String>,
    target_pre_tax_total_value: Option<String>,
    auto: bool,
    offline: bool,
    dry_run: bool,
    force_refresh: bool,
) -> Result<crate::portfolio::PortfolioSnapshot> {
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

#[allow(clippy::too_many_arguments)]
pub async fn portfolio_tax_impact(
    storage: Arc<dyn Storage>,
    config: &ResolvedConfig,
    currency: Option<String>,
    date: Option<String>,
    capital_gains_tax_rate: Option<String>,
    min: Option<String>,
    max: Option<String>,
    points: usize,
) -> Result<TaxImpactOutput> {
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
) -> Result<HistoryOutput> {
    portfolio_history_scoped(
        storage,
        config,
        currency,
        start,
        end,
        granularity,
        include_prices,
        Vec::new(),
        HistoryValueMode::Portfolio {
            include_latent_tax_adjustment: resolve_capital_gains_tax_rate(config, None)?.1,
        },
    )
    .await
}

pub async fn portfolio_history_for_accounts(
    storage: Arc<dyn Storage>,
    config: &ResolvedConfig,
    currency: Option<String>,
    start: Option<String>,
    end: Option<String>,
    granularity: String,
    include_prices: bool,
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
        account_ids,
        HistoryValueMode::Portfolio {
            include_latent_tax_adjustment: false,
        },
    )
    .await
}

pub async fn portfolio_stacked_history(
    storage: Arc<dyn Storage>,
    config: &ResolvedConfig,
    currency: Option<String>,
    start: Option<String>,
    end: Option<String>,
    granularity: String,
    include_prices: bool,
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

    let store: Arc<dyn MarketDataStore> = Arc::new(JsonlMarketDataStore::new(&config.data_dir));
    let storage_arc: Arc<dyn Storage> = storage;
    let options = CollectOptions {
        account_ids: Vec::new(),
        include_prices,
        include_fx: false,
        target_currency: currency.clone(),
    };
    let change_points = collect_change_points(&storage_arc, &store, &options).await?;
    let filtered_by_date = filter_by_date_range(change_points, start_date, end_date);
    let filtered =
        filter_by_granularity(filtered_by_date, granularity_enum, CoalesceStrategy::Last);
    let target_currency = currency.unwrap_or_else(|| config.reporting_currency.clone());

    if filtered.is_empty() {
        return Ok(StackedHistoryOutput {
            currency: target_currency,
            start_date: start_date_output,
            end_date: end_date_output,
            granularity,
            series: Vec::new(),
            points: Vec::new(),
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
            config,
            StackedHistoryPointInput {
                target_currency: &target_currency,
                as_of_date: change_point.timestamp.date_naive(),
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
) -> Result<HistoryOutput> {
    portfolio_history_scoped(
        storage,
        config,
        currency,
        start,
        end,
        granularity,
        include_prices,
        Vec::new(),
        HistoryValueMode::LatentCapitalGainsTax,
    )
    .await
}

async fn portfolio_history_scoped(
    storage: Arc<dyn Storage>,
    config: &ResolvedConfig,
    currency: Option<String>,
    start: Option<String>,
    end: Option<String>,
    granularity: String,
    include_prices: bool,
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

    // Setup storage and market data store
    let store: Arc<dyn MarketDataStore> = Arc::new(JsonlMarketDataStore::new(&config.data_dir));
    let storage_arc: Arc<dyn Storage> = storage;

    // Collect change points
    let options = CollectOptions {
        account_ids: account_ids.clone(),
        include_prices,
        include_fx: false,
        target_currency: currency.clone(),
    };

    let change_points = collect_change_points(&storage_arc, &store, &options).await?;

    // Filter by date range
    let filtered_by_date = filter_by_date_range(change_points, start_date, end_date);

    // Filter by granularity
    let filtered =
        filter_by_granularity(filtered_by_date, granularity_enum, CoalesceStrategy::Last);

    if filtered.is_empty() {
        return Ok(HistoryOutput {
            currency: currency.unwrap_or_else(|| config.reporting_currency.clone()),
            start_date: start_date_output,
            end_date: end_date_output,
            granularity,
            points: Vec::new(),
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
            config,
            HistoryPointInput {
                target_currency: &target_currency,
                as_of_date,
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

    let summary = calculate_history_summary(&history_points);

    Ok(HistoryOutput {
        currency: target_currency,
        start_date: start_date_output,
        end_date: end_date_output,
        granularity,
        points: history_points,
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
    let store: Arc<dyn MarketDataStore> = Arc::new(JsonlMarketDataStore::new(&config.data_dir));
    let storage_arc: Arc<dyn Storage> = storage;

    let options = CollectOptions {
        account_ids: Vec::new(),
        include_prices,
        include_fx: false,
        target_currency: currency.clone(),
    };
    let change_points = collect_change_points(&storage_arc, &store, &options).await?;
    let Some(earliest_date) = change_points
        .iter()
        .map(|point| point.timestamp.date_naive())
        .min()
    else {
        return Ok(Vec::new());
    };

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
            config,
            HistoryPointInput {
                target_currency: &target_currency,
                as_of_date,
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
    };

    let change_points = collect_change_points(&storage_arc, &store, &options).await?;

    // Filter by date range
    let filtered_by_date = filter_by_date_range(change_points, start_date, end_date);

    // Filter by granularity
    let filtered =
        filter_by_granularity(filtered_by_date, granularity_enum, CoalesceStrategy::Last);

    Ok(ChangePointsOutput {
        start_date: start_date_output,
        end_date: end_date_output,
        granularity,
        include_prices,
        points: filtered,
    })
}

#[cfg(test)]
#[path = "../../tests/unit/app/portfolio_tests.rs"]
mod portfolio_tests;
