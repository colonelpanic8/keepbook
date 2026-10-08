use crate::components::*;
use crate::dto::*;
use crate::logic::*;
use dioxus::prelude::*;
use std::collections::HashMap;

#[component]
fn SpendingPieChart(
    tags: Vec<SpendingBreakdownEntry>,
    selected: Option<String>,
    currency: String,
    colors: HashMap<String, &'static str>,
    onclick: EventHandler<String>,
) -> Element {
    let slices = pie_slices(&tags, &colors);
    if slices.is_empty() {
        return rsx! {
            EmptyState {
                compact: true,
                title: "No spending in range",
                detail: "Refresh transactions or adjust the range.",
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
fn TagRow(
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

/// One total card beside the donut; `highlighted` marks a focused period or selection.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SpendingTotal {
    pub(crate) label: String,
    pub(crate) value: String,
    pub(crate) detail: String,
    pub(crate) highlighted: bool,
}

/// Spending by tag: a donut chart beside the totals and one row per tag.
///
/// Mirror: `design/src/charts/SpendingBreakdown.tsx`.
#[component]
pub(crate) fn SpendingBreakdown(
    tags: Vec<SpendingBreakdownEntry>,
    colors: HashMap<String, &'static str>,
    currency: String,
    totals: Vec<SpendingTotal>,
    selected: Option<String>,
    onselect: EventHandler<String>,
) -> Element {
    rsx! {
        div { class: "spending-layout",
            div { class: "spending-chart-area",
                SpendingPieChart {
                    tags: tags.clone(),
                    selected: selected.clone(),
                    currency: currency.clone(),
                    colors: colors.clone(),
                    onclick: move |tag: String| onselect.call(tag),
                }
            }
            div { class: "tag-list",
                for total in totals {
                    div { class: if total.highlighted { "spending-total selected-total" } else { "spending-total" },
                        span { class: "metric-label", "{total.label}" }
                        strong { "{total.value}" }
                        small { "{total.detail}" }
                    }
                }
                for (index, entry) in tags.iter().enumerate() {
                    TagRow {
                        entry: entry.clone(),
                        color: spending_tag_color_for(&colors, &entry.key, index),
                        currency: currency.clone(),
                        selected: selected.as_ref() == Some(&entry.key),
                        onclick: move |tag: String| onselect.call(tag),
                    }
                }
            }
        }
    }
}
