use super::*;
use crate::api::{fetch_recurring_transactions, review_recurring_transaction};

#[component]
pub(super) fn RecurringView() -> Element {
    let mut include_possible = use_signal(|| true);
    let mut include_dismissed = use_signal(|| false);
    let mut min_confidence = use_signal(|| "0.70".to_string());
    let mut busy_key = use_signal(String::new);
    let mut status = use_signal(String::new);
    let mut recurring = use_resource(move || {
        let query =
            recurring_query_string(include_possible(), include_dismissed(), min_confidence());
        async move { fetch_recurring_transactions(query).await }
    });
    let current = recurring.cloned();
    let busy = busy_key();
    let status_text = status();

    rsx! {
        section { class: "panel recurring-panel",
            div { class: "panel-header",
                div {
                    h2 { "Recurring transactions" }
                    span { "Detected candidates with review state" }
                }
                button {
                    class: "control-button",
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
                    span { "Possible" }
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
            }
            if !status_text.is_empty() {
                p { class: "settings-status", "{status_text}" }
            }
            match current {
                None => rsx! { BackendActivity { message: "Loading recurring transactions" } },
                Some(Err(error)) => rsx! { p { class: "validation", "{error}" } },
                Some(Ok(items)) => rsx! {
                    if items.is_empty() {
                        div { class: "chart-empty proposal-empty",
                            strong { "No recurring candidates" }
                            small { "Adjust confidence or include possible candidates to widen the scan." }
                        }
                    } else {
                        div { class: "recurring-list",
                            for item in items {
                                RecurringCandidateCard {
                                    item: item.clone(),
                                    busy: busy.clone(),
                                    onreview: move |(candidate, review_status): (RecurringTransaction, &'static str)| {
                                        busy_key.set(candidate.candidate_key.clone());
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
    onreview: EventHandler<(RecurringTransaction, &'static str)>,
) -> Element {
    let amount_class = if item.amount.typical.trim_start().starts_with('-') {
        "change-negative"
    } else {
        "change-positive"
    };
    let review_class = format!("review-badge review-{}", item.review_status);
    let candidate_for_verify = item.clone();
    let candidate_for_dismiss = item.clone();
    let is_busy = busy == item.candidate_key;
    let any_busy = !busy.is_empty();
    let next = item
        .next_expected
        .clone()
        .unwrap_or_else(|| "unscheduled".to_string());

    rsx! {
        article { class: "recurring-card",
            div { class: "recurring-card-main",
                div { class: "recurring-title-row",
                    h3 { "{item.name}" }
                    span { class: "{review_class}", "{item.review_status}" }
                }
                div { class: "recurring-meta",
                    span { "{item.cadence}" }
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
                strong { class: "{amount_class}", "{item.amount.typical}" }
                small { "{item.first_seen} to {item.last_seen}" }
                small { "{item.occurrence_count} occurrences" }
            }
            div { class: "recurring-actions",
                button {
                    class: "control-button selected",
                    disabled: any_busy || item.review_status == "verified",
                    onclick: move |_| onreview.call((candidate_for_verify.clone(), "verified")),
                    if is_busy { "Working" } else { "Verify" }
                }
                button {
                    class: "control-button danger-button",
                    disabled: any_busy || item.review_status == "dismissed",
                    onclick: move |_| onreview.call((candidate_for_dismiss.clone(), "dismissed")),
                    "Dismiss"
                }
            }
            details { class: "recurring-occurrences",
                summary { "Transactions" }
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
