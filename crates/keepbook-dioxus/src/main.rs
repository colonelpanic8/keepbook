use dioxus::prelude::*;
use serde::Deserialize;

#[cfg(target_arch = "wasm32")]
use gloo_net::http::Request;

static CSS: Asset = asset!("/assets/styles.css");
const API_BASE: &str = "http://127.0.0.1:8799";
const DEFAULT_RANGE_PRESET: RangePreset = RangePreset::OneYear;
const DEFAULT_SAMPLING_GRANULARITY: SamplingGranularity = SamplingGranularity::Weekly;

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct Overview {
    data_dir: String,
    reporting_currency: String,
    connections: Vec<Connection>,
    accounts: Vec<Account>,
    balances: Vec<Balance>,
    snapshot: PortfolioSnapshot,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct Connection {
    id: String,
    name: String,
    synchronizer: String,
    status: String,
    account_count: usize,
    last_sync: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct Account {
    id: String,
    name: String,
    connection_id: String,
    tags: Vec<String>,
    active: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct Balance {
    account_id: String,
    asset: serde_json::Value,
    amount: String,
    value_in_reporting_currency: Option<String>,
    reporting_currency: String,
    timestamp: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct History {
    currency: String,
    points: Vec<HistoryPoint>,
    summary: Option<HistorySummary>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct HistoryPoint {
    date: String,
    total_value: String,
    percentage_change_from_previous: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct HistorySummary {
    initial_value: String,
    final_value: String,
    absolute_change: String,
    percentage_change: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct PortfolioSnapshot {
    as_of_date: String,
    currency: String,
    total_value: String,
}

#[derive(Clone, Debug, PartialEq)]
struct NetWorthDataPoint {
    date: String,
    value: f64,
}

#[derive(Clone, Debug, PartialEq)]
struct ChartPoint {
    date: String,
    value: f64,
    x: f64,
    y: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RangePreset {
    OneMonth,
    NinetyDays,
    SixMonths,
    OneYear,
    TwoYears,
    Max,
    Custom,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SamplingGranularity {
    Auto,
    Daily,
    Weekly,
    Monthly,
    Yearly,
}

impl SamplingGranularity {
    const OPTIONS: [Self; 5] = [
        Self::Auto,
        Self::Daily,
        Self::Weekly,
        Self::Monthly,
        Self::Yearly,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Auto => "Auto",
            Self::Daily => "Daily",
            Self::Weekly => "Weekly",
            Self::Monthly => "Monthly",
            Self::Yearly => "Yearly",
        }
    }

    fn query_value(self) -> &'static str {
        match self {
            Self::Auto => "daily",
            Self::Daily => "daily",
            Self::Weekly => "weekly",
            Self::Monthly => "monthly",
            Self::Yearly => "yearly",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ActiveView {
    Summary,
    Graphs,
    Accounts,
    Connections,
    Balances,
    History,
}

impl ActiveView {
    const ALL: [Self; 6] = [
        Self::Summary,
        Self::Graphs,
        Self::Accounts,
        Self::Connections,
        Self::Balances,
        Self::History,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Summary => "Summary",
            Self::Graphs => "Graphs",
            Self::Accounts => "Accounts",
            Self::Connections => "Connections",
            Self::Balances => "Balances",
            Self::History => "History",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
enum LoadState {
    Loading,
    Failed(String),
}

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    let mut overview = use_resource(fetch_overview);

    rsx! {
        document::Stylesheet { href: CSS }
        main { class: "shell",
            match overview.cloned() {
                None => rsx! { StatusPanel { state: LoadState::Loading } },
                Some(Ok(data)) => rsx! {
                    Dashboard {
                        overview: data,
                        onrefresh: move |_| overview.restart()
                    }
                },
                Some(Err(error)) => rsx! {
                    StatusPanel { state: LoadState::Failed(error) }
                },
            }
        }
    }
}

async fn fetch_overview() -> Result<Overview, String> {
    fetch_overview_impl().await
}

#[cfg(target_arch = "wasm32")]
async fn fetch_overview_impl() -> Result<Overview, String> {
    let response = Request::get(&format!("{API_BASE}/api/overview"))
        .send()
        .await
        .map_err(|error| format!("Could not reach keepbook-server at {API_BASE}: {error}"))?;

    if !response.ok() {
        return Err(format!(
            "keepbook-server returned HTTP {} {}",
            response.status(),
            response.status_text()
        ));
    }

    response
        .json::<Overview>()
        .await
        .map_err(|error| format!("Could not decode keepbook overview: {error}"))
}

#[cfg(not(target_arch = "wasm32"))]
async fn fetch_overview_impl() -> Result<Overview, String> {
    reqwest::get(format!("{API_BASE}/api/overview"))
        .await
        .map_err(|error| format!("Could not reach keepbook-server at {API_BASE}: {error}"))?
        .error_for_status()
        .map_err(|error| format!("keepbook-server returned an error: {error}"))?
        .json::<Overview>()
        .await
        .map_err(|error| format!("Could not decode keepbook overview: {error}"))
}

async fn fetch_history(query: String) -> Result<History, String> {
    fetch_history_impl(query).await
}

#[cfg(target_arch = "wasm32")]
async fn fetch_history_impl(query: String) -> Result<History, String> {
    let response = Request::get(&format!("{API_BASE}/api/portfolio/history?{query}"))
        .send()
        .await
        .map_err(|error| format!("Could not reach keepbook-server at {API_BASE}: {error}"))?;

    if !response.ok() {
        return Err(format!(
            "keepbook-server returned HTTP {} {}",
            response.status(),
            response.status_text()
        ));
    }

    response
        .json::<History>()
        .await
        .map_err(|error| format!("Could not decode net worth history: {error}"))
}

#[cfg(not(target_arch = "wasm32"))]
async fn fetch_history_impl(query: String) -> Result<History, String> {
    reqwest::get(format!("{API_BASE}/api/portfolio/history?{query}"))
        .await
        .map_err(|error| format!("Could not reach keepbook-server at {API_BASE}: {error}"))?
        .error_for_status()
        .map_err(|error| format!("keepbook-server returned an error: {error}"))?
        .json::<History>()
        .await
        .map_err(|error| format!("Could not decode net worth history: {error}"))
}

#[component]
fn StatusPanel(state: LoadState) -> Element {
    let message = match state {
        LoadState::Loading => "Loading local finance data...".to_string(),
        LoadState::Failed(error) => error,
    };

    rsx! {
        section { class: "status-panel",
            h2 { "Connection" }
            p { "{message}" }
        }
    }
}

#[component]
fn Dashboard(overview: Overview, onrefresh: EventHandler<MouseEvent>) -> Element {
    let mut active_view = use_signal(|| ActiveView::Summary);
    let mut nav_open = use_signal(|| false);
    let active = active_view();
    let total = current_net_worth_from_snapshot(&overview.snapshot);
    let last_date = overview.snapshot.as_of_date.clone();
    let active_accounts = overview
        .accounts
        .iter()
        .filter(|account| account.active)
        .count();
    let nav_class = if nav_open() {
        "app-nav open"
    } else {
        "app-nav"
    };

    rsx! {
        div { class: "app-shell",
            aside { class: "{nav_class}",
                div { class: "nav-title",
                    strong { "Keepbook" }
                    small { "{overview.reporting_currency}" }
                }
                nav {
                    for view in ActiveView::ALL {
                        NavButton {
                            label: view.label(),
                            selected: active == view,
                            onclick: move |_| {
                                active_view.set(view);
                                nav_open.set(false);
                            }
                        }
                    }
                }
            }
            div { class: "workspace",
                header { class: "topbar",
                    button {
                        class: "hamburger-button",
                        title: "Menu",
                        onclick: move |_| nav_open.set(!nav_open()),
                        span { class: "hamburger-line" }
                        span { class: "hamburger-line" }
                        span { class: "hamburger-line" }
                    }
                    div {
                        h1 { "{active.label()}" }
                    }
                    button {
                        class: "icon-button",
                        title: "Refresh",
                        onclick: move |event| onrefresh.call(event),
                        "Refresh"
                    }
                }
                match active {
                    ActiveView::Summary => rsx! {
                        SummaryView {
                            net_worth: total,
                            currency: overview.reporting_currency.clone(),
                            last_date: last_date.to_string(),
                            active_accounts,
                            total_accounts: overview.accounts.len(),
                            connection_count: overview.connections.len(),
                        }
                    },
                    ActiveView::Graphs => rsx! {
                        GraphsView {
                            currency: overview.reporting_currency.clone(),
                        }
                    },
                    ActiveView::Accounts => rsx! {
                        AccountsView {
                            accounts: overview.accounts.clone(),
                            connections: overview.connections.clone(),
                            balances: overview.balances.clone(),
                            currency: overview.reporting_currency.clone(),
                        }
                    },
                    ActiveView::Connections => rsx! {
                        ConnectionsView { connections: overview.connections.clone() }
                    },
                    ActiveView::Balances => rsx! {
                        BalancesView { balances: overview.balances.clone() }
                    },
                    ActiveView::History => rsx! {
                        HistoryView {
                            currency: overview.reporting_currency.clone()
                        }
                    },
                }
            }
        }
    }
}

#[component]
fn NavButton(label: &'static str, selected: bool, onclick: EventHandler<MouseEvent>) -> Element {
    let class = if selected {
        "nav-button selected"
    } else {
        "nav-button"
    };

    rsx! {
        button {
            class: "{class}",
            onclick: move |event| onclick.call(event),
            "{label}"
        }
    }
}

#[component]
fn SummaryView(
    net_worth: f64,
    currency: String,
    last_date: String,
    active_accounts: usize,
    total_accounts: usize,
    connection_count: usize,
) -> Element {
    rsx! {
        section { class: "summary-grid",
            MetricCard {
                label: "Net worth",
                value: format_full_money(net_worth, &currency),
                detail: last_date
            }
            MetricCard {
                label: "Accounts",
                value: active_accounts.to_string(),
                detail: format!("{total_accounts} total")
            }
            MetricCard {
                label: "Connections",
                value: connection_count.to_string(),
                detail: "Configured sources".to_string()
            }
        }
    }
}

#[component]
fn GraphsView(currency: String) -> Element {
    rsx! {
        section { class: "panel graph-panel",
            NetWorthPanel { currency }
        }
    }
}

#[component]
fn InlineStatus(title: &'static str, message: String) -> Element {
    rsx! {
        div { class: "inline-status",
            h2 { "{title}" }
            p { "{message}" }
        }
    }
}

#[component]
fn MetricCard(label: String, value: String, detail: String) -> Element {
    rsx! {
        article { class: "metric",
            span { class: "metric-label", "{label}" }
            strong { "{value}" }
            small { "{detail}" }
        }
    }
}

#[component]
fn NetWorthPanel(currency: String) -> Element {
    let mut range_preset = use_signal(|| DEFAULT_RANGE_PRESET);
    let mut start_override = use_signal(String::new);
    let mut end_override = use_signal(String::new);
    let mut y_min_input = use_signal(String::new);
    let mut y_max_input = use_signal(String::new);
    let mut sampling_granularity = use_signal(|| DEFAULT_SAMPLING_GRANULARITY);
    let history = use_resource(move || {
        let selected_range = range_preset();
        let start_text = start_override();
        let end_text = end_override();
        let selected_sampling = sampling_granularity();
        async move {
            fetch_history(history_query_string(
                selected_range,
                &start_text,
                &end_text,
                selected_sampling,
                &current_date_string(),
            ))
            .await
        }
    });

    let selected_range = range_preset();
    let selected_sampling = sampling_granularity();
    let start_text = start_override();
    let end_text = end_override();
    let history_state = history.cloned();
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
    let header_currency = loaded_history
        .map(|history| history.currency.clone())
        .unwrap_or_else(|| currency.clone());

    rsx! {
        div { class: "panel-header",
            h2 { "Net Worth Over Time" }
            span { "{header_currency}" }
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
        match history_state {
            None => rsx! {
                InlineStatus { title: "Net Worth Over Time", message: "Loading graph data..." }
            },
            Some(Err(error)) => rsx! {
                InlineStatus { title: "Net Worth Over Time", message: error }
            },
            Some(Ok(_)) => rsx! {
                NetWorthChart {
                    data: sampled_data.clone(),
                    currency: currency.clone(),
                    y_domain
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
    }
}

#[component]
fn GraphPresetButton(
    label: &'static str,
    selected: bool,
    onclick: EventHandler<MouseEvent>,
) -> Element {
    let class = if selected {
        "control-button selected"
    } else {
        "control-button"
    };

    rsx! {
        button {
            class: "{class}",
            onclick: move |event| onclick.call(event),
            "{label}"
        }
    }
}

#[component]
fn DateInput(
    label: &'static str,
    value: String,
    min: String,
    max: String,
    oninput: EventHandler<String>,
) -> Element {
    rsx! {
        label { class: "control-field",
            span { "{label}" }
            input {
                class: "control-input",
                r#type: "date",
                value: "{value}",
                min: "{min}",
                max: "{max}",
                oninput: move |event| oninput.call(event.value())
            }
        }
    }
}

#[component]
fn NumberInput(label: &'static str, value: String, oninput: EventHandler<String>) -> Element {
    rsx! {
        label { class: "control-field",
            span { "{label}" }
            input {
                class: "control-input",
                r#type: "number",
                value: "{value}",
                step: "0.01",
                oninput: move |event| oninput.call(event.value())
            }
        }
    }
}

#[component]
fn NetWorthChart(
    data: Vec<NetWorthDataPoint>,
    currency: String,
    y_domain: Option<(f64, f64)>,
) -> Element {
    let values = data
        .iter()
        .map(|point| (point.date.clone(), point.value))
        .collect::<Vec<_>>();

    if values.is_empty() {
        return rsx! {
            div { class: "chart-empty",
                strong { "No net worth history" }
                small { "Sync balances to populate the chart." }
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
            }
        }
    }
}

#[component]
fn AccountsView(
    accounts: Vec<Account>,
    connections: Vec<Connection>,
    balances: Vec<Balance>,
    currency: String,
) -> Element {
    rsx! {
        section { class: "panel",
            div { class: "panel-header",
                h2 { "Accounts" }
                span { "{accounts.len()}" }
            }
            div { class: "group-list",
                for connection in connections {
                    AccountGroup {
                        connection: connection.clone(),
                        accounts: accounts
                            .iter()
                            .filter(|account| account.connection_id == connection.id)
                            .cloned()
                            .collect::<Vec<_>>(),
                        balances: balances.clone(),
                        currency: currency.clone(),
                    }
                }
            }
        }
    }
}

#[component]
fn AccountGroup(
    connection: Connection,
    accounts: Vec<Account>,
    balances: Vec<Balance>,
    currency: String,
) -> Element {
    let active_count = accounts.iter().filter(|account| account.active).count();

    rsx! {
        section { class: "tree-group",
            div { class: "tree-parent",
                div {
                    strong { "{connection.name}" }
                    small { "{connection.synchronizer}" }
                }
                span { class: "status", "{active_count}/{accounts.len()} active" }
            }
            div { class: "data-table account-table",
                div { class: "table-head",
                    span { "Account" }
                    span { "Balance ({currency})" }
                    span { "Status" }
                    span { "Tags" }
                }
                for account in accounts {
                    AccountRow {
                        account,
                        balances: balances.clone(),
                        currency: currency.clone(),
                    }
                }
            }
        }
    }
}

#[component]
fn AccountRow(account: Account, balances: Vec<Balance>, currency: String) -> Element {
    let status = if account.active { "Active" } else { "Inactive" };
    let tags = account.tags.join(", ");
    let balance = account_base_value(&account.id, &balances)
        .map(|value| format_full_money(value, &currency))
        .unwrap_or_else(|| "N/A".to_string());

    rsx! {
        div { class: "table-row",
            strong { "{account.name}" }
            span { "{balance}" }
            span { "{status}" }
            small { "{tags}" }
        }
    }
}

#[component]
fn ConnectionsView(connections: Vec<Connection>) -> Element {
    rsx! {
        section { class: "panel",
            div { class: "panel-header",
                h2 { "Connections" }
                span { "{connections.len()}" }
            }
            div { class: "data-table connection-table",
                div { class: "table-head",
                    span { "Name" }
                    span { "Sync" }
                    span { "Accounts" }
                    span { "Last sync" }
                }
                for connection in connections {
                    div { class: "table-row",
                        strong { "{connection.name}" }
                        span { class: "status", "{connection.status}" }
                        span { "{connection.account_count}" }
                        small {
                            "{connection.last_sync.clone().unwrap_or_else(|| \"Never\".to_string())}"
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn BalancesView(balances: Vec<Balance>) -> Element {
    rsx! {
        section { class: "panel",
            div { class: "panel-header",
                h2 { "Balances" }
                span { "{balances.len()}" }
            }
            div { class: "data-table balance-table",
                div { class: "table-head",
                    span { "Asset" }
                    span { "Amount" }
                    span { "Account" }
                    span { "Value" }
                    span { "Timestamp" }
                }
                for balance in balances {
                    div { class: "table-row",
                        strong { "{asset_label(&balance.asset)}" }
                        span { "{balance.amount}" }
                        small { "{balance.account_id}" }
                        span {
                            "{balance.value_in_reporting_currency.clone().unwrap_or_else(|| \"N/A\".to_string())} {balance.reporting_currency}"
                        }
                        small { "{balance.timestamp}" }
                    }
                }
            }
        }
    }
}

#[component]
fn HistoryView(currency: String) -> Element {
    let history = use_resource(|| async {
        fetch_history(history_query_string(
            DEFAULT_RANGE_PRESET,
            "",
            "",
            DEFAULT_SAMPLING_GRANULARITY,
            &current_date_string(),
        ))
        .await
    });
    rsx! {
        section { class: "panel",
            match history.cloned() {
                None => rsx! { InlineStatus { title: "Net Worth History", message: "Loading history..." } },
                Some(Ok(history)) => rsx! {
                    HistoryTable { history, currency }
                },
                Some(Err(error)) => rsx! {
                    InlineStatus { title: "Net Worth History", message: error }
                },
            }
        }
    }
}

#[component]
fn HistoryTable(history: History, currency: String) -> Element {
    let row_count = history.points.len();

    rsx! {
        div { class: "panel-header",
            h2 { "Net Worth History" }
            span { "{row_count}" }
        }
        div { class: "data-table history-table",
            div { class: "table-head",
                span { "Date" }
                span { "Net worth" }
                span { "Daily change" }
            }
            for point in history.points.iter().rev() {
                HistoryPointRow {
                    point: point.clone(),
                    currency: currency.clone()
                }
            }
        }
    }
}

#[component]
fn HistoryPointRow(point: HistoryPoint, currency: String) -> Element {
    let total = point.total_value.parse::<f64>().unwrap_or_default();
    let total_text = format_full_money(total, &currency);
    let change_text = point
        .percentage_change_from_previous
        .map(|value| format!("{value}%"))
        .unwrap_or_else(|| "N/A".to_string());

    rsx! {
        div { class: "table-row",
            strong { "{point.date}" }
            span { "{total_text}" }
            small { "{change_text}" }
        }
    }
}

fn asset_label(asset: &serde_json::Value) -> String {
    asset
        .get("symbol")
        .and_then(|value| value.as_str())
        .or_else(|| asset.get("currency").and_then(|value| value.as_str()))
        .or_else(|| asset.get("ticker").and_then(|value| value.as_str()))
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| asset.to_string())
}

fn current_net_worth_from_snapshot(snapshot: &PortfolioSnapshot) -> f64 {
    snapshot
        .total_value
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite())
        .unwrap_or_default()
}

fn history_data_points(history: &History) -> Vec<NetWorthDataPoint> {
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

fn date_bounds(points: &[NetWorthDataPoint]) -> Option<(String, String)> {
    Some((points.first()?.date.clone(), points.last()?.date.clone()))
}

fn visible_date_range(
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

fn history_query_string(
    preset: RangePreset,
    start_override: &str,
    end_override: &str,
    selected_sampling: SamplingGranularity,
    today: &str,
) -> String {
    let (start, end) = requested_history_date_range(preset, start_override, end_override, today);
    let granularity =
        history_request_granularity(selected_sampling, start.as_deref(), end.as_deref());
    let mut params = vec![format!("granularity={granularity}")];

    if let Some(start) = start {
        params.push(format!("start={start}"));
    }
    if let Some(end) = end {
        params.push(format!("end={end}"));
    }

    params.join("&")
}

fn requested_history_date_range(
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

fn non_empty_string(value: &str) -> Option<String> {
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

fn history_request_granularity(
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
fn current_date_string() -> String {
    let date = js_sys::Date::new_0();
    format!(
        "{:04}-{:02}-{:02}",
        date.get_full_year(),
        date.get_month() + 1,
        date.get_date()
    )
}

#[cfg(not(target_arch = "wasm32"))]
fn current_date_string() -> String {
    chrono::Local::now().date_naive().to_string()
}

fn offset_years(date: &str, years: i32) -> String {
    offset_months(date, years * 12)
}

fn offset_months(date: &str, months: i32) -> String {
    let Some((year, month, day)) = parse_ymd(date) else {
        return date.to_string();
    };

    let month_index = year * 12 + month as i32 - 1 - months;
    let new_year = month_index.div_euclid(12);
    let new_month = month_index.rem_euclid(12) as u32 + 1;
    let new_day = day.min(days_in_month(new_year, new_month));
    format!("{new_year:04}-{new_month:02}-{new_day:02}")
}

fn offset_days(date: &str, days: i64) -> String {
    let Some((year, month, day)) = parse_ymd(date) else {
        return date.to_string();
    };
    civil_from_days(days_from_civil(year, month, day) - days)
}

fn filter_data_by_date_range(
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

fn resolve_sampling_granularity(
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

fn sample_data_by_granularity(
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

fn include_range_endpoints(
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

fn sampling_bucket(date: &str, granularity: SamplingGranularity) -> String {
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

fn days_between(start: &str, end: &str) -> Option<i64> {
    let (start_year, start_month, start_day) = parse_ymd(start)?;
    let (end_year, end_month, end_day) = parse_ymd(end)?;
    Some(
        days_from_civil(end_year, end_month, end_day)
            - days_from_civil(start_year, start_month, start_day),
    )
}

fn parse_ymd(date: &str) -> Option<(i32, u32, u32)> {
    let mut parts = date.split('-');
    let year = parts.next()?.parse::<i32>().ok()?;
    let month = parts.next()?.parse::<u32>().ok()?;
    let day = parts.next()?.parse::<u32>().ok()?;
    if parts.next().is_some() || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    Some((year, month, day))
}

fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 30,
    }
}

fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn days_from_civil(year: i32, month: u32, day: u32) -> i64 {
    let year = year - if month <= 2 { 1 } else { 0 };
    let era = (year as i64).div_euclid(400);
    let yoe = year as i64 - era * 400;
    let month = month as i64;
    let doy = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(days: i64) -> String {
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

fn value_bounds(points: &[NetWorthDataPoint]) -> Option<(f64, f64)> {
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

fn parse_money_input(value: &str) -> Option<f64> {
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

fn account_base_value(account_id: &str, balances: &[Balance]) -> Option<f64> {
    let account_balances = balances
        .iter()
        .filter(|balance| balance.account_id == account_id)
        .collect::<Vec<_>>();

    if account_balances.is_empty() {
        return None;
    }

    account_balances.iter().try_fold(0.0, |total, balance| {
        let value = balance.value_in_reporting_currency.as_deref()?;
        parse_money_input(value).map(|parsed| total + parsed)
    })
}

fn parse_y_domain(min: &str, max: &str) -> Option<(f64, f64)> {
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

fn format_input_number(value: f64) -> String {
    format_number(value, 2)
}

fn format_compact_money(value: f64, currency: &str) -> String {
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
    format!("{currency} {}{suffix}", format_number(scaled, 1))
}

fn format_full_money(value: f64, currency: &str) -> String {
    let sign = if value < 0.0 { "-" } else { "" };
    let abs = value.abs();
    let integer = abs.trunc() as i64;
    let decimals = ((abs.fract() * 100.0).round() as i64).min(99);
    let prefix = if currency.is_empty() {
        String::new()
    } else {
        format!("{currency} ")
    };
    format!(
        "{prefix}{sign}{}.{:02}",
        format_integer_with_commas(integer),
        decimals
    )
}

fn format_signed_money(value: f64, currency: &str) -> String {
    if value >= 0.0 {
        format!("+{}", format_full_money(value, currency))
    } else {
        format_full_money(value, currency)
    }
}

fn format_integer_with_commas(value: i64) -> String {
    let digits = value.to_string();
    let mut formatted = String::new();
    for (index, ch) in digits.chars().rev().enumerate() {
        if index > 0 && index % 3 == 0 {
            formatted.push(',');
        }
        formatted.push(ch);
    }
    formatted.chars().rev().collect()
}

fn format_number(value: f64, decimals: usize) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn point(date: &str, value: f64) -> NetWorthDataPoint {
        NetWorthDataPoint {
            date: date.to_string(),
            value,
        }
    }

    #[test]
    fn two_year_range_starts_two_years_before_latest_point() {
        let points = vec![
            point("2022-01-01", 100.0),
            point("2024-04-25", 150.0),
            point("2026-04-25", 200.0),
        ];

        assert_eq!(
            visible_date_range(&points, RangePreset::TwoYears, "", ""),
            ("2024-04-25".to_string(), "2026-04-25".to_string())
        );
    }

    #[test]
    fn two_year_range_clamps_to_earliest_available_point() {
        let points = vec![point("2026-02-03", 100.0), point("2026-04-25", 200.0)];

        assert_eq!(
            visible_date_range(&points, RangePreset::TwoYears, "", ""),
            ("2026-02-03".to_string(), "2026-04-25".to_string())
        );
    }

    #[test]
    fn custom_range_uses_manual_overrides() {
        let points = vec![point("2024-01-01", 100.0), point("2026-04-25", 200.0)];

        assert_eq!(
            visible_date_range(&points, RangePreset::Custom, "2025-01-01", "2025-12-31"),
            ("2025-01-01".to_string(), "2025-12-31".to_string())
        );
    }

    #[test]
    fn short_range_presets_use_expected_start_dates() {
        let points = vec![point("2025-01-01", 100.0), point("2026-04-25", 200.0)];

        assert_eq!(
            visible_date_range(&points, RangePreset::OneMonth, "", ""),
            ("2026-03-25".to_string(), "2026-04-25".to_string())
        );
        assert_eq!(
            visible_date_range(&points, RangePreset::NinetyDays, "", ""),
            ("2026-01-25".to_string(), "2026-04-25".to_string())
        );
        assert_eq!(
            visible_date_range(&points, RangePreset::SixMonths, "", ""),
            ("2025-10-25".to_string(), "2026-04-25".to_string())
        );
    }

    #[test]
    fn default_graph_query_requests_one_year_weekly_history() {
        assert_eq!(
            history_query_string(
                DEFAULT_RANGE_PRESET,
                "",
                "",
                DEFAULT_SAMPLING_GRANULARITY,
                "2026-04-25"
            ),
            "granularity=weekly&start=2025-04-25&end=2026-04-25"
        );
    }

    #[test]
    fn auto_graph_query_uses_daily_under_three_months() {
        assert_eq!(
            history_query_string(
                RangePreset::NinetyDays,
                "",
                "",
                SamplingGranularity::Auto,
                "2026-04-25"
            ),
            "granularity=daily&start=2026-01-25&end=2026-04-25"
        );
    }

    #[test]
    fn max_graph_query_uses_monthly_without_date_bounds() {
        assert_eq!(
            history_query_string(
                RangePreset::Max,
                "",
                "",
                SamplingGranularity::Auto,
                "2026-04-25"
            ),
            "granularity=monthly"
        );
    }

    #[test]
    fn month_offsets_clamp_to_valid_dates() {
        assert_eq!(offset_months("2026-03-31", 1), "2026-02-28");
        assert_eq!(offset_months("2024-03-31", 1), "2024-02-29");
        assert_eq!(offset_years("2024-02-29", 1), "2023-02-28");
    }

    #[test]
    fn auto_sampling_uses_daily_under_three_months() {
        let points = vec![point("2026-01-26", 100.0), point("2026-04-25", 200.0)];

        assert_eq!(
            resolve_sampling_granularity(SamplingGranularity::Auto, &points),
            SamplingGranularity::Daily
        );
    }

    #[test]
    fn auto_sampling_uses_weekly_for_two_year_ranges() {
        let points = vec![point("2024-04-25", 100.0), point("2026-04-25", 200.0)];

        assert_eq!(
            resolve_sampling_granularity(SamplingGranularity::Auto, &points),
            SamplingGranularity::Weekly
        );
    }

    #[test]
    fn sampled_series_preserves_range_endpoints() {
        let points = vec![
            point("2026-01-01", 100.0),
            point("2026-01-02", 110.0),
            point("2026-01-08", 120.0),
            point("2026-01-09", 130.0),
        ];

        let sampled = sample_data_by_granularity(&points, SamplingGranularity::Weekly);

        assert_eq!(
            sampled.first().map(|point| point.date.as_str()),
            Some("2026-01-01")
        );
        assert_eq!(
            sampled.last().map(|point| point.date.as_str()),
            Some("2026-01-09")
        );
    }

    #[test]
    fn current_net_worth_uses_portfolio_snapshot_total() {
        let snapshot = PortfolioSnapshot {
            as_of_date: "2026-04-25".to_string(),
            currency: "USD".to_string(),
            total_value: "1234.56".to_string(),
        };

        assert_eq!(current_net_worth_from_snapshot(&snapshot), 1234.56);
    }
}
