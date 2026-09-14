use super::*;

#[component]
pub(super) fn SpendingMatchLists(
    exact_entries: Vec<SpendingBreakdownEntry>,
    close_entries: Vec<SpendingBreakdownEntry>,
    currency: String,
) -> Element {
    rsx! {
        div { class: "spending-match-layout",
            SpendingMatchList {
                title: "Exact String Match",
                entries: exact_entries,
                currency: currency.clone(),
            }
            SpendingMatchList {
                title: "Close String Match",
                entries: close_entries,
                currency,
            }
        }
    }
}

#[component]
fn SpendingMatchList(
    title: &'static str,
    entries: Vec<SpendingBreakdownEntry>,
    currency: String,
) -> Element {
    rsx! {
        section { class: "spending-match-list",
            div { class: "spending-match-header",
                h3 { "{title}" }
                span { "Top {entries.len()}" }
            }
            if entries.is_empty() {
                p { class: "range-summary", "No spending in range" }
            } else {
                for entry in entries {
                    SpendingMatchRow {
                        entry,
                        currency: currency.clone(),
                    }
                }
            }
        }
    }
}

#[component]
fn SpendingMatchRow(entry: SpendingBreakdownEntry, currency: String) -> Element {
    let total = format_money_text(&entry.total, &currency).unwrap_or_else(|| entry.total.clone());

    rsx! {
        div { class: "spending-match-row",
            span { class: "spending-match-name", "{entry.key}" }
            strong { "{total}" }
            small { "{entry.transaction_count} tx" }
        }
    }
}
