use dioxus::prelude::*;
use std::rc::Rc;

/// Dismissal for the native `<input type="date">` calendar.
///
/// The picker keeps covering the chart or table it was opened over after a day
/// is clicked, so the committed value is hidden behind the thing that set it.
/// Dropping focus closes the picker in every engine. Keyboard entry is exempt:
/// a date input fires `input` as soon as its segments read as a valid date, so
/// blurring there would cut someone off in the middle of retyping a date.
#[derive(Clone, Copy)]
pub(crate) struct DatePickerDismissal {
    field: Signal<Option<Rc<MountedData>>>,
    keyboard_edit: Signal<bool>,
}

pub(crate) fn use_date_picker_dismissal() -> DatePickerDismissal {
    DatePickerDismissal {
        field: use_signal(|| None),
        keyboard_edit: use_signal(|| false),
    }
}

impl DatePickerDismissal {
    pub(crate) fn on_mounted(&mut self, event: Event<MountedData>) {
        self.field.set(Some(event.data()));
    }

    pub(crate) fn on_key_edit(&mut self) {
        self.keyboard_edit.set(true);
    }

    pub(crate) fn on_pointer_edit(&mut self) {
        self.keyboard_edit.set(false);
    }

    pub(crate) fn dismiss(&self) {
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

/// What a [`TextInput`] accepts.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) enum InputKind {
    #[default]
    Text,
    Number,
    Date,
    Search,
    Color,
}

/// A text, number, date, search, color, or multi-line input, optionally labeled above.
///
/// Date inputs close the native calendar once a date is picked. Mirror:
/// `design/src/forms/TextInput.tsx`.
#[component]
pub(crate) fn TextInput(
    label: Option<String>,
    value: String,
    kind: Option<InputKind>,
    placeholder: Option<String>,
    min: Option<String>,
    max: Option<String>,
    small: Option<bool>,
    multiline: Option<bool>,
    disabled: Option<bool>,
    class: Option<String>,
    oninput: EventHandler<String>,
) -> Element {
    let kind = kind.unwrap_or_default();
    let mut dismissal = use_date_picker_dismissal();
    let mut class_name = String::from("control-input");
    if small == Some(true) {
        class_name.push_str(" small");
    }
    if multiline == Some(true) {
        class_name.push_str(" ai-rule-prompt");
    }
    if let Some(extra) = class {
        class_name.push(' ');
        class_name.push_str(&extra);
    }
    let disabled = disabled.unwrap_or(false);
    let control = if multiline == Some(true) {
        rsx! {
            textarea {
                class: "{class_name}",
                value: "{value}",
                placeholder,
                disabled,
                oninput: move |event| oninput.call(event.value()),
            }
        }
    } else {
        let input_type = match kind {
            InputKind::Text => "text",
            InputKind::Number => "number",
            InputKind::Date => "date",
            InputKind::Search => "search",
            InputKind::Color => "color",
        };
        let is_date = kind == InputKind::Date;
        rsx! {
            input {
                class: "{class_name}",
                r#type: input_type,
                value: "{value}",
                placeholder,
                min,
                max,
                step: if kind == InputKind::Number { Some("0.01") } else { None },
                disabled,
                onmounted: move |event| {
                    if is_date {
                        dismissal.on_mounted(event);
                    }
                },
                onmousedown: move |_| dismissal.on_pointer_edit(),
                onkeydown: move |_| dismissal.on_key_edit(),
                oninput: move |event| {
                    oninput.call(event.value());
                    if is_date {
                        dismissal.dismiss();
                    }
                },
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
