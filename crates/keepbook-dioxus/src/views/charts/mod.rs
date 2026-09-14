mod geometry;
mod net_worth;
mod stacked;

use super::*;
use std::collections::HashSet;

use geometry::*;
use net_worth::*;
use stacked::*;

/// Range presets offered by the net-worth graph panels, in display order.
const CHART_RANGE_PRESETS: [(RangePreset, &str); 6] = [
    (RangePreset::OneMonth, "1M"),
    (RangePreset::NinetyDays, "90D"),
    (RangePreset::SixMonths, "6M"),
    (RangePreset::OneYear, "1Y"),
    (RangePreset::TwoYears, "2Y"),
    (RangePreset::Max, "Max"),
];

fn sampling_options() -> Vec<SegmentedOption> {
    SamplingGranularity::OPTIONS
        .iter()
        .map(|option| SegmentedOption::new(option.value(), option.label()))
        .collect()
}

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
        .map(|history| stacked_history_data_points_with_current(history, &current_date_string()))
        .unwrap_or_default();
    let bounds = date_bounds(&data);
    let (start_date, end_date) = visible_date_range(&data, selected_range, &start_text, &end_text);
    let visible_data = filter_data_by_date_range(&data, &start_date, &end_date);
    let resolved_sampling = resolve_sampling_granularity(selected_sampling, &visible_data);
    let sampled_data = sample_data_by_granularity(&visible_data, resolved_sampling);
    let sampled_point_count = sampled_data.len();
    let sampling_label = resolved_sampling.label();
    let current_value_text = loaded_history
        .and_then(|history| history.current.as_ref().or(history.points.last()))
        .map(|point| point.total_value.clone())
        .unwrap_or_default();
    let current_label = format_money_text(&current_value_text, &currency).unwrap_or_default();
    let change_summary = history_change_summary(
        loaded_history.and_then(|history| history.summary.as_ref()),
        &currency,
    );
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
            title: "Net Worth Breakdown",
            subtitle: "Zero baseline / stacked by account",
            if is_history_loading {
                BackendActivity { message: "Waiting on backend net worth data" }
            }
            div { class: "chart-controls",
                SegmentedControl {
                    label: "Range",
                    options: range_preset_options(&CHART_RANGE_PRESETS),
                    selected: selected_range.value().to_string(),
                    onselect: move |value: String| {
                        let Some(preset) = range_preset_from_value(&CHART_RANGE_PRESETS, &value)
                        else {
                            return;
                        };
                        range_preset.set(preset);
                        start_override.set(String::new());
                        end_override.set(String::new());
                    }
                }
                SegmentedControl {
                    label: "Sampling",
                    options: sampling_options(),
                    selected: selected_sampling.value().to_string(),
                    onselect: move |value: String| {
                        if let Some(option) = SamplingGranularity::from_value(&value) {
                            sampling_granularity.set(option);
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
                    InlineStatus { title: "Net Worth Breakdown", message: error }
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
                            strong { "{current_label}" }
                            span { class: "{change_summary.class}", "{change_summary.text}" }
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
    let local_today = current_date_string();
    let current_point = loaded_history.and_then(|history| history.current.as_ref());
    let data = loaded_history
        .map(|history| {
            match current_point.and_then(|point| parse_money_input(&point.total_value)) {
                Some(value) => {
                    history_data_points_with_current_snapshot(history, &local_today, value)
                }
                None => history_data_points(history),
            }
        })
        .unwrap_or_default();
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
    let current_value_text = current_point
        .or_else(|| loaded_history.and_then(|history| history.points.last()))
        .map(|point| point.total_value.clone())
        .unwrap_or_default();
    let current_label = format_money_text(&current_value_text, &currency).unwrap_or_default();
    let change_summary = history_change_summary(
        loaded_history.and_then(|history| history.summary.as_ref()),
        &currency,
    );
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
            SegmentedControl {
                label: "Range",
                options: range_preset_options(&CHART_RANGE_PRESETS),
                selected: selected_range.value().to_string(),
                onselect: move |value: String| {
                    let Some(preset) = range_preset_from_value(&CHART_RANGE_PRESETS, &value) else {
                        return;
                    };
                    range_preset.set(preset);
                    start_override.set(String::new());
                    end_override.set(String::new());
                }
            }
            SegmentedControl {
                label: "Sampling",
                options: sampling_options(),
                selected: selected_sampling.value().to_string(),
                onselect: move |value: String| {
                    if let Some(option) = SamplingGranularity::from_value(&value) {
                        sampling_granularity.set(option);
                    }
                }
            }
            // "Fit Y" is an action, not one of the range options, so it lives in
            // its own row instead of taking a slot in the range group.
            div { class: "preset-row",
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
                    current_value_text: current_value_text.clone(),
                    change_summary: change_summary.clone(),
                    onselectrange: move |(start, end): (String, String)| {
                        start_override.set(start);
                        end_override.set(end);
                        range_preset.set(RangePreset::Custom);
                    },
                }
                if !sampled_data.is_empty() {
                    div { class: "chart-stats",
                        strong { "{current_label}" }
                        span { class: "{change_summary.class}", "{change_summary.text}" }
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
