use dioxus::prelude::*;

/// An on/off switch for a setting that applies immediately; place it in a `SettingRow`.
///
/// Mirror: `design/src/forms/Switch.tsx`.
#[component]
pub(crate) fn Switch(
    label: String,
    checked: bool,
    disabled: Option<bool>,
    onchange: EventHandler<bool>,
) -> Element {
    rsx! {
        label { class: "switch-control",
            input {
                r#type: "checkbox",
                aria_label: "{label}",
                checked,
                disabled: disabled.unwrap_or(false),
                onchange: move |event| onchange.call(event.checked()),
            }
            span { class: "switch-track",
                span { class: "switch-thumb" }
            }
        }
    }
}
