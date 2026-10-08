use dioxus::prelude::*;

/// A centered dialog over a dimmed backdrop; render it only while open.
///
/// `wide` widens it from 440px to 620px for editors. Mirror:
/// `design/src/layout/Modal.tsx`.
#[component]
pub(crate) fn Modal(
    title: String,
    wide: Option<bool>,
    header_actions: Option<Element>,
    actions: Option<Element>,
    children: Element,
) -> Element {
    let dialog_class = if wide == Some(true) {
        "modal-dialog wide"
    } else {
        "modal-dialog"
    };
    rsx! {
        div { class: "modal-backdrop",
            div { class: dialog_class, role: "dialog", aria_label: "{title}",
                div { class: "modal-header",
                    h3 { "{title}" }
                    {header_actions}
                }
                {children}
                if let Some(actions) = actions {
                    div { class: "modal-actions", {actions} }
                }
            }
        }
    }
}
