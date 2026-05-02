use super::*;
use std::collections::HashSet;

pub(crate) fn range_preset_from_config(value: &str) -> RangePreset {
    match normalize_config_key(value).as_str() {
        "1m" | "1month" | "month" | "onemonth" => RangePreset::OneMonth,
        "90d" | "90days" | "ninetydays" => RangePreset::NinetyDays,
        "6m" | "6months" | "sixmonths" => RangePreset::SixMonths,
        "1y" | "1year" | "year" | "oneyear" => RangePreset::OneYear,
        "2y" | "2years" | "twoyears" => RangePreset::TwoYears,
        "max" | "all" => RangePreset::Max,
        _ => DEFAULT_RANGE_PRESET,
    }
}

pub(crate) fn sampling_granularity_from_config(value: &str) -> SamplingGranularity {
    match normalize_config_key(value).as_str() {
        "auto" => SamplingGranularity::Auto,
        "daily" | "day" => SamplingGranularity::Daily,
        "weekly" | "week" => SamplingGranularity::Weekly,
        "monthly" | "month" => SamplingGranularity::Monthly,
        "yearly" | "annual" | "annually" | "year" => SamplingGranularity::Yearly,
        _ => DEFAULT_SAMPLING_GRANULARITY,
    }
}

pub(crate) fn normalize_config_key(value: &str) -> String {
    value
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(|ch| ch.to_lowercase())
        .collect()
}

pub(crate) fn current_net_worth_from_snapshot(snapshot: &PortfolioSnapshot) -> f64 {
    snapshot
        .total_value
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite())
        .unwrap_or_default()
}

pub(crate) fn history_data_points(history: &History) -> Vec<NetWorthDataPoint> {
    let mut points = history
        .points
        .iter()
        .filter_map(|point| {
            point
                .total_value
                .parse::<f64>()
                .ok()
                .filter(|value| value.is_finite())
                .map(|value| NetWorthDataPoint {
                    date: point.date.clone(),
                    value,
                })
        })
        .collect::<Vec<_>>();
    points.sort_by(|a, b| a.date.cmp(&b.date));
    points
}

pub(crate) fn date_bounds(points: &[NetWorthDataPoint]) -> Option<(String, String)> {
    Some((points.first()?.date.clone(), points.last()?.date.clone()))
}

pub(crate) fn visible_date_range(
    points: &[NetWorthDataPoint],
    preset: RangePreset,
    start_override: &str,
    end_override: &str,
) -> (String, String) {
    let Some((min_date, max_date)) = date_bounds(points) else {
        return (String::new(), String::new());
    };

    if preset == RangePreset::Custom {
        return (
            if start_override.is_empty() {
                min_date.clone()
            } else {
                start_override.to_string()
            },
            if end_override.is_empty() {
                max_date.clone()
            } else {
                end_override.to_string()
            },
        );
    }

    let end = max_date.clone();
    let start = match preset {
        RangePreset::OneMonth => offset_months(&end, 1).max(min_date.clone()),
        RangePreset::NinetyDays => offset_days(&end, 90).max(min_date.clone()),
        RangePreset::SixMonths => offset_months(&end, 6).max(min_date.clone()),
        RangePreset::OneYear => offset_years(&end, 1).max(min_date.clone()),
        RangePreset::TwoYears => offset_years(&end, 2).max(min_date.clone()),
        RangePreset::Max | RangePreset::Custom => min_date.clone(),
    };
    (start, end)
}

pub(crate) fn history_query_string(
    preset: RangePreset,
    start_override: &str,
    end_override: &str,
    selected_sampling: SamplingGranularity,
    today: &str,
    filter_overrides: FilterOverrides,
    account: Option<&str>,
) -> String {
    let (start, end) = requested_history_date_range(preset, start_override, end_override, today);
    let granularity =
        history_request_granularity(selected_sampling, start.as_deref(), end.as_deref());
    let mut params = vec![format!(
        "granularity={}",
        query_encode_component(granularity)
    )];

    if let Some(start) = start {
        push_query_param(&mut params, "start", &start);
    }
    if let Some(end) = end {
        push_query_param(&mut params, "end", &end);
    }
    if let Some(account) = account.filter(|account| !account.is_empty()) {
        push_query_param(&mut params, "account", account);
    }
    append_filter_override_params(&mut params, filter_overrides);

    params.join("&")
}

pub(crate) fn spending_query_string(
    preset: RangePreset,
    start_override: &str,
    end_override: &str,
    today: &str,
    currency: &str,
) -> String {
    let (start, end) = requested_history_date_range(preset, start_override, end_override, today);
    let mut params = vec![
        "period=range".to_string(),
        "group_by=category".to_string(),
        "direction=outflow".to_string(),
        "status=posted".to_string(),
        format!("currency={currency}"),
    ];
    if let Some(start) = start {
        params.push(format!("start={start}"));
    }
    if let Some(end) = end {
        params.push(format!("end={end}"));
    }
    params.join("&")
}

#[cfg(any(target_arch = "wasm32", test))]
pub(crate) fn filter_override_query_string(overrides: FilterOverrides) -> String {
    let mut params = Vec::new();
    append_filter_override_params(&mut params, overrides);
    params.join("&")
}

pub(crate) fn append_filter_override_params(params: &mut Vec<String>, overrides: FilterOverrides) {
    if let Some(enabled) = overrides.include_latent_capital_gains_tax {
        push_query_param(
            params,
            "include_latent_capital_gains_tax",
            bool_query_value(enabled),
        );
    }
}

pub(crate) fn bool_query_value(value: bool) -> &'static str {
    if value {
        "true"
    } else {
        "false"
    }
}

pub(crate) fn push_query_param(params: &mut Vec<String>, key: &str, value: &str) {
    params.push(format!("{key}={}", query_encode_component(value)));
}

pub(crate) fn query_encode_component(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.as_bytes() {
        match *byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(*byte as char);
            }
            other => encoded.push_str(&format!("%{other:02X}")),
        }
    }
    encoded
}

pub(crate) fn requested_history_date_range(
    preset: RangePreset,
    start_override: &str,
    end_override: &str,
    today: &str,
) -> (Option<String>, Option<String>) {
    if preset == RangePreset::Custom {
        return (
            non_empty_string(start_override),
            non_empty_string(end_override).or_else(|| Some(today.to_string())),
        );
    }

    let end = Some(today.to_string());
    let start = match preset {
        RangePreset::OneMonth => Some(offset_months(today, 1)),
        RangePreset::NinetyDays => Some(offset_days(today, 90)),
        RangePreset::SixMonths => Some(offset_months(today, 6)),
        RangePreset::OneYear => Some(offset_years(today, 1)),
        RangePreset::TwoYears => Some(offset_years(today, 2)),
        RangePreset::Max | RangePreset::Custom => None,
    };

    if preset == RangePreset::Max {
        (None, None)
    } else {
        (start, end)
    }
}

pub(crate) fn non_empty_string(value: &str) -> Option<String> {
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

pub(crate) fn range_summary_text(start: &str, end: &str) -> String {
    match (start.is_empty(), end.is_empty()) {
        (false, false) => format!("{start} to {end}"),
        (false, true) => format!("{start} onward"),
        (true, false) => format!("through {end}"),
        (true, true) => "All available dates".to_string(),
    }
}

pub(crate) fn history_request_granularity(
    selected: SamplingGranularity,
    start: Option<&str>,
    end: Option<&str>,
) -> &'static str {
    if selected != SamplingGranularity::Auto {
        return selected.query_value();
    }

    match (start, end) {
        (Some(start), Some(end)) => match days_between(start, end) {
            Some(days) if days < 93 => SamplingGranularity::Daily.query_value(),
            Some(days) if days > 365 * 3 => SamplingGranularity::Monthly.query_value(),
            Some(_) => SamplingGranularity::Weekly.query_value(),
            None => SamplingGranularity::Daily.query_value(),
        },
        _ => SamplingGranularity::Monthly.query_value(),
    }
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn current_date_string() -> String {
    let date = js_sys::Date::new_0();
    format!(
        "{:04}-{:02}-{:02}",
        date.get_full_year(),
        date.get_month() + 1,
        date.get_date()
    )
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn current_date_string() -> String {
    chrono::Local::now().date_naive().to_string()
}

pub(crate) fn offset_years(date: &str, years: i32) -> String {
    offset_months(date, years * 12)
}

pub(crate) fn offset_months(date: &str, months: i32) -> String {
    let Some((year, month, day)) = parse_ymd(date) else {
        return date.to_string();
    };

    let month_index = year * 12 + month as i32 - 1 - months;
    let new_year = month_index.div_euclid(12);
    let new_month = month_index.rem_euclid(12) as u32 + 1;
    let new_day = day.min(days_in_month(new_year, new_month));
    format!("{new_year:04}-{new_month:02}-{new_day:02}")
}

pub(crate) fn offset_days(date: &str, days: i64) -> String {
    let Some((year, month, day)) = parse_ymd(date) else {
        return date.to_string();
    };
    civil_from_days(days_from_civil(year, month, day) - days)
}

pub(crate) fn filter_data_by_date_range(
    points: &[NetWorthDataPoint],
    start_date: &str,
    end_date: &str,
) -> Vec<NetWorthDataPoint> {
    if start_date.is_empty() || end_date.is_empty() || start_date > end_date {
        return Vec::new();
    }

    points
        .iter()
        .filter(|point| point.date.as_str() >= start_date && point.date.as_str() <= end_date)
        .cloned()
        .collect()
}

pub(crate) fn resolve_sampling_granularity(
    selected: SamplingGranularity,
    points: &[NetWorthDataPoint],
) -> SamplingGranularity {
    if selected != SamplingGranularity::Auto {
        return selected;
    }

    let Some(first) = points.first() else {
        return SamplingGranularity::Daily;
    };
    let Some(last) = points.last() else {
        return SamplingGranularity::Daily;
    };

    match days_between(&first.date, &last.date) {
        Some(days) if days < 93 => SamplingGranularity::Daily,
        Some(days) if days > 365 * 3 => SamplingGranularity::Monthly,
        Some(_) => SamplingGranularity::Weekly,
        _ => SamplingGranularity::Daily,
    }
}

pub(crate) fn sample_data_by_granularity(
    points: &[NetWorthDataPoint],
    granularity: SamplingGranularity,
) -> Vec<NetWorthDataPoint> {
    if matches!(
        granularity,
        SamplingGranularity::Auto | SamplingGranularity::Daily
    ) || points.len() <= 2
    {
        return points.to_vec();
    }

    let mut sampled = Vec::new();
    let mut current_bucket: Option<String> = None;
    let mut current_point: Option<NetWorthDataPoint> = None;

    for point in points {
        let bucket = sampling_bucket(&point.date, granularity);
        if current_bucket.as_deref() != Some(bucket.as_str()) {
            if let Some(point) = current_point.take() {
                sampled.push(point);
            }
            current_bucket = Some(bucket);
        }
        current_point = Some(point.clone());
    }

    if let Some(point) = current_point {
        sampled.push(point);
    }

    include_range_endpoints(points, sampled)
}

pub(crate) fn include_range_endpoints(
    points: &[NetWorthDataPoint],
    sampled: Vec<NetWorthDataPoint>,
) -> Vec<NetWorthDataPoint> {
    let Some(first) = points.first() else {
        return sampled;
    };
    let Some(last) = points.last() else {
        return sampled;
    };

    let mut with_endpoints = sampled;
    if !with_endpoints.iter().any(|point| point.date == first.date) {
        with_endpoints.push(first.clone());
    }
    if !with_endpoints.iter().any(|point| point.date == last.date) {
        with_endpoints.push(last.clone());
    }
    with_endpoints.sort_by(|a, b| a.date.cmp(&b.date));
    with_endpoints
}

pub(crate) fn sampling_bucket(date: &str, granularity: SamplingGranularity) -> String {
    match granularity {
        SamplingGranularity::Weekly => parse_ymd(date)
            .map(|(year, month, day)| {
                let day_number = days_from_civil(year, month, day);
                format!("week-{}", day_number.div_euclid(7))
            })
            .unwrap_or_else(|| date.to_string()),
        SamplingGranularity::Monthly => date.get(..7).unwrap_or(date).to_string(),
        SamplingGranularity::Yearly => date.get(..4).unwrap_or(date).to_string(),
        SamplingGranularity::Auto | SamplingGranularity::Daily => date.to_string(),
    }
}

pub(crate) fn days_between(start: &str, end: &str) -> Option<i64> {
    let (start_year, start_month, start_day) = parse_ymd(start)?;
    let (end_year, end_month, end_day) = parse_ymd(end)?;
    Some(
        days_from_civil(end_year, end_month, end_day)
            - days_from_civil(start_year, start_month, start_day),
    )
}

pub(crate) fn parse_ymd(date: &str) -> Option<(i32, u32, u32)> {
    let mut parts = date.split('-');
    let year = parts.next()?.parse::<i32>().ok()?;
    let month = parts.next()?.parse::<u32>().ok()?;
    let day = parts.next()?.parse::<u32>().ok()?;
    if parts.next().is_some() || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    Some((year, month, day))
}

pub(crate) fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 30,
    }
}

pub(crate) fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

pub(crate) fn days_from_civil(year: i32, month: u32, day: u32) -> i64 {
    let year = year - if month <= 2 { 1 } else { 0 };
    let era = (year as i64).div_euclid(400);
    let yoe = year as i64 - era * 400;
    let month = month as i64;
    let doy = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

pub(crate) fn civil_from_days(days: i64) -> String {
    let days = days + 719_468;
    let era = days.div_euclid(146_097);
    let doe = days - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096).div_euclid(365);
    let mut year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2).div_euclid(153);
    let day = doy - (153 * mp + 2).div_euclid(5) + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    year += if month <= 2 { 1 } else { 0 };
    format!("{year:04}-{month:02}-{day:02}")
}

pub(crate) fn value_bounds(points: &[NetWorthDataPoint]) -> Option<(f64, f64)> {
    let first = points.first()?.value;
    let mut min = first;
    let mut max = first;
    for point in points {
        min = min.min(point.value);
        max = max.max(point.value);
    }
    Some(if min == max {
        (min - 1.0, max + 1.0)
    } else {
        (min, max)
    })
}

pub(crate) fn parse_money_input(value: &str) -> Option<f64> {
    let cleaned = value
        .chars()
        .filter(|ch| !matches!(ch, '$' | ',' | ' '))
        .collect::<String>();
    if cleaned.is_empty() {
        None
    } else {
        cleaned
            .parse::<f64>()
            .ok()
            .filter(|value| value.is_finite())
    }
}

pub(crate) fn account_snapshot_value(
    account_id: &str,
    account_summaries: &[AccountSummary],
) -> Option<f64> {
    account_summaries
        .iter()
        .find(|summary| summary.account_id == account_id)
        .and_then(|summary| summary.value_in_base.as_deref())
        .and_then(parse_money_input)
}

pub(crate) fn virtual_account_summaries(snapshot: &PortfolioSnapshot) -> Vec<AccountSummary> {
    snapshot
        .by_account
        .iter()
        .filter(|account| account.account_id.starts_with("virtual:"))
        .cloned()
        .collect()
}

pub(crate) fn parse_y_domain(min: &str, max: &str) -> Option<(f64, f64)> {
    if min.is_empty() && max.is_empty() {
        return None;
    }
    let min = parse_money_input(min)?;
    let max = parse_money_input(max)?;
    if min < max {
        Some((min, max))
    } else {
        None
    }
}

pub(crate) fn format_input_number(value: f64) -> String {
    format_number(value, 2)
}

pub(crate) fn format_compact_money(value: f64, currency: &str) -> String {
    let abs = value.abs();
    let (scaled, suffix) = if abs >= 1_000_000_000.0 {
        (value / 1_000_000_000.0, "B")
    } else if abs >= 1_000_000.0 {
        (value / 1_000_000.0, "M")
    } else if abs >= 1_000.0 {
        (value / 1_000.0, "K")
    } else {
        (value, "")
    };
    format_money_display(scaled, currency, 1, suffix)
}

pub(crate) fn format_full_money(value: f64, currency: &str) -> String {
    format_money_display(value, currency, 2, "")
}

fn format_money_display(value: f64, currency: &str, decimals: usize, suffix: &str) -> String {
    let sign = if value < 0.0 { "-" } else { "" };
    let rounded = format!("{:.*}", decimals, value.abs());
    let amount = match rounded.split_once('.') {
        Some((integer, fraction)) => {
            format!(
                "{}.{fraction}{suffix}",
                format_digit_string_with_commas(integer)
            )
        }
        None => format!("{}{suffix}", format_digit_string_with_commas(&rounded)),
    };

    match currency_display_symbol(currency) {
        Some(symbol) => format!("{sign}{symbol}{amount}"),
        None => {
            let currency = currency.trim();
            if currency.is_empty() {
                format!("{sign}{amount}")
            } else {
                format!("{} {sign}{amount}", currency.to_uppercase())
            }
        }
    }
}

fn currency_display_symbol(currency: &str) -> Option<&'static str> {
    match currency.trim().to_ascii_uppercase().as_str() {
        "USD" | "US DOLLAR" | "UNITED STATES DOLLAR" | "DOLLAR" => Some("$"),
        _ => None,
    }
}

pub(crate) fn format_signed_money(value: f64, currency: &str) -> String {
    if value >= 0.0 {
        format!("+{}", format_full_money(value, currency))
    } else {
        format_full_money(value, currency)
    }
}

fn format_digit_string_with_commas(digits: &str) -> String {
    let mut formatted = String::new();
    for (index, ch) in digits.chars().rev().enumerate() {
        if index > 0 && index % 3 == 0 {
            formatted.push(',');
        }
        formatted.push(ch);
    }
    formatted.chars().rev().collect()
}

pub(crate) fn format_number(value: f64, decimals: usize) -> String {
    let mut formatted = format!("{value:.decimals$}");
    if formatted.contains('.') {
        while formatted.ends_with('0') {
            formatted.pop();
        }
        if formatted.ends_with('.') {
            formatted.pop();
        }
    }
    formatted
}

pub(crate) fn enabled_label(value: bool) -> &'static str {
    if value {
        "Included"
    } else {
        "Excluded"
    }
}

pub(crate) fn spending_categories(spending: &SpendingOutput) -> Vec<SpendingBreakdownEntry> {
    let mut totals: Vec<SpendingBreakdownEntry> = Vec::new();
    for period in &spending.periods {
        for entry in &period.breakdown {
            let key = normalize_spending_category_key(&entry.key);
            if let Some(existing) = totals.iter_mut().find(|item| item.key == key) {
                let current = parse_money_input(&existing.total).unwrap_or_default();
                let next = parse_money_input(&entry.total).unwrap_or_default();
                existing.total = format_number(current + next, 2);
                existing.transaction_count += entry.transaction_count;
            } else {
                totals.push(SpendingBreakdownEntry {
                    key,
                    total: entry.total.clone(),
                    transaction_count: entry.transaction_count,
                });
            }
        }
    }
    totals.sort_by(|a, b| {
        let left = parse_money_input(&a.total).unwrap_or_default();
        let right = parse_money_input(&b.total).unwrap_or_default();
        right
            .partial_cmp(&left)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.key.cmp(&b.key))
    });
    totals
}

pub(crate) fn transaction_category_options(
    transactions: &[Transaction],
    categories: &[SpendingBreakdownEntry],
) -> Vec<String> {
    let mut options = categories
        .iter()
        .map(|entry| entry.key.clone())
        .chain(transactions.iter().map(transaction_category))
        .filter(|category| category != "Uncategorized")
        .collect::<Vec<_>>();
    options.sort_by(|a, b| compare_case_insensitive(a, b));
    options.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
    options
}

pub(crate) fn filtered_transactions(
    transactions: &[Transaction],
    category: Option<&str>,
    title_filter: &str,
    sort_field: TransactionSortField,
    sort_direction: SortDirection,
    show_ignored: bool,
) -> Vec<Transaction> {
    let normalized_title_filter = title_filter.trim().to_lowercase();
    let mut filtered = transactions
        .iter()
        .filter(|transaction| {
            if transaction.ignored_from_spending && !show_ignored {
                return false;
            }
            if !normalized_title_filter.is_empty()
                && !transaction_description(transaction)
                    .to_lowercase()
                    .contains(&normalized_title_filter)
            {
                return false;
            }
            category
                .map(|category| transaction_category(transaction) == category)
                .unwrap_or(true)
        })
        .cloned()
        .collect::<Vec<_>>();
    filtered.sort_by(|a, b| compare_transactions(a, b, sort_field, sort_direction));
    filtered
}

pub(crate) fn compare_transactions(
    a: &Transaction,
    b: &Transaction,
    sort_field: TransactionSortField,
    sort_direction: SortDirection,
) -> std::cmp::Ordering {
    let primary = match sort_field {
        TransactionSortField::Date => a.timestamp.cmp(&b.timestamp),
        TransactionSortField::Amount => compare_transaction_amounts(a, b),
        TransactionSortField::Description => {
            compare_case_insensitive(&transaction_description(a), &transaction_description(b))
        }
        TransactionSortField::Category => {
            compare_case_insensitive(&transaction_category(a), &transaction_category(b)).then_with(
                || {
                    compare_case_insensitive(
                        &transaction_subcategory(a).unwrap_or_default(),
                        &transaction_subcategory(b).unwrap_or_default(),
                    )
                },
            )
        }
        TransactionSortField::Account => compare_case_insensitive(&a.account_name, &b.account_name),
        TransactionSortField::Counted => a.ignored_from_spending.cmp(&b.ignored_from_spending),
    };

    let primary = match sort_direction {
        SortDirection::Asc => primary,
        SortDirection::Desc => primary.reverse(),
    };

    primary
        .then_with(|| b.timestamp.cmp(&a.timestamp))
        .then_with(|| a.account_name.cmp(&b.account_name))
        .then_with(|| a.id.cmp(&b.id))
}

pub(crate) fn ai_rule_transaction_input(transaction: &Transaction) -> AiRuleTransactionInput {
    AiRuleTransactionInput {
        id: transaction.id.clone(),
        account_id: transaction.account_id.clone(),
        account_name: transaction.account_name.clone(),
        timestamp: transaction.timestamp.clone(),
        description: transaction_description(transaction),
        amount: transaction.amount.clone(),
        status: transaction.status.clone(),
        category: transaction_category_value(transaction),
        subcategory: transaction_subcategory(transaction),
        ignored_from_spending: transaction.ignored_from_spending,
    }
}

pub(crate) fn ai_tool_label(name: &str) -> &'static str {
    match name {
        "propose_categorization_rule" => "Categorization rule",
        "propose_ignore_rule" => "Ignore rule",
        "propose_rename_rule" => "Rename rule",
        _ => "Tool call",
    }
}

pub(crate) fn format_json_value(value: &serde_json::Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
}

pub(crate) fn default_transaction_sort_direction(field: TransactionSortField) -> SortDirection {
    match field {
        TransactionSortField::Date | TransactionSortField::Amount => SortDirection::Desc,
        TransactionSortField::Description
        | TransactionSortField::Category
        | TransactionSortField::Account
        | TransactionSortField::Counted => SortDirection::Asc,
    }
}

pub(crate) fn sort_direction_arrow(direction: SortDirection) -> &'static str {
    match direction {
        SortDirection::Asc => "↑",
        SortDirection::Desc => "↓",
    }
}

pub(crate) fn compare_transaction_amounts(a: &Transaction, b: &Transaction) -> std::cmp::Ordering {
    let left = parse_money_input(&a.amount);
    let right = parse_money_input(&b.amount);
    match (left, right) {
        (Some(left), Some(right)) => left
            .partial_cmp(&right)
            .unwrap_or(std::cmp::Ordering::Equal),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => a.amount.cmp(&b.amount),
    }
}

pub(crate) fn compare_case_insensitive(a: &str, b: &str) -> std::cmp::Ordering {
    a.to_lowercase()
        .cmp(&b.to_lowercase())
        .then_with(|| a.cmp(b))
}

pub(crate) fn mark_transactions_excluded_from_spending(
    mut transactions: Vec<Transaction>,
    counted_transactions: &[Transaction],
) -> Vec<Transaction> {
    let counted_ids = counted_transactions
        .iter()
        .map(transaction_key)
        .collect::<HashSet<_>>();
    for transaction in &mut transactions {
        transaction.ignored_from_spending = !counted_ids.contains(&transaction_key(transaction))
            || !is_spending_transaction(transaction);
    }
    transactions
}

pub(crate) fn transaction_key(transaction: &Transaction) -> String {
    format!("{}:{}", transaction.account_id, transaction.id)
}

pub(crate) fn transaction_row_class(transaction: &Transaction) -> &'static str {
    if transaction.ignored_from_spending {
        "table-row ignored-transaction-row"
    } else {
        "table-row"
    }
}

pub(crate) fn is_spending_transaction(transaction: &Transaction) -> bool {
    transaction.status == "posted"
        && parse_money_input(&transaction.amount)
            .map(|amount| amount < 0.0)
            .unwrap_or(false)
}

pub(crate) fn normalize_spending_category_key(category: &str) -> String {
    let trimmed = category.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("uncategorized") {
        "Uncategorized".to_string()
    } else {
        trimmed.to_string()
    }
}

pub(crate) fn pie_slices(categories: &[SpendingBreakdownEntry]) -> Vec<PieSlice> {
    const COLORS: [&str; 10] = [
        "#1f6f8b", "#238a57", "#8a5cf6", "#bf6b21", "#b83280", "#52677a", "#2f9e9e", "#9b6a28",
        "#6f7d1f", "#bf3d3d",
    ];

    let values = categories
        .iter()
        .map(|entry| parse_money_input(&entry.total).unwrap_or_default().abs())
        .collect::<Vec<_>>();
    let total = values.iter().sum::<f64>();
    if total <= 0.0 {
        return Vec::new();
    }

    let mut cursor = -std::f64::consts::FRAC_PI_2;
    categories
        .iter()
        .zip(values.iter())
        .enumerate()
        .filter_map(|(index, (entry, value))| {
            if *value <= 0.0 {
                return None;
            }
            let angle = (*value / total) * std::f64::consts::TAU;
            let start = cursor;
            let end = cursor + angle;
            cursor = end;
            Some(PieSlice {
                key: entry.key.clone(),
                total: *value,
                transaction_count: entry.transaction_count,
                percentage: (*value / total) * 100.0,
                path: pie_slice_path(130.0, 130.0, 104.0, start, end),
                color: COLORS[index % COLORS.len()],
            })
        })
        .collect()
}

pub(crate) fn pie_slice_path(cx: f64, cy: f64, radius: f64, start: f64, end: f64) -> String {
    let start_x = cx + radius * start.cos();
    let start_y = cy + radius * start.sin();
    let end_x = cx + radius * end.cos();
    let end_y = cy + radius * end.sin();
    let large_arc = if end - start > std::f64::consts::PI {
        1
    } else {
        0
    };
    format!(
        "M {:.2} {:.2} L {:.2} {:.2} A {:.2} {:.2} 0 {} 1 {:.2} {:.2} Z",
        cx, cy, start_x, start_y, radius, radius, large_arc, end_x, end_y
    )
}

pub(crate) fn transaction_date(transaction: &Transaction) -> String {
    transaction
        .annotation
        .as_ref()
        .and_then(|annotation| annotation.effective_date.clone())
        .unwrap_or_else(|| {
            transaction
                .timestamp
                .get(..10)
                .unwrap_or(&transaction.timestamp)
                .to_string()
        })
}

pub(crate) fn transaction_description(transaction: &Transaction) -> String {
    transaction
        .annotation
        .as_ref()
        .and_then(|annotation| annotation.description.clone())
        .unwrap_or_else(|| transaction.description.clone())
}

pub(crate) fn transaction_category(transaction: &Transaction) -> String {
    let category = transaction
        .annotation
        .as_ref()
        .and_then(|annotation| annotation.category.clone())
        .or_else(|| transaction.category.clone())
        .unwrap_or_default();
    normalize_spending_category_key(&category)
}

pub(crate) fn transaction_category_value(transaction: &Transaction) -> Option<String> {
    let category = transaction_category(transaction);
    if category == "Uncategorized" {
        None
    } else {
        Some(category)
    }
}

pub(crate) fn transaction_subcategory(transaction: &Transaction) -> Option<String> {
    transaction
        .annotation
        .as_ref()
        .and_then(|annotation| annotation.subcategory.clone())
        .or_else(|| transaction.subcategory.clone())
        .map(|value| normalize_spending_category_key(&value))
        .filter(|value| !value.is_empty() && value != "Uncategorized")
}

pub(crate) fn format_transaction_amount(transaction: &Transaction, currency: &str) -> String {
    parse_money_input(&transaction.amount)
        .map(|amount| format_full_money(amount, currency))
        .unwrap_or_else(|| transaction.amount.clone())
}

pub(crate) fn git_settings_from_remote(remote: &str) -> Result<(String, String, String), String> {
    let trimmed = remote.trim();
    if trimmed.is_empty() {
        return Err("Enter a remote.".to_string());
    }

    if is_explicit_git_remote(trimmed) {
        return Ok((
            remote_host(trimmed).unwrap_or_else(|| "github.com".to_string()),
            trimmed.to_string(),
            remote_user(trimmed).unwrap_or_else(|| "git".to_string()),
        ));
    }

    normalize_github_repo_input(trimmed)
        .map(|repo| ("github.com".to_string(), repo, "git".to_string()))
}

pub(crate) fn remote_input_from_settings(host: &str, repo: &str, ssh_user: &str) -> String {
    let repo = repo.trim();
    if repo.is_empty() {
        return String::new();
    }
    if is_explicit_git_remote(repo) {
        return repo.to_string();
    }

    let host = non_empty_client(host, "github.com");
    let ssh_user = non_empty_client(ssh_user, "git");
    let repo = if repo.ends_with(".git") {
        repo.to_string()
    } else {
        format!("{repo}.git")
    };
    format!("{ssh_user}@{host}:{repo}")
}

pub(crate) fn non_empty_client(value: &str, default: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        default.to_string()
    } else {
        trimmed.to_string()
    }
}

pub(crate) fn is_explicit_git_remote(remote: &str) -> bool {
    remote.contains("://") || (remote.contains('@') && remote.contains(':'))
}

pub(crate) fn remote_user(remote: &str) -> Option<String> {
    remote
        .split('@')
        .next()
        .and_then(|prefix| prefix.rsplit(['/', ':']).next())
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

pub(crate) fn remote_host(remote: &str) -> Option<String> {
    let without_scheme = remote.split("://").nth(1).unwrap_or(remote);
    let after_user = without_scheme.split('@').nth(1).unwrap_or(without_scheme);
    after_user
        .split([':', '/'])
        .next()
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

pub(crate) fn normalize_github_repo_input(input: &str) -> Result<String, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("Enter a repository as owner/repo.".to_string());
    }

    let repo = trim_github_repo_prefix(trimmed)
        .trim_matches('/')
        .strip_suffix(".git")
        .unwrap_or_else(|| trim_github_repo_prefix(trimmed).trim_matches('/'));

    let mut parts = repo.split('/');
    let Some(owner) = parts.next() else {
        return Err("Enter a repository as owner/repo.".to_string());
    };
    let Some(name) = parts.next() else {
        return Err("Enter a repository as owner/repo.".to_string());
    };
    if owner.is_empty() || name.is_empty() || parts.next().is_some() {
        return Err("Enter a repository as owner/repo.".to_string());
    }

    Ok(format!("{owner}/{name}"))
}

pub(crate) fn trim_github_repo_prefix(input: &str) -> &str {
    input
        .strip_prefix("https://github.com/")
        .or_else(|| input.strip_prefix("http://github.com/"))
        .or_else(|| input.strip_prefix("git@github.com:"))
        .unwrap_or(input)
}

pub(crate) fn short_commit(commit: &str) -> String {
    commit.chars().take(12).collect()
}

pub(crate) fn sync_result_summary(result: &serde_json::Value) -> String {
    if let Some(results) = result.get("results").and_then(|value| value.as_array()) {
        let total = results.len();
        let synced = results
            .iter()
            .filter(|row| row.get("success").and_then(|v| v.as_bool()) == Some(true))
            .count();
        let failed = results
            .iter()
            .filter(|row| row.get("success").and_then(|v| v.as_bool()) == Some(false))
            .count();
        let skipped = results
            .iter()
            .filter(|row| row.get("skipped").and_then(|v| v.as_bool()) == Some(true))
            .count();
        return format!("Sync complete: {synced}/{total} ok, {skipped} skipped, {failed} failed.");
    }

    let connection = result
        .get("connection")
        .and_then(|value| {
            value
                .as_str()
                .or_else(|| value.get("name").and_then(|v| v.as_str()))
        })
        .unwrap_or("connection");
    if result.get("success").and_then(|v| v.as_bool()) == Some(true) {
        if result.get("skipped").and_then(|v| v.as_bool()) == Some(true) {
            let reason = result
                .get("reason")
                .and_then(|value| value.as_str())
                .unwrap_or("skipped");
            format!("Sync skipped for {connection}: {reason}.")
        } else {
            format!("Sync complete for {connection}.")
        }
    } else {
        let error = result
            .get("error")
            .and_then(|value| value.as_str())
            .unwrap_or("unknown error");
        format!("Sync failed for {connection}: {error}")
    }
}

pub(crate) fn price_sync_result_summary(result: &serde_json::Value) -> String {
    let Some(refresh) = result.get("result") else {
        return "Price sync finished.".to_string();
    };
    let fetched = refresh
        .get("fetched")
        .and_then(|value| value.as_u64())
        .unwrap_or(0);
    let skipped = refresh
        .get("skipped")
        .and_then(|value| value.as_u64())
        .unwrap_or(0);
    let failed = refresh
        .get("failed_count")
        .and_then(|value| value.as_u64())
        .unwrap_or(0);

    if failed == 0 {
        format!("Price sync complete: {fetched} fetched, {skipped} skipped.")
    } else {
        format!("Price sync complete: {fetched} fetched, {skipped} skipped, {failed} failed.")
    }
}

pub(crate) fn transaction_query_string(start: &str, end: &str, include_ignored: bool) -> String {
    let mut params = Vec::new();
    if !start.trim().is_empty() {
        push_query_param(&mut params, "start", start);
    }
    if !end.trim().is_empty() {
        push_query_param(&mut params, "end", end);
    }
    if include_ignored {
        push_query_param(&mut params, "include_ignored", "true");
    }
    params.join("&")
}

pub(crate) fn proposed_patch_summary(patch: &ProposedTransactionEditPatch) -> String {
    let mut parts = Vec::new();
    push_patch_part(&mut parts, "description", &patch.description);
    push_patch_part(&mut parts, "note", &patch.note);
    push_patch_part(&mut parts, "category", &patch.category);
    if let Some(value) = &patch.tags {
        match value {
            Some(tags) => parts.push(format!("tags={}", tags.join(", "))),
            None => parts.push("tags=clear".to_string()),
        }
    }
    push_patch_part(&mut parts, "effective_date", &patch.effective_date);
    if parts.is_empty() {
        "No changes".to_string()
    } else {
        parts.join("; ")
    }
}

pub(crate) fn push_patch_part(
    parts: &mut Vec<String>,
    label: &str,
    value: &Option<Option<String>>,
) {
    if let Some(value) = value {
        match value {
            Some(text) => parts.push(format!("{label}={text}")),
            None => parts.push(format!("{label}=clear")),
        }
    }
}

pub(crate) fn proposal_action_past_tense(action: &str) -> &'static str {
    match action {
        "approve" => "Approved",
        "reject" => "Rejected",
        "remove" => "Removed",
        _ => "Updated",
    }
}
