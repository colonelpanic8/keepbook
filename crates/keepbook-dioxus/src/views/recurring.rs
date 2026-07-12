use super::*;
use crate::api::{fetch_recurring_transactions, review_recurring_transaction};

#[component]
pub(super) fn RecurringView() -> Element {
    let mut include_possible = use_signal(|| false);
    let mut include_dismissed = use_signal(|| false);
    let mut min_confidence = use_signal(|| "0.70".to_string());
    let mut sort_order = use_signal(|| "annual_desc".to_string());
    let mut busy_key = use_signal(String::new);
    let mut busy_action = use_signal(String::new);
    let mut status = use_signal(String::new);
    let mut recurring = use_resource(move || {
        let query =
            recurring_query_string(include_possible(), include_dismissed(), min_confidence());
        async move { fetch_recurring_transactions(query).await }
    });
    let mut current = recurring.cloned();
    let sort_order_value = sort_order();
    if let Some(Ok(items)) = current.as_mut() {
        sort_recurring_items(items, &sort_order_value);
    }
    let busy = busy_key();
    let status_text = status();

    rsx! {
        section { class: "panel recurring-panel",
            div { class: "panel-header",
                div {
                    h2 { "Predictable recurring costs" }
                    span { "Active, regular outflows with stable amounts" }
                }
                ControlButton {
                    disabled: !busy.is_empty(),
                    onclick: move |_| recurring.restart(),
                    "Refresh"
                }
            }
            div { class: "recurring-controls",
                label { class: "toggle-field",
                    input {
                        r#type: "checkbox",
                        checked: include_possible(),
                        onchange: move |event| {
                            include_possible.set(event.checked());
                            recurring.restart();
                        }
                    }
                    span { "Borderline" }
                }
                label { class: "toggle-field",
                    input {
                        r#type: "checkbox",
                        checked: include_dismissed(),
                        onchange: move |event| {
                            include_dismissed.set(event.checked());
                            recurring.restart();
                        }
                    }
                    span { "Dismissed" }
                }
                label { class: "control-field compact-control-field",
                    span { "Confidence" }
                    input {
                        class: "control-input",
                        r#type: "number",
                        min: "0",
                        max: "1",
                        step: "0.05",
                        value: "{min_confidence()}",
                        oninput: move |event| min_confidence.set(event.value()),
                        onblur: move |_| recurring.restart(),
                    }
                }
                label { class: "control-field compact-control-field",
                    span { "Sort" }
                    select {
                        class: "control-input",
                        value: "{sort_order()}",
                        onchange: move |event| sort_order.set(event.value()),
                        option { value: "annual_desc", "Annual cost: high to low" }
                        option { value: "annual_asc", "Annual cost: low to high" }
                        option { value: "confidence", "Confidence" }
                    }
                }
            }
            if !status_text.is_empty() {
                OperationStatus { message: status_text, busy: !busy.is_empty() }
            }
            match current {
                None => rsx! { BackendActivity { message: "Loading recurring transactions" } },
                Some(Err(error)) => rsx! { p { class: "validation", "{error}" } },
                Some(Ok(items)) => rsx! {
                    if items.is_empty() {
                        div { class: "chart-empty proposal-empty",
                            strong { "No predictable recurring costs" }
                            small { "Include borderline patterns or lower confidence to widen the scan." }
                        }
                    } else {
                        div { class: "recurring-list",
                            for item in items {
                                RecurringCandidateCard {
                                    item: item.clone(),
                                    busy: busy.clone(),
                                    busy_action: busy_action(),
                                    onreview: move |(candidate, review_status): (RecurringTransaction, &'static str)| {
                                        busy_key.set(candidate.candidate_key.clone());
                                        busy_action.set(review_status.to_string());
                                        status.set(format!("Marking {} as {review_status}...", candidate.name));
                                        spawn(async move {
                                            let input = RecurringTransactionReviewInput {
                                                status: review_status.to_string(),
                                                candidate,
                                            };
                                            match review_recurring_transaction(input).await {
                                                Ok(()) => {
                                                    status.set(format!("Marked recurring transaction as {review_status}."));
                                                    recurring.restart();
                                                }
                                                Err(error) => status.set(error),
                                            }
                                            busy_key.set(String::new());
                                            busy_action.set(String::new());
                                        });
                                    }
                                }
                            }
                        }
                    }
                },
            }
        }
    }
}

#[component]
fn RecurringCandidateCard(
    item: RecurringTransaction,
    busy: String,
    busy_action: String,
    onreview: EventHandler<(RecurringTransaction, &'static str)>,
) -> Element {
    let review_class = format!("review-badge review-{}", item.review_status);
    let candidate_for_verify = item.clone();
    let candidate_for_dismiss = item.clone();
    let is_busy = busy == item.candidate_key;
    let any_busy = !busy.is_empty();
    let next = item
        .next_expected
        .clone()
        .unwrap_or_else(|| "unscheduled".to_string());
    let interval = cadence_display(&item.cadence);
    let interval_days = format!("≈{} days", item.estimated_interval_days);
    let recurring_cost = format_recurring_money(&item.estimated_recurring_cost, &item.amount.asset);
    let annual_cost = format_recurring_money(&item.estimated_annual_cost, &item.amount.asset);
    let observed_range = format_observed_cost_range(&item.amount);

    rsx! {
        article { class: "recurring-card",
            div { class: "recurring-card-main",
                div { class: "recurring-title-row",
                    h3 { "{item.name}" }
                    span { class: "{review_class}", "{item.review_status}" }
                }
                div { class: "recurring-meta",
                    span { "{item.status}" }
                    span { "confidence {item.confidence}" }
                    span { "next {next}" }
                }
                div { class: "recurring-reasons",
                    for reason in item.reason_codes.iter() {
                        span { class: "reason-chip", "{reason}" }
                    }
                }
            }
            div { class: "recurring-amount-cell",
                div { class: "recurring-estimate",
                    small { "Estimated interval" }
                    strong { "{interval}" }
                    small { "{interval_days}" }
                }
                div { class: "recurring-estimate",
                    small { "Recurring cost" }
                    strong { class: "change-negative", "{recurring_cost}" }
                    small { "{observed_range}" }
                }
                div { class: "recurring-estimate recurring-estimate-annual",
                    small { "Estimated annual cost" }
                    strong { class: "change-negative", "{annual_cost}" }
                }
            }
            div { class: "recurring-actions",
                ControlButton {
                    selected: true,
                    disabled: any_busy || item.review_status == "verified",
                    busy: is_busy && busy_action == "verified",
                    onclick: move |_| onreview.call((candidate_for_verify.clone(), "verified")),
                    if is_busy && busy_action == "verified" { "Verifying" } else { "Verify" }
                }
                ControlButton {
                    class: "danger-button",
                    disabled: any_busy || item.review_status == "dismissed",
                    busy: is_busy && busy_action == "dismissed",
                    onclick: move |_| onreview.call((candidate_for_dismiss.clone(), "dismissed")),
                    if is_busy && busy_action == "dismissed" { "Dismissing" } else { "Dismiss" }
                }
            }
            details { class: "recurring-occurrences",
                summary { "{item.occurrence_count} transactions · {item.first_seen} to {item.last_seen}" }
                div { class: "data-table recurring-occurrence-table",
                    div { class: "table-head",
                        span { "Date" }
                        span { "Description" }
                        span { "Account" }
                        span { "Amount" }
                    }
                    for occurrence in item.transactions.iter() {
                        div { class: "table-row",
                            small { "{occurrence.date}" }
                            small { "{occurrence.description}" }
                            small { "{occurrence.account_name}" }
                            small { "{occurrence.amount}" }
                        }
                    }
                }
            }
        }
    }
}

fn sort_recurring_items(items: &mut [RecurringTransaction], sort_order: &str) {
    items.sort_by(|left, right| {
        let left_annual = left
            .estimated_annual_cost
            .parse::<f64>()
            .unwrap_or_default();
        let right_annual = right
            .estimated_annual_cost
            .parse::<f64>()
            .unwrap_or_default();
        let left_confidence = left.confidence.parse::<f64>().unwrap_or_default();
        let right_confidence = right.confidence.parse::<f64>().unwrap_or_default();

        let primary = match sort_order {
            "annual_asc" => left_annual.total_cmp(&right_annual),
            "confidence" => right_confidence.total_cmp(&left_confidence),
            _ => right_annual.total_cmp(&left_annual),
        };
        primary.then_with(|| left.name.cmp(&right.name))
    });
}

fn cadence_display(cadence: &str) -> String {
    match cadence {
        "weekly" => "Every week".to_string(),
        "biweekly" => "Every 2 weeks".to_string(),
        "every_4_weeks" => "Every 4 weeks".to_string(),
        "monthly" => "Every month".to_string(),
        "every_2_months" => "Every 2 months".to_string(),
        "quarterly" => "Every 3 months".to_string(),
        "semiannual" => "Every 6 months".to_string(),
        "yearly" => "Every year".to_string(),
        other => other.replace('_', " "),
    }
}

fn format_recurring_money(raw: &str, asset: &serde_json::Value) -> String {
    let currency = asset
        .get("iso_code")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    raw.parse::<f64>()
        .map(|amount| format_full_money(amount, currency))
        .unwrap_or_else(|_| {
            if currency.is_empty() {
                raw.to_string()
            } else {
                format!("{raw} {currency}")
            }
        })
}

fn format_observed_cost_range(amount: &RecurringTransactionAmount) -> String {
    let parsed = amount
        .min
        .parse::<f64>()
        .ok()
        .zip(amount.max.parse::<f64>().ok());
    let Some((left, right)) = parsed else {
        return "Observed amount unavailable".to_string();
    };
    let low = left.abs().min(right.abs());
    let high = left.abs().max(right.abs());
    let currency = amount
        .asset
        .get("iso_code")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    if (high - low).abs() < 0.005 {
        format!("Observed {}", format_full_money(low, currency))
    } else {
        format!(
            "Observed {}–{}",
            format_full_money(low, currency),
            format_full_money(high, currency)
        )
    }
}

fn recurring_query_string(
    include_possible: bool,
    include_dismissed: bool,
    min_confidence: String,
) -> String {
    let confidence = min_confidence
        .trim()
        .parse::<f64>()
        .ok()
        .filter(|value| (0.0..=1.0).contains(value))
        .unwrap_or(0.70);
    format!(
        "include_possible={include_possible}&include_dismissed={include_dismissed}&min_confidence={confidence}"
    )
}
