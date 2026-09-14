use super::*;
use std::collections::HashSet;

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

fn stacked_data_point(point: &StackedHistoryPoint, date: &str) -> Option<StackedHistoryDataPoint> {
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
        date: date.to_string(),
        total,
        components,
    })
}

pub(crate) fn stacked_history_data_points(
    history: &StackedHistory,
) -> Vec<StackedHistoryDataPoint> {
    let mut points = history
        .points
        .iter()
        .filter_map(|point| stacked_data_point(point, &point.date))
        .collect::<Vec<_>>();
    points.sort_by(|a, b| a.date.cmp(&b.date));
    points
}

/// [`stacked_history_data_points`] with the app's current point plotted at the
/// viewer's local today, mirroring
/// [`history_data_points_with_current_snapshot`].
pub(crate) fn stacked_history_data_points_with_current(
    history: &StackedHistory,
    local_today: &str,
) -> Vec<StackedHistoryDataPoint> {
    let mut points = stacked_history_data_points(history);
    let Some(current) = history
        .current
        .as_ref()
        .and_then(|point| stacked_data_point(point, local_today))
    else {
        return points;
    };
    points.retain(|point| point.date.as_str() <= local_today);

    if let Some(point) = points.last_mut().filter(|point| point.date == local_today) {
        *point = current;
    } else {
        points.push(current);
    }

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
