use dioxus::prelude::*;
use gloo_net::http::Request;
use serde::Deserialize;

static CSS: Asset = asset!("/assets/styles.css");
const API_BASE: &str = "http://127.0.0.1:8799";

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct Overview {
    data_dir: String,
    reporting_currency: String,
    connections: Vec<Connection>,
    accounts: Vec<Account>,
    balances: Vec<Balance>,
    history: History,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct Connection {
    name: String,
    synchronizer: String,
    status: String,
    account_count: usize,
    last_sync: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct Account {
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
            header { class: "topbar",
                div {
                    h1 { "Keepbook" }
                    p { "Rust client over the local keepbook API" }
                }
                button {
                    class: "icon-button",
                    title: "Refresh",
                    onclick: move |_| overview.restart(),
                    "Refresh"
                }
            }
            match overview.cloned() {
                None => rsx! { StatusPanel { state: LoadState::Loading } },
                Some(Ok(data)) => rsx! { Dashboard { overview: data } },
                Some(Err(error)) => rsx! {
                    StatusPanel { state: LoadState::Failed(error) }
                },
            }
        }
    }
}

async fn fetch_overview() -> Result<Overview, String> {
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
fn Dashboard(overview: Overview) -> Element {
    let total = overview
        .history
        .points
        .last()
        .map(|point| point.total_value.as_str())
        .unwrap_or("0");
    let last_date = overview
        .history
        .points
        .last()
        .map(|point| point.date.as_str())
        .unwrap_or("No history");
    let active_accounts = overview
        .accounts
        .iter()
        .filter(|account| account.active)
        .count();

    rsx! {
        section { class: "summary-grid",
            MetricCard {
                label: "Net worth",
                value: format_money(total, &overview.reporting_currency),
                detail: last_date.to_string()
            }
            MetricCard {
                label: "Accounts",
                value: active_accounts.to_string(),
                detail: format!("{} total", overview.accounts.len())
            }
            MetricCard {
                label: "Connections",
                value: overview.connections.len().to_string(),
                detail: overview.data_dir.clone()
            }
        }
        div { class: "content-grid",
            section { class: "panel wide",
                div { class: "panel-header",
                    h2 { "Net Worth History" }
                    span { "{overview.history.currency}" }
                }
                HistoryList { history: overview.history.clone() }
            }
            section { class: "panel",
                div { class: "panel-header",
                    h2 { "Connections" }
                    span { "{overview.connections.len()}" }
                }
                ConnectionList { connections: overview.connections.clone() }
            }
            section { class: "panel",
                div { class: "panel-header",
                    h2 { "Balances" }
                    span { "{overview.balances.len()}" }
                }
                BalanceList { balances: overview.balances.clone() }
            }
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
fn HistoryList(history: History) -> Element {
    let rows = history
        .points
        .iter()
        .rev()
        .take(12)
        .cloned()
        .collect::<Vec<_>>();
    rsx! {
        if let Some(summary) = history.summary {
            div { class: "summary-line",
                span { "Change" }
                strong { "{summary.absolute_change} ({summary.percentage_change}%)" }
                small { "{summary.initial_value} -> {summary.final_value}" }
            }
        }
        div { class: "table",
            for point in rows {
                div { class: "table-row",
                    span { "{point.date}" }
                    strong { "{point.total_value}" }
                    small {
                        "{point.percentage_change_from_previous.clone().unwrap_or_else(|| \"-\".to_string())}"
                    }
                }
            }
        }
    }
}

#[component]
fn ConnectionList(connections: Vec<Connection>) -> Element {
    rsx! {
        div { class: "list",
            for connection in connections {
                article { class: "list-item",
                    div {
                        strong { "{connection.name}" }
                        small { "{connection.synchronizer} / {connection.account_count} accounts" }
                    }
                    span { class: "status", "{connection.status}" }
                    if let Some(last_sync) = connection.last_sync {
                        small { "{last_sync}" }
                    }
                }
            }
        }
    }
}

#[component]
fn BalanceList(balances: Vec<Balance>) -> Element {
    rsx! {
        div { class: "list",
            for balance in balances.into_iter().take(10) {
                article { class: "list-item",
                    div {
                        strong { "{asset_label(&balance.asset)}" }
                        small { "{balance.amount} / {balance.account_id}" }
                    }
                    span {
                        "{balance.value_in_reporting_currency.clone().unwrap_or_else(|| \"N/A\".to_string())} {balance.reporting_currency}"
                    }
                    small { "{balance.timestamp}" }
                }
            }
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

fn format_money(value: &str, currency: &str) -> String {
    let parsed = value.parse::<f64>().ok();
    match parsed {
        Some(number) => format!("{currency} {number:.2}"),
        None => format!("{currency} {value}"),
    }
}
