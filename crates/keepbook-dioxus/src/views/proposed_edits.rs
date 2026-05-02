use super::*;
use crate::api::{decide_proposed_transaction_edit, fetch_proposed_transaction_edits};

#[component]
pub(super) fn ProposedEditsView(onrefresh: EventHandler<()>) -> Element {
    let mut proposals = use_resource(fetch_proposed_transaction_edits);
    let mut busy_id = use_signal(String::new);
    let mut status = use_signal(String::new);
    let current = proposals.cloned();
    let busy = busy_id();
    let status_text = status();

    rsx! {
        section { class: "panel",
            div { class: "panel-header",
                h2 { "Proposed transaction edits" }
                button {
                    class: "control-button",
                    disabled: !busy.is_empty(),
                    onclick: move |_| proposals.restart(),
                    "Refresh"
                }
            }
            if !status_text.is_empty() {
                p { class: "settings-status", "{status_text}" }
            }
            match current {
                None => rsx! { BackendActivity { message: "Loading proposed edits" } },
                Some(Err(error)) => rsx! { p { class: "validation", "{error}" } },
                Some(Ok(items)) => rsx! {
                    if items.is_empty() {
                        div { class: "chart-empty proposal-empty",
                            strong { "No pending edits" }
                            small { "Approved, rejected, and removed edits are hidden from this queue." }
                        }
                    } else {
                        div { class: "data-table proposed-edits-table",
                            div { class: "table-head",
                                span { "Transaction" }
                                span { "Account" }
                                span { "Patch" }
                                span { "Created" }
                                span { "Actions" }
                            }
                            for edit in items {
                                ProposedEditRow {
                                    edit: edit.clone(),
                                    busy: busy.clone(),
                                    ondecide: move |(id, action): (String, &'static str)| {
                                        busy_id.set(id.clone());
                                        status.set(format!("{action} {id}..."));
                                        spawn(async move {
                                            match decide_proposed_transaction_edit(id.clone(), action).await {
                                                Ok(()) => {
                                                    status.set(format!("{} {id}.", proposal_action_past_tense(action)));
                                                    proposals.restart();
                                                    onrefresh.call(());
                                                }
                                                Err(error) => status.set(error),
                                            }
                                            busy_id.set(String::new());
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
fn ProposedEditRow(
    edit: ProposedTransactionEdit,
    busy: String,
    ondecide: EventHandler<(String, &'static str)>,
) -> Element {
    let is_busy = busy == edit.id;
    let any_busy = !busy.is_empty();
    let patch = proposed_patch_summary(&edit.patch);
    let amount_class = if edit.transaction_amount.trim_start().starts_with('-') {
        "change-negative"
    } else {
        "change-positive"
    };
    let approve_id = edit.id.clone();
    let reject_id = edit.id.clone();
    let remove_id = edit.id.clone();

    rsx! {
        div { class: "table-row",
            div { class: "proposal-transaction-cell",
                strong { "{edit.transaction_description}" }
                small { "{edit.transaction_timestamp}" }
                small { class: "{amount_class}", "{edit.transaction_amount}" }
            }
            small { "{edit.account_name}" }
            small { "{patch}" }
            small { "{edit.created_at}" }
            div { class: "proposal-actions",
                button {
                    class: "control-button selected",
                    disabled: any_busy,
                    onclick: move |_| ondecide.call((approve_id.clone(), "approve")),
                    if is_busy { "Working" } else { "Approve" }
                }
                button {
                    class: "control-button",
                    disabled: any_busy,
                    onclick: move |_| ondecide.call((reject_id.clone(), "reject")),
                    "Reject"
                }
                button {
                    class: "control-button danger-button",
                    disabled: any_busy,
                    onclick: move |_| ondecide.call((remove_id.clone(), "remove")),
                    "Remove"
                }
            }
        }
    }
}
