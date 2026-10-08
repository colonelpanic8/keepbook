use super::{ButtonIcon, ControlButton, IconSvg};
use dioxus::prelude::*;

/// A less common variant of a [`SplitButton`]'s main action.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct MenuAction {
    /// Stable identifier handed back to `onselect`.
    pub(crate) value: &'static str,
    pub(crate) label: &'static str,
    pub(crate) detail: &'static str,
}

/// A main action plus a chevron that opens its less common variants.
/// Mirror: `design/src/actions/SplitButton.tsx`.
#[component]
pub(crate) fn SplitButton(
    children: Element,
    primary: Option<bool>,
    icon: Option<ButtonIcon>,
    title: Option<String>,
    disabled: Option<bool>,
    busy: Option<bool>,
    menu_label: String,
    actions: Vec<MenuAction>,
    onclick: EventHandler<MouseEvent>,
    onselect: EventHandler<&'static str>,
) -> Element {
    let mut open = use_signal(|| false);
    let is_open = open();
    let caret_class = if primary == Some(true) {
        "control-button primary split-button-caret"
    } else {
        "control-button split-button-caret"
    };

    rsx! {
        div {
            class: "split-button",
            onkeydown: move |event| {
                if event.key() == Key::Escape {
                    open.set(false);
                }
            },
            ControlButton {
                primary,
                icon,
                title,
                disabled,
                busy,
                onclick,
                {children}
            }
            button {
                class: caret_class,
                aria_label: menu_label,
                aria_expanded: is_open,
                disabled: disabled.unwrap_or(false) || busy.unwrap_or(false),
                onclick: move |_| open.toggle(),
                IconSvg { icon: ButtonIcon::ChevronDown }
            }
            if is_open {
                div { class: "menu-backdrop", onclick: move |_| open.set(false) }
                div { class: "menu",
                    for action in actions {
                        button {
                            class: "menu-item",
                            onclick: move |_| {
                                open.set(false);
                                onselect.call(action.value);
                            },
                            strong { "{action.label}" }
                            small { "{action.detail}" }
                        }
                    }
                }
            }
        }
    }
}
