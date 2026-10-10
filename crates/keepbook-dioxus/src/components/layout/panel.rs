use dioxus::prelude::*;

/// The unit of page content: a bordered surface with a titled header.
///
/// `status` reports work started in the panel that has no button of its own
/// (a setting saving itself, an inline edit) on one line at the header's
/// right edge, ellipsized so it never moves the header. Mirror:
/// `design/src/layout/Panel.tsx`.
#[component]
pub(crate) fn Panel(
    title: String,
    subtitle: Option<String>,
    status: Option<String>,
    actions: Option<Element>,
    class: Option<String>,
    children: Element,
) -> Element {
    let class = match class {
        Some(extra) => format!("panel {extra}"),
        None => "panel".to_string(),
    };

    rsx! {
        section { class: "{class}",
            div { class: "panel-header",
                div { class: "panel-title",
                    h2 { "{title}" }
                    if let Some(subtitle) = subtitle {
                        span { "{subtitle}" }
                    }
                }
                if let Some(status) = status {
                    span { class: "panel-status", role: "status", aria_live: "polite", title: "{status}", "{status}" }
                }
                {actions}
            }
            {children}
        }
    }
}
