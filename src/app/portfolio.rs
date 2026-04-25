use std::collections::{HashMap, HashSet};
use std::str::FromStr;
use std::sync::Arc;

use anyhow::{Context, Result};
use chrono::{Datelike, Duration, NaiveDate, Utc};
use rust_decimal::Decimal;
use tracing::warn;

use crate::config::ResolvedConfig;
use crate::format::format_base_currency_value;
use crate::market_data::{
    AssetId, FxRateKind, FxRatePoint, JsonlMarketDataStore, MarketDataService,
    MarketDataServiceBuilder, MarketDataStore, PriceKind, PricePoint,
};
use crate::models::{Account, Asset, Id};
use crate::portfolio::{
    collect_change_points, filter_by_date_range, filter_by_granularity, AccountSummary,
    CoalesceStrategy, CollectOptions, Granularity, Grouping, PortfolioQuery, PortfolioService,
};
use crate::staleness::{
    check_balance_staleness, check_price_staleness, log_balance_staleness, log_price_staleness,
    resolve_balance_staleness,
};
use crate::storage::{find_account, find_connection, Storage};

use super::sync::build_sync_service;
use super::{
    maybe_auto_commit, AssetInfoOutput, ChangePointsOutput, HistoryOutput, HistoryPoint,
    HistorySummary, PriceHistoryFailure, PriceHistoryOutput, PriceHistoryScopeOutput,
    PriceHistoryStats,
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

fn compute_history_total_value_with_carry_forward(
    by_asset: &[crate::portfolio::AssetSummary],
    carry_forward_unit_values: &mut HashMap<String, Decimal>,
) -> Option<Decimal> {
    let mut total_value = Decimal::ZERO;

    for asset_summary in by_asset {
        let asset_id = AssetId::from_asset(&asset_summary.asset).to_string();
        let total_amount = Decimal::from_str(&asset_summary.total_amount).ok()?;

        let asset_value = match &asset_summary.value_in_base {
            Some(value_str) => {
                let value = Decimal::from_str(value_str).ok()?;
                if total_amount != Decimal::ZERO {
                    carry_forward_unit_values.insert(asset_id, value / total_amount);
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

        total_value += asset_value;
    }

    Some(total_value)
}

fn history_total_value_from_snapshot(
    snapshot: &crate::portfolio::PortfolioSnapshot,
    config: &ResolvedConfig,
    carry_forward_unit_values: &mut HashMap<String, Decimal>,
) -> String {
    snapshot
        .by_asset
        .as_ref()
        .and_then(|assets| {
            compute_history_total_value_with_carry_forward(assets, carry_forward_unit_values)
        })
        .map(|value| format_base_currency_value(value, config.display.currency_decimals))
        .unwrap_or_else(|| snapshot.total_value.clone())
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

async fn build_history_point_for_date(
    service: &PortfolioService,
    config: &ResolvedConfig,
    target_currency: &str,
    as_of_date: NaiveDate,
    timestamp: String,
    change_triggers: Option<Vec<String>>,
    previous_total_value: Option<Decimal>,
    carry_forward_unit_values: &mut HashMap<String, Decimal>,
) -> Result<(HistoryPoint, Option<Decimal>)> {
    let query = PortfolioQuery {
        as_of_date,
        currency: target_currency.to_string(),
        currency_decimals: config.display.currency_decimals,
        grouping: Grouping::Asset,
        include_detail: false,
        capital_gains_tax_rate: None,
    };

    let snapshot = service.calculate(&query).await?;
    let history_total_value =
        history_total_value_from_snapshot(&snapshot, config, carry_forward_unit_values);
    let current_total_value = Decimal::from_str(&history_total_value).ok();
    let percentage_change_from_previous =
        compute_percentage_change_from_previous(previous_total_value, current_total_value);

    Ok((
        HistoryPoint {
            timestamp,
            date: as_of_date.to_string(),
            total_value: history_total_value,
            percentage_change_from_previous,
            change_triggers,
        },
        current_total_value,
    ))
}

fn parse_tax_rate_fraction(rate: &str, context: &str) -> Result<Decimal> {
    Decimal::from_str(rate)
        .with_context(|| format!("Invalid {context}: {rate}"))
        .map(|rate| rate / Decimal::from(100))
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

    asset_caches.sort_by(|a, b| a.asset_id.to_string().cmp(&b.asset_id.to_string()));

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
            Asset::Currency { .. } => {}
        }
    }

    let mut fx_cache: HashMap<(String, String), HashMap<NaiveDate, FxRatePoint>> = HashMap::new();

    if include_fx {
        for asset_cache in &asset_caches {
            if let Asset::Currency { iso_code } = &asset_cache.asset {
                let base = iso_code.to_uppercase();
                if base == target_currency_upper {
                    continue;
                }
                let key = (base.clone(), target_currency_upper.clone());
                if !fx_cache.contains_key(&key) {
                    fx_cache.insert(key.clone(), load_fx_cache(&store, &key.0, &key.1).await?);
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
                                    "No close price found for asset {} on or before {}",
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
        if price.kind != PriceKind::Close {
            continue;
        }
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

                        // Find most recent cached price (quote or close, with lookback)
                        let mut cached_price = None;

                        // Try Quote for today first
                        if let Some(p) = store
                            .get_price(&asset_id, query.as_of_date, PriceKind::Quote)
                            .await?
                        {
                            cached_price = Some(p);
                        }

                        // If no quote, try Close with lookback (7 days)
                        if cached_price.is_none() {
                            for offset in 0..=7i64 {
                                let target_date = query.as_of_date - Duration::days(offset);
                                if let Some(p) = store
                                    .get_price(&asset_id, target_date, PriceKind::Close)
                                    .await?
                                {
                                    cached_price = Some(p);
                                    break;
                                }
                            }
                        }

                        let check = check_price_staleness(
                            cached_price.as_ref(),
                            config.refresh.price_staleness,
                        );
                        log_price_staleness(&asset_key, &check);
                    }
                    Asset::Currency { .. } => {
                        // Currency doesn't need price lookup (only FX)
                    }
                }
            }
        }
    }

    // Sync stale connections
    if !connections_to_sync.is_empty() {
        let sync_service = build_sync_service(storage.clone(), config).await;
        for connection in &connections_to_sync {
            let _ = sync_service.sync_connection(connection.id().as_ref()).await;
        }
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

pub async fn portfolio_history(
    storage: Arc<dyn Storage>,
    config: &ResolvedConfig,
    currency: Option<String>,
    start: Option<String>,
    end: Option<String>,
    granularity: String,
    include_prices: bool,
) -> Result<HistoryOutput> {
    // Parse date range
    let start_date = start
        .as_ref()
        .map(|s| {
            NaiveDate::parse_from_str(s, "%Y-%m-%d")
                .with_context(|| format!("Invalid start date: {s}"))
        })
        .transpose()?;
    let end_date = end
        .as_ref()
        .map(|s| {
            NaiveDate::parse_from_str(s, "%Y-%m-%d")
                .with_context(|| format!("Invalid end date: {s}"))
        })
        .transpose()?;

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
            start_date: start,
            end_date: end,
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

    // Create portfolio service
    let service = PortfolioService::new(storage_arc, market_data);

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
            &target_currency,
            as_of_date,
            change_point.timestamp.to_rfc3339(),
            if trigger_descriptions.is_empty() {
                None
            } else {
                Some(trigger_descriptions)
            },
            previous_total_value,
            &mut carry_forward_unit_values,
        )
        .await?;
        history_points.push(history_point);
        previous_total_value = current_total_value;
    }

    let summary = calculate_history_summary(&history_points);

    Ok(HistoryOutput {
        currency: target_currency,
        start_date: start,
        end_date: end,
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
    let service = PortfolioService::new(storage_arc, market_data);
    let target_currency = currency.unwrap_or_else(|| config.reporting_currency.clone());

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
            &target_currency,
            as_of_date,
            timestamp,
            None,
            previous_total_value,
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
    let start_date = start
        .as_ref()
        .map(|s| {
            NaiveDate::parse_from_str(s, "%Y-%m-%d")
                .with_context(|| format!("Invalid start date: {s}"))
        })
        .transpose()?;
    let end_date = end
        .as_ref()
        .map(|s| {
            NaiveDate::parse_from_str(s, "%Y-%m-%d")
                .with_context(|| format!("Invalid end date: {s}"))
        })
        .transpose()?;

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
        start_date: start,
        end_date: end,
        granularity,
        include_prices,
        points: filtered,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::*;
    use crate::clock::{Clock, FixedClock};
    use crate::config::{
        DisplayConfig, GitConfig, HistoryConfig, RefreshConfig, ResolvedConfig, SpendingConfig,
        TrayConfig,
    };
    use crate::models::FixedIdGenerator;
    use crate::models::{Account, AssetBalance, BalanceSnapshot, Connection, ConnectionConfig};
    use crate::storage::JsonFileStorage;
    use crate::storage::MemoryStorage;
    use chrono::TimeZone;
    use chrono::{DateTime, NaiveDate, Utc};
    use std::collections::HashMap;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn connection_config(name: &str) -> ConnectionConfig {
        ConnectionConfig {
            name: name.to_string(),
            synchronizer: "mock".to_string(),
            credentials: None,
            balance_staleness: None,
        }
    }

    async fn write_connection_config(
        storage: &JsonFileStorage,
        conn: &Connection,
    ) -> anyhow::Result<()> {
        storage
            .save_connection_config(conn.id(), &conn.config)
            .await?;
        Ok(())
    }

    fn sample_price(asset: &Asset, date: NaiveDate, timestamp: DateTime<Utc>) -> PricePoint {
        PricePoint {
            asset_id: AssetId::from_asset(asset),
            as_of_date: date,
            timestamp,
            price: "1.00".to_string(),
            quote_currency: "USD".to_string(),
            kind: PriceKind::Close,
            source: "test".to_string(),
        }
    }

    fn sample_fx_rate(
        base: &str,
        quote: &str,
        date: NaiveDate,
        timestamp: DateTime<Utc>,
    ) -> FxRatePoint {
        FxRatePoint {
            base: base.to_string(),
            quote: quote.to_string(),
            as_of_date: date,
            timestamp,
            rate: "1.25".to_string(),
            kind: FxRateKind::Close,
            source: "test".to_string(),
        }
    }

    #[test]
    fn compute_percentage_change_from_previous_handles_expected_cases() {
        assert_eq!(
            compute_percentage_change_from_previous(None, Some(Decimal::ONE)),
            None
        );
        assert_eq!(
            compute_percentage_change_from_previous(Some(Decimal::ZERO), Some(Decimal::ONE)),
            Some("N/A".to_string())
        );
        assert_eq!(
            compute_percentage_change_from_previous(
                Some(Decimal::from_str("100").unwrap()),
                Some(Decimal::from_str("125.5").unwrap())
            ),
            Some("25.50".to_string())
        );
        assert_eq!(
            compute_percentage_change_from_previous(Some(Decimal::ONE), None),
            Some("N/A".to_string())
        );
    }

    #[test]
    fn history_spec_dates_expand_default_recent_history_layout() -> anyhow::Result<()> {
        let dates = history_spec_dates(
            NaiveDate::from_ymd_opt(2025, 4, 19).unwrap(),
            &[
                "last 4 days".to_string(),
                "1 week ago".to_string(),
                "2 weeks ago".to_string(),
                "last 12 months".to_string(),
            ],
        )?;
        assert_eq!(
            dates,
            vec![
                NaiveDate::from_ymd_opt(2024, 5, 19).unwrap(),
                NaiveDate::from_ymd_opt(2024, 6, 19).unwrap(),
                NaiveDate::from_ymd_opt(2024, 7, 19).unwrap(),
                NaiveDate::from_ymd_opt(2024, 8, 19).unwrap(),
                NaiveDate::from_ymd_opt(2024, 9, 19).unwrap(),
                NaiveDate::from_ymd_opt(2024, 10, 19).unwrap(),
                NaiveDate::from_ymd_opt(2024, 11, 19).unwrap(),
                NaiveDate::from_ymd_opt(2024, 12, 19).unwrap(),
                NaiveDate::from_ymd_opt(2025, 1, 19).unwrap(),
                NaiveDate::from_ymd_opt(2025, 2, 19).unwrap(),
                NaiveDate::from_ymd_opt(2025, 3, 19).unwrap(),
                NaiveDate::from_ymd_opt(2025, 4, 5).unwrap(),
                NaiveDate::from_ymd_opt(2025, 4, 12).unwrap(),
                NaiveDate::from_ymd_opt(2025, 4, 16).unwrap(),
                NaiveDate::from_ymd_opt(2025, 4, 17).unwrap(),
                NaiveDate::from_ymd_opt(2025, 4, 18).unwrap(),
                NaiveDate::from_ymd_opt(2025, 4, 19).unwrap(),
            ]
        );
        Ok(())
    }

    #[test]
    fn history_spec_dates_support_each_of_the_last_ranges() -> anyhow::Result<()> {
        let dates = history_spec_dates(
            NaiveDate::from_ymd_opt(2024, 2, 29).unwrap(),
            &["each of the last 3 months".to_string()],
        )?;
        assert_eq!(
            dates,
            vec![
                NaiveDate::from_ymd_opt(2023, 12, 29).unwrap(),
                NaiveDate::from_ymd_opt(2024, 1, 29).unwrap(),
                NaiveDate::from_ymd_opt(2024, 2, 29).unwrap(),
            ]
        );
        Ok(())
    }

    #[tokio::test]
    async fn portfolio_recent_history_uses_configured_history_spec() -> anyhow::Result<()> {
        let dir = TempDir::new()?;
        let mut tray = TrayConfig::default();
        tray.history_spec = vec![
            "last 4 days".to_string(),
            "1 week ago".to_string(),
            "2 weeks ago".to_string(),
            "last 12 months".to_string(),
        ];
        let config = ResolvedConfig {
            data_dir: dir.path().to_path_buf(),
            reporting_currency: "USD".to_string(),
            display: DisplayConfig::default(),
            refresh: RefreshConfig::default(),
            history: HistoryConfig::default(),
            tray,
            spending: SpendingConfig::default(),
            portfolio: crate::config::PortfolioConfig::default(),
            ignore: crate::config::IgnoreConfig::default(),
            git: GitConfig::default(),
        };

        let storage = Arc::new(MemoryStorage::new());
        let connection = Connection::new(connection_config("Cash"));
        storage.save_connection(&connection).await?;

        let account = Account::new("Checking", connection.id().clone());
        storage.save_account(&account).await?;

        storage
            .append_balance_snapshot(
                &account.id,
                &BalanceSnapshot::new(
                    Utc.with_ymd_and_hms(2024, 1, 1, 12, 0, 0).unwrap(),
                    vec![AssetBalance::new(Asset::currency("USD"), "100")],
                ),
            )
            .await?;
        storage
            .append_balance_snapshot(
                &account.id,
                &BalanceSnapshot::new(
                    Utc.with_ymd_and_hms(2025, 4, 15, 12, 0, 0).unwrap(),
                    vec![AssetBalance::new(Asset::currency("USD"), "200")],
                ),
            )
            .await?;

        let output = portfolio_recent_history(
            storage,
            &config,
            None,
            false,
            NaiveDate::from_ymd_opt(2025, 4, 19).unwrap(),
        )
        .await?;

        assert_eq!(
            output
                .iter()
                .map(|point| point.date.as_str())
                .collect::<Vec<_>>(),
            vec![
                "2024-05-19",
                "2024-06-19",
                "2024-07-19",
                "2024-08-19",
                "2024-09-19",
                "2024-10-19",
                "2024-11-19",
                "2024-12-19",
                "2025-01-19",
                "2025-02-19",
                "2025-03-19",
                "2025-04-05",
                "2025-04-12",
                "2025-04-16",
                "2025-04-17",
                "2025-04-18",
                "2025-04-19",
            ]
        );
        assert_eq!(output[12].total_value, "100");
        assert_eq!(output[13].total_value, "200");
        assert_eq!(
            output[13].percentage_change_from_previous.as_deref(),
            Some("100")
        );

        Ok(())
    }

    #[tokio::test]
    async fn portfolio_history_carries_forward_previous_valuation_when_price_missing(
    ) -> anyhow::Result<()> {
        let dir = TempDir::new()?;
        let config = ResolvedConfig {
            data_dir: dir.path().to_path_buf(),
            reporting_currency: "USD".to_string(),
            display: DisplayConfig::default(),
            refresh: RefreshConfig::default(),
            history: HistoryConfig::default(),
            tray: TrayConfig::default(),
            spending: SpendingConfig::default(),
            portfolio: crate::config::PortfolioConfig::default(),
            ignore: crate::config::IgnoreConfig::default(),
            git: GitConfig::default(),
        };

        let storage = Arc::new(MemoryStorage::new());
        let connection = Connection::new(connection_config("Test Broker"));
        storage.save_connection(&connection).await?;

        let account = Account::new("Trading", connection.id().clone());
        storage.save_account(&account).await?;

        let asset = Asset::equity("AAPL");
        storage
            .append_balance_snapshot(
                &account.id,
                &BalanceSnapshot::new(
                    Utc.with_ymd_and_hms(2024, 1, 1, 12, 0, 0).unwrap(),
                    vec![AssetBalance::new(asset.clone(), "10")],
                ),
            )
            .await?;
        storage
            .append_balance_snapshot(
                &account.id,
                &BalanceSnapshot::new(
                    Utc.with_ymd_and_hms(2024, 2, 1, 12, 0, 0).unwrap(),
                    vec![AssetBalance::new(asset.clone(), "10")],
                ),
            )
            .await?;

        // Only seed one early close price. By 2024-02-01 this is outside the default 7-day lookback.
        let store = JsonlMarketDataStore::new(&config.data_dir);
        store
            .put_prices(&[PricePoint {
                asset_id: AssetId::from_asset(&asset),
                as_of_date: NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
                timestamp: Utc.with_ymd_and_hms(2024, 1, 1, 23, 59, 59).unwrap(),
                price: "100".to_string(),
                quote_currency: "USD".to_string(),
                kind: PriceKind::Close,
                source: "test".to_string(),
            }])
            .await?;

        let output = portfolio_history(
            storage,
            &config,
            None,
            None,
            None,
            "none".to_string(),
            false,
        )
        .await?;

        assert_eq!(output.points.len(), 2);
        assert_eq!(output.points[0].total_value, "1000");
        assert_eq!(output.points[1].total_value, "1000");
        assert_eq!(
            output.points[1].percentage_change_from_previous.as_deref(),
            Some("0")
        );

        Ok(())
    }

    #[tokio::test]
    async fn portfolio_history_projects_future_prices_when_configured() -> anyhow::Result<()> {
        let dir = TempDir::new()?;
        let config = ResolvedConfig {
            data_dir: dir.path().to_path_buf(),
            reporting_currency: "USD".to_string(),
            display: DisplayConfig::default(),
            refresh: RefreshConfig::default(),
            history: HistoryConfig {
                allow_future_projection: true,
                lookback_days: Some(7),
            },
            tray: TrayConfig::default(),
            spending: SpendingConfig::default(),
            portfolio: crate::config::PortfolioConfig::default(),
            ignore: crate::config::IgnoreConfig::default(),
            git: GitConfig::default(),
        };

        let storage = Arc::new(MemoryStorage::new());
        let connection = Connection::new(connection_config("Test Broker"));
        storage.save_connection(&connection).await?;

        let account = Account::new("Trading", connection.id().clone());
        storage.save_account(&account).await?;

        let asset = Asset::equity("AAPL");
        storage
            .append_balance_snapshot(
                &account.id,
                &BalanceSnapshot::new(
                    Utc.with_ymd_and_hms(2024, 1, 15, 12, 0, 0).unwrap(),
                    vec![AssetBalance::new(asset.clone(), "10")],
                ),
            )
            .await?;

        let store = JsonlMarketDataStore::new(&config.data_dir);
        store
            .put_prices(&[
                PricePoint {
                    asset_id: AssetId::from_asset(&asset),
                    as_of_date: NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
                    timestamp: Utc.with_ymd_and_hms(2024, 1, 1, 23, 59, 59).unwrap(),
                    price: "100".to_string(),
                    quote_currency: "USD".to_string(),
                    kind: PriceKind::Close,
                    source: "test".to_string(),
                },
                PricePoint {
                    asset_id: AssetId::from_asset(&asset),
                    as_of_date: NaiveDate::from_ymd_opt(2024, 1, 20).unwrap(),
                    timestamp: Utc.with_ymd_and_hms(2024, 1, 20, 23, 59, 59).unwrap(),
                    price: "120".to_string(),
                    quote_currency: "USD".to_string(),
                    kind: PriceKind::Close,
                    source: "test".to_string(),
                },
            ])
            .await?;

        let output = portfolio_history(
            storage,
            &config,
            None,
            Some("2024-01-15".to_string()),
            Some("2024-01-15".to_string()),
            "none".to_string(),
            false,
        )
        .await?;

        assert_eq!(output.points.len(), 1);
        assert_eq!(output.points[0].total_value, "1200");

        Ok(())
    }

    #[tokio::test]
    async fn fill_prices_at_date_wraps_daily_history_fetch() -> anyhow::Result<()> {
        let dir = TempDir::new()?;
        let config = ResolvedConfig {
            data_dir: dir.path().to_path_buf(),
            reporting_currency: "USD".to_string(),
            display: DisplayConfig::default(),
            refresh: RefreshConfig::default(),
            history: HistoryConfig::default(),
            tray: TrayConfig::default(),
            spending: SpendingConfig::default(),
            portfolio: crate::config::PortfolioConfig::default(),
            ignore: crate::config::IgnoreConfig::default(),
            git: GitConfig::default(),
        };

        let storage = Arc::new(MemoryStorage::new());
        let connection = Connection::new(connection_config("Brokerage"));
        storage.save_connection(&connection).await?;

        let account = Account::new("Main", connection.id().clone());
        storage.save_account(&account).await?;

        let asset = Asset::equity("AAPL");
        storage
            .append_balance_snapshot(
                &account.id,
                &BalanceSnapshot::new(
                    Utc.with_ymd_and_hms(2024, 1, 15, 12, 0, 0).unwrap(),
                    vec![AssetBalance::new(asset.clone(), "10")],
                ),
            )
            .await?;

        let store = JsonlMarketDataStore::new(&config.data_dir);
        store
            .put_prices(&[PricePoint {
                asset_id: AssetId::from_asset(&asset),
                as_of_date: NaiveDate::from_ymd_opt(2024, 1, 15).unwrap(),
                timestamp: Utc.with_ymd_and_hms(2024, 1, 15, 23, 59, 59).unwrap(),
                price: "182.5".to_string(),
                quote_currency: "USD".to_string(),
                kind: PriceKind::Close,
                source: "test".to_string(),
            }])
            .await?;

        let output = fill_prices_at_date(PriceHistoryRequest {
            storage: storage.as_ref(),
            config: &config,
            account: None,
            connection: None,
            start: Some("2024-01-15"),
            end: Some("2024-01-15"),
            interval: "monthly",
            lookback_days: 7,
            request_delay_ms: 0,
            currency: None,
            include_fx: false,
        })
        .await?;

        assert_eq!(output.interval, "daily");
        assert_eq!(output.start_date, "2024-01-15");
        assert_eq!(output.end_date, "2024-01-15");
        assert_eq!(output.points, 1);
        assert_eq!(output.prices.existing, 1);

        Ok(())
    }

    #[tokio::test]
    async fn portfolio_history_prefers_same_day_quotes_over_older_closes() -> anyhow::Result<()> {
        let dir = TempDir::new()?;
        let config = ResolvedConfig {
            data_dir: dir.path().to_path_buf(),
            reporting_currency: "USD".to_string(),
            display: DisplayConfig::default(),
            refresh: RefreshConfig::default(),
            history: HistoryConfig::default(),
            tray: TrayConfig::default(),
            spending: SpendingConfig::default(),
            portfolio: crate::config::PortfolioConfig::default(),
            ignore: crate::config::IgnoreConfig::default(),
            git: GitConfig::default(),
        };

        let storage = Arc::new(MemoryStorage::new());
        let connection = Connection::new(connection_config("Test Broker"));
        storage.save_connection(&connection).await?;

        let account = Account::new("Trading", connection.id().clone());
        storage.save_account(&account).await?;

        let asset = Asset::equity("AAPL");
        storage
            .append_balance_snapshot(
                &account.id,
                &BalanceSnapshot::new(
                    Utc.with_ymd_and_hms(2024, 1, 1, 12, 0, 0).unwrap(),
                    vec![AssetBalance::new(asset.clone(), "10")],
                ),
            )
            .await?;

        let store = JsonlMarketDataStore::new(&config.data_dir);
        store
            .put_prices(&[
                PricePoint {
                    asset_id: AssetId::from_asset(&asset),
                    as_of_date: NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
                    timestamp: Utc.with_ymd_and_hms(2024, 1, 1, 23, 59, 59).unwrap(),
                    price: "100".to_string(),
                    quote_currency: "USD".to_string(),
                    kind: PriceKind::Close,
                    source: "test".to_string(),
                },
                PricePoint {
                    asset_id: AssetId::from_asset(&asset),
                    as_of_date: NaiveDate::from_ymd_opt(2024, 1, 2).unwrap(),
                    timestamp: Utc.with_ymd_and_hms(2024, 1, 2, 12, 0, 0).unwrap(),
                    price: "110".to_string(),
                    quote_currency: "USD".to_string(),
                    kind: PriceKind::Quote,
                    source: "test".to_string(),
                },
                PricePoint {
                    asset_id: AssetId::from_asset(&asset),
                    as_of_date: NaiveDate::from_ymd_opt(2024, 1, 3).unwrap(),
                    timestamp: Utc.with_ymd_and_hms(2024, 1, 3, 12, 0, 0).unwrap(),
                    price: "120".to_string(),
                    quote_currency: "USD".to_string(),
                    kind: PriceKind::Quote,
                    source: "test".to_string(),
                },
            ])
            .await?;

        let output = portfolio_history(
            storage,
            &config,
            None,
            Some("2024-01-02".to_string()),
            Some("2024-01-03".to_string()),
            "none".to_string(),
            true,
        )
        .await?;

        assert_eq!(output.points.len(), 2);
        assert_eq!(output.points[0].date, "2024-01-02");
        assert_eq!(output.points[0].timestamp, "2024-01-02T12:00:00+00:00");
        assert_eq!(output.points[0].total_value, "1100");
        assert_eq!(output.points[1].date, "2024-01-03");
        assert_eq!(output.points[1].timestamp, "2024-01-03T12:00:00+00:00");
        assert_eq!(output.points[1].total_value, "1200");
        assert_eq!(
            output.points[1].percentage_change_from_previous.as_deref(),
            Some("9.09")
        );

        Ok(())
    }

    #[tokio::test]
    async fn portfolio_history_can_jump_when_missing_asset_prices_arrive_late() -> anyhow::Result<()>
    {
        let dir = TempDir::new()?;
        let config = ResolvedConfig {
            data_dir: dir.path().to_path_buf(),
            reporting_currency: "USD".to_string(),
            display: DisplayConfig::default(),
            refresh: RefreshConfig::default(),
            history: HistoryConfig::default(),
            tray: TrayConfig::default(),
            spending: SpendingConfig::default(),
            portfolio: crate::config::PortfolioConfig::default(),
            ignore: crate::config::IgnoreConfig::default(),
            git: GitConfig::default(),
        };

        let storage = Arc::new(MemoryStorage::new());
        let connection = Connection::new(connection_config("Test Broker"));
        storage.save_connection(&connection).await?;

        let account = Account::new("Trading", connection.id().clone());
        storage.save_account(&account).await?;

        // Hold several crypto assets from the start of the year.
        storage
            .append_balance_snapshot(
                &account.id,
                &BalanceSnapshot::new(
                    Utc.with_ymd_and_hms(2024, 1, 1, 12, 0, 0).unwrap(),
                    vec![
                        AssetBalance::new(Asset::crypto("BTC"), "10"),
                        AssetBalance::new(Asset::crypto("ETH"), "100"),
                        AssetBalance::new(Asset::crypto("ICP"), "1000"),
                        AssetBalance::new(Asset::crypto("POL"), "1000"),
                    ],
                ),
            )
            .await?;

        let store = JsonlMarketDataStore::new(&config.data_dir);
        let close = |asset: Asset, date: (i32, u32, u32), price: &str| PricePoint {
            asset_id: AssetId::from_asset(&asset),
            as_of_date: NaiveDate::from_ymd_opt(date.0, date.1, date.2).unwrap(),
            timestamp: Utc
                .with_ymd_and_hms(date.0, date.1, date.2, 23, 59, 59)
                .unwrap(),
            price: price.to_string(),
            quote_currency: "USD".to_string(),
            kind: PriceKind::Close,
            source: "test".to_string(),
        };

        // Only ICP has prices for Sep/Oct/Nov. Other assets are priced only in late Dec.
        store
            .put_prices(&[
                close(Asset::crypto("ICP"), (2024, 9, 22), "1"),
                close(Asset::crypto("ICP"), (2024, 10, 27), "1.1"),
                close(Asset::crypto("ICP"), (2024, 11, 24), "1.2"),
                close(Asset::crypto("ICP"), (2024, 12, 31), "1.3"),
                close(Asset::crypto("BTC"), (2024, 12, 31), "50000"),
                close(Asset::crypto("ETH"), (2024, 12, 31), "3000"),
                close(Asset::crypto("POL"), (2024, 12, 31), "2"),
            ])
            .await?;

        let output = portfolio_history(
            storage.clone(),
            &config,
            None,
            Some("2024-09-01".to_string()),
            Some("2025-01-10".to_string()),
            "monthly".to_string(),
            true,
        )
        .await?;

        assert_eq!(output.points.len(), 4);
        assert_eq!(output.points[0].total_value, "1000");
        let last_total = Decimal::from_str(&output.points[3].total_value)?;
        assert!(last_total > Decimal::from_str("800000")?);

        let last_change = Decimal::from_str(
            output.points[3]
                .percentage_change_from_previous
                .as_deref()
                .expect("last point should have previous change"),
        )?;
        assert!(
            last_change > Decimal::from_str("10000")?,
            "expected very large percentage change when late prices arrive"
        );

        // Without price triggers, there are no balance changes in this window.
        let no_price_output = portfolio_history(
            storage,
            &config,
            None,
            Some("2024-09-01".to_string()),
            Some("2025-01-10".to_string()),
            "monthly".to_string(),
            false,
        )
        .await?;
        assert!(no_price_output.points.is_empty());

        Ok(())
    }

    #[tokio::test]
    async fn portfolio_change_points_includes_prices_when_enabled() -> anyhow::Result<()> {
        let dir = TempDir::new()?;
        let config = ResolvedConfig {
            data_dir: dir.path().to_path_buf(),
            reporting_currency: "USD".to_string(),
            display: DisplayConfig::default(),
            refresh: RefreshConfig::default(),
            history: HistoryConfig::default(),
            tray: TrayConfig::default(),
            spending: SpendingConfig::default(),
            portfolio: crate::config::PortfolioConfig::default(),
            ignore: crate::config::IgnoreConfig::default(),
            git: GitConfig::default(),
        };

        let storage = Arc::new(MemoryStorage::new());

        // Minimal account + balance snapshot so the collector considers this asset "held".
        let account = Account::new_with(
            Id::from_string("acct-1"),
            Utc.with_ymd_and_hms(2026, 2, 1, 0, 0, 0).unwrap(),
            "Test Account",
            Id::from_string("conn-1"),
        );
        storage.save_account(&account).await?;

        let aapl = Asset::equity("AAPL");
        storage
            .append_balance_snapshot(
                &account.id,
                &BalanceSnapshot::new(
                    Utc.with_ymd_and_hms(2026, 2, 5, 7, 49, 58).unwrap(),
                    vec![AssetBalance::new(aapl.clone(), "10")],
                ),
            )
            .await?;

        // Seed a cached price for the held asset on the next day.
        let store = JsonlMarketDataStore::new(&config.data_dir);
        store
            .put_prices(&[sample_price(
                &aapl,
                NaiveDate::from_ymd_opt(2026, 2, 6).unwrap(),
                Utc.with_ymd_and_hms(2026, 2, 6, 12, 0, 0).unwrap(),
            )])
            .await?;

        let output =
            portfolio_change_points(storage, &config, None, None, "none".to_string(), true).await?;

        assert!(
            output.points.iter().any(|p| p
                .triggers
                .iter()
                .any(|t| matches!(t, crate::portfolio::ChangeTrigger::Balance { .. }))),
            "expected at least one balance-triggered change point"
        );
        assert!(
            output.points.iter().any(|p| p
                .triggers
                .iter()
                .any(|t| matches!(t, crate::portfolio::ChangeTrigger::Price { .. }))),
            "expected at least one price-triggered change point"
        );

        Ok(())
    }

    #[test]
    fn parse_asset_handles_prefixes() -> anyhow::Result<()> {
        let equity = parse_asset("Equity:AAPL")?;
        match equity {
            Asset::Equity { ticker, .. } => assert_eq!(ticker, "AAPL"),
            _ => anyhow::bail!("expected equity asset"),
        }

        let crypto = parse_asset("CRYPTO:BTC")?;
        match crypto {
            Asset::Crypto { symbol, .. } => assert_eq!(symbol, "BTC"),
            _ => anyhow::bail!("expected crypto asset"),
        }

        let currency = parse_asset(" currency:usd ")?;
        match currency {
            Asset::Currency { iso_code } => assert_eq!(iso_code, "usd"),
            _ => anyhow::bail!("expected currency asset"),
        }

        Ok(())
    }

    #[test]
    fn parse_asset_rejects_empty_values() {
        assert!(parse_asset("").is_err());
        assert!(parse_asset("   ").is_err());
        assert!(parse_asset("equity:").is_err());
        assert!(parse_asset("crypto:   ").is_err());
        assert!(parse_asset("currency:").is_err());
    }

    #[test]
    fn align_start_date_monthly_uses_month_end() {
        let date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();
        let aligned = align_start_date(date, PriceHistoryInterval::Monthly);
        assert_eq!(aligned, NaiveDate::from_ymd_opt(2024, 1, 31).unwrap());
    }

    #[test]
    fn align_start_date_yearly_uses_year_end() {
        let date = NaiveDate::from_ymd_opt(2024, 1, 14).unwrap();
        let aligned = align_start_date(date, PriceHistoryInterval::Yearly);
        assert_eq!(aligned, NaiveDate::from_ymd_opt(2024, 12, 31).unwrap());
    }

    #[test]
    fn advance_interval_date_yearly_uses_next_year_end() {
        let date = NaiveDate::from_ymd_opt(2024, 12, 31).unwrap();
        let next = advance_interval_date(date, PriceHistoryInterval::Yearly);
        assert_eq!(next, NaiveDate::from_ymd_opt(2025, 12, 31).unwrap());
    }

    #[test]
    fn resolve_cached_price_prefers_exact_then_lookback() {
        let asset = Asset::equity("AAPL");
        let date = NaiveDate::from_ymd_opt(2024, 1, 10).unwrap();
        let exact = sample_price(&asset, date, Utc::now());
        let mut cache = HashMap::new();
        cache.insert(date, exact.clone());

        let (found, exact_hit) = resolve_cached_price(&cache, date, 3).expect("exact price");
        assert!(exact_hit);
        assert_eq!(found.as_of_date, date);

        cache.remove(&date);
        let lookback_date = date - chrono::Duration::days(1);
        let lookback = sample_price(&asset, lookback_date, Utc::now());
        cache.insert(lookback_date, lookback.clone());

        let (found, exact_hit) = resolve_cached_price(&cache, date, 3).expect("lookback price");
        assert!(!exact_hit);
        assert_eq!(found.as_of_date, lookback_date);
    }

    #[test]
    fn upsert_price_cache_prefers_newer_timestamp() {
        let asset = Asset::equity("AAPL");
        let date = NaiveDate::from_ymd_opt(2024, 1, 5).unwrap();
        let newer = sample_price(&asset, date, Utc::now());
        let older = sample_price(&asset, date, Utc::now() - chrono::Duration::minutes(5));

        let mut cache = HashMap::new();
        cache.insert(date, newer.clone());

        assert!(!upsert_price_cache(&mut cache, older));
        assert_eq!(cache.get(&date).unwrap().timestamp, newer.timestamp);

        let newest = sample_price(&asset, date, Utc::now() + chrono::Duration::minutes(1));
        assert!(upsert_price_cache(&mut cache, newest.clone()));
        assert_eq!(cache.get(&date).unwrap().timestamp, newest.timestamp);
    }

    #[test]
    fn resolve_cached_fx_prefers_exact_then_lookback() {
        let date = NaiveDate::from_ymd_opt(2024, 1, 10).unwrap();
        let exact = sample_fx_rate("EUR", "USD", date, Utc::now());
        let mut cache = HashMap::new();
        cache.insert(date, exact.clone());

        let (found, exact_hit) = resolve_cached_fx(&cache, date, 3).expect("exact rate");
        assert!(exact_hit);
        assert_eq!(found.as_of_date, date);

        cache.remove(&date);
        let lookback_date = date - chrono::Duration::days(2);
        let lookback = sample_fx_rate("EUR", "USD", lookback_date, Utc::now());
        cache.insert(lookback_date, lookback.clone());

        let (found, exact_hit) = resolve_cached_fx(&cache, date, 3).expect("lookback rate");
        assert!(!exact_hit);
        assert_eq!(found.as_of_date, lookback_date);
    }

    #[tokio::test]
    async fn add_connection_rejects_duplicate_names() -> anyhow::Result<()> {
        let dir = TempDir::new()?;
        let storage = JsonFileStorage::new(dir.path());
        let config = ResolvedConfig {
            data_dir: dir.path().to_path_buf(),
            reporting_currency: "USD".to_string(),
            display: DisplayConfig::default(),
            refresh: RefreshConfig::default(),
            history: HistoryConfig::default(),
            tray: TrayConfig::default(),
            spending: SpendingConfig::default(),
            portfolio: crate::config::PortfolioConfig::default(),
            ignore: crate::config::IgnoreConfig::default(),
            git: GitConfig::default(),
        };

        add_connection(&storage, &config, "Duplicate", "manual").await?;

        let err = add_connection(&storage, &config, "duplicate", "manual")
            .await
            .expect_err("expected duplicate connection name error");
        assert!(err.to_string().contains("Connection name already exists"));

        Ok(())
    }

    #[tokio::test]
    async fn add_connection_and_account_use_injected_ids_and_clock() -> anyhow::Result<()> {
        let dir = TempDir::new()?;
        let storage = JsonFileStorage::new(dir.path());
        let config = ResolvedConfig {
            data_dir: dir.path().to_path_buf(),
            reporting_currency: "USD".to_string(),
            display: DisplayConfig::default(),
            refresh: RefreshConfig::default(),
            history: HistoryConfig::default(),
            tray: TrayConfig::default(),
            spending: SpendingConfig::default(),
            portfolio: crate::config::PortfolioConfig::default(),
            ignore: crate::config::IgnoreConfig::default(),
            git: GitConfig::default(),
        };

        let ids = FixedIdGenerator::new([Id::from_string("conn-id"), Id::from_string("acct-id")]);
        let clock = FixedClock::new(Utc.with_ymd_and_hms(2026, 2, 5, 12, 0, 0).unwrap());

        let out = add_connection_with(&storage, &config, "Test", "manual", &ids, &clock).await?;
        assert_eq!(out["connection"]["id"].as_str(), Some("conn-id"));

        let loaded = storage
            .get_connection(&Id::from_string("conn-id"))
            .await?
            .expect("connection should exist");
        assert_eq!(loaded.state.created_at, clock.now());

        let out = add_account_with(
            &storage,
            &config,
            "conn-id",
            "Checking",
            vec!["tag".to_string()],
            &ids,
            &clock,
        )
        .await?;
        assert_eq!(out["account"]["id"].as_str(), Some("acct-id"));

        let acct = storage
            .get_account(&Id::from_string("acct-id"))
            .await?
            .expect("account should exist");
        assert_eq!(acct.created_at, clock.now());
        assert_eq!(acct.tags, vec!["tag".to_string()]);

        Ok(())
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn add_connection_creates_by_name_symlink() -> anyhow::Result<()> {
        let dir = TempDir::new()?;
        let storage = JsonFileStorage::new(dir.path());
        let config = ResolvedConfig {
            data_dir: dir.path().to_path_buf(),
            reporting_currency: "USD".to_string(),
            display: DisplayConfig::default(),
            refresh: RefreshConfig::default(),
            history: HistoryConfig::default(),
            tray: TrayConfig::default(),
            spending: SpendingConfig::default(),
            portfolio: crate::config::PortfolioConfig::default(),
            ignore: crate::config::IgnoreConfig::default(),
            git: GitConfig::default(),
        };

        let result = add_connection(&storage, &config, "Test Bank", "manual").await?;
        let id = result["connection"]["id"]
            .as_str()
            .expect("connection id missing");

        let link_path = dir
            .path()
            .join("connections")
            .join("by-name")
            .join("Test Bank");
        let metadata = std::fs::symlink_metadata(&link_path)?;
        assert!(metadata.file_type().is_symlink());

        let target = std::fs::read_link(&link_path)?;
        assert_eq!(target, PathBuf::from("..").join(id));

        Ok(())
    }

    #[tokio::test]
    async fn set_balance_rejects_invalid_amount() -> anyhow::Result<()> {
        let dir = TempDir::new()?;
        let storage = JsonFileStorage::new(dir.path());
        let config = ResolvedConfig {
            data_dir: dir.path().to_path_buf(),
            reporting_currency: "USD".to_string(),
            display: DisplayConfig::default(),
            refresh: RefreshConfig::default(),
            history: HistoryConfig::default(),
            tray: TrayConfig::default(),
            spending: SpendingConfig::default(),
            portfolio: crate::config::PortfolioConfig::default(),
            ignore: crate::config::IgnoreConfig::default(),
            git: GitConfig::default(),
        };

        let account = Account::new("Checking", Id::new());
        storage.save_account(&account).await?;

        let err = set_balance(
            &storage,
            &config,
            account.id.as_str(),
            "USD",
            "not-a-number",
            None,
        )
        .await
        .expect_err("expected invalid amount error");
        assert!(err.to_string().contains("Invalid amount"));

        let snapshots = storage.get_balance_snapshots(&account.id).await?;
        assert!(snapshots.is_empty());

        Ok(())
    }

    #[tokio::test]
    async fn set_account_config_updates_balance_backfill() -> anyhow::Result<()> {
        let dir = TempDir::new()?;
        let storage = JsonFileStorage::new(dir.path());
        let config = ResolvedConfig {
            data_dir: dir.path().to_path_buf(),
            reporting_currency: "USD".to_string(),
            display: DisplayConfig::default(),
            refresh: RefreshConfig::default(),
            history: HistoryConfig::default(),
            tray: TrayConfig::default(),
            spending: SpendingConfig::default(),
            portfolio: crate::config::PortfolioConfig::default(),
            ignore: crate::config::IgnoreConfig::default(),
            git: GitConfig::default(),
        };

        let account = Account::new("Checking", Id::new());
        storage.save_account(&account).await?;

        let out = set_account_config(
            &storage,
            &config,
            account.id.as_str(),
            Some("carry_earliest"),
            false,
        )
        .await?;
        assert_eq!(out["success"], serde_json::Value::Bool(true));
        assert_eq!(
            out["config"]["balance_backfill"],
            serde_json::Value::String("carry_earliest".to_string())
        );

        let stored = storage
            .get_account_config(&account.id)?
            .expect("account config should exist");
        assert_eq!(
            stored.balance_backfill,
            Some(crate::models::BalanceBackfillPolicy::CarryEarliest)
        );

        Ok(())
    }

    #[tokio::test]
    async fn set_account_config_clears_balance_backfill() -> anyhow::Result<()> {
        let dir = TempDir::new()?;
        let storage = JsonFileStorage::new(dir.path());
        let config = ResolvedConfig {
            data_dir: dir.path().to_path_buf(),
            reporting_currency: "USD".to_string(),
            display: DisplayConfig::default(),
            refresh: RefreshConfig::default(),
            history: HistoryConfig::default(),
            tray: TrayConfig::default(),
            spending: SpendingConfig::default(),
            portfolio: crate::config::PortfolioConfig::default(),
            ignore: crate::config::IgnoreConfig::default(),
            git: GitConfig::default(),
        };

        let account = Account::new("Checking", Id::new());
        storage.save_account(&account).await?;
        storage
            .save_account_config(
                &account.id,
                &crate::models::AccountConfig {
                    balance_backfill: Some(crate::models::BalanceBackfillPolicy::Zero),
                    ..Default::default()
                },
            )
            .await?;

        let out = set_account_config(&storage, &config, "Checking", None, true).await?;
        assert_eq!(out["success"], serde_json::Value::Bool(true));
        assert_eq!(out["config"]["balance_backfill"], serde_json::Value::Null);

        let stored = storage
            .get_account_config(&account.id)?
            .expect("account config should exist");
        assert_eq!(stored.balance_backfill, None);

        Ok(())
    }

    #[tokio::test]
    async fn resolve_scope_rejects_account_and_connection() -> anyhow::Result<()> {
        let dir = TempDir::new()?;
        let storage = JsonFileStorage::new(dir.path());

        let err = resolve_price_history_scope(&storage, Some("a"), Some("b"))
            .await
            .err()
            .expect("expected invalid scope error");
        assert!(err.to_string().contains("Specify only one"));

        Ok(())
    }

    #[tokio::test]
    async fn resolve_scope_connection_requires_accounts() -> anyhow::Result<()> {
        let dir = TempDir::new()?;
        let storage = JsonFileStorage::new(dir.path());
        let mut conn = Connection::new(connection_config("Test Connection"));

        let missing_account = Account::new("Missing", conn.id().clone());
        conn.state.account_ids = vec![missing_account.id.clone()];

        write_connection_config(&storage, &conn).await?;
        storage.save_connection(&conn).await?;

        let conn_id = conn.id().to_string();
        let err = resolve_price_history_scope(&storage, None, Some(conn_id.as_str()))
            .await
            .err()
            .expect("expected missing accounts error");
        assert!(err.to_string().contains("No accounts found for connection"));

        Ok(())
    }

    #[tokio::test]
    async fn resolve_scope_connection_uses_accounts_by_connection_id() -> anyhow::Result<()> {
        let dir = TempDir::new()?;
        let storage = JsonFileStorage::new(dir.path());
        let conn = Connection::new(connection_config("Test Connection"));

        write_connection_config(&storage, &conn).await?;
        storage.save_connection(&conn).await?;

        let account = Account::new("Checking", conn.id().clone());
        storage.save_account(&account).await?;

        let conn_id = conn.id().to_string();
        let (scope, accounts) =
            resolve_price_history_scope(&storage, None, Some(conn_id.as_str())).await?;
        assert_eq!(accounts.len(), 1);
        assert_eq!(accounts[0].id, account.id);
        match scope {
            PriceHistoryScopeOutput::Connection { id, .. } => {
                assert_eq!(id, conn_id);
            }
            _ => anyhow::bail!("expected connection scope"),
        }

        Ok(())
    }

    #[tokio::test]
    async fn resolve_scope_connection_falls_back_when_state_ids_missing() -> anyhow::Result<()> {
        let dir = TempDir::new()?;
        let storage = JsonFileStorage::new(dir.path());
        let mut conn = Connection::new(connection_config("Test Connection"));

        conn.state.account_ids = vec![Id::from_string("missing-account")];

        write_connection_config(&storage, &conn).await?;
        storage.save_connection(&conn).await?;

        let account = Account::new("Checking", conn.id().clone());
        storage.save_account(&account).await?;

        let conn_id = conn.id().to_string();
        let (scope, accounts) =
            resolve_price_history_scope(&storage, None, Some(conn_id.as_str())).await?;
        assert_eq!(accounts.len(), 1);
        assert_eq!(accounts[0].id, account.id);
        match scope {
            PriceHistoryScopeOutput::Connection { id, .. } => {
                assert_eq!(id, conn_id);
            }
            _ => anyhow::bail!("expected connection scope"),
        }

        Ok(())
    }

    #[tokio::test]
    async fn resolve_scope_connection_includes_accounts_missing_from_state() -> anyhow::Result<()> {
        let dir = TempDir::new()?;
        let storage = JsonFileStorage::new(dir.path());
        let mut conn = Connection::new(connection_config("Test Connection"));

        let account_a = Account::new("Checking", conn.id().clone());
        conn.state.account_ids = vec![account_a.id.clone()];

        write_connection_config(&storage, &conn).await?;
        storage.save_connection(&conn).await?;

        let account_b = Account::new("Savings", conn.id().clone());
        storage.save_account(&account_a).await?;
        storage.save_account(&account_b).await?;

        let conn_id = conn.id().to_string();
        let (_, accounts) =
            resolve_price_history_scope(&storage, None, Some(conn_id.as_str())).await?;
        assert_eq!(accounts.len(), 2);

        Ok(())
    }
}
