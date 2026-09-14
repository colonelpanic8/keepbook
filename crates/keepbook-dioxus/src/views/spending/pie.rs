use super::*;
use std::collections::HashMap;

#[component]
pub(super) fn SpendingPieChart(
    tags: Vec<SpendingBreakdownEntry>,
    selected: Option<String>,
    currency: String,
    colors: HashMap<String, &'static str>,
    onclick: EventHandler<String>,
) -> Element {
    let slices = pie_slices(&tags, &colors);
    if slices.is_empty() {
        return rsx! {
            div { class: "chart-empty spending-empty",
                strong { "No spending in range" }
                small { "Refresh transactions or adjust the range." }
            }
        };
    }

    rsx! {
        svg {
            class: "spending-pie",
            view_box: "0 0 260 260",
            role: "img",
            for slice in slices {
                path {
                    class: if selected.as_ref() == Some(&slice.key) { "pie-slice selected" } else { "pie-slice" },
                    d: "{slice.path}",
                    style: "fill: {slice.color};",
                    onclick: move |_| onclick.call(slice.key.clone()),
                    title { "{slice.key}: {format_full_money(slice.total, &currency)}" }
                }
            }
            circle { class: "pie-hole", cx: "130", cy: "130", r: "56" }
            text { class: "pie-center-label", x: "130", y: "124", "Spend" }
            text { class: "pie-center-value", x: "130", y: "145", "{tags.len()}" }
        }
    }
}

#[component]
pub(super) fn TagRow(
    entry: SpendingBreakdownEntry,
    color: &'static str,
    currency: String,
    selected: bool,
    onclick: EventHandler<String>,
) -> Element {
    let class = if selected {
        "tag-row selected"
    } else {
        "tag-row"
    };
    let total = format_money_text(&entry.total, &currency).unwrap_or_else(|| entry.total.clone());

    rsx! {
        button {
            class: "{class}",
            onclick: move |_| onclick.call(entry.key.clone()),
            span {
                class: "tag-swatch",
                style: "background: {color};",
                aria_hidden: "true",
            }
            span { class: "tag-name", "{entry.key}" }
            strong { "{total}" }
            small { "{entry.transaction_count} tx" }
        }
    }
}
