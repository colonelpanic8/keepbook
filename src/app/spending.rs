use std::collections::{HashMap, HashSet};
use std::str::FromStr;
use std::sync::Arc;

use anyhow::{Context, Result};
use chrono::{Datelike, NaiveDate, Utc};
use chrono_tz::Tz;
use rust_decimal::Decimal;

use crate::config::ResolvedConfig;
use crate::market_data::{MarketDataServiceBuilder, MarketDataStore};
use crate::models::{Account, Asset, Id, TransactionAnnotation, TransactionStatus};
use crate::storage::{find_account, find_connection, Storage};

use super::types::{
    SpendingBreakdownEntryOutput, SpendingOutput, SpendingPeriodOutput, SpendingScopeOutput,
};
use super::value::{value_in_reporting_currency_detailed, MissingMarketData};

#[derive(Debug, Clone)]
pub struct SpendingReportOptions {
    pub currency: Option<String>,
    pub start: Option<String>,
    pub end: Option<String>,
    pub period: String,
    pub tz: Option<String>,
    pub week_start: Option<String>,
    pub bucket: Option<std::time::Duration>,
    pub account: Option<String>,
    pub connection: Option<String>,
    pub status: String,
    pub direction: String,
    pub group_by: String,
    pub top: Option<usize>,
    pub lookback_days: u32,
    pub include_noncurrency: bool,
    pub include_empty: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Period {
    Daily,
    Weekly,
    Monthly,
    Quarterly,
    Yearly,
    Range,
    CustomDays(u32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Direction {
    Outflow,
    Inflow,
    Net,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StatusFilter {
    Posted,
    PostedPending,
    All,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GroupBy {
    None,
    Category,
    Merchant,
    Account,
    Tag,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WeekStart {
    Sunday,
    Monday,
}

#[derive(Debug, Clone)]
enum TzSpec {
    Local,
    Named(Tz),
}

impl TzSpec {
    fn parse(s: Option<&str>) -> Result<(Self, String)> {
        let Some(s) = s else {
            return Ok((TzSpec::Local, "local".to_string()));
        };
        let trimmed = s.trim();
        if trimmed.is_empty()
            || trimmed.eq_ignore_ascii_case("local")
            || trimmed.eq_ignore_ascii_case("current")
        {
            return Ok((TzSpec::Local, "local".to_string()));
        }
        if trimmed.eq_ignore_ascii_case("utc") {
            return Ok((TzSpec::Named(chrono_tz::UTC), "UTC".to_string()));
        }
        let tz: Tz = trimmed.parse().with_context(|| {
            format!("Invalid timezone '{trimmed}' (expected IANA name, e.g. America/New_York)")
        })?;
        Ok((TzSpec::Named(tz), trimmed.to_string()))
    }

    fn date_in_tz(&self, ts: chrono::DateTime<Utc>) -> NaiveDate {
        match self {
            TzSpec::Local => ts.with_timezone(&chrono::Local).date_naive(),
            TzSpec::Named(tz) => ts.with_timezone(tz).date_naive(),
        }
    }

    fn today(&self) -> NaiveDate {
        match self {
            TzSpec::Local => chrono::Local::now().date_naive(),
            TzSpec::Named(tz) => Utc::now().with_timezone(tz).date_naive(),
        }
    }
}

fn format_ymd(date: NaiveDate) -> String {
    date.format("%Y-%m-%d").to_string()
}

fn parse_date_opt(label: &str, s: &Option<String>) -> Result<Option<NaiveDate>> {
    s.as_ref()
        .map(|v| {
            NaiveDate::parse_from_str(v, "%Y-%m-%d")
                .with_context(|| format!("Invalid {label} date: {v}"))
        })
        .transpose()
}

fn parse_period(
    period: &str,
    bucket: Option<std::time::Duration>,
) -> Result<(Period, String, Option<u32>)> {
    let p = period.trim().to_lowercase();
    match p.as_str() {
        "daily" => Ok((Period::Daily, "daily".to_string(), None)),
        "weekly" => Ok((Period::Weekly, "weekly".to_string(), None)),
        "monthly" => Ok((Period::Monthly, "monthly".to_string(), None)),
        "quarterly" => Ok((Period::Quarterly, "quarterly".to_string(), None)),
        "yearly" | "annual" => Ok((Period::Yearly, "yearly".to_string(), None)),
        "range" => Ok((Period::Range, "range".to_string(), None)),
        "custom" => {
            let dur = bucket.context("Missing --bucket for period=custom")?;
            let secs = dur.as_secs();
            let day = 24 * 60 * 60;
            if secs == 0 || secs % day != 0 {
                anyhow::bail!("Custom bucket duration must be a positive multiple of 1d (e.g. 14d)");
            }
            let days = (secs / day) as u32;
            Ok((Period::CustomDays(days), "custom".to_string(), Some(days)))
        }
        _ => anyhow::bail!(
            "Invalid period: {period}. Use: daily, weekly, monthly, quarterly, yearly, range, custom"
        ),
    }
}

fn parse_direction(s: &str) -> Result<(Direction, String)> {
    match s.trim().to_lowercase().as_str() {
        "outflow" => Ok((Direction::Outflow, "outflow".to_string())),
        "inflow" => Ok((Direction::Inflow, "inflow".to_string())),
        "net" => Ok((Direction::Net, "net".to_string())),
        _ => anyhow::bail!("Invalid direction: {s}. Use: outflow, inflow, net"),
    }
}

fn parse_status_filter(s: &str) -> Result<(StatusFilter, String)> {
    match s.trim().to_lowercase().as_str() {
        "posted" => Ok((StatusFilter::Posted, "posted".to_string())),
        "posted+pending" | "posted_pending" | "posted-pending" => {
            Ok((StatusFilter::PostedPending, "posted+pending".to_string()))
        }
        "all" => Ok((StatusFilter::All, "all".to_string())),
        _ => anyhow::bail!("Invalid status: {s}. Use: posted, posted+pending, all"),
    }
}

fn parse_group_by(s: &str) -> Result<(GroupBy, String)> {
    match s.trim().to_lowercase().as_str() {
        "none" => Ok((GroupBy::None, "none".to_string())),
        "category" => Ok((GroupBy::Category, "category".to_string())),
        "merchant" => Ok((GroupBy::Merchant, "merchant".to_string())),
        "account" => Ok((GroupBy::Account, "account".to_string())),
        "tag" => Ok((GroupBy::Tag, "tag".to_string())),
        _ => anyhow::bail!("Invalid group_by: {s}. Use: none, category, merchant, account, tag"),
    }
}

fn parse_week_start(s: Option<&str>) -> Result<(WeekStart, String)> {
    let Some(s) = s else {
        // Default for US-centric expectation: Sunday.
        return Ok((WeekStart::Sunday, "sunday".to_string()));
    };
    match s.trim().to_lowercase().as_str() {
        "sunday" | "sun" => Ok((WeekStart::Sunday, "sunday".to_string())),
        "monday" | "mon" => Ok((WeekStart::Monday, "monday".to_string())),
        _ => anyhow::bail!("Invalid week_start: {s}. Use: sunday, monday"),
    }
}

fn include_status(status: TransactionStatus, filter: StatusFilter) -> bool {
    match filter {
        StatusFilter::All => true,
        StatusFilter::Posted => matches!(status, TransactionStatus::Posted),
        StatusFilter::PostedPending => {
            matches!(
                status,
                TransactionStatus::Posted | TransactionStatus::Pending
            )
        }
    }
}

fn apply_direction(value_in_base: Decimal, direction: Direction) -> Decimal {
    match direction {
        Direction::Net => value_in_base,
        Direction::Outflow => {
            if value_in_base.is_sign_negative() && !value_in_base.is_zero() {
                -value_in_base
            } else {
                Decimal::ZERO
            }
        }
        Direction::Inflow => {
            if value_in_base.is_sign_positive() && !value_in_base.is_zero() {
                value_in_base
            } else {
                Decimal::ZERO
            }
        }
    }
}

fn last_day_of_month(year: i32, month: u32) -> NaiveDate {
    let (next_year, next_month) = if month == 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    };
    let first_next = NaiveDate::from_ymd_opt(next_year, next_month, 1).expect("valid date");
    first_next - chrono::Duration::days(1)
}

fn bucket_start_for(
    date: NaiveDate,
    period: Period,
    week_start: WeekStart,
    range_start: NaiveDate,
) -> NaiveDate {
    match period {
        Period::Daily => date,
        Period::Weekly => {
            let wd = date.weekday(); // Monday..Sunday
            let offset = match week_start {
                WeekStart::Sunday => wd.num_days_from_sunday() as i64,
                WeekStart::Monday => wd.num_days_from_monday() as i64,
            };
            date - chrono::Duration::days(offset)
        }
        Period::Monthly => {
            NaiveDate::from_ymd_opt(date.year(), date.month(), 1).expect("valid date")
        }
        Period::Quarterly => {
            let q0 = (date.month0() / 3) * 3; // 0,3,6,9
            NaiveDate::from_ymd_opt(date.year(), q0 + 1, 1).expect("valid date")
        }
        Period::Yearly => NaiveDate::from_ymd_opt(date.year(), 1, 1).expect("valid date"),
        Period::Range => range_start,
        Period::CustomDays(days) => {
            let delta = (date - range_start).num_days();
            if delta < 0 {
                return range_start;
            }
            range_start + chrono::Duration::days((delta / days as i64) * days as i64)
        }
    }
}

fn bucket_end_for(start: NaiveDate, period: Period, range_end: NaiveDate) -> NaiveDate {
    match period {
        Period::Daily => start,
        Period::Weekly => start + chrono::Duration::days(6),
        Period::Monthly => last_day_of_month(start.year(), start.month()),
        Period::Quarterly => {
            let sm = start.month();
            let end_month = sm + 2;
            last_day_of_month(start.year(), end_month)
        }
        Period::Yearly => NaiveDate::from_ymd_opt(start.year(), 12, 31).expect("valid date"),
        Period::Range => range_end,
        Period::CustomDays(days) => start + chrono::Duration::days(days as i64 - 1),
    }
}

fn next_bucket_start(start: NaiveDate, period: Period) -> NaiveDate {
    match period {
        Period::Daily => start + chrono::Duration::days(1),
        Period::Weekly => start + chrono::Duration::days(7),
        Period::CustomDays(days) => start + chrono::Duration::days(days as i64),
        Period::Monthly => {
            let (y, m) = (start.year(), start.month());
            let (ny, nm) = if m == 12 { (y + 1, 1) } else { (y, m + 1) };
            NaiveDate::from_ymd_opt(ny, nm, 1).expect("valid date")
        }
        Period::Quarterly => {
            let (y, m) = (start.year(), start.month());
            let mut nm = m + 3;
            let mut ny = y;
            while nm > 12 {
                nm -= 12;
                ny += 1;
            }
            NaiveDate::from_ymd_opt(ny, nm, 1).expect("valid date")
        }
        Period::Yearly => NaiveDate::from_ymd_opt(start.year() + 1, 1, 1).expect("valid date"),
        Period::Range => start, // caller should special-case
    }
}

fn clamp_date(date: NaiveDate, min: NaiveDate, max: NaiveDate) -> NaiveDate {
    if date < min {
        min
    } else if date > max {
        max
    } else {
        date
    }
}

#[derive(Default)]
struct BucketAgg {
    total: Decimal,
    tx_count: usize,
    breakdown_total: HashMap<String, (Decimal, usize)>,
}

fn market_data_store_for_prod(data_dir: &std::path::Path) -> Arc<dyn MarketDataStore> {
    Arc::new(crate::market_data::JsonlMarketDataStore::new(data_dir))
}

fn normalized_rule(s: &str) -> Option<String> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_lowercase())
    }
}

async fn ignored_account_ids_for_portfolio_spending(
    storage: &dyn Storage,
    config: &ResolvedConfig,
    accounts: &[Account],
) -> Result<HashSet<Id>> {
    let ignore_accounts: HashSet<String> = config
        .spending
        .ignore_accounts
        .iter()
        .filter_map(|s| normalized_rule(s))
        .collect();
    let ignore_connections_raw: HashSet<String> = config
        .spending
        .ignore_connections
        .iter()
        .filter_map(|s| normalized_rule(s))
        .collect();
    let ignore_tags: HashSet<String> = config
        .spending
        .ignore_tags
        .iter()
        .filter_map(|s| normalized_rule(s))
        .collect();

    if ignore_accounts.is_empty() && ignore_connections_raw.is_empty() && ignore_tags.is_empty() {
        return Ok(HashSet::new());
    }

    let connections = storage.list_connections().await?;
    let mut ignore_connections: HashSet<String> = HashSet::new();
    for value in &ignore_connections_raw {
        ignore_connections.insert(value.clone());
    }
    for conn in connections {
        let conn_id = conn.id().to_string().to_lowercase();
        let conn_name = conn.config.name.to_lowercase();
        if ignore_connections_raw.contains(&conn_id) || ignore_connections_raw.contains(&conn_name)
        {
            ignore_connections.insert(conn_id);
        }
    }

    let mut ignored = HashSet::new();
    for account in accounts {
        let account_id = account.id.to_string().to_lowercase();
        let account_name = account.name.to_lowercase();
        let connection_id = account.connection_id.to_string().to_lowercase();
        let has_ignored_tag = account
            .tags
            .iter()
            .filter_map(|tag| normalized_rule(tag))
            .any(|tag| ignore_tags.contains(&tag));

        if ignore_accounts.contains(&account_id)
            || ignore_accounts.contains(&account_name)
            || ignore_connections.contains(&connection_id)
            || has_ignored_tag
        {
            ignored.insert(account.id.clone());
        }
    }

    Ok(ignored)
}

pub async fn spending_report(
    storage: &dyn Storage,
    config: &ResolvedConfig,
    opts: SpendingReportOptions,
) -> Result<SpendingOutput> {
    spending_report_with_store(
        storage,
        config,
        opts,
        market_data_store_for_prod(&config.data_dir),
    )
    .await
}

async fn spending_report_with_store(
    storage: &dyn Storage,
    config: &ResolvedConfig,
    opts: SpendingReportOptions,
    store: Arc<dyn MarketDataStore>,
) -> Result<SpendingOutput> {
    let currency = opts
        .currency
        .unwrap_or_else(|| config.reporting_currency.clone())
        .trim()
        .to_uppercase();

    let (tz, tz_label) = TzSpec::parse(opts.tz.as_deref())?;
    let (period, period_label, bucket_days) = parse_period(&opts.period, opts.bucket)?;
    let (direction, direction_label) = parse_direction(&opts.direction)?;
    let (status_filter, status_label) = parse_status_filter(&opts.status)?;
    let (group_by, group_by_label) = parse_group_by(&opts.group_by)?;
    let (week_start, week_start_label) = parse_week_start(opts.week_start.as_deref())?;

    if opts.account.is_some() && opts.connection.is_some() {
        anyhow::bail!("--account and --connection are mutually exclusive");
    }

    // Resolve scope and account ids.
    let (scope, account_ids): (SpendingScopeOutput, Vec<Id>) =
        if let Some(id_or_name) = &opts.account {
            let acct = find_account(storage, id_or_name)
                .await?
                .context(format!("Account not found: {id_or_name}"))?;
            (
                SpendingScopeOutput::Account {
                    id: acct.id.to_string(),
                    name: acct.name.clone(),
                },
                vec![acct.id],
            )
        } else if let Some(id_or_name) = &opts.connection {
            let conn = find_connection(storage, id_or_name)
                .await?
                .context(format!("Connection not found: {id_or_name}"))?;
            let accounts = storage.list_accounts().await?;
            let ids: Vec<Id> = accounts
                .into_iter()
                .filter(|a| a.connection_id == *conn.id())
                .map(|a| a.id)
                .collect();
            (
                SpendingScopeOutput::Connection {
                    id: conn.id().to_string(),
                    name: conn.config.name.clone(),
                },
                ids,
            )
        } else {
            let accounts = storage.list_accounts().await?;
            let ignored =
                ignored_account_ids_for_portfolio_spending(storage, config, &accounts).await?;
            let ids: Vec<Id> = accounts
                .into_iter()
                .filter(|a| !ignored.contains(&a.id))
                .map(|a| a.id)
                .collect();
            (SpendingScopeOutput::Portfolio, ids)
        };

    // Setup market data service (store-only).
    let market_data = MarketDataServiceBuilder::new(store, config.data_dir.clone())
        .with_quote_staleness(config.refresh.price_staleness)
        .with_lookback_days(opts.lookback_days)
        .offline_only()
        .build()
        .await;

    let start_date_opt = parse_date_opt("start", &opts.start)?;
    let end_date_opt = parse_date_opt("end", &opts.end)?;

    // Load transactions + materialize annotations (per account) and build rows.
    struct Row {
        account_id: Id,
        local_date: NaiveDate,
        asset: Asset,
        amount: String,
        raw_description: String,
        annotation: Option<TransactionAnnotation>,
    }

    let mut rows: Vec<Row> = Vec::new();
    let mut min_date: Option<NaiveDate> = None;

    for account_id in &account_ids {
        let transactions = storage.get_transactions(account_id).await?;
        let patches = storage
            .get_transaction_annotation_patches(account_id)
            .await?;

        // Materialize last-write-wins annotation state per transaction id.
        let mut annotations_by_tx: HashMap<Id, TransactionAnnotation> = HashMap::new();
        for patch in patches {
            let tx_id = patch.transaction_id.clone();
            let ann = annotations_by_tx
                .entry(tx_id.clone())
                .or_insert_with(|| TransactionAnnotation::new(tx_id));
            patch.apply_to(ann);
        }

        for tx in transactions {
            if !include_status(tx.status, status_filter) {
                continue;
            }

            let local_date = tz.date_in_tz(tx.timestamp);
            min_date = Some(match min_date {
                None => local_date,
                Some(d) => d.min(local_date),
            });

            let asset = tx.asset.normalized();
            if !opts.include_noncurrency && !matches!(asset, Asset::Currency { .. }) {
                continue;
            }

            rows.push(Row {
                account_id: account_id.clone(),
                local_date,
                asset,
                amount: tx.amount,
                raw_description: tx.description,
                annotation: annotations_by_tx.get(&tx.id).cloned(),
            });
        }
    }

    let today = tz.today();
    let start_date = start_date_opt.or(min_date).unwrap_or(today);
    let end_date = end_date_opt.unwrap_or(today);
    if end_date < start_date {
        anyhow::bail!("end date {end_date} is before start date {start_date}");
    }

    let mut buckets: HashMap<NaiveDate, BucketAgg> = HashMap::new();
    let mut skipped = 0usize;
    let mut missing_price = 0usize;
    let mut missing_fx = 0usize;
    let mut included_tx = 0usize;
    let mut grand_total = Decimal::ZERO;

    for row in rows {
        if row.local_date < start_date || row.local_date > end_date {
            continue;
        }

        // Direction prefilter: valuation is linear with positive prices/FX, so sign is preserved.
        // Avoid counting missing market data for transactions that couldn't contribute.
        let amt = Decimal::from_str(&row.amount)
            .with_context(|| format!("Invalid transaction amount: {}", row.amount))?;
        if amt.is_zero() {
            continue;
        }
        match direction {
            Direction::Outflow => {
                if !amt.is_sign_negative() {
                    continue;
                }
            }
            Direction::Inflow => {
                if !amt.is_sign_positive() {
                    continue;
                }
            }
            Direction::Net => {}
        }

        let converted = value_in_reporting_currency_detailed(
            &market_data,
            &row.asset,
            &row.amount,
            &currency,
            row.local_date,
            config.display.currency_decimals,
        )
        .await?;

        let Some(value_str) = converted.value else {
            skipped += 1;
            match converted.missing {
                Some(MissingMarketData::Price) => missing_price += 1,
                Some(MissingMarketData::Fx) => missing_fx += 1,
                None => {}
            }
            continue;
        };

        let value_dec = Decimal::from_str(&value_str).with_context(|| {
            format!("Internal error: formatted decimal did not parse: {value_str}")
        })?;
        let directed = apply_direction(value_dec, direction);
        if directed.is_zero() {
            continue;
        }

        included_tx += 1;
        grand_total += directed;

        let bucket_start = bucket_start_for(row.local_date, period, week_start, start_date);
        let agg = buckets.entry(bucket_start).or_default();
        agg.total += directed;
        agg.tx_count += 1;

        if group_by != GroupBy::None {
            let keys: Vec<String> = match group_by {
                GroupBy::None => vec![],
                GroupBy::Category => vec![row
                    .annotation
                    .as_ref()
                    .and_then(|a| a.category.clone())
                    .unwrap_or_else(|| "uncategorized".to_string())],
                GroupBy::Merchant => vec![row
                    .annotation
                    .as_ref()
                    .and_then(|a| a.description.clone())
                    .unwrap_or_else(|| row.raw_description.clone())],
                GroupBy::Account => vec![row.account_id.to_string()],
                GroupBy::Tag => row
                    .annotation
                    .as_ref()
                    .and_then(|a| a.tags.clone())
                    .unwrap_or_default(),
            };

            let keys = if matches!(group_by, GroupBy::Tag) && keys.is_empty() {
                vec!["untagged".to_string()]
            } else {
                keys
            };

            for key in keys {
                let entry = agg.breakdown_total.entry(key).or_insert((Decimal::ZERO, 0));
                entry.0 += directed;
                entry.1 += 1;
            }
        }
    }

    let mut starts: Vec<NaiveDate> = if opts.include_empty {
        let mut s = bucket_start_for(start_date, period, week_start, start_date);
        let mut out = Vec::new();
        if matches!(period, Period::Range) {
            out.push(s);
        } else {
            while s <= end_date {
                out.push(s);
                s = next_bucket_start(s, period);
            }
        }
        out
    } else {
        let mut out: Vec<NaiveDate> = buckets.keys().cloned().collect();
        out.sort();
        out
    };
    if opts.include_empty {
        starts.sort();
        starts.dedup();
    }

    let mut period_outputs = Vec::new();
    for bstart in starts {
        let bend = bucket_end_for(bstart, period, end_date);
        let clamped_start = clamp_date(bstart, start_date, end_date);
        let clamped_end = clamp_date(bend, start_date, end_date);
        let agg = buckets.get(&bstart);
        let total = agg.map(|a| a.total).unwrap_or(Decimal::ZERO);
        let tx_count = agg.map(|a| a.tx_count).unwrap_or(0);

        let mut breakdown: Vec<SpendingBreakdownEntryOutput> = Vec::new();
        if group_by != GroupBy::None {
            let mut entries: Vec<(String, Decimal, usize)> = agg
                .map(|a| {
                    a.breakdown_total
                        .iter()
                        .map(|(k, (v, c))| (k.clone(), *v, *c))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            entries.sort_by(|a, b| {
                b.1.partial_cmp(&a.1)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| a.0.cmp(&b.0))
            });
            if let Some(top) = opts.top {
                entries.truncate(top);
            }
            for (k, v, c) in entries {
                breakdown.push(SpendingBreakdownEntryOutput {
                    key: k,
                    total: crate::format::format_base_currency_value(
                        v,
                        config.display.currency_decimals,
                    ),
                    transaction_count: c,
                });
            }
        }

        period_outputs.push(SpendingPeriodOutput {
            start_date: format_ymd(clamped_start),
            end_date: format_ymd(clamped_end),
            total: crate::format::format_base_currency_value(
                total,
                config.display.currency_decimals,
            ),
            transaction_count: tx_count,
            breakdown,
        });
    }

    Ok(SpendingOutput {
        scope,
        currency,
        tz: tz_label,
        start_date: format_ymd(start_date),
        end_date: format_ymd(end_date),
        period: period_label,
        week_start: if matches!(period, Period::Weekly) {
            Some(week_start_label)
        } else {
            None
        },
        bucket_days,
        direction: direction_label,
        status: status_label,
        group_by: group_by_label,
        total: crate::format::format_base_currency_value(
            grand_total,
            config.display.currency_decimals,
        ),
        transaction_count: included_tx,
        periods: period_outputs,
        skipped_transaction_count: skipped,
        missing_price_transaction_count: missing_price,
        missing_fx_transaction_count: missing_fx,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::Clock;
    use crate::clock::FixedClock;
    use crate::market_data::{
        FxRateKind, FxRatePoint, MemoryMarketDataStore, PriceKind, PricePoint,
    };
    use crate::models::{Account, Asset, FixedIdGenerator, Transaction};
    use crate::storage::MemoryStorage;
    use chrono::TimeZone;

    #[tokio::test]
    async fn spending_report_buckets_by_timezone_date() -> Result<()> {
        let storage = MemoryStorage::new();
        let conn_id = Id::from_string("conn-1");
        let acct_id = Id::from_string("acct-1");
        let account = Account::new_with(acct_id.clone(), Utc::now(), "Checking", conn_id);
        storage.save_account(&account).await?;

        // 2026-02-01T02:30Z is 2026-01-31 in America/New_York (UTC-05 in winter).
        let tx_id_gen = FixedIdGenerator::new([Id::from_string("tx-1")]);
        let clock = FixedClock::new(Utc.with_ymd_and_hms(2026, 2, 1, 2, 30, 0).unwrap());
        let tx = Transaction::new_with_generator(
            &tx_id_gen,
            &clock,
            "-10",
            Asset::currency("USD"),
            "Test",
        )
        .with_timestamp(clock.now());
        storage.append_transactions(&acct_id, &[tx]).await?;

        let store = Arc::new(MemoryMarketDataStore::default());
        let cfg = ResolvedConfig {
            data_dir: std::path::PathBuf::from("/tmp"),
            reporting_currency: "USD".to_string(),
            display: crate::config::DisplayConfig::default(),
            refresh: crate::config::RefreshConfig::default(),
            tray: crate::config::TrayConfig::default(),
            spending: crate::config::SpendingConfig::default(),
            git: crate::config::GitConfig::default(),
        };

        let out = spending_report_with_store(
            &storage,
            &cfg,
            SpendingReportOptions {
                currency: None,
                start: Some("2026-01-30".to_string()),
                end: Some("2026-02-02".to_string()),
                period: "daily".to_string(),
                tz: Some("America/New_York".to_string()),
                week_start: None,
                bucket: None,
                account: Some("acct-1".to_string()),
                connection: None,
                status: "posted".to_string(),
                direction: "outflow".to_string(),
                group_by: "none".to_string(),
                top: None,
                lookback_days: 7,
                include_noncurrency: false,
                include_empty: false,
            },
            store,
        )
        .await?;

        assert_eq!(out.periods.len(), 1);
        assert_eq!(out.periods[0].start_date, "2026-01-31");
        assert_eq!(out.periods[0].total, "10");
        Ok(())
    }

    #[tokio::test]
    async fn spending_report_converts_fx_and_prices() -> Result<()> {
        let storage = MemoryStorage::new();
        let conn_id = Id::from_string("conn-1");
        let acct_id = Id::from_string("acct-1");
        let account = Account::new_with(acct_id.clone(), Utc::now(), "Checking", conn_id);
        storage.save_account(&account).await?;

        let ids = FixedIdGenerator::new([Id::from_string("tx-eur"), Id::from_string("tx-eq")]);
        let clock = FixedClock::new(Utc.with_ymd_and_hms(2026, 2, 5, 12, 0, 0).unwrap());

        let tx_eur = Transaction::new_with_generator(
            &ids,
            &clock,
            "-10",
            Asset::currency("EUR"),
            "EUR debit",
        )
        .with_timestamp(clock.now());
        let tx_eq = Transaction::new_with_generator(
            &ids,
            &clock,
            "-2",
            Asset::equity("AAPL"),
            "Buy AAPL shares",
        )
        .with_timestamp(clock.now());
        storage
            .append_transactions(&acct_id, &[tx_eur, tx_eq])
            .await?;

        let store = Arc::new(MemoryMarketDataStore::default());
        // EURUSD close 1.2 on 2026-02-05 => -10 EUR -> -12 USD (outflow 12).
        store
            .put_fx_rates(&[FxRatePoint {
                base: "EUR".to_string(),
                quote: "USD".to_string(),
                as_of_date: NaiveDate::from_ymd_opt(2026, 2, 5).unwrap(),
                timestamp: clock.now(),
                rate: "1.2".to_string(),
                kind: FxRateKind::Close,
                source: "test".to_string(),
            }])
            .await?;
        // AAPL close 50 USD on 2026-02-05 => -2 shares -> -100 USD (outflow 100).
        store
            .put_prices(&[PricePoint {
                asset_id: crate::market_data::AssetId::from_asset(
                    &Asset::equity("AAPL").normalized(),
                ),
                as_of_date: NaiveDate::from_ymd_opt(2026, 2, 5).unwrap(),
                timestamp: clock.now(),
                price: "50".to_string(),
                quote_currency: "USD".to_string(),
                kind: PriceKind::Close,
                source: "test".to_string(),
            }])
            .await?;

        let cfg = ResolvedConfig {
            data_dir: std::path::PathBuf::from("/tmp"),
            reporting_currency: "USD".to_string(),
            display: crate::config::DisplayConfig::default(),
            refresh: crate::config::RefreshConfig::default(),
            tray: crate::config::TrayConfig::default(),
            spending: crate::config::SpendingConfig::default(),
            git: crate::config::GitConfig::default(),
        };

        let out = spending_report_with_store(
            &storage,
            &cfg,
            SpendingReportOptions {
                currency: None,
                start: Some("2026-02-01".to_string()),
                end: Some("2026-02-28".to_string()),
                period: "monthly".to_string(),
                tz: Some("UTC".to_string()),
                week_start: None,
                bucket: None,
                account: Some("acct-1".to_string()),
                connection: None,
                status: "posted".to_string(),
                direction: "outflow".to_string(),
                group_by: "none".to_string(),
                top: None,
                lookback_days: 7,
                include_noncurrency: true,
                include_empty: false,
            },
            store,
        )
        .await?;

        assert_eq!(out.total, "112");
        assert_eq!(out.transaction_count, 2);
        Ok(())
    }

    #[tokio::test]
    async fn spending_report_ignores_accounts_by_configured_tags() -> Result<()> {
        let storage = MemoryStorage::new();
        let conn_id = Id::from_string("conn-1");

        let acct_card_id = Id::from_string("acct-card");
        let card = Account::new_with(acct_card_id.clone(), Utc::now(), "Card", conn_id.clone());
        storage.save_account(&card).await?;

        let acct_brokerage_id = Id::from_string("acct-brokerage");
        let mut brokerage = Account::new_with(
            acct_brokerage_id.clone(),
            Utc::now(),
            "Individual",
            conn_id.clone(),
        );
        brokerage.tags = vec!["brokerage".to_string()];
        storage.save_account(&brokerage).await?;

        let ids =
            FixedIdGenerator::new([Id::from_string("tx-card"), Id::from_string("tx-brokerage")]);
        let clock = FixedClock::new(Utc.with_ymd_and_hms(2026, 2, 5, 12, 0, 0).unwrap());
        let tx_card = Transaction::new_with_generator(
            &ids,
            &clock,
            "-10",
            Asset::currency("USD"),
            "Card spend",
        )
        .with_timestamp(clock.now());
        let tx_brokerage = Transaction::new_with_generator(
            &ids,
            &clock,
            "-2000",
            Asset::currency("USD"),
            "Brokerage transfer",
        )
        .with_timestamp(clock.now());
        storage
            .append_transactions(&acct_card_id, &[tx_card])
            .await?;
        storage
            .append_transactions(&acct_brokerage_id, &[tx_brokerage])
            .await?;

        let cfg = ResolvedConfig {
            data_dir: std::path::PathBuf::from("/tmp"),
            reporting_currency: "USD".to_string(),
            display: crate::config::DisplayConfig::default(),
            refresh: crate::config::RefreshConfig::default(),
            tray: crate::config::TrayConfig::default(),
            spending: crate::config::SpendingConfig {
                ignore_accounts: vec![],
                ignore_connections: vec![],
                ignore_tags: vec!["brokerage".to_string()],
            },
            git: crate::config::GitConfig::default(),
        };

        let out = spending_report_with_store(
            &storage,
            &cfg,
            SpendingReportOptions {
                currency: None,
                start: Some("2026-02-01".to_string()),
                end: Some("2026-02-28".to_string()),
                period: "monthly".to_string(),
                tz: Some("UTC".to_string()),
                week_start: None,
                bucket: None,
                account: None,
                connection: None,
                status: "posted".to_string(),
                direction: "outflow".to_string(),
                group_by: "none".to_string(),
                top: None,
                lookback_days: 7,
                include_noncurrency: false,
                include_empty: false,
            },
            Arc::new(MemoryMarketDataStore::default()),
        )
        .await?;

        assert_eq!(out.total, "10");
        assert_eq!(out.transaction_count, 1);
        Ok(())
    }
}
