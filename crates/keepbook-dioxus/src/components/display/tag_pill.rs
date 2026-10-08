use dioxus::prelude::*;

/// A spending tag pill: read-only, removable, or an outlined suggestion.
///
/// Mirror: `design/src/display/TagPill.tsx`.
#[component]
pub(crate) fn TagPill(
    tag: String,
    removable: Option<bool>,
    suggestion: Option<bool>,
    disabled: Option<bool>,
    onclick: Option<EventHandler<MouseEvent>>,
) -> Element {
    let disabled = disabled.unwrap_or(false);
    let click = move |event: MouseEvent| {
        if let Some(handler) = &onclick {
            handler.call(event);
        }
    };
    if suggestion == Some(true) {
        rsx! {
            button { class: "tag-suggestion-pill", disabled, onclick: click, "{tag}" }
        }
    } else if removable == Some(true) {
        rsx! {
            button {
                class: "tag-pill removable",
                title: "Remove tag",
                disabled,
                onclick: click,
                span { "{tag}" }
                span { class: "tag-pill-remove", "x" }
            }
        }
    } else {
        rsx! {
            span { class: "tag-pill readonly", "{tag}" }
        }
    }
}
