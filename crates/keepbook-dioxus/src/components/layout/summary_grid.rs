use dioxus::prelude::*;

/// A row of three headline metrics at the top of a view.
///
/// Mirror: `design/src/layout/SummaryGrid.tsx`.
#[component]
pub(crate) fn SummaryGrid(class: Option<String>, children: Element) -> Element {
    let class_name = match class {
        Some(extra) => format!("summary-grid {extra}"),
        None => "summary-grid".to_string(),
    };
    rsx! {
        section { class: "{class_name}", {children} }
    }
}
