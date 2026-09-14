use super::*;
use std::collections::HashSet;

#[component]
pub(super) fn StackedSeriesControls(
    accounts: Vec<StackedHistorySeries>,
    expanded_accounts: HashSet<String>,
    ontoggle: EventHandler<String>,
) -> Element {
    if accounts.is_empty() {
        return rsx! {};
    }

    rsx! {
        div { class: "stacked-series-controls",
            for account in accounts {
                {
                    let account_id = account.account_id.clone().unwrap_or_default();
                    let checked = expanded_accounts.contains(&account_id);
                    rsx! {
                        label { class: "stacked-account-toggle",
                            input {
                                r#type: "checkbox",
                                checked,
                                onchange: move |_| ontoggle.call(account_id.clone())
                            }
                            span { "{account.label}" }
                        }
                    }
                }
            }
        }
    }
}

#[component]
pub(super) fn StackedNetWorthChart(
    data: Vec<StackedHistoryDataPoint>,
    series: Vec<ActiveStackedSeries>,
    currency: String,
    onselectrange: EventHandler<(String, String)>,
) -> Element {
    let mut drag_start = use_signal(|| None::<usize>);
    let mut drag_current = use_signal(|| None::<usize>);
    if data.is_empty() || series.is_empty() {
        return rsx! {
            div { class: "chart-empty",
                strong { "No net worth breakdown" }
                small { "Refresh balances to populate the stacked graph." }
            }
        };
    }

    let width = 720.0;
    let height = 300.0;
    let padding_left = 68.0;
    let padding_right = 20.0;
    let padding_top = 18.0;
    let padding_bottom = 38.0;
    let plot_width = width - padding_left - padding_right;
    let plot_height = height - padding_top - padding_bottom;
    let count = data.len();
    let stack_points = build_stack_points(&data, &series, padding_left, plot_width);
    let (y_min, y_max) = stacked_y_domain(&stack_points, &data);
    let y_range = (y_max - y_min).max(1.0);
    let zero_y = stacked_y(0.0, y_min, y_range, padding_top, plot_height);
    let mid_value = y_min + y_range / 2.0;
    let min_label = format_compact_money(y_min, &currency);
    let mid_label = format_compact_money(mid_value, &currency);
    let max_label = format_compact_money(y_max, &currency);
    let first_date = data
        .first()
        .map(|point| point.date.clone())
        .unwrap_or_default();
    let latest_date = data
        .last()
        .map(|point| point.date.clone())
        .unwrap_or_default();
    let latest_total = data.last().map(|point| point.total).unwrap_or_default();
    let latest_value = format_compact_money(latest_total, &currency);
    let total_path = data
        .iter()
        .enumerate()
        .map(|(index, point)| {
            let x = if count <= 1 {
                padding_left + plot_width / 2.0
            } else {
                padding_left + (index as f64 / (count - 1) as f64) * plot_width
            };
            let y = stacked_y(point.total, y_min, y_range, padding_top, plot_height);
            let command = if index == 0 { "M" } else { "L" };
            format!("{command} {:.2} {:.2}", x, y)
        })
        .collect::<Vec<_>>()
        .join(" ");
    let layer_details = series
        .iter()
        .enumerate()
        .filter_map(|(index, series)| {
            let path = stack_area_path(
                &series.key,
                &stack_points,
                y_min,
                y_range,
                padding_top,
                plot_height,
            )?;
            Some(StackedLayerRender {
                index,
                series: series.clone(),
                path,
                color: stacked_chart_color(index),
            })
        })
        .collect::<Vec<_>>();
    let hover_points = stacked_hover_points(
        &data,
        &series,
        y_min,
        y_range,
        padding_left,
        padding_right,
        padding_top,
        plot_width,
        plot_height,
        width,
    );
    let drag_selection = chart_drag_selection(
        drag_start(),
        drag_current(),
        hover_points.iter().map(|point| point.x).collect(),
        padding_top,
        plot_height,
    );
    let date_values = hover_points
        .iter()
        .map(|point| point.date.clone())
        .collect::<Vec<_>>();
    let hover_rules = hover_points
        .iter()
        .map(|hover_point| {
            format!(
                ".stacked-point-hit-{0}:hover ~ .stacked-point-tooltip-{0} {{ display: block; }}",
                hover_point.index
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    rsx! {
        div { class: "chart-card stacked-chart-card",
            div { class: "chart-meta",
                div {
                    span { class: "metric-label", "Current" }
                    strong { "{latest_value}" }
                }
                div {
                    span { class: "metric-label", "Series" }
                    strong { "{series.len()}" }
                }
            }
            svg {
                class: "net-worth-chart stacked-net-worth-chart",
                view_box: "0 0 720 300",
                role: "img",
                style { "{hover_rules}" }
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
                    y1: "{zero_y}",
                    y2: "{zero_y}"
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
                    y: "{zero_y + 4.0}",
                    "$0"
                }
                text {
                    class: "chart-axis-label",
                    x: "8",
                    y: "{padding_top + plot_height + 4.0}",
                    "{min_label}"
                }
                text {
                    class: "chart-axis-label date-label",
                    x: "{padding_left}",
                    y: "{height - 10.0}",
                    "{first_date}"
                }
                text {
                    class: "chart-axis-label date-label end",
                    x: "{width - padding_right}",
                    y: "{height - 10.0}",
                    "{latest_date}"
                }
                for layer in layer_details.iter() {
                    path {
                        class: "stacked-area-layer",
                        d: "{layer.path}",
                        style: "fill: {layer.color};",
                        title { "{layer.series.label}" }
                    }
                }
                if !total_path.is_empty() {
                    path { class: "stacked-total-line", d: "{total_path}" }
                }
                if let Some(selection) = drag_selection {
                    rect {
                        class: "chart-drag-selection",
                        x: "{selection.x}",
                        y: "{selection.y}",
                        width: "{selection.width}",
                        height: "{selection.height}"
                    }
                }
                g { class: "chart-hover-layer",
                    for hover_point in hover_points.iter() {
                        {
                            let index = hover_point.index;
                            let dates_for_mouseup = date_values.clone();
                            rsx! {
                        rect {
                            class: "chart-hit-zone stacked-point-hit stacked-point-hit-{hover_point.index}",
                            x: "{hover_point.hit_x}",
                            y: "{padding_top}",
                            width: "{hover_point.hit_width}",
                            height: "{plot_height}",
                            onmousedown: move |_| {
                                drag_start.set(Some(index));
                                drag_current.set(Some(index));
                            },
                            onmouseenter: move |_| {
                                if drag_start().is_some() {
                                    drag_current.set(Some(index));
                                }
                            },
                            onmouseup: move |_| {
                                if let Some((start, end)) = selected_date_range(
                                    drag_start(),
                                    drag_current().or(Some(index)),
                                    &dates_for_mouseup,
                                ) {
                                    onselectrange.call((start, end));
                                }
                                drag_start.set(None);
                                drag_current.set(None);
                            }
                        }
                            }
                        }
                    }
                    for hover_point in hover_points {
                        StackedPointTooltip {
                            hover_point: hover_point.clone(),
                            currency: currency.clone(),
                            chart_width: width,
                            chart_height: height,
                            padding_right,
                            padding_top,
                        }
                    }
                }
            }
            div { class: "stacked-legend",
                for (index, item) in series.iter().enumerate() {
                    {
                        let color = stacked_chart_color(index);
                        let class = if item.series_type == "account_asset" {
                            "stacked-legend-item asset"
                        } else {
                            "stacked-legend-item"
                        };
                        rsx! {
                            span { class: "{class}",
                                span {
                                    class: "stacked-legend-swatch",
                                    style: "background: {color};"
                                }
                                span { "{item.label}" }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn StackedPointTooltip(
    hover_point: StackedHoverPoint,
    currency: String,
    chart_width: f64,
    chart_height: f64,
    padding_right: f64,
    padding_top: f64,
) -> Element {
    let row_height = 16.0;
    let tooltip_width = 272.0;
    let tooltip_height = 50.0 + (hover_point.breakdown.len() as f64 * row_height);
    let tooltip_x = if hover_point.x + tooltip_width + 12.0 > chart_width - padding_right {
        hover_point.x - tooltip_width - 12.0
    } else {
        hover_point.x + 12.0
    }
    .max(8.0);
    let tooltip_y = if hover_point.y + tooltip_height + 12.0 > chart_height - 6.0 {
        hover_point.y - tooltip_height - 12.0
    } else {
        hover_point.y + 12.0
    }
    .max(padding_top);
    let text_x = tooltip_x + 12.0;
    let date_y = tooltip_y + 20.0;
    let total_y = tooltip_y + 39.0;
    let rows_start_y = tooltip_y + 60.0;
    let total_text = format_full_money(hover_point.total, &currency);

    rsx! {
        g { class: "chart-hover-detail stacked-point-tooltip stacked-point-tooltip-{hover_point.index}",
            line {
                class: "chart-hover-line",
                x1: "{hover_point.x}",
                x2: "{hover_point.x}",
                y1: "{padding_top}",
                y2: "{hover_point.y}"
            }
            circle {
                class: "chart-hover-point",
                cx: "{hover_point.x}",
                cy: "{hover_point.y}",
                r: "6"
            }
            rect {
                class: "chart-tooltip",
                x: "{tooltip_x}",
                y: "{tooltip_y}",
                width: "{tooltip_width}",
                height: "{tooltip_height}",
                rx: "6"
            }
            text {
                class: "chart-tooltip-date",
                x: "{text_x}",
                y: "{date_y}",
                "{hover_point.date}"
            }
            text {
                class: "chart-tooltip-value",
                x: "{text_x}",
                y: "{total_y}",
                "Total {total_text}"
            }
            for (row_index, row) in hover_point.breakdown.iter().enumerate() {
                {
                    let row_y = rows_start_y + row_index as f64 * row_height;
                    let label = tooltip_label(&row.label, 28);
                    let value_text = format_full_money(row.value, &currency);
                    let value_class = if row.value < 0.0 {
                        "chart-tooltip-detail stacked-tooltip-value negative"
                    } else {
                        "chart-tooltip-detail stacked-tooltip-value"
                    };
                    rsx! {
                        rect {
                            class: "stacked-tooltip-swatch",
                            x: "{text_x}",
                            y: "{row_y - 8.0}",
                            width: "8",
                            height: "8",
                            rx: "2",
                            style: "fill: {row.color};"
                        }
                        text {
                            class: "chart-tooltip-detail stacked-tooltip-label",
                            x: "{text_x + 14.0}",
                            y: "{row_y}",
                            "{label}"
                        }
                        text {
                            class: "{value_class}",
                            x: "{tooltip_x + tooltip_width - 12.0}",
                            y: "{row_y}",
                            "{value_text}"
                        }
                    }
                }
            }
        }
    }
}
