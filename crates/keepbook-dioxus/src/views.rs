use super::api::*;
use super::logic::*;
use super::*;
use dioxus::prelude::*;

mod accounts;
mod assets;
mod charts;
mod connections;
mod graph_settings;
mod proposed_edits;
mod recurring;
mod shared;
mod spending;

use accounts::AccountsView;
use assets::AssetsView;
use charts::{HistoryGraphPanel, StackedHistoryGraphPanel};
use connections::ConnectionsView;
use graph_settings::{NetWorthBreakdownGraphView, NetWorthGraphView, SettingsView};
use proposed_edits::ProposedEditsView;
use recurring::RecurringView;
use shared::*;
use spending::SpendingView;

const NAV_LOGO_SVG: &str = include_str!("../../../assets/keepbook-icon.svg");

pub(crate) fn repository_can_remove(repository: &Repository) -> bool {
    !repository.active && !repository.managed
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ActiveView {
    Spending,
    NetWorth,
    NetWorthBreakdown,
    Accounts,
    Assets,
    Connections,
    Recurring,
    ProposedEdits,
    Settings,
}

impl ActiveView {
    const ALL: [Self; 9] = [
        Self::Accounts,
        Self::Assets,
        Self::Spending,
        Self::Recurring,
        Self::NetWorth,
        Self::NetWorthBreakdown,
        Self::Connections,
        Self::ProposedEdits,
        Self::Settings,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Spending => "Spending",
            Self::NetWorth => "Net Worth",
            Self::NetWorthBreakdown => "Net Worth Breakdown",
            Self::Accounts => "Accounts",
            Self::Assets => "Assets",
            Self::Connections => "Connections",
            Self::Recurring => "Recurring",
            Self::ProposedEdits => "Proposed Edits",
            Self::Settings => "Settings",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
enum LoadState {
    Loading,
    Failed(String),
}

#[component]
pub(crate) fn App() -> Element {
    #[cfg(all(
        feature = "desktop",
        not(any(target_os = "ios", target_os = "android"))
    ))]
    use_hook(|| {
        use dioxus::desktop::{window, WindowCloseBehaviour};
        window().set_close_behavior(WindowCloseBehaviour::WindowHides);
    });

    let mut refresh_epoch = use_context_provider(|| Signal::new(0u64));
    let mut filter_overrides = use_signal(FilterOverrides::default);
    let mut repositories = use_resource(fetch_repositories);
    let mut repository_status = use_signal(String::new);
    let mut repository_busy = use_signal(|| false);
    let mut overview = use_resource(move || {
        let overrides = filter_overrides();
        async move { fetch_overview(overrides).await }
    });
    let mut tray_status_text = use_signal(|| "Idle".to_string());
    let mut tray_last_cycle_text = use_signal(|| "Last price refresh: never".to_string());
    let mut tray_last_summary = use_signal(|| "No price refresh has run yet".to_string());
    let mut tray_snapshot = use_resource(fetch_tray_snapshot);
    let overview_refreshing =
        overview.state().cloned() == UseResourceState::Pending && overview.cloned().is_some();

    rsx! {
        DesktopTrayBridge {
            overview: overview.cloned().and_then(Result::ok),
            tray_snapshot: tray_snapshot.cloned(),
            status_text: tray_status_text(),
            last_cycle_text: tray_last_cycle_text(),
            next_cycle_text: "Next price refresh: unscheduled".to_string(),
            last_summary: tray_last_summary(),
            onshowwindow: move |_| {
                overview.restart();
                tray_snapshot.restart();
                refresh_epoch.set(refresh_epoch().wrapping_add(1));
            },
            onsyncnow: move |_| {
                tray_status_text.set("Refreshing prices...".to_string());
                tray_last_summary.set("Running price refresh (manual)".to_string());
                spawn(async move {
                    let price_input = SyncPricesInput {
                        scope: "all".to_string(),
                        target: None,
                        force: false,
                        quote_staleness_seconds: None,
                    };

                    let mut had_error = false;
                    let summary = match sync_prices(price_input).await {
                        Ok(price_result) => {
                            had_error |= price_sync_result_has_failures(&price_result);
                            price_sync_result_summary(&price_result)
                        }
                        Err(error) => {
                            had_error = true;
                            format!("Price refresh failed: {error}")
                        }
                    };

                    tray_status_text.set(if had_error {
                        format!("Error: {summary}")
                    } else {
                        "Idle".to_string()
                    });
                    tray_last_cycle_text.set("Last price refresh: just now".to_string());
                    tray_last_summary.set(summary);
                    refresh_epoch.set(refresh_epoch().wrapping_add(1));
                    overview.restart();
                    tray_snapshot.restart();
                });
            },
        }
        document::Title { "{APP_NAME}" }
        document::Meta {
            name: "viewport",
            content: "width=device-width, initial-scale=1, viewport-fit=cover",
        }
        document::Link { rel: "icon", href: "data:," }
        document::Style { "{APP_CSS}" }
        document::Script { "{THEME_BOOTSTRAP_JS}" }
        document::Script { "{SSH_KEY_FILE_PICKER_BRIDGE_JS}" }
        document::Script { "{CONTEXT_MENU_COPY_BRIDGE_JS}" }
        main { class: "shell",
            match overview.cloned() {
                None => rsx! { StatusPanel { state: LoadState::Loading } },
                Some(Ok(data)) => rsx! {
                    Dashboard {
                        overview: data,
                        overview_refreshing,
                        repositories: repositories.cloned(),
                        repository_busy: repository_busy(),
                        repository_status: repository_status(),
                        filter_overrides: filter_overrides(),
                        onfilterchange: move |overrides| filter_overrides.set(overrides),
                        onrepositorychange: move |repository_id: String| {
                            repository_busy.set(true);
                            repository_status.set("Switching repository...".to_string());
                            spawn(async move {
                                match activate_repository(repository_id).await {
                                    Ok(_) => {
                                        filter_overrides.set(FilterOverrides::default());
                                        repository_status.set(String::new());
                                        repositories.restart();
                                        overview.restart();
                                        tray_snapshot.restart();
                                        refresh_epoch.set(refresh_epoch().wrapping_add(1));
                                    }
                                    Err(error) => repository_status.set(error),
                                }
                                repository_busy.set(false);
                            });
                        },
                        onrefresh: move |_| {
                            overview.restart();
                            repositories.restart();
                            tray_snapshot.restart();
                            refresh_epoch.set(refresh_epoch().wrapping_add(1));
                        }
                    }
                },
                Some(Err(error)) => rsx! {
                    StatusPanel {
                        state: LoadState::Failed(error),
                        onretry: move |_| overview.restart(),
                    }
                },
            }
        }
    }
}

#[cfg(all(
    feature = "desktop",
    not(any(target_os = "ios", target_os = "android"))
))]
#[component]
fn DesktopTrayBridge(
    overview: Option<Overview>,
    tray_snapshot: Option<Result<TraySnapshot, String>>,
    status_text: String,
    last_cycle_text: String,
    next_cycle_text: String,
    last_summary: String,
    onshowwindow: EventHandler<()>,
    onsyncnow: EventHandler<()>,
) -> Element {
    rsx! {
        tray::KeepbookTray {
            overview,
            tray_snapshot,
            runtime: tray::TrayRuntime {
                status_text,
                last_cycle_text,
                next_cycle_text,
                last_summary,
            },
            onshowwindow,
            onsyncnow,
        }
    }
}

#[cfg(not(all(
    feature = "desktop",
    not(any(target_os = "ios", target_os = "android"))
)))]
#[component]
fn DesktopTrayBridge(
    overview: Option<Overview>,
    tray_snapshot: Option<Result<TraySnapshot, String>>,
    status_text: String,
    last_cycle_text: String,
    next_cycle_text: String,
    last_summary: String,
    onshowwindow: EventHandler<()>,
    onsyncnow: EventHandler<()>,
) -> Element {
    let _ = overview;
    let _ = tray_snapshot;
    let _ = status_text;
    let _ = last_cycle_text;
    let _ = next_cycle_text;
    let _ = last_summary;
    let _ = onshowwindow;
    let _ = onsyncnow;
    rsx! {}
}

#[component]
fn StatusPanel(state: LoadState, onretry: Option<EventHandler<()>>) -> Element {
    let failed = matches!(state, LoadState::Failed(_));
    let message = match state {
        LoadState::Loading => "Loading local finance data...".to_string(),
        LoadState::Failed(error) => error,
    };

    rsx! {
        section { class: "status-panel",
            h2 { "Connection" }
            p { "{message}" }
            if failed {
                if let Some(onretry) = onretry {
                    ControlButton {
                        selected: true,
                        onclick: move |_| onretry.call(()),
                        "Retry"
                    }
                }
            }
        }
    }
}

fn price_sync_result_has_failures(result: &serde_json::Value) -> bool {
    result
        .get("result")
        .and_then(|value| value.get("failed_count"))
        .and_then(|value| value.as_u64())
        .unwrap_or(0)
        > 0
}

#[component]
fn Dashboard(
    overview: Overview,
    overview_refreshing: bool,
    repositories: Option<Result<RepositoryRegistry, String>>,
    repository_busy: bool,
    repository_status: String,
    filter_overrides: FilterOverrides,
    onfilterchange: EventHandler<FilterOverrides>,
    onrepositorychange: EventHandler<String>,
    onrefresh: EventHandler<()>,
) -> Element {
    let mut active_view = use_signal(|| ActiveView::Accounts);
    let mut mobile_nav_open = use_signal(|| false);
    let active = active_view();
    let nav_class = if mobile_nav_open() {
        "app-nav open"
    } else {
        "app-nav"
    };

    rsx! {
        div { class: "app-shell",
            DesktopTrayViewActions {
                onshowsettings: move |_| active_view.set(ActiveView::Settings),
            }
            aside { class: "{nav_class}",
                div { class: "nav-header",
                    div { class: "nav-title",
                        div { class: "nav-logo", dangerous_inner_html: NAV_LOGO_SVG }
                        div { class: "nav-title-text",
                            strong { "{APP_NAME}" }
                            small { "{overview.reporting_currency}" }
                        }
                    }
                    button {
                        class: "mobile-nav-toggle",
                        r#type: "button",
                        aria_label: "Toggle navigation",
                        aria_expanded: "{mobile_nav_open()}",
                        onclick: move |_| mobile_nav_open.set(!mobile_nav_open()),
                        span { aria_hidden: "true" }
                        span { aria_hidden: "true" }
                        span { aria_hidden: "true" }
                    }
                    if let Some(Ok(registry)) = repositories.clone() {
                        label { class: "repository-switcher",
                            span { "Repository" }
                            select {
                                class: "control-input",
                                aria_label: "Repository",
                                disabled: repository_busy,
                                value: registry.active_repository.as_deref().unwrap_or_default(),
                                onchange: move |event| onrepositorychange.call(event.value()),
                                for repository in registry.repositories {
                                    option {
                                        value: "{repository.id}",
                                        disabled: !repository.cloned,
                                        "{repository.name}"
                                    }
                                }
                            }
                        }
                    }
                }
                if !repository_status.is_empty() {
                    div { class: "repository-switch-status", aria_live: "polite",
                        if repository_busy { span { class: "activity-spinner" } }
                        small { "{repository_status}" }
                    }
                }
                nav {
                    for view in ActiveView::ALL {
                        NavButton {
                            label: view.label(),
                            selected: active == view,
                            onclick: move |_| {
                                if matches!(
                                    view,
                                    ActiveView::Accounts
                                        | ActiveView::Assets
                                        | ActiveView::NetWorth
                                        | ActiveView::NetWorthBreakdown
                                ) {
                                    onrefresh.call(());
                                }
                                active_view.set(view);
                                mobile_nav_open.set(false);
                            }
                        }
                    }
                }
            }
            button {
                class: if mobile_nav_open() { "nav-backdrop open" } else { "nav-backdrop" },
                r#type: "button",
                aria_label: "Close navigation",
                onclick: move |_| mobile_nav_open.set(false),
            }
            div { class: "workspace",
                if overview_refreshing {
                    OperationStatus {
                        message: "Refreshing app data…".to_string(),
                        busy: true,
                    }
                }
                match active {
                    ActiveView::Spending => rsx! {
                        SpendingView {
                            currency: overview.reporting_currency.clone(),
                        }
                    },
                    ActiveView::NetWorth => rsx! {
                        NetWorthGraphView {
                            currency: overview.reporting_currency.clone(),
                            defaults: overview.history_defaults.clone(),
                            filter_overrides,
                            current_value: current_net_worth_from_snapshot(&overview.snapshot),
                        }
                    },
                    ActiveView::NetWorthBreakdown => rsx! {
                        NetWorthBreakdownGraphView {
                            currency: overview.reporting_currency.clone(),
                            defaults: overview.history_defaults.clone(),
                            filter_overrides,
                        }
                    },
                    ActiveView::Accounts => rsx! {
                        AccountsView {
                            accounts: overview.accounts.clone(),
                            connections: overview.connections.clone(),
                            balances: overview.balances.clone(),
                            snapshot: overview.snapshot.clone(),
                            currency: overview.reporting_currency.clone(),
                            defaults: overview.history_defaults.clone(),
                            filter_overrides,
                            onfilterchange,
                            connection_count: overview.connections.len(),
                            onrefresh: move |_| onrefresh.call(()),
                        }
                    },
                    ActiveView::Assets => rsx! {
                        AssetsView {
                            filter_overrides: filter_overrides.clone(),
                        }
                    },
                    ActiveView::Connections => rsx! {
                        ConnectionsView {
                            connections: overview.connections.clone(),
                            onrefresh: move |_| onrefresh.call(())
                        }
                    },
                    ActiveView::Recurring => rsx! {
                        RecurringView {}
                    },
                    ActiveView::ProposedEdits => rsx! {
                        ProposedEditsView {
                            onrefresh: move |_| onrefresh.call(())
                        }
                    },
                    ActiveView::Settings => rsx! {
                        SettingsView {
                            repositories: repositories.clone(),
                            repository_busy,
                            onrepositorychange: move |id| onrepositorychange.call(id),
                            filtering: overview.filtering.clone(),
                            filter_overrides,
                            config_path: overview.config_path.clone(),
                            data_dir: overview.data_dir.clone(),
                            onfilterchange,
                            onrefresh: move |_| onrefresh.call(())
                        }
                    },
                }
            }
        }
    }
}

#[cfg(all(
    feature = "desktop",
    not(any(target_os = "ios", target_os = "android"))
))]
#[component]
fn DesktopTrayViewActions(onshowsettings: EventHandler<()>) -> Element {
    rsx! {
        tray::TrayViewActions {
            onshowsettings,
        }
    }
}

#[cfg(not(all(
    feature = "desktop",
    not(any(target_os = "ios", target_os = "android"))
)))]
#[component]
fn DesktopTrayViewActions(onshowsettings: EventHandler<()>) -> Element {
    let _ = onshowsettings;
    rsx! {}
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
