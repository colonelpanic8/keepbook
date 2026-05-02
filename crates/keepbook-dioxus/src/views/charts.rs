use super::*;

#[component]
pub(super) fn AccountGraphPanel(
    accounts: Vec<Account>,
    connections: Vec<Connection>,
    currency: String,
    defaults: HistoryDefaults,
    filter_overrides: FilterOverrides,
) -> Element {
    let initial_account_id = accounts
        .iter()
        .find(|account| account.active)
        .or_else(|| accounts.first())
        .map(|account| account.id.clone())
        .unwrap_or_default();
    let mut selected_account_id = use_signal(move || initial_account_id.clone());
    let account_options = accounts
        .iter()
        .filter(|account| account.active)
        .cloned()
        .collect::<Vec<_>>();
    let account_options = if account_options.is_empty() {
        accounts.clone()
    } else {
        account_options
    };
    let current_selection = selected_account_id();
    let selected_account = account_options
        .iter()
        .find(|account| account.id == current_selection)
        .or_else(|| account_options.first());
    let selected_id = selected_account
        .map(|account| account.id.clone())
        .unwrap_or_default();
    let selected_name = selected_account
        .map(|account| account.name.clone())
        .unwrap_or_else(|| "No account selected".to_string());
    let selected_connection = selected_account
        .and_then(|account| {
            connections
                .iter()
                .find(|connection| connection.id == account.connection_id)
        })
        .map(|connection| connection.name.clone())
        .unwrap_or_else(|| "Unknown connection".to_string());

    rsx! {
        section { class: "panel graph-panel",
            div { class: "panel-header",
                div { class: "panel-title",
                    h2 { "Account Value Over Time" }
                    span { "{selected_connection}" }
                }
                if !account_options.is_empty() {
                    label { class: "graph-scope-control",
                        span { "Account" }
                        select {
                            class: "control-input",
                            value: "{selected_id}",
                            onchange: move |event| selected_account_id.set(event.value()),
                            for account in account_options.clone() {
                                {
                                    let connection_name = connections
                                        .iter()
                                        .find(|connection| connection.id == account.connection_id)
                                        .map(|connection| connection.name.clone())
                                        .unwrap_or_else(|| "Unknown".to_string());
                                    let label = format!("{} - {}", account.name, connection_name);
                                    rsx! {
                                        option {
                                            value: "{account.id}",
                                            "{label}"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            if selected_id.is_empty() {
                div { class: "chart-empty",
                    strong { "No accounts" }
                    small { "Sync or add an account to populate account charts." }
                }
            } else {
                HistoryGraphPanel {
                    title: selected_name.clone(),
                    scope_label: selected_connection.clone(),
                    empty_title: "No account history".to_string(),
                    empty_detail: "Sync balances for this account to populate the chart.".to_string(),
                    currency,
                    defaults,
                    filter_overrides,
                    account: Some(selected_id),
                    show_header: false,
                }
            }
        }
    }
}

#[component]
pub(super) fn HistoryGraphPanel(
    title: String,
    scope_label: String,
    empty_title: String,
    empty_detail: String,
    currency: String,
    defaults: HistoryDefaults,
    filter_overrides: FilterOverrides,
    account: Option<String>,
    show_header: bool,
) -> Element {
    let initial_range_preset = range_preset_from_config(&defaults.graph_range);
    let initial_sampling_granularity =
        sampling_granularity_from_config(&defaults.graph_granularity);
    let mut range_preset = use_signal(move || initial_range_preset);
    let mut start_override = use_signal(String::new);
    let mut end_override = use_signal(String::new);
    let mut y_min_input = use_signal(String::new);
    let mut y_max_input = use_signal(String::new);
    let mut sampling_granularity = use_signal(move || initial_sampling_granularity);
    let history = use_resource(move || {
        let selected_range = range_preset();
        let start_text = start_override();
        let end_text = end_override();
        let selected_sampling = sampling_granularity();
        let selected_account = account.clone();
        async move {
            fetch_history(history_query_string(
                selected_range,
                &start_text,
                &end_text,
                selected_sampling,
                &current_date_string(),
                filter_overrides,
                selected_account.as_deref(),
            ))
            .await
        }
    });

    let selected_range = range_preset();
    let selected_sampling = sampling_granularity();
    let start_text = start_override();
    let end_text = end_override();
    let history_state = history.cloned();
    let is_history_loading = history_state.is_none();
    let loaded_history = match &history_state {
        Some(Ok(history)) => Some(history),
        _ => None,
    };
    let data = loaded_history.map(history_data_points).unwrap_or_default();
    let bounds = date_bounds(&data);
    let (start_date, end_date) = visible_date_range(&data, selected_range, &start_text, &end_text);
    let visible_data = filter_data_by_date_range(&data, &start_date, &end_date);
    let resolved_sampling = resolve_sampling_granularity(selected_sampling, &visible_data);
    let sampled_data = sample_data_by_granularity(&visible_data, resolved_sampling);
    let sampled_point_count = sampled_data.len();
    let sampling_label = resolved_sampling.label();
    let visible_value_bounds = value_bounds(&sampled_data);
    let y_min_text = y_min_input();
    let y_max_text = y_max_input();
    let y_domain = parse_y_domain(&y_min_text, &y_max_text);
    let has_date_error = !start_date.is_empty() && !end_date.is_empty() && start_date > end_date;
    let has_y_error = !y_min_text.is_empty() && !y_max_text.is_empty() && y_domain.is_none();
    let current_value = sampled_data
        .last()
        .map(|point| point.value)
        .unwrap_or_default();
    let start_value = sampled_data
        .first()
        .map(|point| point.value)
        .unwrap_or_default();
    let absolute_change = current_value - start_value;
    let percentage_change = if start_value == 0.0 {
        None
    } else {
        Some((absolute_change / start_value) * 100.0)
    };
    let data_y_range = visible_value_bounds
        .map(|(min, max)| {
            format!(
                "{} to {}",
                format_full_money(min, &currency),
                format_full_money(max, &currency)
            )
        })
        .unwrap_or_else(|| "No visible data".to_string());
    let axis_y_range = y_domain
        .map(|(min, max)| {
            format!(
                "{} to {}",
                format_full_money(min, &currency),
                format_full_money(max, &currency)
            )
        })
        .unwrap_or_else(|| "Auto".to_string());
    let change_class = if absolute_change >= 0.0 {
        "change-positive"
    } else {
        "change-negative"
    };
    let percent_text = percentage_change
        .map(|value| format!("{}%", format_number(value, 2)))
        .unwrap_or_else(|| "N/A".to_string());
    let min_date = bounds
        .as_ref()
        .map(|bounds| bounds.0.clone())
        .unwrap_or_default();
    let max_date = bounds
        .as_ref()
        .map(|bounds| bounds.1.clone())
        .unwrap_or_default();
    let header_label = loaded_history
        .map(|history| history.currency.clone())
        .unwrap_or(scope_label);

    rsx! {
        if show_header {
            div { class: "panel-header",
                h2 { "{title}" }
                span { "{header_label}" }
            }
        }
        if is_history_loading {
            BackendActivity { message: "Waiting on backend graph data" }
        }
        div { class: "chart-controls",
            div { class: "preset-row",
                span { class: "control-label", "Range" }
                GraphPresetButton {
                    label: "1M",
                    selected: selected_range == RangePreset::OneMonth,
                    onclick: move |_| {
                        range_preset.set(RangePreset::OneMonth);
                        start_override.set(String::new());
                        end_override.set(String::new());
                    }
                }
                GraphPresetButton {
                    label: "90D",
                    selected: selected_range == RangePreset::NinetyDays,
                    onclick: move |_| {
                        range_preset.set(RangePreset::NinetyDays);
                        start_override.set(String::new());
                        end_override.set(String::new());
                    }
                }
                GraphPresetButton {
                    label: "6M",
                    selected: selected_range == RangePreset::SixMonths,
                    onclick: move |_| {
                        range_preset.set(RangePreset::SixMonths);
                        start_override.set(String::new());
                        end_override.set(String::new());
                    }
                }
                GraphPresetButton {
                    label: "1Y",
                    selected: selected_range == RangePreset::OneYear,
                    onclick: move |_| {
                        range_preset.set(RangePreset::OneYear);
                        start_override.set(String::new());
                        end_override.set(String::new());
                    }
                }
                GraphPresetButton {
                    label: "2Y",
                    selected: selected_range == RangePreset::TwoYears,
                    onclick: move |_| {
                        range_preset.set(RangePreset::TwoYears);
                        start_override.set(String::new());
                        end_override.set(String::new());
                    }
                }
                GraphPresetButton {
                    label: "Max",
                    selected: selected_range == RangePreset::Max,
                    onclick: move |_| {
                        range_preset.set(RangePreset::Max);
                        start_override.set(String::new());
                        end_override.set(String::new());
                    }
                }
                button {
                    class: "control-button",
                    onclick: move |_| {
                        if let Some((min, max)) = visible_value_bounds {
                            y_min_input.set(format_input_number(min));
                            y_max_input.set(format_input_number(max));
                        }
                    },
                    "Fit Y"
                }
            }
            div { class: "sampling-row",
                span { class: "control-label", "Sampling" }
                for option in SamplingGranularity::OPTIONS {
                    GraphPresetButton {
                        label: option.label(),
                        selected: selected_sampling == option,
                        onclick: move |_| sampling_granularity.set(option)
                    }
                }
            }
        }
        match history_state {
            None => rsx! {
                GraphLoadingPanel {
                    range: range_summary_text(&start_date, &end_date),
                    sampling: selected_sampling.label()
                }
            },
            Some(Err(error)) => rsx! {
                InlineStatus { title: title.clone(), message: error }
            },
            Some(Ok(_)) => rsx! {
                NetWorthChart {
                    data: sampled_data.clone(),
                    currency: currency.clone(),
                    y_domain,
                    empty_title: empty_title.clone(),
                    empty_detail: empty_detail.clone(),
                }
                if !sampled_data.is_empty() {
                    div { class: "chart-stats",
                        strong { "{format_full_money(current_value, &currency)}" }
                        span { class: "{change_class}",
                            "{format_signed_money(absolute_change, &currency)} ({percent_text})"
                        }
                    }
                }
            }
        }
        div { class: "chart-controls chart-bottom-controls",
            div { class: "control-grid",
                DateInput {
                    label: "Start",
                    value: start_date.clone(),
                    min: min_date.clone(),
                    max: end_date.clone(),
                    oninput: move |value| {
                        start_override.set(value);
                        range_preset.set(RangePreset::Custom);
                    }
                }
                DateInput {
                    label: "End",
                    value: end_date.clone(),
                    min: start_date.clone(),
                    max: max_date.clone(),
                    oninput: move |value| {
                        end_override.set(value);
                        range_preset.set(RangePreset::Custom);
                    }
                }
                NumberInput {
                    label: "Min",
                    value: y_min_text.clone(),
                    oninput: move |value| y_min_input.set(value)
                }
                NumberInput {
                    label: "Max",
                    value: y_max_text.clone(),
                    oninput: move |value| y_max_input.set(value)
                }
            }
            if has_date_error {
                p { class: "validation", "Use a valid start date before end date." }
            }
            if has_y_error {
                p { class: "validation", "Y min must be less than Y max." }
            }
            div { class: "range-summary",
                span { "Date range {start_date} to {end_date}" }
                span { "Data range {data_y_range}" }
                span { "Axis range {axis_y_range}" }
                span { "Sampling {sampling_label} / {sampled_point_count} points" }
            }
        }
    }
}

#[component]
fn NetWorthChart(
    data: Vec<NetWorthDataPoint>,
    currency: String,
    y_domain: Option<(f64, f64)>,
    empty_title: String,
    empty_detail: String,
) -> Element {
    let values = data
        .iter()
        .map(|point| (point.date.clone(), point.value))
        .collect::<Vec<_>>();

    if values.is_empty() {
        return rsx! {
            div { class: "chart-empty",
                strong { "{empty_title}" }
                small { "{empty_detail}" }
            }
        };
    }

    let width = 720.0;
    let height = 260.0;
    let padding_left = 68.0;
    let padding_right = 20.0;
    let padding_top = 18.0;
    let padding_bottom = 38.0;
    let plot_width = width - padding_left - padding_right;
    let plot_height = height - padding_top - padding_bottom;

    let min_value = values
        .iter()
        .map(|(_, value)| *value)
        .fold(f64::INFINITY, f64::min);
    let max_value = values
        .iter()
        .map(|(_, value)| *value)
        .fold(f64::NEG_INFINITY, f64::max);
    let (y_min, y_max) = if let Some((min, max)) = y_domain {
        (min, max)
    } else {
        let range = (max_value - min_value).abs();
        let padding = if range == 0.0 {
            (max_value.abs() * 0.05).max(1.0)
        } else {
            range * 0.08
        };
        (min_value - padding, max_value + padding)
    };
    let y_range = (y_max - y_min).max(1.0);
    let count = values.len();

    let chart_points = values
        .iter()
        .enumerate()
        .map(|(index, (date, value))| {
            let x = if count <= 1 {
                padding_left + plot_width / 2.0
            } else {
                padding_left + (index as f64 / (count - 1) as f64) * plot_width
            };
            let y = padding_top + ((y_max - value) / y_range) * plot_height;
            ChartPoint {
                date: date.clone(),
                value: *value,
                x,
                y,
            }
        })
        .collect::<Vec<_>>();

    let line_path = chart_points
        .iter()
        .enumerate()
        .map(|(index, point)| {
            let command = if index == 0 { "M" } else { "L" };
            format!("{command} {:.2} {:.2}", point.x, point.y)
        })
        .collect::<Vec<_>>()
        .join(" ");
    let area_path = match (chart_points.first(), chart_points.last()) {
        (Some(first), Some(last)) => format!(
            "{line_path} L {:.2} {:.2} L {:.2} {:.2} Z",
            last.x,
            padding_top + plot_height,
            first.x,
            padding_top + plot_height
        ),
        _ => String::new(),
    };
    let hover_points = chart_points
        .iter()
        .enumerate()
        .map(|(index, point)| {
            let previous_x = if index == 0 {
                padding_left
            } else {
                (chart_points[index - 1].x + point.x) / 2.0
            };
            let next_x = if index + 1 == chart_points.len() {
                width - padding_right
            } else {
                (point.x + chart_points[index + 1].x) / 2.0
            };
            ChartHoverPoint {
                index,
                point: point.clone(),
                hit_x: previous_x,
                hit_width: (next_x - previous_x).max(1.0),
            }
        })
        .collect::<Vec<_>>();
    let hover_rules = hover_points
        .iter()
        .map(|hover_point| {
            format!(
                ".chart-hit-zone-{0}:hover ~ .chart-hover-detail-{0} {{ display: block; }}",
                hover_point.index
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let latest = chart_points.last().expect("values is non-empty");
    let first = chart_points.first().expect("values is non-empty");
    let y_mid = y_min + y_range / 2.0;
    let latest_value = format_compact_money(latest.value, &currency);
    let min_label = format_compact_money(y_min, &currency);
    let mid_label = format_compact_money(y_mid, &currency);
    let max_label = format_compact_money(y_max, &currency);
    let first_date = first.date.clone();
    let latest_date = latest.date.clone();
    let absolute_change = latest.value - first.value;
    let percentage_change = if first.value == 0.0 {
        None
    } else {
        Some((absolute_change / first.value) * 100.0)
    };
    let summary = percentage_change
        .map(|percentage| {
            format!(
                "{} ({}%)",
                format_signed_money(absolute_change, &currency),
                format_number(percentage, 2)
            )
        })
        .unwrap_or_else(|| "No range change".to_string());

    rsx! {
        div { class: "chart-card",
            div { class: "chart-meta",
                div {
                    span { class: "metric-label", "Current" }
                    strong { "{latest_value}" }
                }
                div {
                    span { class: "metric-label", "Range change" }
                    strong { "{summary}" }
                }
            }
            svg {
                class: "net-worth-chart",
                view_box: "0 0 720 260",
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
                if chart_points.len() > 1 {
                    path { class: "chart-area", d: "{area_path}" }
                    path { class: "chart-line", d: "{line_path}" }
                }
                for point in chart_points {
                    circle {
                        class: "chart-point",
                        cx: "{point.x}",
                        cy: "{point.y}",
                        r: "3.4",
                        title { "{point.date}: {format_full_money(point.value, &currency)}" }
                    }
                }
                g { class: "chart-hover-layer",
                    for hover_point in hover_points.iter() {
                        rect {
                            class: "chart-hit-zone chart-hit-zone-{hover_point.index}",
                            x: "{hover_point.hit_x}",
                            y: "{padding_top}",
                            width: "{hover_point.hit_width}",
                            height: "{plot_height}"
                        }
                    }
                    for hover_point in hover_points {
                        ChartHoverDetail {
                            index: hover_point.index,
                            point: hover_point.point.clone(),
                            currency: currency.clone(),
                            chart_width: width,
                            padding_right,
                            padding_top
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn ChartHoverDetail(
    index: usize,
    point: ChartPoint,
    currency: String,
    chart_width: f64,
    padding_right: f64,
    padding_top: f64,
) -> Element {
    let tooltip_width = 184.0;
    let tooltip_height = 50.0;
    let tooltip_x = if point.x + tooltip_width + 12.0 > chart_width - padding_right {
        point.x - tooltip_width - 12.0
    } else {
        point.x + 12.0
    }
    .max(8.0);
    let tooltip_y = if point.y - tooltip_height - 10.0 < padding_top {
        point.y + 12.0
    } else {
        point.y - tooltip_height - 10.0
    }
    .max(8.0);
    let date_y = tooltip_y + 20.0;
    let value_y = tooltip_y + 38.0;
    let text_x = tooltip_x + 10.0;
    let value_text = format_full_money(point.value, &currency);

    rsx! {
        g { class: "chart-hover-detail chart-hover-detail-{index}",
            line {
                class: "chart-hover-line",
                x1: "{point.x}",
                x2: "{point.x}",
                y1: "{padding_top}",
                y2: "{point.y}"
            }
            circle {
                class: "chart-hover-point",
                cx: "{point.x}",
                cy: "{point.y}",
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
                "{point.date}"
            }
            text {
                class: "chart-tooltip-value",
                x: "{text_x}",
                y: "{value_y}",
                "{value_text}"
            }
        }
    }
}
