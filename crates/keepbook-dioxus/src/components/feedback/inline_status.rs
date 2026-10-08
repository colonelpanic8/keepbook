use dioxus::prelude::*;

/// A centered message filling a region that couldn't load.
///
/// Mirror: `design/src/feedback/InlineStatus.tsx`.
#[component]
pub(crate) fn InlineStatus(title: String, message: String) -> Element {
    rsx! {
        div { class: "inline-status",
            h2 { "{title}" }
            p { "{message}" }
        }
    }
}
