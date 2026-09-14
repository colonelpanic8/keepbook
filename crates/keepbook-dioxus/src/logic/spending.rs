use super::*;
use std::collections::{HashMap, HashSet};

/// Sort key for a breakdown row: the app's total, read only for ordering.
fn spending_entry_order(entry: &SpendingBreakdownEntry) -> f64 {
    parse_money_input(&entry.total).unwrap_or_default()
}

/// Per-key totals for a `period=range` spending report. The single range
/// period's breakdown already holds the range total for each key, so this only
/// relabels and orders it.
pub(crate) fn spending_tags(spending: &SpendingOutput) -> Vec<SpendingBreakdownEntry> {
    let rename_untagged = can_rename_untagged(spending);
    let mut totals = spending
        .periods
        .iter()
        .flat_map(|period| &period.breakdown)
        .map(|entry| SpendingBreakdownEntry {
            key: spending_tag_key(&entry.key, rename_untagged),
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
    let rename_untagged = can_rename_untagged(spending);
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
                        key: spending_tag_key(&entry.key, rename_untagged),
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

/// True for a breakdown key that stands for the app's catch-all bucket of
/// transactions with no tags, however it is spelled.
pub(crate) fn is_untagged_spending_key(key: &str) -> bool {
    let trimmed = key.trim();
    trimmed.is_empty() || trimmed.eq_ignore_ascii_case("untagged")
}

/// The app's synthetic `untagged` key reads as `Untagged` beside real tags.
/// `rename_untagged` is false when the report also holds a literal tag of that
/// name: renaming would then give two buckets one label and one selection, so
/// both keep the app's spelling and stay two distinguishable rows.
fn spending_tag_key(key: &str, rename_untagged: bool) -> String {
    let trimmed = key.trim();
    if rename_untagged && is_untagged_spending_key(trimmed) {
        "Untagged".to_string()
    } else {
        trimmed.to_string()
    }
}

/// False when a report holds more than one key standing for the untagged
/// bucket, which [`spending_tag_key`] would otherwise collapse into one row.
fn can_rename_untagged(spending: &SpendingOutput) -> bool {
    spending
        .periods
        .iter()
        .flat_map(|period| &period.breakdown)
        .map(|entry| entry.key.trim())
        .filter(|key| is_untagged_spending_key(key))
        .collect::<HashSet<_>>()
        .len()
        < 2
}

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
