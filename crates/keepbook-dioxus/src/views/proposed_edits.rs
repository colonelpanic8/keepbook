use super::*;
use crate::api::{decide_proposed_transaction_edit, fetch_proposed_transaction_edits};

#[component]
pub(super) fn ProposedEditsView(onrefresh: EventHandler<()>) -> Element {
    let mut proposals = use_resource(fetch_proposed_transaction_edits);
    let mut busy_id = use_signal(String::new);
    let mut busy_action = use_signal(String::new);
    let status = use_action_feedback();
    let current = proposals.cloned();
    let busy = busy_id();

    rsx! {
        Panel {
            title: "Proposed transaction edits",
            status: status.status_for("decide"),
            actions: rsx! {
                ControlButton {
                    disabled: !busy.is_empty(),
                    onclick: move |_| proposals.restart(),
                    "Refresh"
                }
            },
            if let Some(error) = status.error_for("decide") {
                ErrorNotice { message: error, ondismiss: move |_| status.dismiss() }
            }
            match current {
                None => rsx! { OperationStatus { message: "Loading proposed edits".to_string(), busy: true } },
                Some(Err(error)) => rsx! { p { class: "validation", "{error}" } },
                Some(Ok(items)) => rsx! {
                    if items.is_empty() {
                        EmptyState {
                            compact: true,
                            title: "No pending edits",
                            detail: "Approved, rejected, and removed edits are hidden from this queue.",
                        }
                    } else {
                        DataTable {
                            class: "proposed-edits-table",
                            columns: ["Transaction", "Account", "Patch", "Created", "Actions"].map(str::to_string).to_vec(),
                            for edit in items {
                                ProposedEditRow {
                                    edit: edit.clone(),
                                    busy: busy.clone(),
                                    busy_action: busy_action(),
                                    ondecide: move |(id, action): (String, &'static str)| {
                                        busy_id.set(id.clone());
                                        busy_action.set(action.to_string());
                                        spawn(async move {
                                            match decide_proposed_transaction_edit(id.clone(), action).await {
                                                Ok(()) => {
                                                    status.succeed("decide", format!("Edit {}", proposal_action_past_tense(action).to_lowercase()), None);
                                                    proposals.restart();
                                                    onrefresh.call(());
                                                }
                                                Err(error) => status.fail("decide", "Update failed", format!("Updating the proposed edit failed: {error}")),
                                            }
                                            busy_id.set(String::new());
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
fn ProposedEditRow(
    edit: ProposedTransactionEdit,
    busy: String,
    busy_action: String,
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
                ControlButton {
                    primary: true,
                    disabled: any_busy,
                    busy: is_busy && busy_action == "approve",
                    onclick: move |_| ondecide.call((approve_id.clone(), "approve")),
                    if is_busy && busy_action == "approve" { "Approving" } else { "Approve" }
                }
                ControlButton {
                    disabled: any_busy,
                    busy: is_busy && busy_action == "reject",
                    onclick: move |_| ondecide.call((reject_id.clone(), "reject")),
                    if is_busy && busy_action == "reject" { "Rejecting" } else { "Reject" }
                }
                ControlButton {
                    danger: true,
                    disabled: any_busy,
                    busy: is_busy && busy_action == "remove",
                    onclick: move |_| ondecide.call((remove_id.clone(), "remove")),
                    if is_busy && busy_action == "remove" { "Removing" } else { "Remove" }
                }
            }
        }
    }
}
