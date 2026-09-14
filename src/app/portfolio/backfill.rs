//! Backfilling the market data a portfolio needs: walking a date range per
//! asset, fetching the missing closes and FX rates, and caching what the
//! store already holds.

use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use anyhow::{Context, Result};
use chrono::{Duration, NaiveDate, Utc};
use tracing::warn;

use crate::app::{
    maybe_auto_commit, AssetInfoOutput, PriceHistoryFailure, PriceHistoryOutput,
    PriceHistoryScopeOutput, PriceHistoryStats,
};
use crate::config::ResolvedConfig;
use crate::market_data::{
    AssetId, FxRateKind, FxRatePoint, JsonlMarketDataStore, MarketDataService,
    MarketDataServiceBuilder, MarketDataStore, PricePoint,
};
use crate::models::{Account, Asset, Id};
use crate::storage::{find_account, find_connection, Storage};

use super::intervals::{advance_interval_date, align_start_date, PriceHistoryInterval};

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

pub(super) async fn resolve_price_history_scope(
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

pub(super) fn resolve_cached_price(
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

pub(super) fn resolve_cached_fx(
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

pub(super) fn upsert_price_cache(
    cache: &mut HashMap<NaiveDate, PricePoint>,
    price: PricePoint,
) -> bool {
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

    let cache = match ctx.fx_cache.entry(key.clone()) {
        Entry::Occupied(entry) => entry.into_mut(),
        Entry::Vacant(entry) => {
            entry.insert(load_fx_cache(ctx.store, &base_upper, &quote_upper).await?)
        }
    };

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
