use dioxus::prelude::*;

/// A compact labeled checkbox for view options and row toggles.
///
/// Mirror: `design/src/forms/Checkbox.tsx`.
#[component]
pub(crate) fn Checkbox(
    label: String,
    checked: bool,
    disabled: Option<bool>,
    class: Option<String>,
    onchange: EventHandler<bool>,
) -> Element {
    let class_name = match class {
        Some(extra) => format!("compact-check {extra}"),
        None => "compact-check".to_string(),
    };
    rsx! {
        label { class: "{class_name}",
            input {
                r#type: "checkbox",
                checked,
                disabled: disabled.unwrap_or(false),
                onchange: move |event| onchange.call(event.checked()),
            }
            span { "{label}" }
        }
    }
}
