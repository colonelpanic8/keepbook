use dioxus::prelude::*;

/// One choice in a [`Select`].
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SelectOption {
    pub(crate) value: String,
    pub(crate) label: String,
    pub(crate) disabled: bool,
}

impl SelectOption {
    pub(crate) fn new(value: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
            disabled: false,
        }
    }
}

/// A themed dropdown with its own caret, optionally labeled above.
///
/// Mirror: `design/src/forms/Select.tsx`.
#[component]
pub(crate) fn Select(
    label: Option<String>,
    options: Vec<SelectOption>,
    value: String,
    small: Option<bool>,
    disabled: Option<bool>,
    class: Option<String>,
    onchange: EventHandler<String>,
) -> Element {
    let mut class_name = String::from("control-input");
    if small == Some(true) {
        class_name.push_str(" small");
    }
    if let Some(extra) = class {
        class_name.push(' ');
        class_name.push_str(&extra);
    }
    let control = rsx! {
        select {
            class: "{class_name}",
            aria_label: label.clone(),
            disabled: disabled.unwrap_or(false),
            value: "{value}",
            onchange: move |event| onchange.call(event.value()),
            for option in options {
                option {
                    key: "{option.value}",
                    value: "{option.value}",
                    disabled: option.disabled,
                    selected: option.value == value,
                    "{option.label}"
                }
            }
        }
    };
    match label {
        Some(label) => rsx! {
            label { class: "control-field",
                span { "{label}" }
                {control}
            }
        },
        None => control,
    }
}
