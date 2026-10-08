use dioxus::prelude::*;

/// A transparent 28px button for a single glyph: expand, paginate, or close.
///
/// `label` is both the tooltip and the accessible name. Mirror:
/// `design/src/actions/IconButton.tsx`.
#[component]
pub(crate) fn IconButton(
    label: String,
    glyph: String,
    class: Option<String>,
    disabled: Option<bool>,
    onclick: EventHandler<MouseEvent>,
) -> Element {
    let class_name = match class {
        Some(extra) => format!("icon-button {extra}"),
        None => "icon-button".to_string(),
    };
    rsx! {
        button {
            class: "{class_name}",
            r#type: "button",
            title: "{label}",
            aria_label: "{label}",
            disabled: disabled.unwrap_or(false),
            onclick: move |event| onclick.call(event),
            "{glyph}"
        }
    }
}
