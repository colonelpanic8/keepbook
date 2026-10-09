use super::*;
use std::collections::HashSet;

#[component]
pub(super) fn TransactionList(
    transactions: Vec<Transaction>,
    currency: String,
    range_text: String,
    sort_field: TransactionSortField,
    sort_direction: SortDirection,
    show_ignored: bool,
    title_filter: String,
    selected_keys: HashSet<String>,
    selected_count: usize,
    page: usize,
    page_count: usize,
    tag_options: Vec<String>,
    onshowignoredchange: EventHandler<bool>,
    onsortfieldchange: EventHandler<TransactionSortField>,
    onsortdirectionchange: EventHandler<SortDirection>,
    ontitlefilterchange: EventHandler<String>,
    ontoggleselection: EventHandler<String>,
    onselectpage: EventHandler<MouseEvent>,
    onselectfiltered: EventHandler<MouseEvent>,
    onclearselection: EventHandler<MouseEvent>,
    onprev: EventHandler<MouseEvent>,
    onnext: EventHandler<MouseEvent>,
    ai_prompt: String,
    ai_status: Option<String>,
    ai_busy: bool,
    mutation_busy: bool,
    ai_result: Option<AiRuleSuggestionsOutput>,
    tag_targets: Vec<TransactionTagTargetInput>,
    onpromptchange: EventHandler<String>,
    onairulesubmit: EventHandler<MouseEvent>,
    ontagssave: EventHandler<SetTransactionTagsInput>,
    ontagsbulksave: EventHandler<SetTransactionTagsInput>,
    oneffectivedatesave: EventHandler<SetTransactionEffectiveDateInput>,
    onignoresave: EventHandler<SetTransactionIgnoreInput>,
) -> Element {
    let has_any_selection = !selected_keys.is_empty();
    let has_visible_selection = selected_count > 0 && !tag_targets.is_empty();
    let has_transactions = !transactions.is_empty();
    let mut group_editor_open = use_signal(|| false);
    let mut expanded_transaction_key = use_signal(|| None::<String>);
    rsx! {
        div { class: "transaction-panel",
            div { class: "panel-header transaction-header",
                div { class: "panel-title",
                    h2 { "Transactions" }
                    span { "{range_text}" }
                }
                div { class: "pagination-controls",
                    IconButton {
                        label: "Previous page",
                        glyph: "‹",
                        disabled: page == 0,
                        onclick: move |event| onprev.call(event),
                    }
                    span { "{page + 1} / {page_count}" }
                    IconButton {
                        label: "Next page",
                        glyph: "›",
                        disabled: page + 1 >= page_count,
                        onclick: move |event| onnext.call(event),
                    }
                }
            }
            div { class: "transaction-controls",
                TextInput {
                    kind: InputKind::Search,
                    class: "transaction-search-input",
                    value: title_filter.clone(),
                    placeholder: "Filter titles",
                    oninput: move |value| ontitlefilterchange.call(value)
                }
                Checkbox {
                    label: "Show ignored",
                    checked: show_ignored,
                    onchange: move |checked| onshowignoredchange.call(checked)
                }
                ControlButton {
                    onclick: move |event| onselectpage.call(event),
                    disabled: !has_transactions,
                    "Select Page"
                }
                button {
                    class: "control-button",
                    title: "Select all transactions matching the current filters",
                    onclick: move |event| onselectfiltered.call(event),
                    disabled: !has_transactions,
                    "Select All"
                }
                ControlButton {
                    onclick: move |event| onclearselection.call(event),
                    disabled: !has_any_selection,
                    "Clear"
                }
                if has_visible_selection {
                    ControlButton {
                        primary: true,
                        disabled: mutation_busy,
                        onclick: move |_| group_editor_open.set(true),
                        "Edit Tags"
                    }
                    button {
                        class: "control-button",
                        title: "Exclude selected transactions from spending totals",
                        disabled: mutation_busy,
                        onclick: {
                            let targets = tag_targets.clone();
                            move |_| onignoresave.call(SetTransactionIgnoreInput {
                                transactions: targets.clone(),
                                ignore: true,
                            })
                        },
                        "Exclude from spending"
                    }
                    button {
                        class: "control-button",
                        title: "Include selected transactions in spending totals",
                        disabled: mutation_busy,
                        onclick: {
                            let targets = tag_targets.clone();
                            move |_| onignoresave.call(SetTransactionIgnoreInput {
                                transactions: targets.clone(),
                                ignore: false,
                            })
                        },
                        "Include in spending"
                    }
                }
            }
            div { class: "ai-rule-panel",
                div { class: "ai-rule-copy",
                    strong { "AI rule assistant" }
                    small { "{selected_count} selected" }
                }
                TextInput {
                    multiline: true,
                    value: ai_prompt.clone(),
                    placeholder: "Ask for a tag, ignore, or rename rule for the selected transactions.",
                    disabled: ai_busy,
                    oninput: move |value| onpromptchange.call(value)
                }
                div { class: "ai-rule-actions",
                    ControlButton {
                        primary: true,
                        busy: ai_busy,
                        onclick: move |event| onairulesubmit.call(event),
                        disabled: selected_count == 0 || ai_prompt.trim().is_empty() || ai_busy,
                        if ai_busy { "Asking AI" } else { "Ask AI" }
                    }
                    if let Some(status) = ai_status.clone() {
                        FloatingStatus { message: status, busy: ai_busy }
                    }
                }
                if let Some(result) = ai_result.clone() {
                    AiRuleSuggestions { result }
                }
            }
            if has_visible_selection && group_editor_open() {
                Modal {
                    wide: true,
                    title: "Group edit",
                    header_actions: rsx! {
                        IconButton {
                            label: "Close group edit",
                            glyph: "×",
                            onclick: move |_| group_editor_open.set(false),
                        }
                    },
                    GroupTagsEditor {
                        selected_count,
                        tag_targets: tag_targets.clone(),
                        tag_options: tag_options.clone(),
                        disabled: mutation_busy,
                        onclose: move |_| group_editor_open.set(false),
                        ontagsbulksave,
                    }
                }
            }
            if !has_transactions {
                EmptyState {
                    compact: true,
                    title: "No matching transactions",
                    detail: "Select another tag or range.",
                }
            } else {
                div { class: "data-table transaction-table",
                    div { class: "table-head",
                        span { "" }
                        TransactionSortHeader {
                            label: "Date",
                            field: TransactionSortField::Date,
                            selected_field: sort_field,
                            direction: sort_direction,
                            onsortfieldchange,
                            onsortdirectionchange,
                        }
                        TransactionSortHeader {
                            label: "Description",
                            field: TransactionSortField::Description,
                            selected_field: sort_field,
                            direction: sort_direction,
                            onsortfieldchange,
                            onsortdirectionchange,
                        }
                        TransactionSortHeader {
                            label: "Tags",
                            field: TransactionSortField::Tag,
                            selected_field: sort_field,
                            direction: sort_direction,
                            onsortfieldchange,
                            onsortdirectionchange,
                        }
                        TransactionSortHeader {
                            label: "Account",
                            field: TransactionSortField::Account,
                            selected_field: sort_field,
                            direction: sort_direction,
                            onsortfieldchange,
                            onsortdirectionchange,
                        }
                        TransactionSortHeader {
                            label: "Amount",
                            field: TransactionSortField::Amount,
                            selected_field: sort_field,
                            direction: sort_direction,
                            onsortfieldchange,
                            onsortdirectionchange,
                        }
                        span { "" }
                    }
                    for tx in transactions.clone() {
                        {
                            let key = transaction_key(&tx);
                            let is_expanded = expanded_transaction_key().as_deref() == Some(key.as_str());
                            let checkbox_key = key.clone();
                            let row_toggle_key = key.clone();
                            let chevron_toggle_key = key.clone();
                            let is_selected = selected_keys.contains(&key);
                            let has_effective_date = tx
                                .annotation
                                .as_ref()
                                .and_then(|annotation| annotation.effective_date.as_ref())
                                .is_some();
                            let posted_date = tx
                                .timestamp
                                .get(..10)
                                .unwrap_or(&tx.timestamp)
                                .to_string();
                            let row_tags = visible_transaction_tags(&tx);
                            let row_class = if is_expanded {
                                format!("{} expanded", transaction_row_class(&tx))
                            } else {
                                transaction_row_class(&tx).to_string()
                            };
                            rsx! {
                                div {
                                    key: "{key}",
                                    class: "{row_class}",
                                    title: if tx.ignored_from_spending { "Not counted in spending totals" } else { "" },
                                    onclick: move |_| {
                                        let next = if expanded_transaction_key().as_deref()
                                            == Some(row_toggle_key.as_str())
                                        {
                                            None
                                        } else {
                                            Some(row_toggle_key.clone())
                                        };
                                        expanded_transaction_key.set(next);
                                    },
                                    label {
                                        class: "transaction-select-cell",
                                        onclick: move |event| event.stop_propagation(),
                                        input {
                                            r#type: "checkbox",
                                            checked: is_selected,
                                            onchange: move |_| ontoggleselection.call(checkbox_key.clone())
                                        }
                                    }
                                    span { class: "transaction-date-cell",
                                        span { class: "transaction-date-text", "{transaction_date(&tx)}" }
                                        if has_effective_date {
                                            span {
                                                class: "effective-date-indicator",
                                                title: "Posted {posted_date}",
                                                "*"
                                            }
                                        }
                                    }
                                    strong { class: "transaction-description-cell", "{transaction_description(&tx)}" }
                                    span { class: "transaction-tag-cell",
                                        div { class: "transaction-tag-readonly",
                                            if row_tags.is_empty() {
                                                span { class: "tag-empty", "Untagged" }
                                            }
                                            for tag in row_tags.clone() {
                                                TagPill { tag: tag.clone() }
                                            }
                                        }
                                        if tx.ignored_from_spending {
                                            Badge { "Not counted" }
                                        }
                                    }
                                    span { class: "transaction-account-cell", "{tx.account_name}" }
                                    strong { class: "transaction-amount-cell", "{format_transaction_amount(&tx, &currency)}" }
                                    IconButton {
                                        class: "transaction-expand-toggle",
                                        label: if is_expanded { "Collapse editor" } else { "Expand to edit" },
                                        glyph: if is_expanded { "\u{2304}" } else { "\u{203A}" },
                                        onclick: move |event: MouseEvent| {
                                            event.stop_propagation();
                                            let next = if expanded_transaction_key().as_deref()
                                                == Some(chevron_toggle_key.as_str())
                                            {
                                                None
                                            } else {
                                                Some(chevron_toggle_key.clone())
                                            };
                                            expanded_transaction_key.set(next);
                                        },
                                    }
                                }
                                if is_expanded {
                                    TransactionEditorPanel {
                                        transaction: tx.clone(),
                                        tag_options: tag_options.clone(),
                                        disabled: mutation_busy,
                                        ontagssave,
                                        oneffectivedatesave,
                                        onignoresave,
                                    }
                                }
                            }
                        }
                    }
                }
                div { class: "pagination-footer",
                    ControlButton {
                        disabled: page == 0,
                        onclick: move |event| onprev.call(event),
                        "Previous"
                    }
                    span { "{page + 1} / {page_count}" }
                    ControlButton {
                        primary: true,
                        disabled: page + 1 >= page_count,
                        onclick: move |event| onnext.call(event),
                        "Next"
                    }
                }
            }
        }
    }
}

#[component]
fn TransactionSortHeader(
    label: &'static str,
    field: TransactionSortField,
    selected_field: TransactionSortField,
    direction: SortDirection,
    onsortfieldchange: EventHandler<TransactionSortField>,
    onsortdirectionchange: EventHandler<SortDirection>,
) -> Element {
    let selected = field == selected_field;
    let class = if selected {
        "sort-header-button selected"
    } else {
        "sort-header-button"
    };
    let title = if selected {
        format!("Sort {label} {}", direction.toggle().label().to_lowercase())
    } else {
        format!("Sort by {label}")
    };
    let next_direction = if selected {
        direction.toggle()
    } else {
        default_transaction_sort_direction(field)
    };

    rsx! {
        button {
            class: "{class}",
            title: "{title}",
            onclick: move |_| {
                onsortfieldchange.call(field);
                onsortdirectionchange.call(next_direction);
            },
            span { "{label}" }
            span { class: "sort-arrow",
                if selected {
                    "{sort_direction_arrow(direction)}"
                }
            }
        }
    }
}
