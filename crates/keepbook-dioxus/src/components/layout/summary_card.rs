use dioxus::prelude::*;

#[component]
pub(crate) fn SummaryCard(children: Element) -> Element {
    rsx! { section { class: "summary-card", aria_label: "Account overview", {children} } }
}
