use super::api::*;
use super::logic::*;
use super::*;
use dioxus::prelude::*;

mod accounts;
mod charts;
mod connections;
mod graph_settings;
mod proposed_edits;
mod shared;
mod spending;

use accounts::AccountsView;
use charts::HistoryGraphPanel;
use connections::ConnectionsView;
use graph_settings::{GraphsView, SettingsView};
use proposed_edits::ProposedEditsView;
use shared::*;
use spending::SpendingView;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ActiveView {
    Spending,
    Graphs,
    Accounts,
    Connections,
    ProposedEdits,
    Settings,
}

impl ActiveView {
    const ALL: [Self; 6] = [
        Self::Accounts,
        Self::Spending,
        Self::Graphs,
        Self::Connections,
        Self::ProposedEdits,
        Self::Settings,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Spending => "Spending",
            Self::Graphs => "Net Worth",
            Self::Accounts => "Accounts",
            Self::Connections => "Connections",
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

    let mut filter_overrides = use_signal(FilterOverrides::default);
    let mut overview = use_resource(move || {
        let overrides = filter_overrides();
        async move { fetch_overview(overrides).await }
    });
    let mut tray_status_text = use_signal(|| "Idle".to_string());
    let mut tray_last_cycle_text = use_signal(|| "Last price refresh: never".to_string());
    let mut tray_last_summary = use_signal(|| "No price refresh has run yet".to_string());
    let mut tray_snapshot = use_resource(fetch_tray_snapshot);

    rsx! {
        DesktopTrayBridge {
            overview: overview.cloned().and_then(Result::ok),
            tray_snapshot: tray_snapshot.cloned(),
            status_text: tray_status_text(),
            last_cycle_text: tray_last_cycle_text(),
            next_cycle_text: "Next price refresh: unscheduled".to_string(),
            last_summary: tray_last_summary(),
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
                    overview.restart();
                    tray_snapshot.restart();
                });
            },
        }
        document::Title { "Keepbook" }
        document::Meta {
            name: "viewport",
            content: "width=device-width, initial-scale=1, viewport-fit=cover",
        }
        document::Link { rel: "icon", href: "data:," }
        document::Style { "{APP_CSS}" }
        document::Script { "{SSH_KEY_FILE_PICKER_BRIDGE_JS}" }
        main { class: "shell",
            match overview.cloned() {
                None => rsx! { StatusPanel { state: LoadState::Loading } },
                Some(Ok(data)) => rsx! {
                    Dashboard {
                        overview: data,
                        filter_overrides: filter_overrides(),
                        onfilterchange: move |overrides| filter_overrides.set(overrides),
                        onrefresh: move |_| {
                            overview.restart();
                            tray_snapshot.restart();
                        }
                    }
                },
                Some(Err(error)) => rsx! {
                    StatusPanel { state: LoadState::Failed(error) }
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
    onsyncnow: EventHandler<()>,
) -> Element {
    let _ = overview;
    let _ = tray_snapshot;
    let _ = status_text;
    let _ = last_cycle_text;
    let _ = next_cycle_text;
    let _ = last_summary;
    let _ = onsyncnow;
    rsx! {}
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
    filter_overrides: FilterOverrides,
    onfilterchange: EventHandler<FilterOverrides>,
    onrefresh: EventHandler<()>,
) -> Element {
    let mut active_view = use_signal(|| ActiveView::Accounts);
    let mut nav_open = use_signal(|| false);
    let active = active_view();
    let nav_class = if nav_open() {
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
            if nav_open() {
                button {
                    class: "nav-backdrop",
                    aria_label: "Close menu",
                    title: "Close menu",
                    onclick: move |_| nav_open.set(false),
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
                        class: "topbar-button",
                        title: "Refresh",
                        onclick: move |_| onrefresh.call(()),
                        "Refresh"
                    }
                }
                match active {
                    ActiveView::Spending => rsx! {
                        SpendingView {
                            currency: overview.reporting_currency.clone(),
                        }
                    },
                    ActiveView::Graphs => rsx! {
                        GraphsView {
                            currency: overview.reporting_currency.clone(),
                            defaults: overview.history_defaults.clone(),
                            accounts: overview.accounts.clone(),
                            connections: overview.connections.clone(),
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
                            connection_count: overview.connections.len(),
                            onrefresh: move |_| onrefresh.call(()),
                        }
                    },
                    ActiveView::Connections => rsx! {
                        ConnectionsView {
                            connections: overview.connections.clone(),
                            onrefresh: move |_| onrefresh.call(())
                        }
                    },
                    ActiveView::ProposedEdits => rsx! {
                        ProposedEditsView {
                            onrefresh: move |_| onrefresh.call(())
                        }
                    },
                    ActiveView::Settings => rsx! {
                        SettingsView {
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
