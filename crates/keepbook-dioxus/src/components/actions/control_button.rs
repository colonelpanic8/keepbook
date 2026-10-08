use super::{ButtonIcon, IconSvg};
use crate::components::{Spinner, SpinnerSize};
use dioxus::prelude::*;

/// keepbook's one button: quiet by default, filled with the brand color when `primary`.
///
/// While `busy`, a spinner takes the icon's slot and the button is disabled.
/// Mirror: `design/src/actions/ControlButton.tsx`.
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
    let is_busy = busy.unwrap_or(false);

    rsx! {
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
    }
}
