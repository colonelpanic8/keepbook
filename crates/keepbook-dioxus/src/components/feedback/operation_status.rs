use super::Spinner;
use crate::components::IconButton;
use dioxus::prelude::*;

/// Non-modal feedback for an operation: a busy line while work runs, then
/// its result.
///
/// Busy operations get an indeterminate spinner and `aria-busy`. Inline, it
/// holds a region's first load; feedback for work the user started goes in a
/// [`StatusStack`](super::StatusStack) instead, where `ondismiss` adds a close
/// button to a settled message. Mirror: `design/src/feedback/OperationStatus.tsx`.
#[component]
pub(crate) fn OperationStatus(
    message: String,
    busy: bool,
    ondismiss: Option<EventHandler<()>>,
) -> Element {
    let class = if busy { "notice busy" } else { "notice" };
    let dismiss = ondismiss.filter(|_| !busy);

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
            if let Some(handler) = dismiss {
                IconButton {
                    label: "Dismiss",
                    glyph: "×",
                    class: "notice-dismiss",
                    onclick: move |_| handler.call(()),
                }
            }
        }
    }
}
