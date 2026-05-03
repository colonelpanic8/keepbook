use super::*;
use crate::api::{fetch_spending_dashboard, set_transaction_category, suggest_ai_rules};
use std::collections::HashSet;

#[component]
pub(super) fn SpendingView(currency: String) -> Element {
    let mut range_preset = use_signal(|| RangePreset::NinetyDays);
    let mut start_override = use_signal(String::new);
    let mut end_override = use_signal(String::new);
    let mut selected_category = use_signal(|| None::<String>);
    let mut transaction_page = use_signal(|| 0usize);
    let mut transaction_sort_field = use_signal(|| TransactionSortField::Date);
    let mut transaction_sort_direction = use_signal(|| SortDirection::Desc);
    let mut show_ignored_transactions = use_signal(|| false);
    let mut transaction_title_filter = use_signal(String::new);
    let mut selected_transaction_keys = use_signal(HashSet::<String>::new);
    let mut ai_prompt = use_signal(String::new);
    let mut ai_result = use_signal(|| None::<AiRuleSuggestionsOutput>);
    let mut ai_status = use_signal(|| None::<String>);
    let mut category_update_status = use_signal(|| None::<String>);
    let spending = use_resource({
        let currency = currency.clone();
        move || {
            let selected_range = range_preset();
            let start_text = start_override();
            let end_text = end_override();
            let currency = currency.clone();
            async move {
                fetch_spending_dashboard(spending_query_string(
                    selected_range,
                    &start_text,
                    &end_text,
                    &current_date_string(),
                    &currency,
                ))
                .await
            }
        }
    });

    let selected_range = range_preset();
    let start_text = start_override();
    let end_text = end_override();
    let selected = selected_category();
    let selected_sort_field = transaction_sort_field();
    let selected_sort_direction = transaction_sort_direction();
    let show_ignored = show_ignored_transactions();
    let title_filter = transaction_title_filter();
    let selected_keys = selected_transaction_keys();
    let state = spending.cloned();
    let loaded = match &state {
        Some(Ok(data)) => Some(data),
        _ => None,
    };
    let resolved_start = loaded
        .map(|data| data.spending.start_date.clone())
        .unwrap_or_else(|| start_text.clone());
    let resolved_end = loaded
        .map(|data| data.spending.end_date.clone())
        .unwrap_or_else(|| end_text.clone());
    let categories = loaded
        .map(|data| spending_categories(&data.spending))
        .unwrap_or_default();
    let category_options = loaded
        .map(|data| transaction_category_options(&data.transactions, &categories))
        .unwrap_or_default();
    let total = loaded
        .and_then(|data| parse_money_input(&data.spending.total))
        .unwrap_or_default();
    let selected_total = selected.as_ref().and_then(|category| {
        categories
            .iter()
            .find(|entry| &entry.key == category)
            .and_then(|entry| parse_money_input(&entry.total))
    });
    let filtered_transactions = loaded
        .map(|data| {
            filtered_transactions(
                &data.transactions,
                selected.as_deref(),
                &title_filter,
                selected_sort_field,
                selected_sort_direction,
                show_ignored,
            )
        })
        .unwrap_or_default();
    let selected_ai_transactions = filtered_transactions
        .iter()
        .filter(|transaction| selected_keys.contains(&transaction_key(transaction)))
        .map(ai_rule_transaction_input)
        .collect::<Vec<_>>();
    let page_size = 100usize;
    let page_count = filtered_transactions.len().max(1).div_ceil(page_size);
    let current_page = transaction_page().min(page_count.saturating_sub(1));
    if current_page != transaction_page() {
        transaction_page.set(current_page);
    }
    let page_start = current_page * page_size;
    let page_transactions = filtered_transactions
        .iter()
        .skip(page_start)
        .take(page_size)
        .cloned()
        .collect::<Vec<_>>();
    let transaction_range = if filtered_transactions.is_empty() {
        "0 of 0".to_string()
    } else {
        let first = page_start + 1;
        let last = (page_start + page_transactions.len()).min(filtered_transactions.len());
        format!("{first}-{last} of {}", filtered_transactions.len())
    };
    let selected_label = selected.as_deref().unwrap_or("All categories");

    rsx! {
        section { class: "panel spending-panel",
            div { class: "panel-header",
                div { class: "panel-title",
                    h2 { "Spending Categories" }
                    span { "{selected_label}" }
                }
                span { "{currency}" }
            }
            if state.is_none() {
                BackendActivity { message: "Waiting on backend spending data" }
            }
            if let Some(message) = category_update_status() {
                div { class: "inline-notice", "{message}" }
            }
            div { class: "chart-controls",
                div { class: "preset-row",
                    span { class: "control-label", "Range" }
                    SpendingPresetButton {
                        label: "30D",
                        selected: selected_range == RangePreset::OneMonth,
                        onclick: move |_| {
                            range_preset.set(RangePreset::OneMonth);
                            start_override.set(String::new());
                            end_override.set(String::new());
                            selected_category.set(None);
                            transaction_page.set(0);
                        }
                    }
                    SpendingPresetButton {
                        label: "90D",
                        selected: selected_range == RangePreset::NinetyDays,
                        onclick: move |_| {
                            range_preset.set(RangePreset::NinetyDays);
                            start_override.set(String::new());
                            end_override.set(String::new());
                            selected_category.set(None);
                            transaction_page.set(0);
                        }
                    }
                    SpendingPresetButton {
                        label: "6M",
                        selected: selected_range == RangePreset::SixMonths,
                        onclick: move |_| {
                            range_preset.set(RangePreset::SixMonths);
                            start_override.set(String::new());
                            end_override.set(String::new());
                            selected_category.set(None);
                            transaction_page.set(0);
                        }
                    }
                    SpendingPresetButton {
                        label: "1Y",
                        selected: selected_range == RangePreset::OneYear,
                        onclick: move |_| {
                            range_preset.set(RangePreset::OneYear);
                            start_override.set(String::new());
                            end_override.set(String::new());
                            selected_category.set(None);
                            transaction_page.set(0);
                        }
                    }
                    SpendingPresetButton {
                        label: "Max",
                        selected: selected_range == RangePreset::Max,
                        onclick: move |_| {
                            range_preset.set(RangePreset::Max);
                            start_override.set(String::new());
                            end_override.set(String::new());
                            selected_category.set(None);
                            transaction_page.set(0);
                        }
                    }
                    button {
                        class: "control-button",
                        disabled: selected.is_none(),
                        onclick: move |_| {
                            selected_category.set(None);
                            transaction_page.set(0);
                        },
                        "All"
                    }
                }
                div { class: "control-grid spending-date-grid",
                    DateInput {
                        label: "Start",
                        value: resolved_start.clone(),
                        min: String::new(),
                        max: resolved_end.clone(),
                        oninput: move |value| {
                            start_override.set(value);
                            range_preset.set(RangePreset::Custom);
                            selected_category.set(None);
                            transaction_page.set(0);
                        }
                    }
                    DateInput {
                        label: "End",
                        value: resolved_end.clone(),
                        min: resolved_start.clone(),
                        max: current_date_string(),
                        oninput: move |value| {
                            end_override.set(value);
                            range_preset.set(RangePreset::Custom);
                            selected_category.set(None);
                            transaction_page.set(0);
                        }
                    }
                }
            }
            match state {
                None => rsx! {
                    GraphLoadingPanel {
                        range: range_summary_text(&resolved_start, &resolved_end),
                        sampling: "Categories"
                    }
                },
                Some(Err(error)) => rsx! {
                    InlineStatus { title: "Spending Categories", message: error }
                },
                Some(Ok(data)) => rsx! {
                    div { class: "spending-layout",
                        div { class: "spending-chart-area",
                            SpendingPieChart {
                                categories: categories.clone(),
                                selected: selected.clone(),
                                currency: data.spending.currency.clone(),
                                onclick: move |category| {
                                    selected_category.set(Some(category));
                                    transaction_page.set(0);
                                }
                            }
                        }
                        div { class: "category-list",
                            div { class: "spending-total",
                                span { class: "metric-label", "Total" }
                                strong { "{format_full_money(total, &data.spending.currency)}" }
                                small { "{data.spending.transaction_count} transactions / {data.spending.start_date} to {data.spending.end_date}" }
                            }
                            if let Some(value) = selected_total {
                                div { class: "spending-total selected-total",
                                    span { class: "metric-label", "Selected" }
                                    strong { "{format_full_money(value, &data.spending.currency)}" }
                                    small { "{selected_label}" }
                                }
                            }
                            for entry in categories.iter() {
                                CategoryRow {
                                    entry: entry.clone(),
                                    currency: data.spending.currency.clone(),
                                    selected: selected.as_ref() == Some(&entry.key),
                                    onclick: move |category| {
                                        selected_category.set(Some(category));
                                        transaction_page.set(0);
                                    }
                                }
                            }
                        }
                    }
                    TransactionList {
                        transactions: page_transactions.clone(),
                        currency: data.spending.currency.clone(),
                        range_text: transaction_range.clone(),
                        sort_field: selected_sort_field,
                        sort_direction: selected_sort_direction,
                        show_ignored,
                        title_filter: title_filter.clone(),
                        selected_keys: selected_keys.clone(),
                        page: current_page,
                        page_count,
                        category_options: category_options.clone(),
                        onshowignoredchange: move |checked| {
                            show_ignored_transactions.set(checked);
                            transaction_page.set(0);
                        },
                        onsortfieldchange: move |field| {
                            transaction_sort_field.set(field);
                            transaction_page.set(0);
                        },
                        onsortdirectionchange: move |direction| {
                            transaction_sort_direction.set(direction);
                            transaction_page.set(0);
                        },
                        ontitlefilterchange: move |value| {
                            transaction_title_filter.set(value);
                            transaction_page.set(0);
                        },
                        ontoggleselection: move |key: String| {
                            let mut next = selected_transaction_keys();
                            if !next.insert(key.clone()) {
                                next.remove(&key);
                            }
                            selected_transaction_keys.set(next);
                        },
                        onselectpage: move |_| {
                            let mut next = selected_transaction_keys();
                            for transaction in &page_transactions {
                                next.insert(transaction_key(transaction));
                            }
                            selected_transaction_keys.set(next);
                        },
                        onclearselection: move |_| selected_transaction_keys.set(HashSet::new()),
                        onprev: move |_| transaction_page.set(current_page.saturating_sub(1)),
                        onnext: move |_| {
                            if current_page + 1 < page_count {
                                transaction_page.set(current_page + 1);
                            }
                        },
                        ai_prompt: ai_prompt(),
                        ai_status: ai_status(),
                        ai_result: ai_result(),
                        onpromptchange: move |value| ai_prompt.set(value),
                        onairulesubmit: move |_| {
                            let prompt = ai_prompt().trim().to_string();
                            let transactions = selected_ai_transactions.clone();
                            let existing_categories = category_options.clone();
                            ai_result.set(None);
                            if prompt.is_empty() {
                                ai_status.set(Some("Enter a prompt for the rule assistant.".to_string()));
                                return;
                            }
                            if transactions.is_empty() {
                                ai_status.set(Some("Select at least one matching transaction.".to_string()));
                                return;
                            }
                            ai_status.set(Some("Requesting AI rule suggestions...".to_string()));
                            spawn({
                                let mut ai_status = ai_status;
                                let mut ai_result = ai_result;
                                async move {
                                    match suggest_ai_rules(AiRuleSuggestionInput {
                                        prompt,
                                        transactions,
                                        existing_categories,
                                    }).await {
                                        Ok(output) => {
                                            ai_status.set(Some(format!(
                                                "Received {} suggestion(s) from {}.",
                                                output.suggestions.len(),
                                                output.model
                                            )));
                                            ai_result.set(Some(output));
                                        }
                                        Err(error) => ai_status.set(Some(error)),
                                    }
                                }
                            });
                        },
                        oncategorysave: move |input: SetTransactionCategoryInput| {
                            category_update_status.set(Some("Saving category...".to_string()));
                            spawn({
                                let mut spending = spending;
                                let mut category_update_status = category_update_status;
                                async move {
                                    match set_transaction_category(input).await {
                                        Ok(()) => {
                                            category_update_status.set(Some("Category saved.".to_string()));
                                            spending.restart();
                                        }
                                        Err(error) => {
                                            category_update_status.set(Some(error));
                                        }
                                    }
                                }
                            });
                        }
                    }
                    if data.spending.skipped_transaction_count > 0 {
                        p { class: "range-summary",
                            "Skipped {data.spending.skipped_transaction_count} transactions because market data was unavailable."
                        }
                    }
                },
            }
        }
    }
}

#[component]
fn SpendingPresetButton(
    label: &'static str,
    selected: bool,
    onclick: EventHandler<MouseEvent>,
) -> Element {
    rsx! {
        GraphPresetButton {
            label: label,
            selected: selected,
            onclick: move |event| onclick.call(event),
        }
    }
}

#[component]
fn SpendingPieChart(
    categories: Vec<SpendingBreakdownEntry>,
    selected: Option<String>,
    currency: String,
    onclick: EventHandler<String>,
) -> Element {
    let slices = pie_slices(&categories);
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
                    fill: "{slice.color}",
                    onclick: move |_| onclick.call(slice.key.clone()),
                    title { "{slice.key}: {format_full_money(slice.total, &currency)}" }
                }
            }
            circle { class: "pie-hole", cx: "130", cy: "130", r: "56" }
            text { class: "pie-center-label", x: "130", y: "124", "Spend" }
            text { class: "pie-center-value", x: "130", y: "145", "{categories.len()}" }
        }
    }
}

#[component]
fn CategoryRow(
    entry: SpendingBreakdownEntry,
    currency: String,
    selected: bool,
    onclick: EventHandler<String>,
) -> Element {
    let class = if selected {
        "category-row selected"
    } else {
        "category-row"
    };
    let total = parse_money_input(&entry.total).unwrap_or_default();

    rsx! {
        button {
            class: "{class}",
            onclick: move |_| onclick.call(entry.key.clone()),
            span { class: "category-name", "{entry.key}" }
            strong { "{format_full_money(total, &currency)}" }
            small { "{entry.transaction_count} tx" }
        }
    }
}

#[component]
fn TransactionList(
    transactions: Vec<Transaction>,
    currency: String,
    range_text: String,
    sort_field: TransactionSortField,
    sort_direction: SortDirection,
    show_ignored: bool,
    title_filter: String,
    selected_keys: HashSet<String>,
    page: usize,
    page_count: usize,
    category_options: Vec<String>,
    onshowignoredchange: EventHandler<bool>,
    onsortfieldchange: EventHandler<TransactionSortField>,
    onsortdirectionchange: EventHandler<SortDirection>,
    ontitlefilterchange: EventHandler<String>,
    ontoggleselection: EventHandler<String>,
    onselectpage: EventHandler<MouseEvent>,
    onclearselection: EventHandler<MouseEvent>,
    onprev: EventHandler<MouseEvent>,
    onnext: EventHandler<MouseEvent>,
    ai_prompt: String,
    ai_status: Option<String>,
    ai_result: Option<AiRuleSuggestionsOutput>,
    onpromptchange: EventHandler<String>,
    onairulesubmit: EventHandler<MouseEvent>,
    oncategorysave: EventHandler<SetTransactionCategoryInput>,
) -> Element {
    let selected_count = selected_keys.len();
    let has_transactions = !transactions.is_empty();
    rsx! {
        div { class: "transaction-panel",
            div { class: "panel-header transaction-header",
                div { class: "panel-title",
                    h2 { "Transactions" }
                    span { "{range_text}" }
                }
                div { class: "pagination-controls",
                    button {
                        class: "icon-button",
                        title: "Previous page",
                        disabled: page == 0,
                        onclick: move |event| onprev.call(event),
                        "‹"
                    }
                    span { "{page + 1} / {page_count}" }
                    button {
                        class: "icon-button",
                        title: "Next page",
                        disabled: page + 1 >= page_count,
                        onclick: move |event| onnext.call(event),
                        "›"
                    }
                }
            }
            div { class: "transaction-controls",
                input {
                    class: "transaction-search-input",
                    r#type: "search",
                    value: "{title_filter}",
                    placeholder: "Filter titles",
                    oninput: move |event| ontitlefilterchange.call(event.value())
                }
                label { class: "compact-check",
                    input {
                        r#type: "checkbox",
                        checked: show_ignored,
                        onchange: move |event| onshowignoredchange.call(event.checked())
                    }
                    span { "Show ignored" }
                }
                button {
                    class: "control-button",
                    onclick: move |event| onselectpage.call(event),
                    disabled: !has_transactions,
                    "Select Page"
                }
                button {
                    class: "control-button",
                    onclick: move |event| onclearselection.call(event),
                    disabled: selected_count == 0,
                    "Clear"
                }
            }
            div { class: "ai-rule-panel",
                div { class: "ai-rule-copy",
                    strong { "AI rule assistant" }
                    small { "{selected_count} selected" }
                }
                textarea {
                    class: "ai-rule-prompt",
                    value: "{ai_prompt}",
                    placeholder: "Ask for a categorization, ignore, or rename rule for the selected transactions.",
                    oninput: move |event| onpromptchange.call(event.value())
                }
                div { class: "ai-rule-actions",
                    button {
                        class: "control-button selected",
                        onclick: move |event| onairulesubmit.call(event),
                        disabled: selected_count == 0 || ai_prompt.trim().is_empty(),
                        "Ask AI"
                    }
                    if let Some(status) = ai_status.clone() {
                        span { class: "ai-rule-status", "{status}" }
                    }
                }
                if let Some(result) = ai_result.clone() {
                    AiRuleSuggestions { result }
                }
            }
            if !has_transactions {
                div { class: "chart-empty transaction-empty",
                    strong { "No matching transactions" }
                    small { "Select another category or range." }
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
                            label: "Category / Subcategory",
                            field: TransactionSortField::Category,
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
                    }
                    for tx in transactions.clone() {
                        div {
                            key: "{transaction_key(&tx)}",
                            class: "{transaction_row_class(&tx)}",
                            title: if tx.ignored_from_spending { "Not counted in spending totals" } else { "" },
                            label { class: "transaction-select-cell",
                                input {
                                    r#type: "checkbox",
                                    checked: selected_keys.contains(&transaction_key(&tx)),
                                    onchange: move |_| ontoggleselection.call(transaction_key(&tx))
                                }
                            }
                            span { "{transaction_date(&tx)}" }
                            strong { "{transaction_description(&tx)}" }
                            span { class: "transaction-category-cell",
                                div { class: "transaction-category-stack",
                                    TransactionCategoryEditor {
                                        transaction: tx.clone(),
                                        category_options: category_options.clone(),
                                        oncategorysave,
                                    }
                                    if let Some(subcategory) = transaction_subcategory(&tx) {
                                        small { class: "transaction-subcategory", "{subcategory}" }
                                    }
                                }
                                if tx.ignored_from_spending {
                                    small { class: "ignored-badge", "Not counted" }
                                }
                            }
                            span { "{tx.account_name}" }
                            strong { "{format_transaction_amount(&tx, &currency)}" }
                        }
                    }
                }
                div { class: "pagination-footer",
                    button {
                        class: "control-button",
                        disabled: page == 0,
                        onclick: move |event| onprev.call(event),
                        "Previous"
                    }
                    span { "{page + 1} / {page_count}" }
                    button {
                        class: "control-button selected",
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
fn AiRuleSuggestions(result: AiRuleSuggestionsOutput) -> Element {
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

#[component]
fn TransactionCategoryEditor(
    transaction: Transaction,
    category_options: Vec<String>,
    oncategorysave: EventHandler<SetTransactionCategoryInput>,
) -> Element {
    let current_category = transaction_category(&transaction);
    let mut draft_category = use_signal(|| {
        if current_category == "Uncategorized" {
            String::new()
        } else {
            current_category.clone()
        }
    });
    let draft = draft_category();
    let trimmed = draft.trim().to_string();
    let normalized_draft = normalize_spending_category_key(&trimmed);
    let changed = if trimmed.is_empty() {
        current_category != "Uncategorized"
    } else {
        normalized_draft != current_category
    };
    let list_id = format!(
        "category-options-{}-{}",
        transaction.account_id, transaction.id
    );

    rsx! {
        div { class: "category-editor",
            input {
                class: "category-editor-input",
                r#type: "text",
                list: "{list_id}",
                value: "{draft}",
                placeholder: "Uncategorized",
                oninput: move |event| draft_category.set(event.value())
            }
            datalist { id: "{list_id}",
                for category in category_options {
                    option { value: "{category}" }
                }
            }
            button {
                class: "category-editor-button",
                title: "Save category",
                disabled: !changed,
                onclick: move |_| {
                    let category = draft_category().trim().to_string();
                    oncategorysave.call(SetTransactionCategoryInput {
                        account_id: transaction.account_id.clone(),
                        transaction_id: transaction.id.clone(),
                        clear_category: category.is_empty(),
                        category: if category.is_empty() { None } else { Some(category) },
                    });
                },
                "Save"
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
