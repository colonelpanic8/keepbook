use super::{Spinner, SpinnerSize};
use dioxus::prelude::*;

/// An indeterminate progress bar with a spinner and label, for long operations.
///
/// With `busy` false only the label shows, for the operation's final message.
/// Mirror: `design/src/feedback/Progress.tsx`.
#[component]
pub(crate) fn Progress(label: String, busy: Option<bool>) -> Element {
    let busy = busy.unwrap_or(true);
    rsx! {
        div { class: "clone-progress", role: "status", aria_live: "polite",
            if busy {
                Spinner { size: SpinnerSize::Large }
            }
            div { class: "clone-progress-copy",
                p { "{label}" }
                if busy {
                    div { class: "indeterminate-progress",
                        span {}
                    }
                }
            }
        }
    }
}
