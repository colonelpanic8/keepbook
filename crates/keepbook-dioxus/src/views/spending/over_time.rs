use super::*;
use std::collections::HashMap;

#[derive(Clone, Debug)]
struct SpendingBarRect {
    key: String,
    label: String,
    start_date: String,
    end_date: String,
    total: f64,
    value: f64,
    transaction_count: usize,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    color: &'static str,
}

#[derive(Clone, Debug)]
struct SpendingBarHitZone {
    selection: SpendingPeriodSelection,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    tooltip_x: f64,
    tooltip_y: f64,
}

/// Identifies a single category subsection (segment) within one time period.
/// Used to hover or "pin" the tooltip to that category.
#[derive(Clone, Debug, PartialEq)]
struct SpendingSegmentKey {
    key: String,
    start_date: String,
    end_date: String,
}

/// Payload emitted when a segment is clicked to focus the view on a single
/// category within a single time period.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct SpendingSegmentSelection {
    pub(super) key: String,
    pub(super) period: SpendingPeriodSelection,
}

#[component]
pub(super) fn SpendingOverTimeChart(
    spending: SpendingOutput,
    /// Per-tag totals for the whole range, from the `period=range` report.
    series: Vec<SpendingBreakdownEntry>,
    selected: Option<String>,
    selected_period: Option<SpendingPeriodSelection>,
    bucket_label: String,
    colors: HashMap<String, &'static str>,
    onclick: EventHandler<SpendingPeriodSelection>,
    onfocussegment: EventHandler<SpendingSegmentSelection>,
    onselecttag: EventHandler<String>,
) -> Element {
    let mut hovered_period = use_signal(|| None::<SpendingPeriodSelection>);
    let mut hovered_segment = use_signal(|| None::<SpendingSegmentKey>);
    let mut pinned_segment = use_signal(|| None::<SpendingSegmentKey>);
    let points = spending_over_time_points(&spending);
    let narrowed_points = selected
        .as_ref()
        .map(|tag| narrow_spending_points_to_tag(&points, tag));
    let display_points = narrowed_points.as_ref().unwrap_or(&points);
    let visible_points = visible_spending_over_time_points(display_points, &series);

    if visible_points.is_empty() || series.is_empty() {
        return rsx! {
            div { class: "chart-empty spending-over-time-empty",
                strong { "No spending over time" }
                small { "Refresh transactions or adjust the range." }
            }
        };
    }

    let width = 720.0;
    let height = 300.0;
    let padding_left = 68.0;
    let padding_right = 20.0;
    let padding_top = 18.0;
    let padding_bottom = 44.0;
    let plot_width = width - padding_left - padding_right;
    let plot_height = height - padding_top - padding_bottom;
    let max_total = visible_points
        .iter()
        .map(|point| point.total)
        .fold(0.0_f64, f64::max)
        .max(1.0);
    let y_max = max_total * 1.08;
    let count = visible_points.len();
    let slot_width = plot_width / count as f64;
    let gap = (slot_width * 0.18).clamp(3.0, 14.0);
    let bar_width = (slot_width - gap).max(2.0);
    let series_keys = series
        .iter()
        .map(|entry| entry.key.clone())
        .collect::<Vec<_>>();
    let range_total_text = match &selected {
        Some(tag) => series
            .iter()
            .find(|entry| &entry.key == tag)
            .map(|entry| entry.total.clone())
            .unwrap_or_else(|| "0".to_string()),
        None => spending.total.clone(),
    };
    let over_time_label = match &selected {
        Some(tag) => format!("Over Time · {tag}"),
        None => "Over Time".to_string(),
    };
    let first_label = visible_points
        .first()
        .map(|point| point.label.clone())
        .unwrap_or_default();
    let last_label = visible_points
        .last()
        .map(|point| point.label.clone())
        .unwrap_or_default();
    let mid_label = format_compact_money(y_max / 2.0, &spending.currency);
    let max_label = format_compact_money(y_max, &spending.currency);
    let total_label =
        format_money_text(range_total_text.trim_start_matches('-'), &spending.currency)
            .unwrap_or(range_total_text);
    let mut bar_rects = Vec::new();
    let mut bar_hit_zones = Vec::new();
    for (point_index, point) in visible_points.iter().enumerate() {
        let x = padding_left + point_index as f64 * slot_width + gap / 2.0;
        bar_hit_zones.push(SpendingBarHitZone {
            selection: SpendingPeriodSelection {
                label: point.label.clone(),
                start_date: point.start_date.clone(),
                end_date: point.end_date.clone(),
                total: point.total,
                transaction_count: point.transaction_count,
            },
            x: padding_left + point_index as f64 * slot_width,
            y: padding_top,
            width: slot_width,
            height: plot_height,
            tooltip_x: x + bar_width / 2.0,
            tooltip_y: padding_top + 14.0,
        });
        let values = point
            .segments
            .iter()
            .map(|segment| (segment.key.as_str(), segment))
            .collect::<std::collections::HashMap<_, _>>();
        let mut cumulative = 0.0_f64;
        for (series_index, key) in series_keys.iter().enumerate() {
            if let Some(segment) = values.get(key.as_str()) {
                let y1_value = cumulative + segment.value;
                let y0 = padding_top + ((y_max - cumulative) / y_max) * plot_height;
                let y1 = padding_top + ((y_max - y1_value) / y_max) * plot_height;
                cumulative = y1_value;
                bar_rects.push(SpendingBarRect {
                    key: key.clone(),
                    label: point.label.clone(),
                    start_date: point.start_date.clone(),
                    end_date: point.end_date.clone(),
                    total: point.total,
                    value: segment.value,
                    transaction_count: segment.transaction_count,
                    x,
                    y: y1,
                    width: bar_width,
                    height: (y0 - y1).max(0.6),
                    color: spending_tag_color_for(&colors, key, series_index),
                });
            }
        }
    }
    // A clicked (pinned) segment takes precedence over a hovered segment. Both
    // show the category amount alongside the full bucket total. Empty column
    // space and externally-selected periods continue to show period totals.
    let active_segment = pinned_segment().or_else(&*hovered_segment);
    let tooltip = active_segment
        .as_ref()
        .and_then(|pin| {
            bar_rects
                .iter()
                .find(|rect| {
                    rect.key == pin.key
                        && rect.start_date == pin.start_date
                        && rect.end_date == pin.end_date
                })
                .map(|rect| {
                    (
                        format!("{} · {}", rect.label, rect.key),
                        spending_segment_tooltip_detail(
                            rect.value,
                            rect.total,
                            &spending.currency,
                            &bucket_label,
                        ),
                        rect.x + rect.width / 2.0,
                        padding_top + 14.0,
                    )
                })
        })
        .or_else(|| {
            let tooltip_period = hovered_period().or_else(|| selected_period.clone());
            tooltip_period.as_ref().and_then(|period| {
                bar_hit_zones
                    .iter()
                    .find(|zone| {
                        zone.selection.start_date == period.start_date
                            && zone.selection.end_date == period.end_date
                    })
                    .map(|zone| {
                        (
                            period.label.clone(),
                            format!(
                                "{} / {} tx",
                                format_full_money(period.total, &spending.currency),
                                period.transaction_count
                            ),
                            zone.tooltip_x,
                            zone.tooltip_y,
                        )
                    })
            })
        });

    rsx! {
        div { class: "chart-card spending-over-time-card",
            div { class: "chart-meta",
                div {
                    span { class: "metric-label", "{over_time_label}" }
                    strong { "{total_label}" }
                }
                div {
                    span { class: "metric-label", "Bucket" }
                    strong { "{bucket_label}" }
                }
            }
            svg {
                class: "net-worth-chart spending-bar-chart",
                view_box: "0 0 720 300",
                role: "img",
                line {
                    class: "chart-grid",
                    x1: "{padding_left}",
                    x2: "{width - padding_right}",
                    y1: "{padding_top}",
                    y2: "{padding_top}"
                }
                line {
                    class: "chart-grid",
                    x1: "{padding_left}",
                    x2: "{width - padding_right}",
                    y1: "{padding_top + plot_height / 2.0}",
                    y2: "{padding_top + plot_height / 2.0}"
                }
                line {
                    class: "chart-grid axis",
                    x1: "{padding_left}",
                    x2: "{width - padding_right}",
                    y1: "{padding_top + plot_height}",
                    y2: "{padding_top + plot_height}"
                }
                text {
                    class: "chart-axis-label",
                    x: "8",
                    y: "{padding_top + 4.0}",
                    "{max_label}"
                }
                text {
                    class: "chart-axis-label",
                    x: "8",
                    y: "{padding_top + plot_height / 2.0 + 4.0}",
                    "{mid_label}"
                }
                text {
                    class: "chart-axis-label",
                    x: "8",
                    y: "{padding_top + plot_height + 4.0}",
                    "$0"
                }
                text {
                    class: "chart-axis-label date-label",
                    x: "{padding_left}",
                    y: "{height - 10.0}",
                    "{first_label}"
                }
                text {
                    class: "chart-axis-label date-label end",
                    x: "{width - padding_right}",
                    y: "{height - 10.0}",
                    "{last_label}"
                }
                // Period-wide hit zones render first (behind the bars) so they only
                // catch hover/click in the empty column space above each bar; the
                // segment rects above handle per-category interaction.
                g { class: "spending-bar-hit-layer",
                    for hit in bar_hit_zones {
                        {
                            let selection_for_click = hit.selection.clone();
                            let selection_for_enter = hit.selection.clone();
                            let tooltip_text = format!(
                                "{}: {} / {} transactions / {} to {}",
                                hit.selection.label,
                                format_full_money(hit.selection.total, &spending.currency),
                                hit.selection.transaction_count,
                                hit.selection.start_date,
                                hit.selection.end_date
                            );
                            rsx! {
                                rect {
                                    class: "spending-bar-hit-zone",
                                    x: "{hit.x}",
                                    y: "{hit.y}",
                                    width: "{hit.width}",
                                    height: "{hit.height}",
                                    onmouseenter: move |_| hovered_period.set(Some(selection_for_enter.clone())),
                                    onmouseleave: move |_| hovered_period.set(None),
                                    onclick: move |_| onclick.call(selection_for_click.clone()),
                                    title { "{tooltip_text}" }
                                }
                            }
                        }
                    }
                }
                for rect in bar_rects {
                    {
                        let key = rect.key.clone();
                        let label = rect.label.clone();
                        let start_date = rect.start_date.clone();
                        let end_date = rect.end_date.clone();
                        let total = rect.total;
                        let value = rect.value;
                        let transaction_count = rect.transaction_count;
                        let x = rect.x;
                        let y = rect.y;
                        let width = rect.width;
                        let height = rect.height;
                        let color = rect.color;
                        let selection = SpendingPeriodSelection {
                            label: label.clone(),
                            start_date: start_date.clone(),
                            end_date: end_date.clone(),
                            total,
                            transaction_count,
                        };
                        let is_pinned = pinned_segment().is_some_and(|pin| {
                            pin.key == key
                                && pin.start_date == start_date
                                && pin.end_date == end_date
                        });
                        let selected_class = if selected_period
                            .as_ref()
                            .is_some_and(|period| period.start_date == start_date && period.end_date == end_date)
                        {
                            " selected"
                        } else {
                            ""
                        };
                        let pinned_class = if is_pinned { " pinned" } else { "" };
                        let class = format!("spending-bar-segment{selected_class}{pinned_class}");
                        // Clones for the individual event closures.
                        let selection_for_click = selection.clone();
                        let selection_for_enter = selection.clone();
                        let pin_key = key.clone();
                        let pin_start = start_date.clone();
                        let pin_end = end_date.clone();
                        let hover_key = key.clone();
                        let hover_start = start_date.clone();
                        let hover_end = end_date.clone();
                        rsx! {
                            rect {
                                class: "{class}",
                                x: "{x}",
                                y: "{y}",
                                width: "{width}",
                                height: "{height}",
                                style: "fill: {color};",
                                onmouseenter: move |_| {
                                    hovered_period.set(Some(selection_for_enter.clone()));
                                    hovered_segment.set(Some(SpendingSegmentKey {
                                        key: hover_key.clone(),
                                        start_date: hover_start.clone(),
                                        end_date: hover_end.clone(),
                                    }));
                                },
                                onmouseleave: move |_| {
                                    hovered_period.set(None);
                                    hovered_segment.set(None);
                                },
                                onclick: move |_| {
                                    // Toggle this category's pinned tooltip, and focus the
                                    // view on this category + period.
                                    let already_pinned = pinned_segment().is_some_and(|pin| {
                                        pin.key == pin_key
                                            && pin.start_date == pin_start
                                            && pin.end_date == pin_end
                                    });
                                    pinned_segment.set(if already_pinned {
                                        None
                                    } else {
                                        Some(SpendingSegmentKey {
                                            key: pin_key.clone(),
                                            start_date: pin_start.clone(),
                                            end_date: pin_end.clone(),
                                        })
                                    });
                                    onfocussegment.call(SpendingSegmentSelection {
                                        key: pin_key.clone(),
                                        period: selection_for_click.clone(),
                                    });
                                },
                                title {
                                    "{label}: {format_full_money(total, &spending.currency)} total / {key}: {format_full_money(value, &spending.currency)} ({transaction_count} tx)"
                                }
                            }
                        }
                    }
                }
                if let Some((tooltip_title, tooltip_detail, tooltip_x, tooltip_y)) = tooltip {
                    {
                        let (tooltip_width, tooltip_center_x) = spending_tooltip_layout(
                            &tooltip_title,
                            &tooltip_detail,
                            tooltip_x,
                            width,
                        );
                        let tooltip_left = -tooltip_width / 2.0;
                        rsx! {
                    g { class: "spending-chart-tooltip",
                        transform: "translate({tooltip_center_x}, {tooltip_y})",
                        rect {
                            x: "{tooltip_left}",
                            y: "-11",
                            width: "{tooltip_width}",
                            height: "42",
                            rx: "5"
                        }
                        text {
                            class: "spending-tooltip-title",
                            x: "0",
                            y: "3",
                            "{tooltip_title}"
                        }
                        text {
                            class: "spending-tooltip-detail",
                            x: "0",
                            y: "19",
                            "{tooltip_detail}"
                        }
                    }
                        }
                    }
                }
            }
            div { class: "stacked-legend spending-bar-legend",
                for (index, item) in series.iter().enumerate() {
                    {
                        let color = spending_tag_color_for(&colors, &item.key, index);
                        let key = item.key.clone();
                        let key_for_click = key.clone();
                        let class = if selected.as_ref() == Some(&key) {
                            "stacked-legend-item selected"
                        } else {
                            "stacked-legend-item"
                        };
                        rsx! {
                            button {
                                class: "{class}",
                                onclick: move |_| onselecttag.call(key_for_click.clone()),
                                span {
                                    class: "stacked-legend-swatch",
                                    style: "background: {color};"
                                }
                                span { "{key}" }
                            }
                        }
                    }
                }
            }
        }
    }
}
