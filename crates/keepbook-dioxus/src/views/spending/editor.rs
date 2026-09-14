use super::*;

#[component]
pub(super) fn GroupTagsEditor(
    selected_count: usize,
    tag_targets: Vec<TransactionTagTargetInput>,
    tag_options: Vec<String>,
    disabled: bool,
    onclose: EventHandler<()>,
    ontagsbulksave: EventHandler<SetTransactionTagsInput>,
) -> Element {
    let mut selected_tags = use_signal(Vec::<String>::new);
    let mut draft_tag = use_signal(String::new);
    let draft = draft_tag();
    let has_selection = selected_count > 0 && !tag_targets.is_empty() && !disabled;
    let apply_targets = tag_targets.clone();
    let clear_targets = tag_targets.clone();
    let tags = selected_tags();
    let suggestions = if has_selection {
        tag_editor_suggestions(&tag_options, &tags)
    } else {
        Vec::new()
    };
    let can_add = has_selection && !draft.trim().is_empty();
    let list_id = "group-tag-options";

    rsx! {
        div { class: "group-edit-body",
            div { class: "group-edit-copy",
                small { "{selected_count} selected" }
            }
            div { class: "tag-editor bulk-tag-editor",
                div { class: "tag-pill-list",
                    if tags.is_empty() {
                        span { class: "tag-empty", "No tags selected" }
                    }
                    for tag in tags.clone() {
                        button {
                            class: "tag-pill removable",
                            title: "Remove tag",
                            disabled: !has_selection,
                            onclick: move |_| selected_tags.set(remove_tag_from_list(&selected_tags(), &tag)),
                            span { "{tag}" }
                            span { class: "tag-pill-remove", "x" }
                        }
                    }
                    div { class: "tag-entry-row",
                        input {
                            class: "tag-editor-input tag-entry-input",
                            r#type: "text",
                            list: "{list_id}",
                            value: "{draft}",
                            placeholder: "Add tag",
                            disabled: !has_selection,
                            oninput: move |event| draft_tag.set(event.value())
                        }
                        datalist { id: "{list_id}",
                            for tag in tag_options.clone() {
                                option { value: "{tag}" }
                            }
                        }
                        button {
                            class: "tag-editor-button",
                            title: "Add tag",
                            disabled: !can_add,
                            onclick: move |_| {
                                selected_tags.set(add_tag_to_list(selected_tags(), &draft_tag()));
                                draft_tag.set(String::new());
                            },
                            "+"
                        }
                    }
                }
                if !suggestions.is_empty() {
                    div { class: "tag-suggestion-list",
                        for tag in suggestions {
                            button {
                                class: "tag-suggestion-pill",
                                disabled: !has_selection,
                                onclick: move |_| selected_tags.set(add_tag_to_list(selected_tags(), &tag)),
                                "{tag}"
                            }
                        }
                    }
                }
            }
            div { class: "group-tag-row tag-action-row",
                ControlButton {
                    selected: true,
                    disabled: !has_selection || tags.is_empty(),
                    onclick: move |_| {
                        ontagsbulksave.call(SetTransactionTagsInput {
                            transactions: apply_targets.clone(),
                            tags: selected_tags(),
                            clear_tags: false,
                        });
                        onclose.call(());
                    },
                    "Apply Tags"
                }
                ControlButton {
                    disabled: !has_selection,
                    onclick: move |_| {
                        selected_tags.set(Vec::new());
                        draft_tag.set(String::new());
                        ontagsbulksave.call(SetTransactionTagsInput {
                            transactions: clear_targets.clone(),
                            tags: Vec::new(),
                            clear_tags: true,
                        });
                        onclose.call(());
                    },
                    "Clear Tags"
                }
            }
        }
    }
}

#[component]
pub(super) fn AiRuleSuggestions(result: AiRuleSuggestionsOutput) -> Element {
    rsx! {
        div { class: "ai-rule-suggestions",
            if let Some(message) = result.message.clone() {
                p { class: "ai-rule-message", "{message}" }
            }
            for suggestion in result.suggestions {
                div { class: "ai-rule-suggestion",
                    strong { "{ai_tool_label(&suggestion.name)}" }
                    pre { "{format_json_value(&suggestion.arguments)}" }
                }
            }
        }
    }
}

fn single_transaction_target(
    account_id: &str,
    transaction_id: &str,
) -> Vec<TransactionTagTargetInput> {
    vec![TransactionTagTargetInput {
        account_id: account_id.to_string(),
        transaction_id: transaction_id.to_string(),
    }]
}

fn transaction_tags_input(
    account_id: &str,
    transaction_id: &str,
    tags: Vec<String>,
) -> SetTransactionTagsInput {
    SetTransactionTagsInput {
        transactions: single_transaction_target(account_id, transaction_id),
        clear_tags: tags.is_empty(),
        tags,
    }
}

/// Full-width editor rendered directly beneath an expanded transaction row.
/// Tag edits and the spending toggle commit immediately; the reporting date
/// commits on change.
#[component]
pub(super) fn TransactionEditorPanel(
    transaction: Transaction,
    tag_options: Vec<String>,
    disabled: bool,
    ontagssave: EventHandler<SetTransactionTagsInput>,
    oneffectivedatesave: EventHandler<SetTransactionEffectiveDateInput>,
    onignoresave: EventHandler<SetTransactionIgnoreInput>,
) -> Element {
    let account_id = transaction.account_id.clone();
    let transaction_id = transaction.id.clone();
    let current_tags = transaction_tags(&transaction);
    let mut draft_tag = use_signal(String::new);
    let mut date_dismissal = use_date_picker_dismissal();
    let draft = draft_tag();
    let can_add = !disabled && !draft.trim().is_empty();
    let suggestions = tag_editor_suggestions(&tag_options, &current_tags);
    let list_id = format!("tag-options-{account_id}-{transaction_id}");

    let posted_date = transaction
        .timestamp
        .get(..10)
        .unwrap_or(&transaction.timestamp)
        .to_string();
    let current_effective = transaction
        .annotation
        .as_ref()
        .and_then(|annotation| annotation.effective_date.clone());
    let has_effective_date = current_effective.is_some();
    let date_value = current_effective.unwrap_or_else(|| posted_date.clone());

    let rule_ignored = spending_ignore_is_configured(&transaction);
    let not_spending_shaped = spending_ignore_is_shape(&transaction);
    let exclude_checked = rule_ignored || spending_ignore_is_annotation(&transaction);

    rsx! {
        div { class: "transaction-editor-panel",
            div { class: "transaction-editor-grid",
                div { class: "transaction-editor-section",
                    span { class: "transaction-editor-label", "Tags" }
                    div { class: "tag-editor transaction-tag-editor",
                        div { class: "tag-pill-list",
                            if current_tags.is_empty() {
                                span { class: "tag-empty", "Untagged" }
                            }
                            for tag in current_tags.clone() {
                                button {
                                    class: "tag-pill removable",
                                    title: "Remove tag",
                                    disabled,
                                    onclick: {
                                        let account_id = account_id.clone();
                                        let transaction_id = transaction_id.clone();
                                        let current_tags = current_tags.clone();
                                        let tag = tag.clone();
                                        move |_| {
                                            let next = remove_tag_from_list(&current_tags, &tag);
                                            ontagssave.call(transaction_tags_input(
                                                &account_id,
                                                &transaction_id,
                                                next,
                                            ));
                                        }
                                    },
                                    span { "{tag}" }
                                    span { class: "tag-pill-remove", "x" }
                                }
                            }
                            div { class: "tag-entry-row",
                                input {
                                    class: "tag-editor-input tag-entry-input",
                                    r#type: "text",
                                    list: "{list_id}",
                                    value: "{draft}",
                                    placeholder: "Add tag",
                                    disabled,
                                    oninput: move |event| draft_tag.set(event.value()),
                                    onkeydown: {
                                        let account_id = account_id.clone();
                                        let transaction_id = transaction_id.clone();
                                        let current_tags = current_tags.clone();
                                        move |event: KeyboardEvent| {
                                            if event.key() != Key::Enter {
                                                return;
                                            }
                                            let value = draft_tag();
                                            if value.trim().is_empty() {
                                                return;
                                            }
                                            let next = add_tag_to_list(current_tags.clone(), &value);
                                            draft_tag.set(String::new());
                                            ontagssave.call(transaction_tags_input(
                                                &account_id,
                                                &transaction_id,
                                                next,
                                            ));
                                        }
                                    }
                                }
                                datalist { id: "{list_id}",
                                    for tag in tag_options.clone() {
                                        option { value: "{tag}" }
                                    }
                                }
                                button {
                                    class: "tag-editor-button",
                                    title: "Add tag",
                                    disabled: !can_add,
                                    onclick: {
                                        let account_id = account_id.clone();
                                        let transaction_id = transaction_id.clone();
                                        let current_tags = current_tags.clone();
                                        move |_| {
                                            let value = draft_tag();
                                            if value.trim().is_empty() {
                                                return;
                                            }
                                            let next = add_tag_to_list(current_tags.clone(), &value);
                                            draft_tag.set(String::new());
                                            ontagssave.call(transaction_tags_input(
                                                &account_id,
                                                &transaction_id,
                                                next,
                                            ));
                                        }
                                    },
                                    "+"
                                }
                            }
                        }
                        if !suggestions.is_empty() {
                            div { class: "tag-suggestion-list",
                                for tag in suggestions.clone() {
                                    button {
                                        class: "tag-suggestion-pill",
                                        disabled,
                                        onclick: {
                                            let account_id = account_id.clone();
                                            let transaction_id = transaction_id.clone();
                                            let current_tags = current_tags.clone();
                                            let tag = tag.clone();
                                            move |_| {
                                                let next = add_tag_to_list(current_tags.clone(), &tag);
                                                ontagssave.call(transaction_tags_input(
                                                    &account_id,
                                                    &transaction_id,
                                                    next,
                                                ));
                                            }
                                        },
                                        "{tag}"
                                    }
                                }
                            }
                        }
                    }
                }
                div { class: "transaction-editor-section",
                    span { class: "transaction-editor-label", "Reporting date" }
                    div { class: "effective-date-editor",
                        input {
                            class: "effective-date-input",
                            r#type: "date",
                            value: "{date_value}",
                            title: "Reporting date",
                            disabled,
                            onmounted: move |event| date_dismissal.on_mounted(event),
                            onmousedown: move |_| date_dismissal.on_pointer_edit(),
                            onkeydown: move |_| date_dismissal.on_key_edit(),
                            onchange: {
                                let account_id = account_id.clone();
                                let transaction_id = transaction_id.clone();
                                move |event: FormEvent| {
                                    date_dismissal.dismiss();
                                    let value = event.value().trim().to_string();
                                    let clear_effective_date =
                                        value.is_empty() || value == posted_date;
                                    oneffectivedatesave.call(SetTransactionEffectiveDateInput {
                                        account_id: account_id.clone(),
                                        transaction_id: transaction_id.clone(),
                                        effective_date: if clear_effective_date {
                                            None
                                        } else {
                                            Some(value)
                                        },
                                        clear_effective_date,
                                    });
                                }
                            }
                        }
                        small { "Posted {posted_date}" }
                        if has_effective_date {
                            button {
                                class: "tag-editor-button",
                                title: "Reset to posted date",
                                disabled,
                                onclick: {
                                    let account_id = account_id.clone();
                                    let transaction_id = transaction_id.clone();
                                    move |_| {
                                        oneffectivedatesave.call(SetTransactionEffectiveDateInput {
                                            account_id: account_id.clone(),
                                            transaction_id: transaction_id.clone(),
                                            effective_date: None,
                                            clear_effective_date: true,
                                        });
                                    }
                                },
                                "Reset to posted"
                            }
                        }
                    }
                }
                div { class: "transaction-editor-section",
                    span { class: "transaction-editor-label", "Spending" }
                    label { class: "compact-check transaction-exclude-toggle",
                        input {
                            r#type: "checkbox",
                            checked: exclude_checked,
                            disabled: rule_ignored || disabled,
                            onchange: {
                                let account_id = account_id.clone();
                                let transaction_id = transaction_id.clone();
                                move |event: FormEvent| {
                                    onignoresave.call(SetTransactionIgnoreInput {
                                        transactions: single_transaction_target(
                                            &account_id,
                                            &transaction_id,
                                        ),
                                        ignore: event.checked(),
                                    });
                                }
                            }
                        }
                        span { "Exclude from spending" }
                    }
                    if rule_ignored {
                        small { class: "transaction-exclude-note", "Excluded by an ignore rule" }
                    } else if not_spending_shaped {
                        small { class: "transaction-exclude-note",
                            "Never counted: only posted charges count toward spending"
                        }
                    }
                }
            }
        }
    }
}

fn add_tag_to_list(tags: Vec<String>, tag: &str) -> Vec<String> {
    let mut tags = tags;
    tags.push(tag.to_string());
    normalize_tags(tags)
}

fn remove_tag_from_list(tags: &[String], tag_to_remove: &str) -> Vec<String> {
    normalize_tags(
        tags.iter()
            .filter(|tag| !tag.eq_ignore_ascii_case(tag_to_remove))
            .cloned()
            .collect(),
    )
}

fn tag_editor_suggestions(tag_options: &[String], selected_tags: &[String]) -> Vec<String> {
    tag_options
        .iter()
        .filter(|tag| {
            !selected_tags
                .iter()
                .any(|selected| selected.eq_ignore_ascii_case(tag))
        })
        .take(8)
        .cloned()
        .collect()
}
