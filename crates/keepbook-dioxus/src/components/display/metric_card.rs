use dioxus::prelude::*;

/// A headline number with its label and one line of context.
///
/// Mirror: `design/src/display/MetricCard.tsx`.
#[component]
pub(crate) fn MetricCard(label: String, value: String, detail: String) -> Element {
    rsx! {
        article { class: "metric",
            span { class: "metric-label", "{label}" }
            strong { "{value}" }
            small { "{detail}" }
        }
    }
}
