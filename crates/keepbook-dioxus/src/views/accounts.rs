use super::*;

#[component]
pub(super) fn AccountsView(
    accounts: Vec<Account>,
    connections: Vec<Connection>,
    balances: Vec<Balance>,
    snapshot: PortfolioSnapshot,
    currency: String,
    connection_count: usize,
    onrefresh: EventHandler<()>,
) -> Element {
    let mut price_busy = use_signal(|| false);
    let mut force_prices = use_signal(|| false);
    let mut price_status = use_signal(String::new);
    let mut pull_start = use_signal(|| None::<PullStart>);
    let mut pull_distance = use_signal(|| 0.0);
    let virtual_accounts = virtual_account_summaries(&snapshot);
    let account_count = accounts.len() + virtual_accounts.len();
    let active_accounts = accounts.iter().filter(|account| account.active).count();
    let net_worth = current_net_worth_from_snapshot(&snapshot);
    let account_summaries = snapshot.by_account.clone();
    let _ = balances;
    let is_price_busy = price_busy();
    let price_status_text = price_status();
    let pull_distance_value = pull_distance();
    let pull_offset = pull_refresh_offset(pull_distance_value);
    let pull_ready = pull_distance_value >= PULL_REFRESH_TRIGGER_PX;
    let pull_indicator_class = if pull_ready {
        "pull-refresh-indicator ready"
    } else {
        "pull-refresh-indicator"
    };

    rsx! {
        div {
            class: "pull-refresh-surface",
            ontouchstart: move |event| {
                if let Some((x, y)) = first_touch_position(&event) {
                    if y <= PULL_REFRESH_START_MAX_Y {
                        pull_start.set(Some(PullStart { x, y }));
                    }
                }
            },
            ontouchmove: move |event| {
                let Some(start) = pull_start() else {
                    return;
                };
                let Some((x, y)) = first_touch_position(&event) else {
                    return;
                };
                let horizontal_distance = (x - start.x).abs();
                let vertical_distance = y - start.y;
                if horizontal_distance > PULL_REFRESH_HORIZONTAL_SLOP_PX {
                    pull_start.set(None);
                    pull_distance.set(0.0);
                } else if vertical_distance > 0.0 {
                    pull_distance.set(vertical_distance);
                } else {
                    pull_distance.set(0.0);
                }
            },
            ontouchend: move |_| {
                if pull_distance() >= PULL_REFRESH_TRIGGER_PX {
                    onrefresh.call(());
                }
                pull_start.set(None);
                pull_distance.set(0.0);
            },
            ontouchcancel: move |_| {
                pull_start.set(None);
                pull_distance.set(0.0);
            },
            div {
                class: "{pull_indicator_class}",
                aria_label: "Refresh",
                aria_live: "polite",
                style: "height: {pull_offset}px; opacity: {pull_offset / PULL_REFRESH_MAX_OFFSET_PX};",
                if pull_ready {
                    span { class: "activity-spinner" }
                } else {
                    span { class: "pull-refresh-dot" }
                }
            }
            div { class: "pull-refresh-content",
                section { class: "summary-grid",
                    MetricCard {
                        label: "Net worth",
                        value: format_full_money(net_worth, &currency),
                        detail: snapshot.as_of_date.clone()
                    }
                    MetricCard {
                        label: "Accounts",
                        value: active_accounts.to_string(),
                        detail: format!("{account_count} total")
                    }
                    MetricCard {
                        label: "Connections",
                        value: connection_count.to_string(),
                        detail: "Configured sources".to_string()
                    }
                }
                section { class: "panel",
                    div { class: "panel-header",
                        div { class: "panel-title",
                            h2 { "Accounts" }
                            span { "{account_count}" }
                        }
                        div { class: "settings-actions inline-actions",
                            label { class: "compact-check",
                                input {
                                    r#type: "checkbox",
                                    checked: force_prices(),
                                    disabled: is_price_busy,
                                    onchange: move |event| force_prices.set(event.checked())
                                }
                                span { "Force prices" }
                            }
                            button {
                                class: "control-button",
                                disabled: is_price_busy,
                                onclick: move |_| {
                                    price_busy.set(true);
                                    let input = SyncPricesInput {
                                        scope: "all".to_string(),
                                        target: None,
                                        force: force_prices(),
                                        quote_staleness_seconds: None,
                                    };
                                    price_status.set(if input.force {
                                        "Refreshing all prices...".to_string()
                                    } else {
                                        "Refreshing stale prices...".to_string()
                                    });
                                    spawn(async move {
                                        match sync_prices(input).await {
                                            Ok(result) => {
                                                price_status.set(price_sync_result_summary(&result));
                                                onrefresh.call(());
                                            }
                                            Err(error) => {
                                                price_status.set(format!("Price sync failed: {error}"));
                                            }
                                        }
                                        price_busy.set(false);
                                    });
                                },
                                if is_price_busy { "Refreshing" } else { "Sync prices" }
                            }
                        }
                    }
                    if !price_status_text.is_empty() {
                        p { class: "settings-status", "{price_status_text}" }
                    }
                    div { class: "group-list",
                        if !virtual_accounts.is_empty() {
                            VirtualAccountGroup {
                                accounts: virtual_accounts,
                                currency: currency.clone(),
                            }
                        }
                        for connection in connections {
                            AccountGroup {
                                connection: connection.clone(),
                                accounts: accounts
                                    .iter()
                                    .filter(|account| account.connection_id == connection.id)
                                    .cloned()
                                    .collect::<Vec<_>>(),
                                account_summaries: account_summaries.clone(),
                                currency: currency.clone(),
                            }
                        }
                                    }
                }
            }
        }
    }
}

#[component]
fn VirtualAccountGroup(accounts: Vec<AccountSummary>, currency: String) -> Element {
    rsx! {
        section { class: "tree-group virtual-group",
            div { class: "tree-parent",
                div {
                    strong { "Virtual" }
                    small { "Portfolio adjustments" }
                }
                span { class: "status liability-status", "{accounts.len()} active" }
            }
            div { class: "data-table account-table",
                div { class: "table-head",
                    span { "Account" }
                    span { "Balance ({currency})" }
                    span { "Status" }
                    span { "Tags" }
                }
                for account in accounts {
                    VirtualAccountRow {
                        account,
                        currency: currency.clone(),
                    }
                }
            }
        }
    }
}

#[component]
fn VirtualAccountRow(account: AccountSummary, currency: String) -> Element {
    let value = account
        .value_in_base
        .as_deref()
        .and_then(parse_money_input)
        .map(|value| format_full_money(value, &currency))
        .unwrap_or_else(|| "N/A".to_string());

    rsx! {
        div { class: "table-row virtual-account-row",
            strong { "{account.account_name}" }
            span { "{value}" }
            span { class: "status liability-status", "Virtual" }
            small { "{account.connection_name}" }
        }
    }
}

#[component]
fn AccountGroup(
    connection: Connection,
    accounts: Vec<Account>,
    account_summaries: Vec<AccountSummary>,
    currency: String,
) -> Element {
    let active_count = accounts.iter().filter(|account| account.active).count();
    let ignored_count = accounts
        .iter()
        .filter(|account| account.exclude_from_portfolio)
        .count();
    let status_text = if ignored_count == 0 {
        format!("{active_count}/{} active", accounts.len())
    } else {
        format!(
            "{active_count}/{} active, {ignored_count} ignored",
            accounts.len()
        )
    };

    rsx! {
        section { class: "tree-group",
            div { class: "tree-parent",
                div {
                    strong { "{connection.name}" }
                    small { "{connection.synchronizer}" }
                }
                span { class: "status", "{status_text}" }
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
                        account_summaries: account_summaries.clone(),
                        currency: currency.clone(),
                    }
                }
            }
        }
    }
}

#[component]
fn AccountRow(
    account: Account,
    account_summaries: Vec<AccountSummary>,
    currency: String,
) -> Element {
    let status = if account.exclude_from_portfolio {
        "Ignored"
    } else if account.active {
        "Active"
    } else {
        "Inactive"
    };
    let row_class = if account.exclude_from_portfolio {
        "table-row ignored-account-row"
    } else {
        "table-row"
    };
    let status_class = if account.exclude_from_portfolio {
        "status ignored-status"
    } else {
        "status"
    };
    let tags = account.tags.join(", ");
    let balance = account_snapshot_value(&account.id, &account_summaries)
        .map(|value| format_full_money(value, &currency))
        .unwrap_or_else(|| "N/A".to_string());

    rsx! {
        div { class: "{row_class}",
            strong { "{account.name}" }
            span { "{balance}" }
            span { class: "{status_class}", "{status}" }
            small { "{tags}" }
        }
    }
}
