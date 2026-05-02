use super::*;
use crate::api::{sync_connections, sync_prices};

#[component]
pub(super) fn ConnectionsView(
    connections: Vec<Connection>,
    onrefresh: EventHandler<()>,
) -> Element {
    let mut busy_target = use_signal(String::new);
    let mut status = use_signal(String::new);
    let mut only_stale = use_signal(|| true);
    let mut full_transactions = use_signal(|| false);
    let mut force_prices = use_signal(|| false);
    let busy = busy_target();
    let is_busy = !busy.is_empty();
    let status_text = status();

    rsx! {
        section { class: "panel",
            div { class: "panel-header",
                div { class: "panel-title",
                    h2 { "Connections" }
                    span { "{connections.len()}" }
                }
                div { class: "settings-actions inline-actions",
                    label { class: "compact-check",
                        input {
                            r#type: "checkbox",
                            checked: only_stale(),
                            disabled: is_busy,
                            onchange: move |event| only_stale.set(event.checked())
                        }
                        span { "Stale only" }
                    }
                    label { class: "compact-check",
                        input {
                            r#type: "checkbox",
                            checked: full_transactions(),
                            disabled: is_busy,
                            onchange: move |event| full_transactions.set(event.checked())
                        }
                        span { "Full transactions" }
                    }
                    label { class: "compact-check",
                        input {
                            r#type: "checkbox",
                            checked: force_prices(),
                            disabled: is_busy,
                            onchange: move |event| force_prices.set(event.checked())
                        }
                        span { "Force prices" }
                    }
                    button {
                        class: "control-button selected",
                        disabled: is_busy,
                        onclick: move |_| {
                            busy_target.set("all".to_string());
                            let input = SyncConnectionsInput {
                                target: None,
                                if_stale: only_stale(),
                                full_transactions: full_transactions(),
                            };
                            status.set(if input.if_stale {
                                "Syncing stale connections...".to_string()
                            } else {
                                "Syncing all connections...".to_string()
                            });
                            spawn(async move {
                                match sync_connections(input).await {
                                    Ok(result) => {
                                        status.set(sync_result_summary(&result));
                                        onrefresh.call(());
                                    }
                                    Err(error) => status.set(format!("Sync failed: {error}")),
                                }
                                busy_target.set(String::new());
                            });
                        },
                        if busy == "all" { "Syncing" } else { "Sync all" }
                    }
                    button {
                        class: "control-button",
                        disabled: is_busy,
                        onclick: move |_| {
                            busy_target.set("prices:all".to_string());
                            let input = SyncPricesInput {
                                scope: "all".to_string(),
                                target: None,
                                force: force_prices(),
                                quote_staleness_seconds: None,
                            };
                            status.set(if input.force {
                                "Refreshing all prices...".to_string()
                            } else {
                                "Refreshing stale prices...".to_string()
                            });
                            spawn(async move {
                                match sync_prices(input).await {
                                    Ok(result) => {
                                        status.set(price_sync_result_summary(&result));
                                        onrefresh.call(());
                                    }
                                    Err(error) => status.set(format!("Price sync failed: {error}")),
                                }
                                busy_target.set(String::new());
                            });
                        },
                        if busy == "prices:all" { "Refreshing" } else { "Sync prices" }
                    }
                }
            }
            if !status_text.is_empty() {
                p { class: "settings-status", "{status_text}" }
            }
            div { class: "data-table connection-table",
                div { class: "table-head",
                    span { "Name" }
                    span { "Sync" }
                    span { "Accounts" }
                    span { "Last sync" }
                    span { "Actions" }
                }
                for connection in connections {
                    {
                        let target = connection.id.clone();
                        let price_target = format!("prices:{target}");
                        let row_busy = busy == target;
                        let price_busy = busy == price_target;
                        let sync_target = target.clone();
                        let sync_name = connection.name.clone();
                        let prices_target = target.clone();
                        let prices_name = connection.name.clone();
                        rsx! {
                    div { class: "table-row",
                        strong { "{connection.name}" }
                        span { class: "status", "{connection.status}" }
                        span { "{connection.account_count}" }
                        small {
                            "{connection.last_sync.clone().unwrap_or_else(|| \"Never\".to_string())}"
                        }
                        div { class: "connection-actions",
                            button {
                                class: "control-button",
                                disabled: is_busy,
                                onclick: move |_| {
                                    let target = sync_target.clone();
                                    busy_target.set(target.clone());
                                    let input = SyncConnectionsInput {
                                        target: Some(target.clone()),
                                        if_stale: only_stale(),
                                        full_transactions: full_transactions(),
                                    };
                                    status.set(format!("Syncing {sync_name}..."));
                                    spawn(async move {
                                        match sync_connections(input).await {
                                            Ok(result) => {
                                                status.set(sync_result_summary(&result));
                                                onrefresh.call(());
                                            }
                                            Err(error) => status.set(format!("Sync failed: {error}")),
                                        }
                                        busy_target.set(String::new());
                                    });
                                },
                                if row_busy { "Syncing" } else { "Sync" }
                            }
                            button {
                                class: "control-button",
                                disabled: is_busy,
                                onclick: move |_| {
                                    let target = prices_target.clone();
                                    let price_target = format!("prices:{target}");
                                    busy_target.set(price_target);
                                    let input = SyncPricesInput {
                                        scope: "connection".to_string(),
                                        target: Some(target),
                                        force: force_prices(),
                                        quote_staleness_seconds: None,
                                    };
                                    status.set(format!("Refreshing prices for {prices_name}..."));
                                    spawn(async move {
                                        match sync_prices(input).await {
                                            Ok(result) => {
                                                status.set(price_sync_result_summary(&result));
                                                onrefresh.call(());
                                            }
                                            Err(error) => status.set(format!("Price sync failed: {error}")),
                                        }
                                        busy_target.set(String::new());
                                    });
                                },
                                if price_busy { "Refreshing" } else { "Prices" }
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
