use super::*;
use crate::api::{sync_connections, sync_prices};

#[component]
pub(super) fn ConnectionsView(
    connections: Vec<Connection>,
    onrefresh: EventHandler<()>,
) -> Element {
    let operation = use_action_feedback();
    let mut only_stale = use_signal(|| true);
    let mut full_transactions = use_signal(|| false);
    let mut force_prices = use_signal(|| false);
    let is_busy = operation.is_busy();
    let connection_count = connections.len();
    // Row buttons are too narrow for words; they show only the state's icon.
    let labels = |target: &Option<String>, label: &'static str| {
        if target.is_some() {
            ""
        } else {
            label
        }
    };
    // Failures name the connection, since a row button can't say which one failed.
    let failure_scope = |name: &Option<String>| {
        name.as_ref()
            .map(|name| format!("{name}: "))
            .unwrap_or_default()
    };
    let refresh_balances = move |key: String, target: Option<String>, name: Option<String>| {
        let scope = failure_scope(&name);
        operation.start(key.clone(), labels(&target, "Refreshing…"));
        let done = labels(&target, "Refreshed");
        let failed = labels(&target, "Failed");
        let input = SyncConnectionsInput {
            target,
            if_stale: only_stale(),
            full_transactions: full_transactions(),
        };
        spawn(async move {
            match sync_connections(input).await {
                Ok(result) => {
                    operation.succeed(key, done, Some(sync_result_summary(&result)));
                    onrefresh.call(());
                }
                Err(error) => operation.fail(
                    key,
                    failed,
                    format!("{scope}Balance refresh failed: {error}"),
                ),
            }
        });
    };
    let refresh_prices = move |key: String, target: Option<String>, name: Option<String>| {
        let scope = failure_scope(&name);
        operation.start(key.clone(), labels(&target, "Refreshing…"));
        let done = labels(&target, "Refreshed");
        let failed = labels(&target, "Failed");
        let partial = labels(&target, "Some failed");
        let input = SyncPricesInput {
            scope: if target.is_some() {
                "connection"
            } else {
                "all"
            }
            .to_string(),
            target,
            force: force_prices(),
            quote_staleness_seconds: None,
        };
        spawn(async move {
            match sync_prices(input).await {
                Ok(result) => {
                    let summary = price_sync_result_summary(&result);
                    if price_sync_result_has_failures(&result) {
                        operation.fail(key, partial, format!("{scope}{summary}"));
                    } else {
                        operation.succeed(key, done, Some(summary));
                    }
                    onrefresh.call(());
                }
                Err(error) => {
                    operation.fail(key, failed, format!("{scope}Price refresh failed: {error}"))
                }
            }
        });
    };

    rsx! {
        Panel {
            title: "Connections",
            subtitle: connection_count.to_string(),
            actions: rsx! {
                div { class: "settings-actions inline-actions",
                    Checkbox {
                        label: "Stale only",
                        checked: only_stale(),
                        disabled: is_busy,
                        onchange: move |checked| only_stale.set(checked)
                    }
                    Checkbox {
                        label: "Full transactions",
                        checked: full_transactions(),
                        disabled: is_busy,
                        onchange: move |checked| full_transactions.set(checked)
                    }
                    Checkbox {
                        label: "Force prices",
                        checked: force_prices(),
                        disabled: is_busy,
                        onchange: move |checked| force_prices.set(checked)
                    }
                    ControlButton {
                        disabled: is_busy,
                        feedback: operation.for_key("balances:all"),
                        onclick: move |_| refresh_balances("balances:all".to_string(), None, None),
                        "Refresh balances"
                    }
                    ControlButton {
                        selected: true,
                        disabled: is_busy,
                        feedback: operation.for_key("prices:all"),
                        onclick: move |_| refresh_prices("prices:all".to_string(), None, None),
                        "Refresh prices"
                    }
                }
            },
            if let Some(error) = operation.error() {
                ErrorNotice { message: error, ondismiss: move |_| operation.dismiss() }
            }
            DataTable {
                class: "connection-table",
                columns: ["Name", "Source", "Accounts", "Last balance refresh", "Actions"].map(str::to_string).to_vec(),
                for connection in connections {
                    {
                        let balances_key = format!("balances:{}", connection.id);
                        let prices_key = format!("prices:{}", connection.id);
                        let balances_target = connection.id.clone();
                        let prices_target = connection.id.clone();
                        let balances_name = connection.name.clone();
                        let prices_name = connection.name.clone();
                        rsx! {
                    div { class: "table-row",
                        strong { "{connection.name}" }
                        Badge { tone: BadgeTone::Positive, "{connection.status}" }
                        span { "{connection.account_count}" }
                        small {
                            "{connection.last_sync.clone().unwrap_or_else(|| \"Never\".to_string())}"
                        }
                        div { class: "connection-actions",
                            ControlButton {
                                disabled: is_busy,
                                feedback: operation.for_key(&balances_key),
                                onclick: move |_| {
                                    refresh_balances(balances_key.clone(), Some(balances_target.clone()), Some(balances_name.clone()))
                                },
                                "Balances"
                            }
                            ControlButton {
                                disabled: is_busy,
                                feedback: operation.for_key(&prices_key),
                                onclick: move |_| {
                                    refresh_prices(prices_key.clone(), Some(prices_target.clone()), Some(prices_name.clone()))
                                },
                                "Prices"
                            }
                        }
                    }
                        }
                    }
                }
            }
        }
    }
}
