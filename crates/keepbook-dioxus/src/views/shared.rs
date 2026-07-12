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

/// Persistent, non-modal feedback for an operation the user started.
///
/// Keep this near the controls that started the work. Busy operations get an
/// indeterminate spinner and `aria-busy`; completed and failed messages remain
/// readable without blocking navigation or replacing already-loaded content.
#[component]
pub(super) fn OperationStatus(message: String, busy: bool) -> Element {
    let class = if busy {
        "operation-status busy"
    } else {
        "operation-status"
    };

    rsx! {
        div {
            class: "{class}",
            role: "status",
            aria_live: "polite",
            aria_busy: busy,
            if busy {
                span { class: "activity-spinner", aria_hidden: "true" }
            }
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
pub(super) fn Panel(
    title: String,
    subtitle: Option<String>,
    actions: Option<Element>,
    class: Option<String>,
    children: Element,
) -> Element {
    let class = match class {
        Some(extra) => format!("panel {extra}"),
        None => "panel".to_string(),
    };

    rsx! {
        section { class: "{class}",
            div { class: "panel-header",
                div { class: "panel-title",
                    h2 { "{title}" }
                    if let Some(subtitle) = subtitle {
                        span { "{subtitle}" }
                    }
                }
                {actions}
            }
            {children}
        }
    }
}

#[component]
pub(super) fn ControlButton(
    children: Element,
    selected: Option<bool>,
    danger: Option<bool>,
    class: Option<String>,
    disabled: Option<bool>,
    busy: Option<bool>,
    onclick: EventHandler<MouseEvent>,
) -> Element {
    let mut class_name = String::from("control-button");
    if selected == Some(true) {
        class_name.push_str(" selected");
    }
    if danger == Some(true) {
        class_name.push_str(" danger");
    }
    if let Some(extra) = class {
        class_name.push(' ');
        class_name.push_str(&extra);
    }

    let is_busy = busy.unwrap_or(false);

    rsx! {
        button {
            class: "{class_name}",
            disabled: disabled.unwrap_or(false) || is_busy,
            aria_busy: is_busy,
            onclick: move |event| onclick.call(event),
            if is_busy {
                span { class: "activity-spinner control-spinner", aria_hidden: "true" }
            }
            {children}
        }
    }
}

#[component]
pub(super) fn GraphPresetButton(
    label: &'static str,
    selected: bool,
    onclick: EventHandler<MouseEvent>,
) -> Element {
    rsx! {
        ControlButton {
            selected,
            onclick: move |event| onclick.call(event),
            "{label}"
        }
    }
}

#[component]
pub(super) fn Modal(
    title: String,
    dialog_class: Option<String>,
    header_actions: Option<Element>,
    actions: Option<Element>,
    children: Element,
) -> Element {
    let dialog_class = match dialog_class {
        Some(extra) => format!("modal-dialog {extra}"),
        None => "modal-dialog".to_string(),
    };

    rsx! {
        div { class: "modal-backdrop",
            div { class: "{dialog_class}",
                div { class: "modal-header",
                    h3 { "{title}" }
                    {header_actions}
                }
                {children}
                if let Some(actions) = actions {
                    div { class: "modal-actions", {actions} }
                }
            }
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
