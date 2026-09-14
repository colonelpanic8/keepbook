use super::*;
use crate::logic::css_var_palette;
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug)]
pub(super) struct StackPoint {
    pub(super) series_key: String,
    pub(super) x: f64,
    pub(super) y0_value: f64,
    pub(super) y1_value: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct StackedLayerRender {
    pub(super) index: usize,
    pub(super) series: ActiveStackedSeries,
    pub(super) path: String,
    pub(super) color: &'static str,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct StackedHoverPoint {
    pub(super) index: usize,
    pub(super) date: String,
    pub(super) total: f64,
    pub(super) x: f64,
    pub(super) y: f64,
    pub(super) hit_x: f64,
    pub(super) hit_width: f64,
    pub(super) breakdown: Vec<StackedTooltipRow>,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct StackedTooltipRow {
    pub(super) label: String,
    pub(super) value: f64,
    pub(super) color: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct ChartDragSelection {
    pub(super) x: f64,
    pub(super) y: f64,
    pub(super) width: f64,
    pub(super) height: f64,
}

pub(super) fn account_stacked_series(history: &StackedHistory) -> Vec<StackedHistorySeries> {
    let final_values = final_component_values(history);
    let mut series = history
        .series
        .iter()
        .filter(|series| series.series_type == "account")
        .cloned()
        .collect::<Vec<_>>();
    series.sort_by(|a, b| compare_series_by_final_value(a, b, &final_values));
    series
}

pub(super) fn active_stacked_series(
    history: &StackedHistory,
    expanded_accounts: &HashSet<String>,
) -> Vec<ActiveStackedSeries> {
    let final_values = final_component_values(history);
    let asset_series_by_account = history.series.iter().fold(
        HashMap::<String, Vec<&StackedHistorySeries>>::new(),
        |mut acc, series| {
            if series.series_type == "account_asset" {
                if let Some(account_id) = series.account_id.as_ref() {
                    acc.entry(account_id.clone()).or_default().push(series);
                }
            }
            acc
        },
    );

    let mut active = Vec::new();
    for account in account_stacked_series(history) {
        let account_id = account.account_id.clone().unwrap_or_default();
        if expanded_accounts.contains(&account_id) {
            if let Some(asset_series) = asset_series_by_account.get(&account_id) {
                let mut sorted_assets = asset_series.clone();
                sorted_assets.sort_by(|a, b| compare_series_by_final_value(a, b, &final_values));
                for asset in sorted_assets {
                    active.push(ActiveStackedSeries {
                        key: asset.key.clone(),
                        label: asset.label.clone(),
                        account_id: asset.account_id.clone(),
                        series_type: asset.series_type.clone(),
                    });
                }
                continue;
            }
        }
        active.push(ActiveStackedSeries {
            key: account.key,
            label: account.label,
            account_id: account.account_id,
            series_type: account.series_type,
        });
    }
    active.sort_by(|a, b| compare_active_series_by_final_value(a, b, &final_values));
    active
}

fn final_component_values(history: &StackedHistory) -> HashMap<&str, f64> {
    history
        .points
        .last()
        .map(|point| {
            point
                .components
                .iter()
                .filter_map(|component| {
                    component
                        .value
                        .parse::<f64>()
                        .ok()
                        .filter(|value| value.is_finite())
                        .map(|value| (component.series_key.as_str(), value))
                })
                .collect::<HashMap<_, _>>()
        })
        .unwrap_or_default()
}

fn compare_series_by_final_value(
    a: &StackedHistorySeries,
    b: &StackedHistorySeries,
    final_values: &HashMap<&str, f64>,
) -> std::cmp::Ordering {
    let value_a = *final_values.get(a.key.as_str()).unwrap_or(&0.0);
    let value_b = *final_values.get(b.key.as_str()).unwrap_or(&0.0);
    value_b
        .partial_cmp(&value_a)
        .unwrap_or(std::cmp::Ordering::Equal)
        .then_with(|| a.label.cmp(&b.label))
}

fn compare_active_series_by_final_value(
    a: &ActiveStackedSeries,
    b: &ActiveStackedSeries,
    final_values: &HashMap<&str, f64>,
) -> std::cmp::Ordering {
    let value_a = *final_values.get(a.key.as_str()).unwrap_or(&0.0);
    let value_b = *final_values.get(b.key.as_str()).unwrap_or(&0.0);
    value_b
        .partial_cmp(&value_a)
        .unwrap_or(std::cmp::Ordering::Equal)
        .then_with(|| a.label.cmp(&b.label))
}

pub(super) fn build_stack_points(
    data: &[StackedHistoryDataPoint],
    series: &[ActiveStackedSeries],
    padding_left: f64,
    plot_width: f64,
) -> Vec<StackPoint> {
    let count = data.len();
    data.iter()
        .enumerate()
        .flat_map(|(index, point)| {
            let x = if count <= 1 {
                padding_left + plot_width / 2.0
            } else {
                padding_left + (index as f64 / (count - 1) as f64) * plot_width
            };
            let values = point
                .components
                .iter()
                .map(|component| (component.series_key.as_str(), component.value))
                .collect::<HashMap<_, _>>();
            let mut positive = 0.0;
            let mut negative = 0.0;
            series
                .iter()
                .map(|series| {
                    let value = *values.get(series.key.as_str()).unwrap_or(&0.0);
                    if value >= 0.0 {
                        let y0_value = positive;
                        positive += value;
                        StackPoint {
                            series_key: series.key.clone(),
                            x,
                            y0_value,
                            y1_value: positive,
                        }
                    } else {
                        let y0_value = negative;
                        negative += value;
                        StackPoint {
                            series_key: series.key.clone(),
                            x,
                            y0_value,
                            y1_value: negative,
                        }
                    }
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

pub(super) fn stacked_y_domain(
    points: &[StackPoint],
    data: &[StackedHistoryDataPoint],
) -> (f64, f64) {
    let mut min = 0.0_f64;
    let mut max = 0.0_f64;
    for point in points {
        min = min.min(point.y0_value).min(point.y1_value);
        max = max.max(point.y0_value).max(point.y1_value);
    }
    for point in data {
        min = min.min(point.total);
        max = max.max(point.total);
    }
    if min == max {
        (0.0, max + 1.0)
    } else {
        let top_padding = (max - min).abs() * 0.04;
        (min.min(0.0), max + top_padding)
    }
}

pub(super) fn stack_area_path(
    series_key: &str,
    points: &[StackPoint],
    y_min: f64,
    y_range: f64,
    padding_top: f64,
    plot_height: f64,
) -> Option<String> {
    let series_points = points
        .iter()
        .filter(|point| point.series_key == series_key)
        .collect::<Vec<_>>();
    if series_points.is_empty() {
        return None;
    }

    let mut commands = series_points
        .iter()
        .enumerate()
        .map(|(index, point)| {
            let command = if index == 0 { "M" } else { "L" };
            format!(
                "{command} {:.2} {:.2}",
                point.x,
                stacked_y(point.y1_value, y_min, y_range, padding_top, plot_height)
            )
        })
        .collect::<Vec<_>>();
    commands.extend(series_points.iter().rev().map(|point| {
        format!(
            "L {:.2} {:.2}",
            point.x,
            stacked_y(point.y0_value, y_min, y_range, padding_top, plot_height)
        )
    }));
    commands.push("Z".to_string());
    Some(commands.join(" "))
}

pub(super) fn stacked_y(
    value: f64,
    y_min: f64,
    y_range: f64,
    padding_top: f64,
    plot_height: f64,
) -> f64 {
    padding_top + ((y_min + y_range - value) / y_range) * plot_height
}

pub(super) fn chart_drag_selection(
    start: Option<usize>,
    current: Option<usize>,
    point_xs: Vec<f64>,
    padding_top: f64,
    plot_height: f64,
) -> Option<ChartDragSelection> {
    let start = start?;
    let current = current?;
    if start == current {
        return None;
    }
    let start_x = *point_xs.get(start)?;
    let current_x = *point_xs.get(current)?;
    let x = start_x.min(current_x);
    Some(ChartDragSelection {
        x,
        y: padding_top,
        width: (start_x - current_x).abs().max(1.0),
        height: plot_height,
    })
}

pub(super) fn selected_date_range(
    start: Option<usize>,
    current: Option<usize>,
    dates: &[String],
) -> Option<(String, String)> {
    let start = start?;
    let current = current?;
    if start == current {
        return None;
    }
    let min_index = start.min(current);
    let max_index = start.max(current);
    Some((dates.get(min_index)?.clone(), dates.get(max_index)?.clone()))
}

#[allow(clippy::too_many_arguments)]
pub(super) fn stacked_hover_points(
    data: &[StackedHistoryDataPoint],
    series: &[ActiveStackedSeries],
    y_min: f64,
    y_range: f64,
    padding_left: f64,
    padding_right: f64,
    padding_top: f64,
    plot_width: f64,
    plot_height: f64,
    chart_width: f64,
) -> Vec<StackedHoverPoint> {
    let count = data.len();
    data.iter()
        .enumerate()
        .map(|(index, point)| {
            let x = if count <= 1 {
                padding_left + plot_width / 2.0
            } else {
                padding_left + (index as f64 / (count - 1) as f64) * plot_width
            };
            let previous_x = if index == 0 {
                padding_left
            } else {
                padding_left + ((index as f64 - 0.5) / (count - 1) as f64) * plot_width
            };
            let next_x = if index + 1 == count {
                chart_width - padding_right
            } else {
                padding_left + ((index as f64 + 0.5) / (count - 1) as f64) * plot_width
            };
            let component_values = point
                .components
                .iter()
                .map(|component| (component.series_key.as_str(), component.value))
                .collect::<HashMap<_, _>>();
            let mut breakdown = series
                .iter()
                .enumerate()
                .filter_map(|(series_index, series)| {
                    let value = *component_values.get(series.key.as_str()).unwrap_or(&0.0);
                    if value.abs() <= f64::EPSILON {
                        return None;
                    }
                    Some(StackedTooltipRow {
                        label: series.label.clone(),
                        value,
                        color: stacked_chart_color(series_index),
                    })
                })
                .collect::<Vec<_>>();
            breakdown.sort_by(|a, b| {
                b.value
                    .abs()
                    .partial_cmp(&a.value.abs())
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| a.label.cmp(&b.label))
            });

            StackedHoverPoint {
                index,
                date: point.date.clone(),
                total: point.total,
                x,
                y: stacked_y(point.total, y_min, y_range, padding_top, plot_height),
                hit_x: previous_x,
                hit_width: (next_x - previous_x).max(1.0),
                breakdown,
            }
        })
        .collect()
}

pub(super) fn tooltip_label(label: &str, max_chars: usize) -> String {
    if label.chars().count() <= max_chars {
        return label.to_string();
    }

    let mut truncated = label
        .chars()
        .take(max_chars.saturating_sub(1))
        .collect::<String>();
    truncated.push_str("...");
    truncated
}

css_var_palette!(pub(super) fn stacked_chart_color from "series" [
    1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18
]);
