use dioxus::prelude::*;

/// One setting: a title and description beside a single control.
///
/// Set `stacked` when the control is a select or option group, so it moves
/// under the copy on narrow screens. Mirror: `design/src/forms/SettingRow.tsx`.
#[component]
pub(crate) fn SettingRow(
    title: String,
    description: String,
    stacked: Option<bool>,
    children: Element,
) -> Element {
    let class_name = if stacked == Some(true) {
        "setting-row setting-row-stacked"
    } else {
        "setting-row"
    };
    rsx! {
        article { class: class_name,
            div { class: "setting-copy",
                strong { "{title}" }
                small { "{description}" }
            }
            {children}
        }
    }
}
