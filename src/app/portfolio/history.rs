//! Valuation of portfolio history points: carry-forward asset valuations,
//! cost basis backfill, and the per-date point builders the history commands
//! sample with.

use std::collections::{HashMap, HashSet};
use std::str::FromStr;

use anyhow::{Context, Result};
use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;

use crate::config::ResolvedConfig;
use crate::format::format_base_currency_value;
use crate::market_data::{AssetId, MarketDataService};
use crate::models::{Asset, Id};
use crate::portfolio::{Grouping, PortfolioQuery, PortfolioService, ValuationHistory};
use crate::storage::Storage;

use crate::app::{
    HistoryPoint, HistorySummary, StackedHistoryComponent, StackedHistoryPoint,
    StackedHistorySeries,
};

use super::LATENT_CAPITAL_GAINS_TAX_ACCOUNT_ID;

pub(super) fn compute_percentage_change_from_previous(
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
pub(super) struct HistoryCostBasisBackfill {
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
pub(super) enum HistoryValueMode {
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

pub(super) async fn collect_history_cost_basis_backfill(
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

pub(super) fn configure_history_market_data(
    mut market_data: MarketDataService,
    config: &ResolvedConfig,
) -> MarketDataService {
    if let Some(days) = config.history.lookback_days {
        market_data = market_data.with_lookback_days(days);
    }

    market_data.with_future_projection(config.history.allow_future_projection)
}

pub(super) fn calculate_history_summary<'a>(
    total_values: impl IntoIterator<Item = &'a str>,
) -> Option<HistorySummary> {
    let mut total_values = total_values.into_iter();
    let first = total_values.next()?;
    let last = total_values.last()?;

    let initial = Decimal::from_str(first).unwrap_or(Decimal::ZERO);
    let final_val = Decimal::from_str(last).unwrap_or(Decimal::ZERO);
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

pub(super) struct HistoryPointInput<'a> {
    pub(super) target_currency: &'a str,
    pub(super) as_of_date: NaiveDate,
    /// Instant this point is valued at. `None` values at the end of
    /// `as_of_date`, which is what sampled (non change point) history wants.
    pub(super) as_of_timestamp: Option<DateTime<Utc>>,
    pub(super) timestamp: String,
    pub(super) change_triggers: Option<Vec<String>>,
    pub(super) previous_total_value: Option<Decimal>,
    pub(super) capital_gains_tax_rate: Option<Decimal>,
    pub(super) value_mode: HistoryValueMode,
    pub(super) cost_basis_backfill: &'a HashMap<String, HistoryCostBasisBackfill>,
    pub(super) account_ids: &'a [Id],
}

pub(super) struct StackedHistoryPointInput<'a> {
    pub(super) target_currency: &'a str,
    pub(super) as_of_date: NaiveDate,
    pub(super) as_of_timestamp: Option<DateTime<Utc>>,
    pub(super) timestamp: String,
    pub(super) capital_gains_tax_rate: Option<Decimal>,
    pub(super) include_latent_tax_adjustment: bool,
    pub(super) cost_basis_backfill: &'a HashMap<String, HistoryCostBasisBackfill>,
}

pub(super) async fn build_history_point_for_date(
    service: &PortfolioService,
    history: &ValuationHistory,
    config: &ResolvedConfig,
    input: HistoryPointInput<'_>,
    carry_forward_unit_values: &mut HashMap<String, Decimal>,
) -> Result<(HistoryPoint, Option<Decimal>)> {
    let query = PortfolioQuery {
        as_of_date: input.as_of_date,
        as_of_timestamp: input.as_of_timestamp,
        currency: input.target_currency.to_string(),
        currency_decimals: config.display.currency_decimals,
        grouping: Grouping::Asset,
        include_detail: false,
        capital_gains_tax_rate: input.capital_gains_tax_rate,
        equity_valuation_adjustment: None,
        account_ids: input.account_ids.to_vec(),
    };

    let snapshot = service.calculate_with_history(history, &query).await?;
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

pub(super) async fn build_stacked_history_point_for_date(
    service: &PortfolioService,
    history: &ValuationHistory,
    config: &ResolvedConfig,
    input: StackedHistoryPointInput<'_>,
    carry_forward_unit_values: &mut HashMap<String, Decimal>,
    series_by_key: &mut HashMap<String, StackedHistorySeries>,
    series_order: &mut Vec<String>,
) -> Result<StackedHistoryPoint> {
    let query = PortfolioQuery {
        as_of_date: input.as_of_date,
        as_of_timestamp: input.as_of_timestamp,
        currency: input.target_currency.to_string(),
        currency_decimals: config.display.currency_decimals,
        grouping: Grouping::Both,
        include_detail: true,
        capital_gains_tax_rate: input.capital_gains_tax_rate,
        equity_valuation_adjustment: None,
        account_ids: Vec::new(),
    };

    let snapshot = service.calculate_with_history(history, &query).await?;
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
