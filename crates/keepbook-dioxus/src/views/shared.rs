use super::*;
use std::rc::Rc;

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

/// One choice inside a [`SegmentedControl`].
#[derive(Clone, Debug, PartialEq)]
pub(super) struct SegmentedOption {
    /// Stable identifier handed back to `onselect`.
    pub value: String,
    pub label: String,
}

impl SegmentedOption {
    pub(super) fn new(value: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
        }
    }
}

/// Build [`SegmentedControl`] options from a view's `(preset, label)` table.
/// Each view offers its own preset subset and labels, but they all round-trip
/// through [`RangePreset::value`].
pub(super) fn range_preset_options(
    presets: &[(RangePreset, &'static str)],
) -> Vec<SegmentedOption> {
    presets
        .iter()
        .map(|(preset, label)| SegmentedOption::new(preset.value(), *label))
        .collect()
}

pub(super) fn range_preset_from_value(
    presets: &[(RangePreset, &'static str)],
    value: &str,
) -> Option<RangePreset> {
    presets
        .iter()
        .find(|(preset, _)| preset.value() == value)
        .map(|(preset, _)| *preset)
}

/// A labeled group of mutually exclusive choices rendered as a single row.
///
/// The option count is published to CSS as `--segment-count`, and the stylesheet
/// lays the options out as exactly that many equal grid columns. A grid track
/// list cannot wrap, so a group can never spill an orphan option onto a second
/// line — no matter how narrow the viewport is or how many options a caller
/// adds. Labels shrink (fluid type, then ellipsis) instead of reflowing.
#[component]
pub(super) fn SegmentedControl(
    label: String,
    options: Vec<SegmentedOption>,
    selected: String,
    onselect: EventHandler<String>,
    class: Option<String>,
) -> Element {
    let segment_count = options.len().max(1);
    let field_class = match class {
        Some(extra) => format!("segmented-field {extra}"),
        None => "segmented-field".to_string(),
    };

    rsx! {
        div { class: "{field_class}",
            span { class: "control-label segmented-label", "{label}" }
            div {
                class: "segmented-control",
                role: "group",
                aria_label: "{label}",
                style: "--segment-count: {segment_count};",
                for option in options {
                    {
                        let value = option.value.clone();
                        let is_selected = value == selected;
                        let segment_class = if is_selected {
                            "segment selected"
                        } else {
                            "segment"
                        };
                        rsx! {
                            button {
                                key: "{option.value}",
                                class: "{segment_class}",
                                r#type: "button",
                                title: "{option.label}",
                                aria_pressed: is_selected,
                                onclick: move |_| onselect.call(value.clone()),
                                "{option.label}"
                            }
                        }
                    }
                }
            }
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

/// Dismissal for the native `<input type="date">` calendar.
///
/// The picker keeps covering the chart or table it was opened over after a day
/// is clicked, so the committed value is hidden behind the thing that set it.
/// Dropping focus closes the picker in every engine. Keyboard entry is exempt:
/// a date input fires `input` as soon as its segments read as a valid date, so
/// blurring there would cut someone off in the middle of retyping a date.
#[derive(Clone, Copy)]
pub(super) struct DatePickerDismissal {
    field: Signal<Option<Rc<MountedData>>>,
    keyboard_edit: Signal<bool>,
}

pub(super) fn use_date_picker_dismissal() -> DatePickerDismissal {
    DatePickerDismissal {
        field: use_signal(|| None),
        keyboard_edit: use_signal(|| false),
    }
}

impl DatePickerDismissal {
    pub(super) fn on_mounted(&mut self, event: Event<MountedData>) {
        self.field.set(Some(event.data()));
    }

    pub(super) fn on_key_edit(&mut self) {
        self.keyboard_edit.set(true);
    }

    pub(super) fn on_pointer_edit(&mut self) {
        self.keyboard_edit.set(false);
    }

    pub(super) fn dismiss(&self) {
        if (self.keyboard_edit)() {
            return;
        }
        let Some(field) = (self.field)() else {
            return;
        };
        spawn(async move {
            let _ = field.set_focus(false).await;
        });
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
    let mut dismissal = use_date_picker_dismissal();

    rsx! {
        label { class: "control-field",
            span { "{label}" }
            input {
                class: "control-input",
                r#type: "date",
                value: "{value}",
                min: "{min}",
                max: "{max}",
                onmounted: move |event| dismissal.on_mounted(event),
                onmousedown: move |_| dismissal.on_pointer_edit(),
                onkeydown: move |_| dismissal.on_key_edit(),
                oninput: move |event| {
                    oninput.call(event.value());
                    dismissal.dismiss();
                }
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
