use super::{Spinner, SpinnerSize};
use dioxus::prelude::*;

/// A dashed placeholder that keeps the footprint of the content it replaces.
///
/// `loading` shows a spinner for a first load. `compact` drops the chart aspect
/// ratio for lists and tables. Mirror: `design/src/feedback/EmptyState.tsx`.
#[component]
pub(crate) fn EmptyState(
    title: String,
    detail: String,
    loading: Option<bool>,
    compact: Option<bool>,
    class: Option<String>,
) -> Element {
    let loading = loading == Some(true);
    let mut class_name = String::from(if loading {
        "chart-loading"
    } else {
        "chart-empty"
    });
    if compact == Some(true) {
        class_name.push_str(" compact");
    }
    if let Some(extra) = class {
        class_name.push(' ');
        class_name.push_str(&extra);
    }
    if loading {
        rsx! {
            div { class: "{class_name}", role: "status", aria_live: "polite",
                Spinner { size: SpinnerSize::Large }
                strong { "{title}" }
                span { "{detail}" }
            }
        }
    } else {
        rsx! {
            div { class: "{class_name}",
                strong { "{title}" }
                small { "{detail}" }
            }
        }
    }
}
