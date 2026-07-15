use super::*;

#[derive(Clone, Copy, Debug, PartialEq)]
struct PullStart {
    x: f64,
    y: f64,
}

const PULL_REFRESH_START_MAX_Y: f64 = 132.0;
const PULL_REFRESH_TRIGGER_PX: f64 = 84.0;
const PULL_REFRESH_MAX_OFFSET_PX: f64 = 64.0;
const PULL_REFRESH_HORIZONTAL_SLOP_PX: f64 = 48.0;

#[derive(Clone, Debug, PartialEq)]
struct AccountGraphSelection {
    id: String,
    name: String,
    connection_name: String,
}

fn first_touch_position(event: &TouchEvent) -> Option<(f64, f64)> {
    event.touches().first().map(|touch| {
        let position = touch.client_coordinates();
        (position.x, position.y)
    })
}

fn pull_refresh_offset(distance: f64) -> f64 {
    (distance.max(0.0) * 0.45).min(PULL_REFRESH_MAX_OFFSET_PX)
}

#[component]
pub(super) fn AccountsView(
    accounts: Vec<Account>,
    connections: Vec<Connection>,
    balances: Vec<Balance>,
    snapshot: PortfolioSnapshot,
    currency: String,
    defaults: HistoryDefaults,
    filter_overrides: FilterOverrides,
    onfilterchange: EventHandler<FilterOverrides>,
    connection_count: usize,
    onrefresh: EventHandler<()>,
) -> Element {
    let mut price_busy = use_signal(|| false);
    let mut force_prices = use_signal(|| false);
    let mut price_status = use_signal(String::new);
    let mut resync_busy = use_signal(|| false);
    let mut resync_status = use_signal(String::new);
    let mut git_sync_busy = use_signal(|| false);
    let mut git_sync_status = use_signal(String::new);
    let mut pull_start = use_signal(|| None::<PullStart>);
    let mut pull_distance = use_signal(|| 0.0);
    let mut selected_graph = use_signal(|| None::<AccountGraphSelection>);
    let virtual_accounts = virtual_account_summaries(&snapshot);
    let account_count = accounts.len() + virtual_accounts.len();
    let active_accounts = accounts.iter().filter(|account| account.active).count();
    let net_worth = current_net_worth_from_snapshot(&snapshot);
    let account_summaries = snapshot.by_account.clone();
    let selected_graph_selection = selected_graph();
    let selected_graph_current_value = selected_graph_selection
        .as_ref()
        .and_then(|selection| account_snapshot_value(&selection.id, &account_summaries));
    let _ = balances;
    let is_price_busy = price_busy();
    let price_status_text = price_status();
    let is_resync_busy = resync_busy();
    let resync_status_text = resync_status();
    let is_git_sync_busy = git_sync_busy();
    let git_sync_status_text = git_sync_status();
    let any_account_operation_busy = is_git_sync_busy || is_price_busy || is_resync_busy;
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
                if let Some(selection) = selected_graph_selection {
                    Panel {
                        class: "graph-panel account-detail-graph",
                        title: selection.name.clone(),
                        subtitle: selection.connection_name.clone(),
                        actions: rsx! {
                            button {
                                class: "icon-button",
                                title: "Close",
                                onclick: move |_| selected_graph.set(None),
                                "x"
                            }
                        },
                        HistoryGraphPanel {
                            title: selection.name.clone(),
                            scope_label: selection.connection_name.clone(),
                            empty_title: "No account history".to_string(),
                            empty_detail: "Refresh balances for this account to populate the chart.".to_string(),
                            currency: currency.clone(),
                            defaults: defaults.clone(),
                            filter_overrides: filter_overrides.clone(),
                            account: Some(selection.id.clone()),
                            current_value: selected_graph_current_value,
                            show_header: false,
                        }
                    }
                }
                Panel {
                    title: "Accounts",
                    subtitle: account_count.to_string(),
                    actions: rsx! {
                        div { class: "settings-actions inline-actions",
                            label { class: "compact-check",
                                input {
                                    r#type: "checkbox",
                                    checked: force_prices(),
                                    disabled: any_account_operation_busy,
                                    onchange: move |event| force_prices.set(event.checked())
                                }
                                span { "Force prices" }
                            }
                            ControlButton {
                                disabled: any_account_operation_busy,
                                busy: is_price_busy,
                                onclick: move |_| {
                                    price_busy.set(true);
                                    git_sync_status.set(String::new());
                                    resync_status.set(String::new());
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
                                                price_status.set(format!("Price refresh failed: {error}"));
                                            }
                                        }
                                        price_busy.set(false);
                                    });
                                },
                                if is_price_busy { "Refreshing" } else { "Refresh prices" }
                            }
                            ControlButton {
                                disabled: any_account_operation_busy,
                                busy: is_resync_busy,
                                onclick: move |_| {
                                    resync_busy.set(true);
                                    git_sync_status.set(String::new());
                                    price_status.set(String::new());
                                    resync_status.set("Resyncing data from disk...".to_string());
                                    spawn(async move {
                                        match reload_data().await {
                                            Ok(_) => {
                                                resync_status.set("Data resynced.".to_string());
                                                onrefresh.call(());
                                            }
                                            Err(error) => {
                                                resync_status.set(format!("Resync failed: {error}"));
                                            }
                                        }
                                        resync_busy.set(false);
                                    });
                                },
                                if is_resync_busy { "Resyncing" } else { "Resync data" }
                            }
                            ControlButton {
                                disabled: any_account_operation_busy,
                                busy: is_git_sync_busy,
                                onclick: move |_| {
                                    git_sync_busy.set(true);
                                    price_status.set(String::new());
                                    resync_status.set(String::new());
                                    git_sync_status.set("Syncing Git repository...".to_string());
                                    spawn(async move {
                                        let result = async {
                                            let settings = fetch_git_settings().await?;
                                            let input = GitSyncInput {
                                                data_dir: normalize_git_data_dir_for_client(settings.data_dir),
                                                host: settings.git.host,
                                                repo: settings.git.repo,
                                                branch: settings.git.branch,
                                                ssh_user: settings.git.ssh_user,
                                                private_key_pem: String::new(),
                                                save_settings: false,
                                            };
                                            sync_git_repo_cancelable(
                                                input,
                                                new_git_sync_cancel_handle(),
                                            )
                                            .await
                                        }
                                        .await;

                                        match result {
                                            Ok(result) => {
                                                git_sync_status.set(format!(
                                                    "Git sync complete: {} {}.",
                                                    result.remote_url, result.branch
                                                ));
                                                onrefresh.call(());
                                            }
                                            Err(error) => {
                                                git_sync_status.set(format!("Git sync failed: {error}"));
                                            }
                                        }
                                        git_sync_busy.set(false);
                                    });
                                },
                                if is_git_sync_busy { "Syncing Git" } else { "Git sync" }
                            }
                        }
                    },
                    if !price_status_text.is_empty() {
                        OperationStatus { message: price_status_text, busy: is_price_busy }
                    }
                    if !resync_status_text.is_empty() {
                        OperationStatus { message: resync_status_text, busy: is_resync_busy }
                    }
                    if !git_sync_status_text.is_empty() {
                        OperationStatus { message: git_sync_status_text, busy: is_git_sync_busy }
                    }
                    div { class: "group-list",
                        if !virtual_accounts.is_empty() {
                            VirtualAccountGroup {
                                accounts: virtual_accounts,
                                currency: currency.clone(),
                                onselect: move |selection| selected_graph.set(Some(selection)),
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
                                filter_overrides: filter_overrides.clone(),
                                onselect: move |selection| selected_graph.set(Some(selection)),
                                onfilterchange,
                            }
                        }
                                    }
                }
            }
        }
    }
}

#[component]
fn VirtualAccountGroup(
    accounts: Vec<AccountSummary>,
    currency: String,
    onselect: EventHandler<AccountGraphSelection>,
) -> Element {
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
                    span { "Include" }
                }
                for account in accounts {
                    VirtualAccountRow {
                        account,
                        currency: currency.clone(),
                        onselect,
                    }
                }
            }
        }
    }
}

#[component]
fn VirtualAccountRow(
    account: AccountSummary,
    currency: String,
    onselect: EventHandler<AccountGraphSelection>,
) -> Element {
    let value = account
        .value_in_base
        .as_deref()
        .and_then(parse_money_input)
        .map(|value| format_full_money(value, &currency))
        .unwrap_or_else(|| "N/A".to_string());
    let selection = AccountGraphSelection {
        id: account.account_id.clone(),
        name: account.account_name.clone(),
        connection_name: account.connection_name.clone(),
    };

    rsx! {
        button {
            class: "table-row virtual-account-row account-click-row",
            title: "View graph",
            onclick: move |_| onselect.call(selection.clone()),
            strong { "{account.account_name}" }
            span { "{value}" }
            span { class: "status liability-status", "Virtual" }
            small { "{account.connection_name}" }
            span {}
        }
    }
}

#[component]
fn AccountGroup(
    connection: Connection,
    accounts: Vec<Account>,
    account_summaries: Vec<AccountSummary>,
    currency: String,
    filter_overrides: FilterOverrides,
    onselect: EventHandler<AccountGraphSelection>,
    onfilterchange: EventHandler<FilterOverrides>,
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
                    span { "Include" }
                }
                for account in accounts {
                    AccountRow {
                        account,
                        connection_name: connection.name.clone(),
                        account_summaries: account_summaries.clone(),
                        currency: currency.clone(),
                        filter_overrides: filter_overrides.clone(),
                        onselect,
                        onfilterchange,
                    }
                }
            }
        }
    }
}

#[component]
fn AccountRow(
    account: Account,
    connection_name: String,
    account_summaries: Vec<AccountSummary>,
    currency: String,
    filter_overrides: FilterOverrides,
    onselect: EventHandler<AccountGraphSelection>,
    onfilterchange: EventHandler<FilterOverrides>,
) -> Element {
    let configured_excluded = account.exclude_from_portfolio;
    let override_excluded = filter_overrides.account_exclude_override(&account.id);
    let effective_excluded = override_excluded.unwrap_or(configured_excluded);
    let override_active = override_excluded.is_some();
    let included = !effective_excluded;
    let status = if effective_excluded {
        "Ignored"
    } else if account.active {
        "Active"
    } else {
        "Inactive"
    };
    let row_class = if effective_excluded {
        "table-row ignored-account-row"
    } else {
        "table-row"
    };
    let status_class = if effective_excluded {
        "status ignored-status"
    } else {
        "status"
    };
    let tags = account.tags.join(", ");
    let balance = account_snapshot_value(&account.id, &account_summaries)
        .map(|value| format_full_money(value, &currency))
        .unwrap_or_else(|| "N/A".to_string());
    let account_id = account.id.clone();
    let account_name = account.name.clone();
    let toggle_account_id = account_id.clone();
    let reset_account_id = account_id.clone();
    let toggle_filter_overrides = filter_overrides.clone();
    let reset_filter_overrides = filter_overrides.clone();
    let selection = AccountGraphSelection {
        id: account_id.clone(),
        name: account_name.clone(),
        connection_name,
    };

    rsx! {
        div {
            class: "{row_class} account-row-with-toggle",
            button {
                class: "account-row-main account-click-row",
                title: "View graph",
                onclick: move |_| onselect.call(selection.clone()),
                strong { "{account_name}" }
            }
            span { "{balance}" }
            span { class: "{status_class}", "{status}" }
            small { "{tags}" }
            div { class: "account-override-cell",
                label { class: "compact-check account-include-toggle",
                    input {
                        r#type: "checkbox",
                        checked: included,
                        onchange: move |event| {
                            let next = toggle_filter_overrides
                                .clone()
                                .with_account_exclude_override(
                                    toggle_account_id.clone(),
                                    !event.checked(),
                                );
                            onfilterchange.call(next);
                        }
                    }
                    span { "Include" }
                }
                if override_active {
                    button {
                        class: "text-button reset-account-override",
                        title: "Reset account override",
                        onclick: move |_| {
                            onfilterchange.call(
                                reset_filter_overrides
                                    .clone()
                                    .without_account_exclude_override(&reset_account_id)
                            );
                        },
                        "Reset"
                    }
                }
            }
        }
    }
}
