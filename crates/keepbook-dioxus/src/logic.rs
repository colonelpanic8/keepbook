use super::*;
use std::collections::{HashMap, HashSet};

impl FilterOverrides {
    pub(crate) fn account_exclude_override(&self, account_id: &str) -> Option<bool> {
        self.account_portfolio_exclusions
            .iter()
            .find(|override_entry| override_entry.account_id == account_id)
            .map(|override_entry| override_entry.exclude_from_portfolio)
    }

    pub(crate) fn with_account_exclude_override(
        mut self,
        account_id: String,
        exclude_from_portfolio: bool,
    ) -> Self {
        if let Some(override_entry) = self
            .account_portfolio_exclusions
            .iter_mut()
            .find(|override_entry| override_entry.account_id == account_id)
        {
            override_entry.exclude_from_portfolio = exclude_from_portfolio;
        } else {
            self.account_portfolio_exclusions
                .push(AccountPortfolioExclusionOverride {
                    account_id,
                    exclude_from_portfolio,
                });
        }
        self
    }

    pub(crate) fn without_account_exclude_override(mut self, account_id: &str) -> Self {
        self.account_portfolio_exclusions
            .retain(|override_entry| override_entry.account_id != account_id);
        self
    }
}

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

pub(crate) fn history_data_points_with_current_snapshot(
    history: &History,
    local_today: &str,
    current_value: f64,
) -> Vec<NetWorthDataPoint> {
    let mut points = history_data_points(history);
    points.retain(|point| point.date.as_str() <= local_today);

    if !current_value.is_finite() {
        return points;
    }

    if let Some(point) = points.last_mut().filter(|point| point.date == local_today) {
        point.value = current_value;
    } else {
        points.push(NetWorthDataPoint {
            date: local_today.to_string(),
            value: current_value,
        });
    }

    points
}

pub(crate) fn stacked_history_data_points(
    history: &StackedHistory,
) -> Vec<StackedHistoryDataPoint> {
    let mut points = history
        .points
        .iter()
        .filter_map(|point| {
            let total = point
                .total_value
                .parse::<f64>()
                .ok()
                .filter(|value| value.is_finite())?;
            let components = point
                .components
                .iter()
                .filter_map(|component| {
                    component
                        .value
                        .parse::<f64>()
                        .ok()
                        .filter(|value| value.is_finite())
                        .map(|value| StackedValue {
                            series_key: component.series_key.clone(),
                            value,
                        })
                })
                .collect::<Vec<_>>();
            Some(StackedHistoryDataPoint {
                date: point.date.clone(),
                total,
                components,
            })
        })
        .collect::<Vec<_>>();
    points.sort_by(|a, b| a.date.cmp(&b.date));
    points
}

pub(crate) fn coalesce_minor_stacked_series(
    data: &[StackedHistoryDataPoint],
    series: &[ActiveStackedSeries],
    threshold_percent: f64,
) -> (Vec<StackedHistoryDataPoint>, Vec<ActiveStackedSeries>) {
    if data.is_empty()
        || series.is_empty()
        || !threshold_percent.is_finite()
        || threshold_percent <= 0.0
    {
        return (data.to_vec(), series.to_vec());
    }

    let threshold = threshold_percent / 100.0;
    let mut major_series = Vec::new();
    let mut minor_keys = HashSet::new();

    for item in series {
        let max_share = data
            .iter()
            .map(|point| {
                let value = point
                    .components
                    .iter()
                    .find(|component| component.series_key == item.key)
                    .map(|component| component.value)
                    .unwrap_or_default()
                    .abs();
                let total = point.total.abs();
                if total <= f64::EPSILON {
                    if value <= f64::EPSILON {
                        0.0
                    } else {
                        f64::INFINITY
                    }
                } else {
                    value / total
                }
            })
            .fold(0.0_f64, f64::max);

        if max_share < threshold {
            minor_keys.insert(item.key.clone());
        } else {
            major_series.push(item.clone());
        }
    }

    if minor_keys.is_empty() {
        return (data.to_vec(), series.to_vec());
    }

    let other_key = "__other_minor_contributions".to_string();
    major_series.push(ActiveStackedSeries {
        key: other_key.clone(),
        label: format!("Other <{threshold_percent}%"),
        account_id: None,
        series_type: "other".to_string(),
    });

    let coalesced_data = data
        .iter()
        .map(|point| {
            let mut other_value = 0.0;
            let mut components = point
                .components
                .iter()
                .filter_map(|component| {
                    if minor_keys.contains(&component.series_key) {
                        other_value += component.value;
                        None
                    } else {
                        Some(component.clone())
                    }
                })
                .collect::<Vec<_>>();
            if other_value.abs() > f64::EPSILON {
                components.push(StackedValue {
                    series_key: other_key.clone(),
                    value: other_value,
                });
            }
            StackedHistoryDataPoint {
                date: point.date.clone(),
                total: point.total,
                components,
            }
        })
        .collect::<Vec<_>>();

    (coalesced_data, major_series)
}

/// A point on a dated series. The range, sampling, and endpoint helpers below
/// only ever need the date, so they work over this rather than being written
/// once per concrete point type.
pub(crate) trait DatedSeriesPoint: Clone {
    fn date(&self) -> &str;
}

impl DatedSeriesPoint for NetWorthDataPoint {
    fn date(&self) -> &str {
        &self.date
    }
}

impl DatedSeriesPoint for StackedHistoryDataPoint {
    fn date(&self) -> &str {
        &self.date
    }
}

pub(crate) fn date_bounds<P: DatedSeriesPoint>(points: &[P]) -> Option<(String, String)> {
    Some((
        points.first()?.date().to_string(),
        points.last()?.date().to_string(),
    ))
}

pub(crate) fn visible_date_range<P: DatedSeriesPoint>(
    points: &[P],
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
    // The request itself omits the end bound so the server never trims freshly
    // synced change points on local/UTC date mismatches, but auto-granularity
    // selection still assumes the range runs through today.
    let granularity = history_request_granularity(
        selected_sampling,
        start.as_deref(),
        end.as_deref().or(Some(today)),
    );
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
    spending_group_query_string(
        preset,
        start_override,
        end_override,
        today,
        currency,
        "tag",
        None,
    )
}

pub(crate) fn spending_description_query_string(
    preset: RangePreset,
    start_override: &str,
    end_override: &str,
    today: &str,
    currency: &str,
    group_by: &str,
    top: usize,
) -> String {
    spending_group_query_string(
        preset,
        start_override,
        end_override,
        today,
        currency,
        group_by,
        Some(top),
    )
}

fn spending_group_query_string(
    preset: RangePreset,
    start_override: &str,
    end_override: &str,
    today: &str,
    currency: &str,
    group_by: &str,
    top: Option<usize>,
) -> String {
    let (start, end) = requested_spending_date_range(preset, start_override, end_override, today);
    let mut params = vec![
        "period=range".to_string(),
        format!("group_by={}", query_encode_component(group_by)),
        "direction=outflow".to_string(),
        "status=posted".to_string(),
    ];
    if let Some(top) = top {
        params.push(format!("top={top}"));
    }
    push_query_param(&mut params, "currency", currency);
    if let Some(start) = start {
        push_query_param(&mut params, "start", &start);
    }
    if let Some(end) = end {
        push_query_param(&mut params, "end", &end);
    }
    params.join("&")
}

pub(crate) fn spending_over_time_query_string(
    preset: RangePreset,
    start_override: &str,
    end_override: &str,
    today: &str,
    currency: &str,
    bucket: SpendingBucket,
) -> String {
    let (start, end) = requested_spending_date_range(preset, start_override, end_override, today);
    let mut params = vec![
        format!("period={}", bucket.query_value()),
        "period_alignment=calendar".to_string(),
        "group_by=tag".to_string(),
        "direction=outflow".to_string(),
        "status=posted".to_string(),
        "include_empty=true".to_string(),
    ];
    push_query_param(&mut params, "currency", currency);
    if let Some(start) = start {
        push_query_param(&mut params, "start", &start);
    }
    if let Some(end) = end {
        push_query_param(&mut params, "end", &end);
    }
    params.join("&")
}

/// Query string for the asset breakdown endpoint. Assets only honor the
/// account include/exclude overrides; the latent-tax override is a synthetic
/// account and never applies to per-asset rows.
pub(crate) fn assets_query_string(
    overrides: FilterOverrides,
    include_amount_changes: bool,
) -> String {
    let mut params = Vec::new();
    if include_amount_changes {
        params.push("include_amount_changes=true".to_string());
    }
    append_filter_override_params(
        &mut params,
        FilterOverrides {
            include_latent_capital_gains_tax: None,
            account_portfolio_exclusions: overrides.account_portfolio_exclusions,
        },
    );
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
    let mut account_overrides = overrides.account_portfolio_exclusions;
    account_overrides.sort_by(|a, b| a.account_id.cmp(&b.account_id));
    if !account_overrides.is_empty() {
        if let Ok(encoded) = serde_json::to_string(&account_overrides) {
            push_query_param(params, "account_portfolio_overrides", &encoded);
        }
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
            non_empty_string(end_override),
        );
    }

    // Presets mean "from the computed start until now", so no end bound is
    // sent. Sending `today` computed from the local timezone used to trim
    // change points stamped with tomorrow's UTC date off the end of charts.
    let start = match preset {
        RangePreset::OneMonth => Some(offset_months(today, 1)),
        RangePreset::NinetyDays => Some(offset_days(today, 90)),
        RangePreset::SixMonths => Some(offset_months(today, 6)),
        RangePreset::OneYear => Some(offset_years(today, 1)),
        RangePreset::TwoYears => Some(offset_years(today, 2)),
        RangePreset::Max | RangePreset::Custom => None,
    };

    (start, None)
}

pub(crate) fn requested_spending_date_range(
    preset: RangePreset,
    start_override: &str,
    end_override: &str,
    today: &str,
) -> (Option<String>, Option<String>) {
    let (start, end) = requested_history_date_range(preset, start_override, end_override, today);
    // Spending queries keep their explicit end bound (today for presets, and a
    // today fallback for custom ranges); only Max stays unbounded.
    if preset == RangePreset::Max {
        (start, end)
    } else {
        (start, end.or_else(|| Some(today.to_string())))
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

pub(crate) fn filter_data_by_date_range<P: DatedSeriesPoint>(
    points: &[P],
    start_date: &str,
    end_date: &str,
) -> Vec<P> {
    if start_date.is_empty() || end_date.is_empty() || start_date > end_date {
        return Vec::new();
    }

    points
        .iter()
        .filter(|point| point.date() >= start_date && point.date() <= end_date)
        .cloned()
        .collect()
}

pub(crate) fn resolve_sampling_granularity<P: DatedSeriesPoint>(
    selected: SamplingGranularity,
    points: &[P],
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

    match days_between(first.date(), last.date()) {
        Some(days) if days < 93 => SamplingGranularity::Daily,
        Some(days) if days > 365 * 3 => SamplingGranularity::Monthly,
        Some(_) => SamplingGranularity::Weekly,
        _ => SamplingGranularity::Daily,
    }
}

pub(crate) fn sample_data_by_granularity<P: DatedSeriesPoint>(
    points: &[P],
    granularity: SamplingGranularity,
) -> Vec<P> {
    if matches!(
        granularity,
        SamplingGranularity::Auto | SamplingGranularity::Daily
    ) || points.len() <= 2
    {
        return points.to_vec();
    }

    let mut sampled = Vec::new();
    let mut current_bucket: Option<String> = None;
    let mut current_point: Option<P> = None;

    for point in points {
        let bucket = sampling_bucket(point.date(), granularity);
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

pub(crate) fn include_range_endpoints<P: DatedSeriesPoint>(
    points: &[P],
    sampled: Vec<P>,
) -> Vec<P> {
    let Some(first) = points.first() else {
        return sampled;
    };
    let Some(last) = points.last() else {
        return sampled;
    };

    let mut with_endpoints = sampled;
    if !with_endpoints
        .iter()
        .any(|point| point.date() == first.date())
    {
        with_endpoints.push(first.clone());
    }
    if !with_endpoints
        .iter()
        .any(|point| point.date() == last.date())
    {
        with_endpoints.push(last.clone());
    }
    with_endpoints.sort_by(|a, b| a.date().cmp(b.date()));
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

/// The app's decimal text for an account's snapshot value, kept as text so
/// display sites format it without an f64 round trip.
pub(crate) fn account_snapshot_value_text(
    account_id: &str,
    account_summaries: &[AccountSummary],
) -> Option<String> {
    account_summaries
        .iter()
        .find(|summary| summary.account_id == account_id)
        .and_then(|summary| summary.value_in_base.clone())
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

    apply_currency_display(&amount, currency, value < 0.0)
}

fn apply_currency_display(amount: &str, currency: &str, negative: bool) -> String {
    let sign = if negative { "-" } else { "" };
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

/// Canonical decimal text as produced by the app layer: an optional sign, then
/// integer and fraction digit runs. Keeping money in this form lets the UI
/// render app values without an f64 round trip.
struct DecimalText<'a> {
    negative: bool,
    integer: &'a str,
    fraction: &'a str,
}

fn parse_decimal_text(value: &str) -> Option<DecimalText<'_>> {
    let trimmed = value.trim();
    let (negative, digits) = match trimmed.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, trimmed.strip_prefix('+').unwrap_or(trimmed)),
    };
    let (integer, fraction) = digits.split_once('.').unwrap_or((digits, ""));
    if integer.is_empty() && fraction.is_empty() {
        return None;
    }
    if !integer.bytes().all(|byte| byte.is_ascii_digit())
        || !fraction.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    Some(DecimalText {
        negative,
        integer,
        fraction,
    })
}

/// Round the digits of a [`DecimalText`] to `decimals` places, half away from
/// zero, returning the integer and fraction digit runs.
fn round_decimal_digits(value: &DecimalText<'_>, decimals: usize) -> (String, String) {
    let mut digits: Vec<u8> = value
        .integer
        .bytes()
        .chain(value.fraction.bytes())
        .map(|byte| byte - b'0')
        .collect();
    let keep = value.integer.len() + decimals;
    let round_up = digits.get(keep).is_some_and(|digit| *digit >= 5);
    digits.truncate(keep);
    digits.resize(keep, 0);
    if round_up {
        let mut index = digits.len();
        loop {
            if index == 0 {
                digits.insert(0, 1);
                break;
            }
            index -= 1;
            if digits[index] == 9 {
                digits[index] = 0;
            } else {
                digits[index] += 1;
                break;
            }
        }
    }

    let split = digits.len() - decimals;
    let render = |run: &[u8]| run.iter().map(|digit| (digit + b'0') as char).collect();
    let integer: String = render(&digits[..split]);
    let fraction: String = render(&digits[split..]);
    let trimmed = integer.trim_start_matches('0');
    (
        if trimmed.is_empty() {
            "0".to_string()
        } else {
            trimmed.to_string()
        },
        fraction,
    )
}

fn is_zero_digits(integer: &str, fraction: &str) -> bool {
    integer.bytes().chain(fraction.bytes()).all(|b| b == b'0')
}

/// Round app decimal text to `decimals` places and strip trailing fraction
/// zeros: the string-domain counterpart of [`format_number`].
pub(crate) fn format_decimal_text(value: &str, decimals: usize) -> Option<String> {
    let parsed = parse_decimal_text(value)?;
    let (integer, fraction) = round_decimal_digits(&parsed, decimals);
    let fraction = fraction.trim_end_matches('0');
    let sign = if parsed.negative && !is_zero_digits(&integer, fraction) {
        "-"
    } else {
        ""
    };
    Some(if fraction.is_empty() {
        format!("{sign}{integer}")
    } else {
        format!("{sign}{integer}.{fraction}")
    })
}

/// Currency text for app decimal money, matching [`format_full_money`] without
/// the f64 round trip. `None` when the text is not a decimal.
pub(crate) fn format_money_text(value: &str, currency: &str) -> Option<String> {
    let parsed = parse_decimal_text(value)?;
    let (integer, fraction) = round_decimal_digits(&parsed, 2);
    let amount = format!("{}.{fraction}", format_digit_string_with_commas(&integer));
    Some(apply_currency_display(&amount, currency, parsed.negative))
}

/// [`format_money_text`] with an explicit `+` on non-negative amounts.
pub(crate) fn format_signed_money_text(value: &str, currency: &str) -> Option<String> {
    let formatted = format_money_text(value, currency)?;
    Some(if parse_decimal_text(value)?.negative {
        formatted
    } else {
        format!("+{formatted}")
    })
}

/// Signed percentage text for app decimal percentages, using the same sign
/// convention as [`format_signed_money_text`]: non-negative values get a `+`.
pub(crate) fn format_signed_percent_text(value: &str) -> Option<String> {
    let formatted = format_decimal_text(value, 2)?;
    Some(if formatted.starts_with('-') {
        format!("{formatted}%")
    } else {
        format!("+{formatted}%")
    })
}

/// The chart's "Range change" readout, rendered from the app's history summary.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct HistoryChangeSummary {
    pub(crate) class: &'static str,
    pub(crate) text: String,
}

pub(crate) fn history_change_summary(
    summary: Option<&HistorySummary>,
    currency: &str,
) -> HistoryChangeSummary {
    let Some(summary) = summary else {
        return HistoryChangeSummary {
            class: "",
            text: "No range change".to_string(),
        };
    };
    let absolute = format_signed_money_text(&summary.absolute_change, currency)
        .unwrap_or_else(|| summary.absolute_change.clone());
    // The app reports "N/A" when the range starts at zero; pass that through
    // rather than inventing a percentage.
    let percentage = format_decimal_text(&summary.percentage_change, 2)
        .map(|percentage| format!("{percentage}%"))
        .unwrap_or_else(|| summary.percentage_change.clone());
    HistoryChangeSummary {
        class: signed_change_class_text(&summary.absolute_change),
        text: format!("{absolute} ({percentage})"),
    }
}

/// Gain/loss coloring class for a change readout that always takes a side, the
/// way the chart's range change does: zero reads as a gain.
pub(crate) fn signed_change_class_text(value: &str) -> &'static str {
    if change_value_class_text(value) == "change-negative" {
        "change-negative"
    } else {
        "change-positive"
    }
}

/// Compact currency text (`$1.6K`) for app decimal money, matching
/// [`format_compact_money`] without the f64 round trip.
pub(crate) fn format_compact_money_text(value: &str, currency: &str) -> Option<String> {
    let parsed = parse_decimal_text(value)?;
    let integer = parsed.integer.trim_start_matches('0');
    let (shift, suffix) = match integer.len() {
        digits if digits > 9 => (9, "B"),
        digits if digits > 6 => (6, "M"),
        digits if digits > 3 => (3, "K"),
        _ => (0, ""),
    };
    let split = integer.len() - shift;
    let scaled_integer = &integer[..split];
    let scaled_fraction = format!("{}{}", &integer[split..], parsed.fraction);
    let (integer, fraction) = round_decimal_digits(
        &DecimalText {
            negative: parsed.negative,
            integer: scaled_integer,
            fraction: &scaled_fraction,
        },
        1,
    );
    let amount = format!(
        "{}.{fraction}{suffix}",
        format_digit_string_with_commas(&integer)
    );
    Some(apply_currency_display(&amount, currency, parsed.negative))
}

/// Gain/loss coloring class for app decimal text. Zero stays neutral.
pub(crate) fn change_value_class_text(value: &str) -> &'static str {
    match parse_decimal_text(value) {
        Some(parsed) if is_zero_digits(parsed.integer, parsed.fraction) => "",
        Some(parsed) if parsed.negative => "change-negative",
        Some(_) => "change-positive",
        None => "",
    }
}

pub(crate) fn enabled_label(value: bool) -> &'static str {
    if value {
        "Included"
    } else {
        "Excluded"
    }
}

/// Sort key for a breakdown row: the app's total, read only for ordering.
fn spending_entry_order(entry: &SpendingBreakdownEntry) -> f64 {
    parse_money_input(&entry.total).unwrap_or_default()
}

/// Per-key totals for a `period=range` spending report. The single range
/// period's breakdown already holds the range total for each key, so this only
/// relabels and orders it.
pub(crate) fn spending_tags(spending: &SpendingOutput) -> Vec<SpendingBreakdownEntry> {
    let mut totals = spending
        .periods
        .iter()
        .flat_map(|period| &period.breakdown)
        .map(|entry| SpendingBreakdownEntry {
            key: normalize_spending_tag_key(&entry.key),
            total: entry.total.clone(),
            transaction_count: entry.transaction_count,
        })
        .collect::<Vec<_>>();
    totals.sort_by(|a, b| {
        spending_entry_order(b)
            .partial_cmp(&spending_entry_order(a))
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.key.cmp(&b.key))
    });
    totals
}

/// Per-key totals for a `period=range` report grouped by something other than
/// tag (merchant, fuzzy merchant), keeping the app's keys verbatim.
pub(crate) fn spending_breakdown_entries(spending: &SpendingOutput) -> Vec<SpendingBreakdownEntry> {
    let mut totals = spending
        .periods
        .iter()
        .flat_map(|period| &period.breakdown)
        .filter(|entry| !entry.key.trim().is_empty())
        .map(|entry| SpendingBreakdownEntry {
            key: entry.key.trim().to_string(),
            total: entry.total.clone(),
            transaction_count: entry.transaction_count,
        })
        .collect::<Vec<_>>();
    totals.sort_by(|a, b| {
        spending_entry_order(b)
            .partial_cmp(&spending_entry_order(a))
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| compare_case_insensitive(&a.key, &b.key))
    });
    totals
}

pub(crate) fn spending_over_time_points(spending: &SpendingOutput) -> Vec<SpendingBarChartPoint> {
    spending
        .periods
        .iter()
        .map(|period| {
            let mut segments = period
                .breakdown
                .iter()
                .filter_map(|entry| {
                    let value = parse_money_input(&entry.total)?.abs();
                    if value <= 0.0 {
                        return None;
                    }
                    Some(SpendingBarSegment {
                        key: normalize_spending_tag_key(&entry.key),
                        value,
                        transaction_count: entry.transaction_count,
                    })
                })
                .collect::<Vec<_>>();
            segments.sort_by(|a, b| {
                b.value
                    .partial_cmp(&a.value)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| a.key.cmp(&b.key))
            });
            let total = parse_money_input(&period.total).unwrap_or_default().abs();
            SpendingBarChartPoint {
                label: spending_period_label(&period.start_date, &period.end_date),
                start_date: period.start_date.clone(),
                end_date: period.end_date.clone(),
                total,
                transaction_count: period.transaction_count,
                segments,
            }
        })
        .collect()
}

pub(crate) fn narrow_spending_points_to_tag(
    points: &[SpendingBarChartPoint],
    tag: &str,
) -> Vec<SpendingBarChartPoint> {
    points
        .iter()
        .map(|point| {
            // A bucket breakdown holds at most one entry per key, so the tag's
            // bucket total is that entry's, never a sum.
            let segment = point
                .segments
                .iter()
                .find(|segment| segment.key == tag)
                .cloned();
            SpendingBarChartPoint {
                label: point.label.clone(),
                start_date: point.start_date.clone(),
                end_date: point.end_date.clone(),
                total: segment.as_ref().map(|segment| segment.value).unwrap_or(0.0),
                transaction_count: segment
                    .as_ref()
                    .map(|segment| segment.transaction_count)
                    .unwrap_or(0),
                segments: segment.into_iter().collect(),
            }
        })
        .collect()
}

pub(crate) fn visible_spending_over_time_points(
    points: &[SpendingBarChartPoint],
    series: &[SpendingBreakdownEntry],
) -> Vec<SpendingBarChartPoint> {
    if series.is_empty() {
        Vec::new()
    } else {
        points.to_vec()
    }
}

pub(crate) fn spending_segment_tooltip_detail(
    value: f64,
    period_total: f64,
    currency: &str,
    bucket_label: &str,
) -> String {
    let period_noun = match bucket_label {
        "Daily" => "day",
        "Weekly" => "week",
        "Monthly" => "month",
        "Quarterly" => "quarter",
        "Yearly" => "year",
        _ => "period",
    };
    format!(
        "{} category · {} {period_noun} total",
        format_full_money(value, currency),
        format_full_money(period_total, currency),
    )
}

pub(crate) fn spending_tooltip_layout(
    title: &str,
    detail: &str,
    preferred_center_x: f64,
    chart_width: f64,
) -> (f64, f64) {
    const EDGE_GAP: f64 = 8.0;
    const MIN_WIDTH: f64 = 184.0;
    const HORIZONTAL_PADDING: f64 = 28.0;
    const TITLE_CHAR_WIDTH: f64 = 8.5;
    const DETAIL_CHAR_WIDTH: f64 = 7.5;

    let title_width = title.chars().count() as f64 * TITLE_CHAR_WIDTH;
    let detail_width = detail.chars().count() as f64 * DETAIL_CHAR_WIDTH;
    let max_width = (chart_width - EDGE_GAP * 2.0).max(MIN_WIDTH);
    let tooltip_width =
        (title_width.max(detail_width) + HORIZONTAL_PADDING).clamp(MIN_WIDTH, max_width);
    let half_width = tooltip_width / 2.0;
    let center_x =
        preferred_center_x.clamp(EDGE_GAP + half_width, chart_width - EDGE_GAP - half_width);

    (tooltip_width, center_x)
}

fn spending_period_label(start: &str, end: &str) -> String {
    if start == end {
        return start.to_string();
    }
    if start.get(..4) == end.get(..4) && start.ends_with("-01-01") && end.ends_with("-12-31") {
        return start.get(..4).unwrap_or(start).to_string();
    }
    if start.get(..7) == end.get(..7) {
        return start.get(..7).unwrap_or(start).to_string();
    }
    format!("{start} to {end}")
}

pub(crate) fn transaction_tag_options(
    transactions: &[Transaction],
    tags: &[SpendingBreakdownEntry],
) -> Vec<String> {
    let mut options = tags
        .iter()
        .map(|entry| entry.key.clone())
        .chain(transactions.iter().flat_map(transaction_tags))
        .filter(|tag| tag != "Untagged" && !is_ignore_spending_tag(tag))
        .collect::<Vec<_>>();
    options.sort_by(|a, b| compare_case_insensitive(a, b));
    options.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
    options
}

pub(crate) fn filtered_transactions(
    transactions: &[Transaction],
    selected_tag: Option<&str>,
    selected_period: Option<(&str, &str)>,
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
            if let Some((start_date, end_date)) = selected_period {
                let date = transaction_date(transaction);
                if date.as_str() < start_date || date.as_str() > end_date {
                    return false;
                }
            }
            if !normalized_title_filter.is_empty()
                && !transaction_description(transaction)
                    .to_lowercase()
                    .contains(&normalized_title_filter)
            {
                return false;
            }
            selected_tag
                .map(|tag| transaction_has_tag(transaction, tag))
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
        TransactionSortField::Tag => {
            compare_case_insensitive(&transaction_tags_label(a), &transaction_tags_label(b))
        }
        TransactionSortField::Account => compare_case_insensitive(&a.account_name, &b.account_name),
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
        tag: transaction_tags(transaction).first().cloned(),
        subtag: transaction_subtags(transaction).first().cloned(),
        ignored_from_spending: transaction.ignored_from_spending,
    }
}

pub(crate) fn ai_tool_label(name: &str) -> &'static str {
    match name {
        "propose_tag_rule" => "Tag rule",
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
        | TransactionSortField::Tag
        | TransactionSortField::Account => SortDirection::Asc,
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

fn asset_string_field<'a>(asset: &'a serde_json::Value, key: &str) -> Option<&'a str> {
    asset
        .get(key)
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

/// Primary display name for an asset breakdown row: currency ISO code, equity
/// ticker, crypto symbol, or manual-value name, falling back to the asset id.
pub(crate) fn asset_display_name(asset: &serde_json::Value, fallback: &str) -> String {
    let name = match asset_string_field(asset, "type") {
        Some("currency") => asset_string_field(asset, "iso_code"),
        Some("equity") => asset_string_field(asset, "ticker"),
        Some("crypto") => asset_string_field(asset, "symbol"),
        Some("manual_value") => asset_string_field(asset, "name"),
        _ => None,
    };
    name.unwrap_or(fallback).to_string()
}

/// Secondary detail for an asset row: an equity's exchange or a crypto
/// asset's network, when present.
pub(crate) fn asset_secondary_text(asset: &serde_json::Value) -> Option<String> {
    match asset_string_field(asset, "type") {
        Some("equity") => asset_string_field(asset, "exchange"),
        Some("crypto") => asset_string_field(asset, "network"),
        _ => None,
    }
    .map(str::to_string)
}

pub(crate) fn asset_kind_label(asset: &serde_json::Value) -> &'static str {
    match asset_string_field(asset, "type") {
        Some("currency") => "Currency",
        Some("equity") => "Equity",
        Some("crypto") => "Crypto",
        Some("manual_value") => "Manual",
        _ => "Asset",
    }
}

/// Display form of an asset holding amount: rounded and trimmed like the rest
/// of the app's decimal output, keeping the raw text when unparseable.
pub(crate) fn format_asset_amount(amount: &str) -> String {
    format_decimal_text(amount, 4).unwrap_or_else(|| amount.to_string())
}

/// Expansion-state key for an asset breakdown row. Asset and liability rows
/// for the same asset are distinct rows, so the flag is part of the key.
pub(crate) fn asset_expansion_key(entry: &AssetBreakdownEntry) -> String {
    format!(
        "{}:{}",
        entry.asset_id,
        if entry.liability {
            "liability"
        } else {
            "asset"
        }
    )
}

pub(crate) fn default_asset_sort_direction(field: AssetSortField) -> SortDirection {
    match field {
        AssetSortField::Name => SortDirection::Asc,
        AssetSortField::Amount
        | AssetSortField::AmountChecked
        | AssetSortField::AmountChanged
        | AssetSortField::PriceUpdated
        | AssetSortField::Value
        | AssetSortField::DayChange
        | AssetSortField::WeekChange
        | AssetSortField::MonthChange
        | AssetSortField::YearChange => SortDirection::Desc,
    }
}

pub(crate) fn asset_period_change(
    entry: &AssetBreakdownEntry,
    field: AssetSortField,
) -> Option<&AssetChange> {
    match field {
        AssetSortField::DayChange => entry.changes.day.as_ref(),
        AssetSortField::WeekChange => entry.changes.week.as_ref(),
        AssetSortField::MonthChange => entry.changes.month.as_ref(),
        AssetSortField::YearChange => entry.changes.year.as_ref(),
        AssetSortField::Name
        | AssetSortField::Amount
        | AssetSortField::AmountChecked
        | AssetSortField::AmountChanged
        | AssetSortField::PriceUpdated
        | AssetSortField::Value => None,
    }
}

fn asset_sort_metric(
    entry: &AssetBreakdownEntry,
    field: AssetSortField,
    use_absolute_changes: bool,
) -> Option<f64> {
    match field {
        AssetSortField::Name => None,
        AssetSortField::Amount => parse_money_input(&entry.total_amount),
        AssetSortField::AmountChecked => {
            asset_timestamp_sort_metric(entry.amount_last_checked_at.as_deref())
        }
        AssetSortField::AmountChanged => {
            asset_timestamp_sort_metric(entry.amount_last_changed_at.as_deref())
        }
        AssetSortField::PriceUpdated => {
            asset_timestamp_sort_metric(entry.price_updated_at.as_deref())
        }
        AssetSortField::Value => entry
            .value_in_base
            .as_deref()
            .and_then(parse_money_input)
            .map(f64::abs),
        AssetSortField::DayChange
        | AssetSortField::WeekChange
        | AssetSortField::MonthChange
        | AssetSortField::YearChange => asset_period_change(entry, field).and_then(|change| {
            if use_absolute_changes {
                parse_money_input(&change.absolute)
            } else {
                change.percentage.as_deref().and_then(parse_money_input)
            }
        }),
    }
}

/// RFC 3339 app timestamps are UTC (`+00:00`), so their compact numeric form
/// preserves chronological ordering without target-specific date libraries.
fn asset_timestamp_sort_metric(timestamp: Option<&str>) -> Option<f64> {
    timestamp.map(|value| {
        value
            .bytes()
            .filter(u8::is_ascii_digit)
            .take(17)
            .fold(0_f64, |metric, digit| {
                metric * 10.0 + f64::from(digit - b'0')
            })
    })
}

pub(crate) fn format_asset_timestamp(timestamp: Option<&str>) -> String {
    let Some(value) = timestamp else {
        return "—".to_string();
    };
    if value.len() >= 16 && value.as_bytes().get(10) == Some(&b'T') {
        format!("{} {} UTC", &value[..10], &value[11..16])
    } else {
        value.to_string()
    }
}

/// Missing metrics (unpriced rows, absent change periods) always sort last,
/// regardless of direction.
fn compare_metrics_missing_last(
    left: Option<f64>,
    right: Option<f64>,
    direction: SortDirection,
) -> std::cmp::Ordering {
    match (left, right) {
        (Some(left), Some(right)) => {
            let ordering = left
                .partial_cmp(&right)
                .unwrap_or(std::cmp::Ordering::Equal);
            match direction {
                SortDirection::Asc => ordering,
                SortDirection::Desc => ordering.reverse(),
            }
        }
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    }
}

pub(crate) fn compare_asset_entries_with_change_metric(
    a: &AssetBreakdownEntry,
    b: &AssetBreakdownEntry,
    field: AssetSortField,
    direction: SortDirection,
    use_absolute_changes: bool,
) -> std::cmp::Ordering {
    let ordering = match field {
        AssetSortField::Name => {
            let ordering = compare_case_insensitive(
                &asset_display_name(&a.asset, &a.asset_id),
                &asset_display_name(&b.asset, &b.asset_id),
            );
            match direction {
                SortDirection::Asc => ordering,
                SortDirection::Desc => ordering.reverse(),
            }
        }
        _ => compare_metrics_missing_last(
            asset_sort_metric(a, field, use_absolute_changes),
            asset_sort_metric(b, field, use_absolute_changes),
            direction,
        ),
    };
    ordering
        .then_with(|| a.asset_id.cmp(&b.asset_id))
        .then_with(|| a.liability.cmp(&b.liability))
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

/// Excluded from spending by a per-transaction annotation, which the row
/// editor can toggle off.
pub(crate) fn spending_ignore_is_annotation(transaction: &Transaction) -> bool {
    transaction.spending_ignore_reason.as_deref() == Some("annotation")
}

/// Excluded from spending by configuration (an ignore rule, an internal-transfer
/// hint, or an ignored account) rather than by an annotation, so the per-row
/// toggle cannot change it.
pub(crate) fn spending_ignore_is_configured(transaction: &Transaction) -> bool {
    matches!(
        transaction.spending_ignore_reason.as_deref(),
        Some("rule" | "internal_transfer" | "account")
    )
}

/// Never counted because of the transaction's own shape: only posted outflows
/// count toward spending.
pub(crate) fn spending_ignore_is_shape(transaction: &Transaction) -> bool {
    matches!(
        transaction.spending_ignore_reason.as_deref(),
        Some("not_posted" | "not_outflow")
    )
}

/// Tag spellings that mark a transaction as ignored-from-spending when present
/// on its annotation tag list.
const IGNORE_SPENDING_TAGS: [&str; 3] = ["ignore_spending", "ignore-spending", "ignore:spending"];

pub(crate) fn is_ignore_spending_tag(tag: &str) -> bool {
    IGNORE_SPENDING_TAGS
        .iter()
        .any(|candidate| tag.trim().eq_ignore_ascii_case(candidate))
}

/// Tags to display for a transaction row, hiding the legacy ignore-spending
/// control tags (the "Not counted" badge communicates that state instead).
/// Editing surfaces should keep using [`transaction_tags`] so saves round-trip
/// the full list.
pub(crate) fn visible_transaction_tags(transaction: &Transaction) -> Vec<String> {
    transaction_tags(transaction)
        .into_iter()
        .filter(|tag| !is_ignore_spending_tag(tag))
        .collect()
}

pub(crate) fn normalize_spending_tag_key(tag: &str) -> String {
    let trimmed = tag.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("untagged") {
        "Untagged".to_string()
    } else {
        trimmed.to_string()
    }
}

/// Defines `fn $name(index: usize) -> &'static str` over the numbered CSS
/// custom-property palette `--$prefix-$slot`, wrapping the index into range.
/// Spending tags and stacked chart series both colour by position this way.
macro_rules! css_var_palette {
    ($vis:vis fn $name:ident from $prefix:literal [$($slot:literal),+ $(,)?]) => {
        $vis fn $name(index: usize) -> &'static str {
            const PALETTE: &[&str] = &[$(concat!("var(--", $prefix, "-", $slot, ")")),+];
            PALETTE[index % PALETTE.len()]
        }
    };
}

pub(crate) use css_var_palette;

css_var_palette!(pub(crate) fn spending_tag_color from "cat" [1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);

pub(crate) fn spending_tag_color_map(
    tags: &[SpendingBreakdownEntry],
) -> HashMap<String, &'static str> {
    tags.iter()
        .enumerate()
        .map(|(index, entry)| (entry.key.clone(), spending_tag_color(index)))
        .collect()
}

pub(crate) fn spending_tag_color_for(
    colors: &HashMap<String, &'static str>,
    key: &str,
    fallback_index: usize,
) -> &'static str {
    colors
        .get(key)
        .copied()
        .unwrap_or_else(|| spending_tag_color(fallback_index))
}

pub(crate) fn pie_slices(
    tags: &[SpendingBreakdownEntry],
    colors: &HashMap<String, &'static str>,
) -> Vec<PieSlice> {
    let values = tags
        .iter()
        .map(|entry| parse_money_input(&entry.total).unwrap_or_default().abs())
        .collect::<Vec<_>>();
    let total = values.iter().sum::<f64>();
    if total <= 0.0 {
        return Vec::new();
    }

    let mut cursor = -std::f64::consts::FRAC_PI_2;
    tags.iter()
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
                color: spending_tag_color_for(colors, &entry.key, index),
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

pub(crate) fn transaction_subtags(transaction: &Transaction) -> Vec<String> {
    transaction
        .annotation
        .as_ref()
        .and_then(|annotation| annotation.subtags.clone())
        .unwrap_or_else(|| transaction.subtags.clone())
        .into_iter()
        .map(|value| normalize_spending_tag_key(&value))
        .filter(|value| !value.is_empty() && value != "Untagged")
        .fold(Vec::<String>::new(), |mut acc, subtag| {
            if !acc
                .iter()
                .any(|existing| existing.eq_ignore_ascii_case(&subtag))
            {
                acc.push(subtag);
            }
            acc
        })
}

pub(crate) fn transaction_tags(transaction: &Transaction) -> Vec<String> {
    if !transaction.tags.is_empty() {
        return normalize_tags(transaction.tags.clone());
    }
    if let Some(tags) = transaction
        .annotation
        .as_ref()
        .and_then(|annotation| annotation.tags.clone())
    {
        return normalize_tags(tags);
    }

    Vec::new()
}

pub(crate) fn transaction_has_tag(transaction: &Transaction, tag: &str) -> bool {
    let tags = transaction_tags(transaction);
    if is_untagged_tag(tag) && tags.is_empty() {
        return true;
    }
    tags.iter()
        .any(|candidate| candidate.eq_ignore_ascii_case(tag))
}

pub(crate) fn is_untagged_tag(tag: &str) -> bool {
    tag.trim().eq_ignore_ascii_case("untagged")
}

pub(crate) fn transaction_tags_label(transaction: &Transaction) -> String {
    let tags = transaction_tags(transaction);
    if tags.is_empty() {
        "Untagged".to_string()
    } else {
        tags.join(", ")
    }
}

pub(crate) fn normalize_tags(tags: Vec<String>) -> Vec<String> {
    tags.into_iter()
        .map(|tag| tag.trim().to_string())
        .filter(|tag| !tag.is_empty())
        .fold(Vec::<String>::new(), |mut acc, tag| {
            if !acc
                .iter()
                .any(|existing| existing.eq_ignore_ascii_case(&tag))
            {
                acc.push(tag);
            }
            acc
        })
}

pub(crate) fn format_transaction_amount(transaction: &Transaction, currency: &str) -> String {
    format_money_text(&transaction.amount, currency).unwrap_or_else(|| transaction.amount.clone())
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
        return format!(
            "Balance refresh complete: {synced}/{total} ok, {skipped} skipped, {failed} failed."
        );
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
            format!("Balance refresh skipped for {connection}: {reason}.")
        } else {
            format!("Balance refresh complete for {connection}.")
        }
    } else {
        let error = result
            .get("error")
            .and_then(|value| value.as_str())
            .unwrap_or("unknown error");
        format!("Balance refresh failed for {connection}: {error}")
    }
}

pub(crate) fn price_sync_result_summary(result: &serde_json::Value) -> String {
    let Some(refresh) = result.get("result") else {
        return "Price refresh finished.".to_string();
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
        format!("Price refresh complete: {fetched} fetched, {skipped} skipped.")
    } else {
        format!("Price refresh complete: {fetched} fetched, {skipped} skipped, {failed} failed.")
    }
}

pub(crate) fn transaction_query_string(
    start: &str,
    end: &str,
    tz: Option<&str>,
    include_ignored: bool,
) -> String {
    let mut params = Vec::new();
    if !start.trim().is_empty() {
        push_query_param(&mut params, "start", start);
    }
    if !end.trim().is_empty() {
        push_query_param(&mut params, "end", end);
    }
    if let Some(tz) = tz.filter(|tz| !tz.trim().is_empty()) {
        push_query_param(&mut params, "tz", tz);
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
    if let Some(value) = &patch.tags {
        match value {
            Some(tags) => parts.push(format!("tags={}", tags.join(", "))),
            None => parts.push("tags=clear".to_string()),
        }
    }
    if let Some(value) = &patch.subtags {
        match value {
            Some(subtags) => parts.push(format!("subtags={}", subtags.join(", "))),
            None => parts.push("subtags=clear".to_string()),
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
