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
    account_totals: AccountTotals,
    connection_count: usize,
    onrefresh: EventHandler<()>,
) -> Element {
    let mut price_busy = use_signal(|| false);
    let mut price_status = use_signal(String::new);
    let mut resync_busy = use_signal(|| false);
    let mut resync_status = use_signal(String::new);
    let mut git_sync_busy = use_signal(|| false);
    let mut git_sync_status = use_signal(String::new);
    let mut pull_start = use_signal(|| None::<PullStart>);
    let mut pull_distance = use_signal(|| 0.0);
    let mut selected_graph = use_signal(|| None::<AccountGraphSelection>);
    let virtual_accounts = virtual_account_summaries(&snapshot);
    let account_count = account_totals.account_count;
    let active_accounts = account_totals.active_account_count;
    let net_worth = format_money_text(&snapshot.total_value, &currency)
        .unwrap_or_else(|| snapshot.total_value.clone());
    let account_summaries = snapshot.by_account.clone();
    let selected_graph_selection = selected_graph();
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
    let mut refresh_prices = move |force: bool| {
        price_busy.set(true);
        git_sync_status.set(String::new());
        resync_status.set(String::new());
        price_status.set(if force {
            "Refreshing all prices...".to_string()
        } else {
            "Refreshing stale prices...".to_string()
        });
        let input = SyncPricesInput {
            scope: "all".to_string(),
            target: None,
            force,
            quote_staleness_seconds: None,
        };
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
    };
    let mut resync_data = move || {
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
                    Spinner {}
                } else {
                    span { class: "pull-refresh-dot" }
                }
            }
            div { class: "pull-refresh-content",
                PageToolbar {
                    ControlButton {
                        icon: ButtonIcon::GitBranch,
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
                    SplitButton {
                        primary: true,
                        icon: ButtonIcon::Refresh,
                        title: "Refetch every price, ignoring staleness",
                        menu_label: "More refresh options",
                        actions: vec![
                            MenuAction {
                                value: "stale-prices",
                                label: "Refresh stale prices",
                                detail: "Skip prices that are still fresh",
                            },
                            MenuAction {
                                value: "resync",
                                label: "Resync data",
                                detail: "Reload keepbook data from disk",
                            },
                        ],
                        disabled: any_account_operation_busy,
                        busy: is_price_busy || is_resync_busy,
                        onclick: move |_| refresh_prices(true),
                        onselect: move |value| match value {
                            "stale-prices" => refresh_prices(false),
                            _ => resync_data(),
                        },
                        if is_price_busy {
                            "Refreshing"
                        } else if is_resync_busy {
                            "Resyncing"
                        } else {
                            "Refresh all prices"
                        }
                    }
                }
                if !price_status_text.is_empty() {
                    FloatingStatus { message: price_status_text, busy: is_price_busy }
                }
                if !resync_status_text.is_empty() {
                    FloatingStatus { message: resync_status_text, busy: is_resync_busy }
                }
                if !git_sync_status_text.is_empty() {
                    FloatingStatus { message: git_sync_status_text, busy: is_git_sync_busy }
                }
                SummaryGrid {
                    MetricCard {
                        label: "Net worth",
                        value: net_worth,
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
                            IconButton {
                                label: "Close",
                                glyph: "×",
                                onclick: move |_| selected_graph.set(None),
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
                            show_header: false,
                        }
                    }
                }
                Panel {
                    title: "Accounts",
                    subtitle: account_count.to_string(),
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
        TreeGroup {
            title: "Virtual",
            subtitle: "Portfolio adjustments",
            aside: rsx! { Badge { "{accounts.len()} active" } },
            DataTable {
                class: "account-table",
                columns: account_table_columns(&currency),
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
        .and_then(|value| format_money_text(value, &currency))
        .unwrap_or_else(|| "N/A".to_string());
    let selection = AccountGraphSelection {
        id: account.account_id.clone(),
        name: account.account_name.clone(),
        connection_name: account.connection_name.clone(),
    };

    rsx! {
        button {
            class: "table-row account-click-row",
            title: "View graph",
            onclick: move |_| onselect.call(selection.clone()),
            strong { "{account.account_name}" }
            span { "{value}" }
            Badge { "Virtual" }
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
    let active_count = connection.active_account_count;
    let ignored_count = connection.excluded_account_count;
    let status_text = if ignored_count == 0 {
        format!("{active_count}/{} active", connection.account_count)
    } else {
        format!(
            "{active_count}/{} active, {ignored_count} ignored",
            connection.account_count
        )
    };

    rsx! {
        TreeGroup {
            title: connection.name.clone(),
            subtitle: connection.synchronizer.clone(),
            aside: rsx! { Badge { tone: BadgeTone::Positive, "{status_text}" } },
            DataTable {
                class: "account-table",
                columns: account_table_columns(&currency),
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
    let status_tone = if account.active && !effective_excluded {
        BadgeTone::Positive
    } else {
        BadgeTone::Neutral
    };
    let tags = account.tags.join(", ");
    let balance = account_snapshot_value_text(&account.id, &account_summaries)
        .and_then(|value| format_money_text(&value, &currency))
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
            Badge { tone: status_tone, "{status}" }
            small { "{tags}" }
            div { class: "account-override-cell",
                Checkbox {
                    label: "Include",
                    class: "account-include-toggle",
                    checked: included,
                    onchange: move |checked: bool| {
                        let next = toggle_filter_overrides
                            .clone()
                            .with_account_exclude_override(toggle_account_id.clone(), !checked);
                        onfilterchange.call(next);
                    }
                }
                if override_active {
                    ControlButton {
                        small: true,
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

fn account_table_columns(currency: &str) -> Vec<String> {
    [
        "Account",
        &format!("Balance ({currency})"),
        "Status",
        "Tags",
        "Include",
    ]
    .map(str::to_string)
    .to_vec()
}
