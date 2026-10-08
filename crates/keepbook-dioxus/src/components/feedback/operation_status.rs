use super::Spinner;
use dioxus::prelude::*;

/// Persistent, non-modal feedback for an operation the user started.
///
/// Keep this near the controls that started the work. Busy operations get an
/// indeterminate spinner and `aria-busy`; completed and failed messages remain
/// readable without blocking navigation or replacing already-loaded content.
/// Mirror: `design/src/feedback/OperationStatus.tsx`.
#[component]
pub(crate) fn OperationStatus(message: String, busy: bool) -> Element {
    let class = if busy { "notice busy" } else { "notice" };

    rsx! {
        div {
            class: "{class}",
            role: "status",
            aria_live: "polite",
            aria_busy: if busy { Some("true") } else { None },
            if busy {
                Spinner {}
            }
            span { "{message}" }
        }
    }
}
