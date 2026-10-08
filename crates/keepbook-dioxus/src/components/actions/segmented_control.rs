use dioxus::prelude::*;

/// One choice inside a [`SegmentedControl`].
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SegmentedOption {
    /// Stable identifier handed back to `onselect`.
    pub(crate) value: String,
    pub(crate) label: String,
}

impl SegmentedOption {
    pub(crate) fn new(value: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
        }
    }
}

/// A labeled group of mutually exclusive choices rendered as a single row.
///
/// The option count is published to CSS as `--segment-count`, and the stylesheet
/// lays the options out as exactly that many equal grid columns. A grid track
/// list cannot wrap, so a group can never spill an orphan option onto a second
/// line — no matter how narrow the viewport is or how many options a caller
/// adds. Labels shrink (fluid type, then ellipsis) instead of reflowing.
/// Mirror: `design/src/actions/SegmentedControl.tsx`.
#[component]
pub(crate) fn SegmentedControl(
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
