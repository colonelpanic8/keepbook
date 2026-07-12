use super::*;
use std::collections::{HashMap, HashSet};

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
        Panel {
            class: "graph-panel",
            title: "Account Value Over Time",
            subtitle: "{selected_connection}",
            actions: rsx! {
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
            },
            if selected_id.is_empty() {
                div { class: "chart-empty",
                    strong { "No accounts" }
                    small { "Refresh balances or add an account to populate account charts." }
                }
            } else {
                HistoryGraphPanel {
                    title: selected_name.clone(),
                    scope_label: selected_connection.clone(),
                    empty_title: "No account history".to_string(),
                    empty_detail: "Refresh balances for this account to populate the chart.".to_string(),
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
pub(super) fn StackedHistoryGraphPanel(
    currency: String,
    defaults: HistoryDefaults,
    filter_overrides: FilterOverrides,
) -> Element {
    let initial_range_preset = range_preset_from_config(&defaults.graph_range);
    let initial_sampling_granularity =
        sampling_granularity_from_config(&defaults.graph_granularity);
    let mut range_preset = use_signal(move || initial_range_preset);
    let mut start_override = use_signal(String::new);
    let mut end_override = use_signal(String::new);
    let mut sampling_granularity = use_signal(move || initial_sampling_granularity);
    let mut expanded_accounts = use_signal(HashSet::<String>::new);
    let mut group_minor_series = use_signal(|| false);
    let mut minor_series_threshold = use_signal(|| "2".to_string());
    let refresh_epoch = use_context::<Signal<u64>>();
    let history = use_resource(move || {
        let _refresh_epoch = refresh_epoch();
        let selected_range = range_preset();
        let start_text = start_override();
        let end_text = end_override();
        let selected_sampling = sampling_granularity();
        let current_filter_overrides = filter_overrides.clone();
        async move {
            fetch_stacked_history(history_query_string(
                selected_range,
                &start_text,
                &end_text,
                selected_sampling,
                &current_date_string(),
                current_filter_overrides,
                None,
            ))
            .await
        }
    });

    let selected_range = range_preset();
    let selected_sampling = sampling_granularity();
    let group_minor = group_minor_series();
    let threshold_text = minor_series_threshold();
    let threshold_percent = threshold_text
        .trim()
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite() && *value > 0.0)
        .unwrap_or_default();
    let start_text = start_override();
    let end_text = end_override();
    let history_state = history.cloned();
    let is_history_loading = history_state.is_none();
    let loaded_history = match &history_state {
        Some(Ok(history)) => Some(history),
        _ => None,
    };
    let data = loaded_history
        .map(stacked_history_data_points)
        .unwrap_or_default();
    let bounds = stacked_date_bounds(&data);
    let (start_date, end_date) =
        visible_stacked_date_range(&data, selected_range, &start_text, &end_text);
    let visible_data = filter_stacked_data_by_date_range(&data, &start_date, &end_date);
    let resolved_sampling = resolve_stacked_sampling_granularity(selected_sampling, &visible_data);
    let sampled_data = sample_stacked_data_by_granularity(&visible_data, resolved_sampling);
    let sampled_point_count = sampled_data.len();
    let sampling_label = resolved_sampling.label();
    let current_total = sampled_data
        .last()
        .map(|point| point.total)
        .unwrap_or_default();
    let start_total = sampled_data
        .first()
        .map(|point| point.total)
        .unwrap_or_default();
    let absolute_change = current_total - start_total;
    let change_class = if absolute_change >= 0.0 {
        "change-positive"
    } else {
        "change-negative"
    };
    let min_date = bounds
        .as_ref()
        .map(|bounds| bounds.0.clone())
        .unwrap_or_default();
    let max_date = bounds
        .as_ref()
        .map(|bounds| bounds.1.clone())
        .unwrap_or_default();
    let account_series = loaded_history
        .map(account_stacked_series)
        .unwrap_or_default();
    let active_series = loaded_history
        .map(|history| active_stacked_series(history, &expanded_accounts()))
        .unwrap_or_default();
    let (display_data, display_series) = if group_minor {
        coalesce_minor_stacked_series(&sampled_data, &active_series, threshold_percent)
    } else {
        (sampled_data.clone(), active_series.clone())
    };
    let has_date_error = !start_date.is_empty() && !end_date.is_empty() && start_date > end_date;

    rsx! {
        Panel {
            class: "graph-panel stacked-graph-panel",
            title: "Net Worth Contributions",
            subtitle: "Zero baseline / stacked by account",
            if is_history_loading {
                BackendActivity { message: "Waiting on backend contribution data" }
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
                div { class: "sampling-row minor-series-row",
                    label { class: "stacked-account-toggle minor-series-toggle",
                        input {
                            r#type: "checkbox",
                            checked: group_minor,
                            onchange: move |_| group_minor_series.set(!group_minor_series())
                        }
                        span { "Group small" }
                    }
                    label { class: "minor-series-threshold",
                        span { class: "control-label", "Below" }
                        input {
                            class: "control-input minor-series-input",
                            r#type: "number",
                            min: "0",
                            max: "100",
                            step: "0.1",
                            disabled: !group_minor,
                            value: "{threshold_text}",
                            oninput: move |event| minor_series_threshold.set(event.value())
                        }
                        span { class: "control-label", "%" }
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
                    InlineStatus { title: "Net Worth Contributions", message: error }
                },
                Some(Ok(_)) => rsx! {
                    StackedNetWorthChart {
                        data: display_data.clone(),
                        series: display_series.clone(),
                        currency: currency.clone(),
                        onselectrange: move |(start, end): (String, String)| {
                            start_override.set(start);
                            end_override.set(end);
                            range_preset.set(RangePreset::Custom);
                        },
                    }
                    if !sampled_data.is_empty() {
                        div { class: "chart-stats",
                            strong { "{format_full_money(current_total, &currency)}" }
                            span { class: "{change_class}",
                                "{format_signed_money(absolute_change, &currency)}"
                            }
                        }
                    }
                    StackedSeriesControls {
                        accounts: account_series.clone(),
                        expanded_accounts: expanded_accounts(),
                        ontoggle: move |account_id: String| {
                            let mut next = expanded_accounts();
                            if !next.insert(account_id.clone()) {
                                next.remove(&account_id);
                            }
                            expanded_accounts.set(next);
                        }
                    }
                }
            }
            div { class: "chart-controls chart-bottom-controls",
                div { class: "control-grid compact-date-grid",
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
                }
                if has_date_error {
                    p { class: "validation", "Use a valid start date before end date." }
                }
                div { class: "range-summary",
                    span { "Date range {start_date} to {end_date}" }
                    span { "Sampling {sampling_label} / {sampled_point_count} points" }
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
    let refresh_epoch = use_context::<Signal<u64>>();
    let history = use_resource(move || {
        let _refresh_epoch = refresh_epoch();
        let selected_range = range_preset();
        let start_text = start_override();
        let end_text = end_override();
        let selected_sampling = sampling_granularity();
        let selected_account = account.clone();
        let current_filter_overrides = filter_overrides.clone();
        async move {
            fetch_history(history_query_string(
                selected_range,
                &start_text,
                &end_text,
                selected_sampling,
                &current_date_string(),
                current_filter_overrides,
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
                ControlButton {
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
                    onselectrange: move |(start, end): (String, String)| {
                        start_override.set(start);
                        end_override.set(end);
                        range_preset.set(RangePreset::Custom);
                    },
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
    onselectrange: EventHandler<(String, String)>,
) -> Element {
    let mut drag_start = use_signal(|| None::<usize>);
    let mut drag_current = use_signal(|| None::<usize>);
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
    let drag_selection = chart_drag_selection(
        drag_start(),
        drag_current(),
        hover_points.iter().map(|point| point.point.x).collect(),
        padding_top,
        plot_height,
    );
    let date_values = hover_points
        .iter()
        .map(|point| point.point.date.clone())
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
                if let Some(selection) = drag_selection {
                    rect {
                        class: "chart-drag-selection",
                        x: "{selection.x}",
                        y: "{selection.y}",
                        width: "{selection.width}",
                        height: "{selection.height}"
                    }
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
                        {
                            let index = hover_point.index;
                            let dates_for_mouseup = date_values.clone();
                            rsx! {
                        rect {
                            class: "chart-hit-zone chart-hit-zone-{hover_point.index}",
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
fn StackedSeriesControls(
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
fn StackedNetWorthChart(
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
                strong { "No contribution history" }
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
                        style: "fill: {layer.color}; stroke: {layer.color};",
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

#[derive(Clone, Debug)]
struct StackPoint {
    series_key: String,
    x: f64,
    y0_value: f64,
    y1_value: f64,
}

#[derive(Clone, Debug, PartialEq)]
struct StackedLayerRender {
    index: usize,
    series: ActiveStackedSeries,
    path: String,
    color: &'static str,
}

#[derive(Clone, Debug, PartialEq)]
struct StackedHoverPoint {
    index: usize,
    date: String,
    total: f64,
    x: f64,
    y: f64,
    hit_x: f64,
    hit_width: f64,
    breakdown: Vec<StackedTooltipRow>,
}

#[derive(Clone, Debug, PartialEq)]
struct StackedTooltipRow {
    label: String,
    value: f64,
    color: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct ChartDragSelection {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

fn account_stacked_series(history: &StackedHistory) -> Vec<StackedHistorySeries> {
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

fn active_stacked_series(
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

fn build_stack_points(
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

fn stacked_y_domain(points: &[StackPoint], data: &[StackedHistoryDataPoint]) -> (f64, f64) {
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

fn stack_area_path(
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

fn stacked_y(value: f64, y_min: f64, y_range: f64, padding_top: f64, plot_height: f64) -> f64 {
    padding_top + ((y_min + y_range - value) / y_range) * plot_height
}

fn chart_drag_selection(
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

fn selected_date_range(
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
fn stacked_hover_points(
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

fn tooltip_label(label: &str, max_chars: usize) -> String {
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

fn stacked_chart_color(index: usize) -> &'static str {
    const COLORS: [&str; 18] = [
        "var(--series-1)",
        "var(--series-2)",
        "var(--series-3)",
        "var(--series-4)",
        "var(--series-5)",
        "var(--series-6)",
        "var(--series-7)",
        "var(--series-8)",
        "var(--series-9)",
        "var(--series-10)",
        "var(--series-11)",
        "var(--series-12)",
        "var(--series-13)",
        "var(--series-14)",
        "var(--series-15)",
        "var(--series-16)",
        "var(--series-17)",
        "var(--series-18)",
    ];
    COLORS[index % COLORS.len()]
}
