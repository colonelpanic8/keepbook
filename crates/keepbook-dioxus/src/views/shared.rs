use super::*;

#[component]
pub(super) fn InlineStatus(title: String, message: String) -> Element {
    rsx! {
        div { class: "inline-status",
            h2 { "{title}" }
            p { "{message}" }
        }
    }
}

#[component]
pub(super) fn MetricCard(label: String, value: String, detail: String) -> Element {
    rsx! {
        article { class: "metric",
            span { class: "metric-label", "{label}" }
            strong { "{value}" }
            small { "{detail}" }
        }
    }
}

#[component]
pub(super) fn BackendActivity(message: &'static str) -> Element {
    rsx! {
        div {
            class: "backend-activity",
            role: "status",
            aria_live: "polite",
            span { class: "activity-spinner" }
            span { "{message}" }
        }
    }
}

#[component]
pub(super) fn GraphLoadingPanel(range: String, sampling: &'static str) -> Element {
    rsx! {
        div {
            class: "chart-loading",
            role: "status",
            aria_live: "polite",
            span { class: "activity-spinner large" }
            strong { "Updating graph" }
            span { "{range} / {sampling}" }
        }
    }
}

#[component]
pub(super) fn GraphPresetButton(
    label: &'static str,
    selected: bool,
    onclick: EventHandler<MouseEvent>,
) -> Element {
    let class = if selected {
        "control-button selected"
    } else {
        "control-button"
    };

    rsx! {
        button {
            class: "{class}",
            onclick: move |event| onclick.call(event),
            "{label}"
        }
    }
}

#[component]
pub(super) fn DateInput(
    label: &'static str,
    value: String,
    min: String,
    max: String,
    oninput: EventHandler<String>,
) -> Element {
    rsx! {
        label { class: "control-field",
            span { "{label}" }
            input {
                class: "control-input",
                r#type: "date",
                value: "{value}",
                min: "{min}",
                max: "{max}",
                oninput: move |event| oninput.call(event.value())
            }
        }
    }
}

#[component]
pub(super) fn NumberInput(
    label: &'static str,
    value: String,
    oninput: EventHandler<String>,
) -> Element {
    rsx! {
        label { class: "control-field",
            span { "{label}" }
            input {
                class: "control-input",
                r#type: "number",
                value: "{value}",
                step: "0.01",
                oninput: move |event| oninput.call(event.value())
            }
        }
    }
}

#[component]
pub(super) fn TextInput(
    label: &'static str,
    value: String,
    placeholder: &'static str,
    oninput: EventHandler<String>,
) -> Element {
    rsx! {
        label { class: "control-field",
            span { "{label}" }
            input {
                class: "control-input",
                r#type: "text",
                value: "{value}",
                placeholder: "{placeholder}",
                oninput: move |event| oninput.call(event.value())
            }
        }
    }
}

#[allow(dead_code)]
#[component]
pub(super) fn DataDirectoryControl(
    value: String,
    recommended: Option<String>,
    disabled: bool,
    onselect: EventHandler<String>,
) -> Element {
    let display_value = if value.trim().is_empty() {
        recommended
            .clone()
            .unwrap_or_else(|| "/path/to/keepbook-data".to_string())
    } else {
        value
    };

    rsx! {
        div { class: "control-field directory-field",
            span { "Data directory" }
            if let Some(path) = recommended {
                div { class: "directory-picker",
                    code { class: "directory-picker-path", "{display_value}" }
                    button {
                        class: "control-button",
                        disabled,
                        onclick: move |_| onselect.call(path.clone()),
                        "Use app data folder"
                    }
                }
            } else {
                input {
                    class: "control-input",
                    r#type: "text",
                    value: "{display_value}",
                    placeholder: "/path/to/keepbook-data",
                    disabled,
                    oninput: move |event| onselect.call(event.value())
                }
            }
        }
    }
}
