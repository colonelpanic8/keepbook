use crate::components::{ButtonIcon, IconButton, IconSvg};
use dioxus::prelude::*;

/// A failure of work the user started, in full, beneath the control that
/// started it.
///
/// Unlike other feedback it stays until dismissed or the work is retried, so
/// the message can't be missed. Mirror: `design/src/feedback/ErrorNotice.tsx`.
#[component]
pub(crate) fn ErrorNotice(message: String, ondismiss: EventHandler<()>) -> Element {
    rsx! {
        div { class: "notice error", role: "alert",
            IconSvg { icon: ButtonIcon::CircleAlert }
            span { "{message}" }
            IconButton {
                label: "Dismiss",
                glyph: "×",
                class: "notice-dismiss",
                onclick: move |_| ondismiss.call(()),
            }
        }
    }
}
