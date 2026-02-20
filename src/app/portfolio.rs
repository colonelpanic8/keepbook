use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use anyhow::{Context, Result};
use chrono::{Datelike, Duration, NaiveDate, Utc};
use tracing::warn;

use crate::config::ResolvedConfig;
use crate::market_data::{
    AssetId, FxRateKind, FxRatePoint, JsonlMarketDataStore, MarketDataService,
    MarketDataServiceBuilder, MarketDataStore, PriceKind, PricePoint,
};
use crate::models::{Account, Asset, Id};
use crate::portfolio::{
    collect_change_points, filter_by_date_range, filter_by_granularity, CoalesceStrategy,
    CollectOptions, Granularity, Grouping, PortfolioQuery, PortfolioService,
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

struct AssetPriceCache {
    asset: Asset,
    asset_id: AssetId,
    prices: HashMap<NaiveDate, PricePoint>,
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
    let anchor_day = start_date.day();
    let anchor_month = start_date.month();
    let aligned_start = align_start_date(start_date, interval, anchor_month, anchor_day);

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
        });
    }

    asset_caches.sort_by(|a, b| a.asset_id.to_string().cmp(&b.asset_id.to_string()));

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
    let mut failures = Vec::new();
    let mut failure_count = 0usize;
    let failure_limit = 50usize;
    let request_delay = if request_delay_ms > 0 {
        Some(std::time::Duration::from_millis(request_delay_ms))
    } else {
        None
    };

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
                                price_stats.existing += 1;
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

                        match market_data.price_close(&asset_cache.asset, current).await {
                            Ok(price) => {
                                let exact = price.as_of_date == current;
                                if exact {
                                    price_stats.fetched += 1;
                                } else {
                                    price_stats.lookback += 1;
                                }

                                upsert_price_cache(&mut asset_cache.prices, price.clone());
                                should_delay = request_delay.is_some();

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
                            }
                            Err(e) => {
                                price_stats.missing += 1;
                                *fx_ctx.failure_count += 1;
                                if fx_ctx.failures.len() < fx_ctx.failure_limit {
                                    fx_ctx.failures.push(PriceHistoryFailure {
                                        kind: "price".to_string(),
                                        date: current.to_string(),
                                        error: e.to_string(),
                                        asset_id: Some(asset_cache.asset_id.to_string()),
                                        asset: Some(asset_cache.asset.clone()),
                                        base: None,
                                        quote: None,
                                    });
                                }
                                should_delay = request_delay.is_some();
                            }
                        }
                    }
                }

                if should_delay {
                    if let Some(delay) = request_delay {
                        tokio::time::sleep(delay).await;
                    }
                }
            }

            current = advance_interval_date(current, interval, anchor_day, anchor_month);
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

fn advance_interval_date(
    date: NaiveDate,
    interval: PriceHistoryInterval,
    anchor_day: u32,
    anchor_month: u32,
) -> NaiveDate {
    match interval {
        PriceHistoryInterval::Daily => date + Duration::days(1),
        PriceHistoryInterval::Weekly => date + Duration::days(7),
        PriceHistoryInterval::Monthly => next_month_end(date),
        PriceHistoryInterval::Yearly => add_years(date, 1, anchor_month, anchor_day),
    }
}

fn align_start_date(
    date: NaiveDate,
    interval: PriceHistoryInterval,
    anchor_month: u32,
    anchor_day: u32,
) -> NaiveDate {
    match interval {
        PriceHistoryInterval::Monthly => month_end(date),
        PriceHistoryInterval::Yearly => {
            let day = anchor_day.min(days_in_month(date.year(), anchor_month));
            NaiveDate::from_ymd_opt(date.year(), anchor_month, day).expect("valid yearly date")
        }
        _ => date,
    }
}

fn add_years(date: NaiveDate, years: i32, anchor_month: u32, anchor_day: u32) -> NaiveDate {
    let year = date.year() + years;
    let day = anchor_day.min(days_in_month(year, anchor_month));
    NaiveDate::from_ymd_opt(year, anchor_month, day).expect("valid yearly date")
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

pub async fn portfolio_snapshot(
    storage: Arc<dyn Storage>,
    config: &ResolvedConfig,
    currency: Option<String>,
    date: Option<String>,
    group_by: String,
    detail: bool,
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
    let snapshot = service.calculate(&query).await?;

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
    use rust_decimal::Decimal;
    use std::str::FromStr;

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
    let market_data = Arc::new(
        MarketDataServiceBuilder::new(store, config.data_dir.clone())
            .with_quote_staleness(config.refresh.price_staleness)
            .offline_only()
            .build()
            .await,
    );

    // Create portfolio service
    let service = PortfolioService::new(storage_arc, market_data);

    // Calculate portfolio value at each change point
    let target_currency = currency
        .clone()
        .unwrap_or_else(|| config.reporting_currency.clone());
    let mut history_points = Vec::with_capacity(filtered.len());

    for change_point in &filtered {
        let as_of_date = change_point.timestamp.date_naive();
        let query = PortfolioQuery {
            as_of_date,
            currency: target_currency.clone(),
            currency_decimals: config.display.currency_decimals,
            grouping: Grouping::Asset,
            include_detail: false,
        };

        let snapshot = service.calculate(&query).await?;

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

        history_points.push(HistoryPoint {
            timestamp: change_point.timestamp.to_rfc3339(),
            date: as_of_date.to_string(),
            total_value: snapshot.total_value,
            change_triggers: if trigger_descriptions.is_empty() {
                None
            } else {
                Some(trigger_descriptions)
            },
        });
    }

    // Calculate summary if we have points
    let summary = if history_points.len() >= 2 {
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
    } else {
        None
    };

    Ok(HistoryOutput {
        currency: target_currency,
        start_date: start,
        end_date: end,
        granularity,
        points: history_points,
        summary,
    })
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
        DisplayConfig, GitConfig, RefreshConfig, ResolvedConfig, SpendingConfig, TrayConfig,
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

    #[tokio::test]
    async fn portfolio_change_points_includes_prices_when_enabled() -> anyhow::Result<()> {
        let dir = TempDir::new()?;
        let config = ResolvedConfig {
            data_dir: dir.path().to_path_buf(),
            reporting_currency: "USD".to_string(),
            display: DisplayConfig::default(),
            refresh: RefreshConfig::default(),
            tray: TrayConfig::default(),
            spending: SpendingConfig::default(),
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
        let aligned = align_start_date(date, PriceHistoryInterval::Monthly, 1, 15);
        assert_eq!(aligned, NaiveDate::from_ymd_opt(2024, 1, 31).unwrap());
    }

    #[test]
    fn add_years_handles_leap_day() {
        let date = NaiveDate::from_ymd_opt(2024, 2, 29).unwrap();
        let next = add_years(date, 1, 2, 29);
        assert_eq!(next, NaiveDate::from_ymd_opt(2025, 2, 28).unwrap());
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
            tray: TrayConfig::default(),
            spending: SpendingConfig::default(),
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
            tray: TrayConfig::default(),
            spending: SpendingConfig::default(),
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
            tray: TrayConfig::default(),
            spending: SpendingConfig::default(),
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
            tray: TrayConfig::default(),
            spending: SpendingConfig::default(),
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
        )
        .await
        .expect_err("expected invalid amount error");
        assert!(err.to_string().contains("Invalid amount"));

        let snapshots = storage.get_balance_snapshots(&account.id).await?;
        assert!(snapshots.is_empty());

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
