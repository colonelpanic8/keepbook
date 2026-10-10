use super::{ButtonIcon, IconSvg};
use crate::components::{Spinner, SpinnerSize};
use dioxus::prelude::*;

/// What a [`ButtonFeedback`] reports.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum FeedbackTone {
    Busy,
    Done,
    Failed,
}

/// The state of the work a button started, shown on the button itself.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ButtonFeedback {
    pub(crate) tone: FeedbackTone,
    /// A short label that fits the button: "Refreshing…", "Prices refreshed".
    pub(crate) label: String,
    /// The full message, shown as the tooltip.
    pub(crate) detail: Option<String>,
}

/// keepbook's one button: quiet by default, filled with the brand color when `primary`.
///
/// While `busy`, a spinner takes the icon's slot and the button is disabled.
/// `feedback` lays the state of the work it started over the button's own
/// label, which stays in place hidden so the button keeps its width; a longer
/// feedback label is ellipsized. Mirror: `design/src/actions/ControlButton.tsx`.
#[component]
pub(crate) fn ControlButton(
    children: Element,
    selected: Option<bool>,
    primary: Option<bool>,
    danger: Option<bool>,
    small: Option<bool>,
    icon: Option<ButtonIcon>,
    title: Option<String>,
    class: Option<String>,
    disabled: Option<bool>,
    busy: Option<bool>,
    feedback: Option<ButtonFeedback>,
    onclick: EventHandler<MouseEvent>,
) -> Element {
    let mut class_name = String::from("control-button");
    for (on, modifier) in [
        (selected, " selected"),
        (primary, " primary"),
        (danger, " danger"),
        (small, " small"),
    ] {
        if on == Some(true) {
            class_name.push_str(modifier);
        }
    }
    if let Some(extra) = class {
        class_name.push(' ');
        class_name.push_str(&extra);
    }
    let is_busy = busy.unwrap_or(false)
        || feedback
            .as_ref()
            .is_some_and(|feedback| feedback.tone == FeedbackTone::Busy);

    let Some(feedback) = feedback else {
        return rsx! {
            button {
                class: "{class_name}",
                title,
                disabled: disabled.unwrap_or(false) || is_busy,
                aria_busy: if is_busy { Some("true") } else { None },
                onclick: move |event| onclick.call(event),
                if is_busy {
                    Spinner { size: SpinnerSize::Small }
                } else if let Some(icon) = icon {
                    IconSvg { icon }
                }
                {children}
            }
        };
    };

    class_name.push_str(match feedback.tone {
        FeedbackTone::Busy => " has-feedback feedback-busy",
        FeedbackTone::Done => " has-feedback feedback-done",
        FeedbackTone::Failed => " has-feedback feedback-failed",
    });
    let tooltip = feedback.detail.clone().or(title);

    rsx! {
        button {
            class: "{class_name}",
            title: tooltip,
            disabled: disabled.unwrap_or(false) || is_busy,
            aria_busy: if is_busy { Some("true") } else { None },
            onclick: move |event| onclick.call(event),
            span { class: "button-face", aria_hidden: "true",
                if let Some(icon) = icon {
                    IconSvg { icon }
                }
                {children}
            }
            span { class: "button-feedback", role: "status", aria_live: "polite",
                match feedback.tone {
                    FeedbackTone::Busy => rsx! { Spinner { size: SpinnerSize::Small } },
                    FeedbackTone::Done => rsx! { IconSvg { icon: ButtonIcon::Check } },
                    FeedbackTone::Failed => rsx! { IconSvg { icon: ButtonIcon::CircleAlert } },
                }
                span { class: "button-feedback-label", "{feedback.label}" }
            }
        }
    }
}
