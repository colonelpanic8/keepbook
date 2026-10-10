use super::Spinner;
use dioxus::prelude::*;

/// A line that holds a region while its data first loads.
///
/// Busy lines get an indeterminate spinner and `aria-busy`. Feedback on work
/// the user started belongs on the control that started it (a button's
/// `feedback`, a panel's `status`), not here. Mirror: `design/src/feedback/OperationStatus.tsx`.
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
