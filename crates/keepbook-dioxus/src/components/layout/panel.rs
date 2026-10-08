use dioxus::prelude::*;

/// The unit of page content: a bordered surface with a titled header.
///
/// Mirror: `design/src/layout/Panel.tsx`.
#[component]
pub(crate) fn Panel(
    title: String,
    subtitle: Option<String>,
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
                {actions}
            }
            {children}
        }
    }
}
