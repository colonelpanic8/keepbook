use crate::components::{SelectOption, Spinner};
use dioxus::prelude::*;

const LOGO_SVG: &str = include_str!("../../../../../assets/keepbook-icon.svg");

/// The full app frame: sidebar navigation beside a scrolling workspace.
///
/// Holds the logo, the repository switcher (shown when `repositories` is not
/// empty), and the navigation. Below 1200px wide the sidebar becomes a sticky
/// compact header whose hamburger opens a drawer. Mirror:
/// `design/src/layout/AppShell.tsx`.
#[component]
pub(crate) fn AppShell(
    title: String,
    currency: String,
    repositories: Vec<SelectOption>,
    repository: String,
    repository_busy: Option<bool>,
    repository_status: Option<String>,
    nav_items: Vec<String>,
    active: String,
    onrepositorychange: Option<EventHandler<String>>,
    onnavigate: Option<EventHandler<String>>,
    children: Element,
) -> Element {
    let mut open = use_signal(|| false);
    let repository_busy = repository_busy.unwrap_or(false);

    rsx! {
        div { class: "app-shell",
            aside { class: if open() { "app-nav open" } else { "app-nav" },
                div { class: "nav-header",
                    div { class: "nav-title",
                        div { class: "nav-logo", dangerous_inner_html: LOGO_SVG }
                        div { class: "nav-title-text",
                            strong { "{title}" }
                            small { "{currency}" }
                        }
                    }
                    button {
                        class: "mobile-nav-toggle",
                        r#type: "button",
                        aria_label: "Toggle navigation",
                        aria_expanded: "{open()}",
                        onclick: move |_| open.set(!open()),
                        span { aria_hidden: "true" }
                        span { aria_hidden: "true" }
                        span { aria_hidden: "true" }
                    }
                    if !repositories.is_empty() {
                        label { class: "repository-switcher",
                            span { "Repository" }
                            select {
                                class: "control-input",
                                aria_label: "Repository",
                                disabled: repository_busy,
                                value: "{repository}",
                                onchange: move |event| {
                                    if let Some(handler) = &onrepositorychange {
                                        handler.call(event.value());
                                    }
                                },
                                for option in repositories {
                                    option {
                                        key: "{option.value}",
                                        value: "{option.value}",
                                        disabled: option.disabled,
                                        selected: option.value == repository,
                                        "{option.label}"
                                    }
                                }
                            }
                        }
                    }
                }
                if let Some(status) = repository_status {
                    div { class: "repository-switch-status", aria_live: "polite",
                        if repository_busy {
                            Spinner {}
                        }
                        small { "{status}" }
                    }
                }
                nav {
                    for item in nav_items {
                        button {
                            key: "{item}",
                            class: if item == active { "nav-button selected" } else { "nav-button" },
                            onclick: {
                                let item = item.clone();
                                move |_| {
                                    open.set(false);
                                    if let Some(handler) = &onnavigate {
                                        handler.call(item.clone());
                                    }
                                }
                            },
                            "{item}"
                        }
                    }
                }
            }
            button {
                class: if open() { "nav-backdrop open" } else { "nav-backdrop" },
                r#type: "button",
                aria_label: "Close navigation",
                onclick: move |_| open.set(false),
            }
            div { class: "workspace", {children} }
        }
    }
}
