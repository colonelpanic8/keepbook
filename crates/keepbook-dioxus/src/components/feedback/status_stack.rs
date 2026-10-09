use dioxus::prelude::*;

/// A floating column of [`OperationStatus`](super::OperationStatus) notices
/// pinned to the bottom of the viewport.
///
/// Feedback for work the user started goes here so starting, finishing, or
/// dismissing it never shifts the page. Hovering the stack is reported so
/// settled notices can wait while they are being read. Mirror:
/// `design/src/feedback/StatusStack.tsx`.
#[component]
pub(crate) fn StatusStack(onhoverchange: Option<EventHandler<bool>>, children: Element) -> Element {
    rsx! {
        div {
            class: "status-stack",
            onmouseenter: move |_| {
                if let Some(handler) = &onhoverchange {
                    handler.call(true);
                }
            },
            onmouseleave: move |_| {
                if let Some(handler) = &onhoverchange {
                    handler.call(false);
                }
            },
            {children}
        }
    }
}
