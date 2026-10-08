use dioxus::prelude::*;

/// Page-level actions, right-aligned at the top of a view above the metrics.
///
/// Mirror: `design/src/layout/PageToolbar.tsx`.
#[component]
pub(crate) fn PageToolbar(children: Element) -> Element {
    rsx! {
        div { class: "page-toolbar", {children} }
    }
}
