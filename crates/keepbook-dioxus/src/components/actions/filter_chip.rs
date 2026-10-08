use dioxus::prelude::*;

/// A pill shown only while a filter is active; clicking it clears the filter.
///
/// Mirror: `design/src/actions/FilterChip.tsx`.
#[component]
pub(crate) fn FilterChip(
    label: String,
    title: String,
    onclear: EventHandler<MouseEvent>,
) -> Element {
    rsx! {
        div { class: "filter-chip-row",
            button {
                class: "filter-clear-chip",
                r#type: "button",
                title: "{title}",
                onclick: move |event| onclear.call(event),
                span { class: "filter-clear-chip-text", "{label}" }
                span { aria_hidden: "true", "✕" }
            }
        }
    }
}
