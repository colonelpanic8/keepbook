use super::*;
use crate::api::{
    fetch_spending_dashboard, set_transaction_effective_date, set_transaction_ignore,
    set_transaction_tags, suggest_ai_rules,
};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SpendingTab {
    Tags,
    StringMatches,
}

impl SpendingTab {
    const OPTIONS: [Self; 2] = [Self::Tags, Self::StringMatches];

    fn label(self) -> &'static str {
        match self {
            Self::Tags => "Tags",
            Self::StringMatches => "String Matches",
        }
    }

    fn value(self) -> &'static str {
        match self {
            Self::Tags => "tags",
            Self::StringMatches => "string_matches",
        }
    }

    fn from_value(value: &str) -> Option<Self> {
        Self::OPTIONS.into_iter().find(|tab| tab.value() == value)
    }
}

/// Range presets offered by the spending view, in display order.
const SPENDING_RANGE_PRESETS: [(RangePreset, &str); 5] = [
    (RangePreset::OneMonth, "30D"),
    (RangePreset::NinetyDays, "90D"),
    (RangePreset::SixMonths, "6M"),
    (RangePreset::OneYear, "1Y"),
    (RangePreset::Max, "Max"),
];

#[component]
pub(super) fn SpendingView(currency: String) -> Element {
    let mut range_preset = use_signal(|| DEFAULT_SPENDING_RANGE_PRESET);
    let mut start_override = use_signal(String::new);
    let mut end_override = use_signal(String::new);
    let mut spending_bucket = use_signal(|| DEFAULT_SPENDING_BUCKET);
    let mut custom_range_open = use_signal(|| false);
    let mut active_tab = use_signal(|| SpendingTab::Tags);
    let mut selected_tag = use_signal(|| None::<String>);
    let mut selected_period = use_signal(|| None::<SpendingPeriodSelection>);
    let mut transaction_page = use_signal(|| 0usize);
    let mut transaction_sort_field = use_signal(|| TransactionSortField::Date);
    let mut transaction_sort_direction = use_signal(|| SortDirection::Desc);
    let mut show_ignored_transactions = use_signal(|| false);
    let mut transaction_title_filter = use_signal(String::new);
    let mut selected_transaction_keys = use_signal(HashSet::<String>::new);
    let mut ai_prompt = use_signal(String::new);
    let mut ai_result = use_signal(|| None::<AiRuleSuggestionsOutput>);
    let mut ai_status = use_signal(|| None::<String>);
    let mut tag_update_status = use_signal(|| None::<String>);
    let mut ai_busy = use_signal(|| false);
    let mut mutation_busy = use_signal(|| false);
    let spending = use_resource({
        let currency = currency.clone();
        move || {
            let selected_range = range_preset();
            let start_text = start_override();
            let end_text = end_override();
            let selected_bucket = spending_bucket();
            let currency = currency.clone();
            async move {
                let today = current_date_string();
                fetch_spending_dashboard(
                    spending_query_string(
                        selected_range,
                        &start_text,
                        &end_text,
                        &today,
                        &currency,
                    ),
                    spending_over_time_query_string(
                        selected_range,
                        &start_text,
                        &end_text,
                        &today,
                        &currency,
                        selected_bucket,
                    ),
                    spending_description_query_string(
                        selected_range,
                        &start_text,
                        &end_text,
                        &today,
                        &currency,
                        "merchant",
                        12,
                    ),
                    spending_description_query_string(
                        selected_range,
                        &start_text,
                        &end_text,
                        &today,
                        &currency,
                        "merchant_fuzzy",
                        12,
                    ),
                )
                .await
            }
        }
    });

    let selected_range = range_preset();
    let selected_bucket = spending_bucket();
    let selected_tab = active_tab();
    let start_text = start_override();
    let end_text = end_override();
    let selected = selected_tag();
    let selected_period_value = selected_period();
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
    let tags = loaded
        .map(|data| spending_tags(&data.spending))
        .unwrap_or_default();
    let tag_colors = spending_tag_color_map(&tags);
    let tag_options = loaded
        .map(|data| transaction_tag_options(&data.transactions, &tags))
        .unwrap_or_default();
    let total = loaded
        .and_then(|data| parse_money_input(&data.spending.total))
        .unwrap_or_default();
    let selected_total = selected.as_ref().and_then(|tag| {
        tags.iter()
            .find(|entry| &entry.key == tag)
            .and_then(|entry| parse_money_input(&entry.total))
    });
    let period_metric = selected_period_value.as_ref().map(|period| {
        let (total, transaction_count) = loaded
            .and_then(|data| {
                let points = spending_over_time_points(&data.spending_over_time);
                let points = match selected.as_deref() {
                    Some(tag) => narrow_spending_points_to_tag(&points, tag),
                    None => points,
                };
                points
                    .iter()
                    .find(|point| {
                        point.start_date == period.start_date && point.end_date == period.end_date
                    })
                    .map(|point| (point.total, point.transaction_count))
            })
            .unwrap_or((period.total, period.transaction_count));
        SpendingPeriodSelection {
            label: period.label.clone(),
            start_date: period.start_date.clone(),
            end_date: period.end_date.clone(),
            total,
            transaction_count,
        }
    });
    let filtered_transactions = loaded
        .map(|data| {
            filtered_transactions(
                &data.transactions,
                selected.as_deref(),
                selected_period_value
                    .as_ref()
                    .map(|period| (period.start_date.as_str(), period.end_date.as_str())),
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
    let selected_tag_targets = filtered_transactions
        .iter()
        .filter(|transaction| selected_keys.contains(&transaction_key(transaction)))
        .map(|transaction| TransactionTagTargetInput {
            account_id: transaction.account_id.clone(),
            transaction_id: transaction.id.clone(),
        })
        .collect::<Vec<_>>();
    let filtered_transaction_keys = filtered_transactions
        .iter()
        .map(transaction_key)
        .collect::<Vec<_>>();
    let selected_filtered_count = filtered_transaction_keys
        .iter()
        .filter(|key| selected_keys.contains(*key))
        .count();
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
    // A single contextual chip replaces the always-present "All" button: it only
    // exists while something is focused, and clearing it is the only thing that
    // button ever did.
    let focus_chip_label = match (selected.as_ref(), selected_period_value.as_ref()) {
        (Some(tag), Some(period)) => Some(format!("{tag} · {}", period.label)),
        (Some(tag), None) => Some(tag.clone()),
        (None, Some(period)) => Some(period.label.clone()),
        (None, None) => None,
    };
    // The date pickers stay collapsed behind a readout of the range in effect.
    // A custom range can only be entered through those pickers, so the toggle
    // alone drives visibility; a collapsed custom range stays marked as the
    // active source of the range instead of forcing the panel back open.
    let dates_open = custom_range_open();
    let custom_range_active = selected_range == RangePreset::Custom;
    let selected_label = selected.as_deref().unwrap_or("All tags");
    let panel_title = match selected_tab {
        SpendingTab::Tags => "Spending Tags",
        SpendingTab::StringMatches => "Spending Matches",
    };
    let panel_subtitle = match selected_tab {
        SpendingTab::Tags => selected_label,
        SpendingTab::StringMatches => "Exact and close strings",
    };
    let transaction_scope = selected_period_value
        .as_ref()
        .map(|period| format!("{} / {}", period.label, transaction_range))
        .unwrap_or_else(|| transaction_range.clone());

    rsx! {
        Panel {
            class: "spending-panel",
            title: "{panel_title}",
            subtitle: "{panel_subtitle}",
            actions: rsx! {
                span { "{currency}" }
            },
            if state.is_none() {
                BackendActivity { message: "Waiting on backend spending data" }
            }
            if let Some(message) = tag_update_status() {
                OperationStatus { message, busy: mutation_busy() }
            }
            div { class: "chart-controls spending-controls",
                SegmentedControl {
                    label: "Range",
                    options: range_preset_options(&SPENDING_RANGE_PRESETS),
                    selected: selected_range.value().to_string(),
                    onselect: move |value: String| {
                        let Some(preset) = range_preset_from_value(&SPENDING_RANGE_PRESETS, &value)
                        else {
                            return;
                        };
                        range_preset.set(preset);
                        start_override.set(String::new());
                        end_override.set(String::new());
                        custom_range_open.set(false);
                        selected_tag.set(None);
                        selected_period.set(None);
                        transaction_page.set(0);
                    }
                }
                div { class: "segmented-field range-disclosure-field",
                    span { class: "control-label segmented-label", "Dates" }
                    div { class: "range-disclosure-body",
                        button {
                            class: match (dates_open, custom_range_active) {
                                (true, true) => "range-disclosure open active",
                                (true, false) => "range-disclosure open",
                                (false, true) => "range-disclosure active",
                                (false, false) => "range-disclosure",
                            },
                            r#type: "button",
                            aria_expanded: dates_open,
                            title: "Set an exact start and end date",
                            onclick: move |_| custom_range_open.set(!dates_open),
                            span { class: "range-disclosure-value", "{resolved_start} → {resolved_end}" }
                            span { class: "range-disclosure-caret", aria_hidden: "true",
                                if dates_open { "▴" } else { "▾" }
                            }
                        }
                        if dates_open {
                            div { class: "control-grid spending-date-grid",
                                DateInput {
                                    label: "Start",
                                    value: resolved_start.clone(),
                                    min: String::new(),
                                    max: resolved_end.clone(),
                                    oninput: move |value| {
                                        start_override.set(value);
                                        range_preset.set(RangePreset::Custom);
                                        selected_tag.set(None);
                                        selected_period.set(None);
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
                                        selected_tag.set(None);
                                        selected_period.set(None);
                                        transaction_page.set(0);
                                    }
                                }
                            }
                        }
                    }
                }
                SegmentedControl {
                    label: "View",
                    options: SpendingTab::OPTIONS
                        .iter()
                        .map(|tab| SegmentedOption::new(tab.value(), tab.label()))
                        .collect::<Vec<_>>(),
                    selected: selected_tab.value().to_string(),
                    onselect: move |value: String| {
                        let Some(tab) = SpendingTab::from_value(&value) else {
                            return;
                        };
                        active_tab.set(tab);
                        if tab == SpendingTab::StringMatches {
                            selected_tag.set(None);
                            selected_period.set(None);
                            transaction_page.set(0);
                        }
                    }
                }
                if selected_tab == SpendingTab::Tags {
                    SegmentedControl {
                        label: "Bucket",
                        options: SpendingBucket::OPTIONS
                            .iter()
                            .map(|option| SegmentedOption::new(option.query_value(), option.label()))
                            .collect::<Vec<_>>(),
                        selected: selected_bucket.query_value().to_string(),
                        onselect: move |value: String| {
                            let Some(bucket) = SpendingBucket::from_value(&value) else {
                                return;
                            };
                            spending_bucket.set(bucket);
                            selected_period.set(None);
                            transaction_page.set(0);
                        }
                    }
                }
                if let Some(label) = focus_chip_label.clone() {
                    div { class: "filter-chip-row",
                        button {
                            class: "filter-clear-chip",
                            r#type: "button",
                            title: "Clear the focused tag and period",
                            onclick: move |_| {
                                selected_tag.set(None);
                                selected_period.set(None);
                                transaction_page.set(0);
                            },
                            span { class: "filter-clear-chip-text", "Focused: {label}" }
                            span { aria_hidden: "true", "✕" }
                        }
                    }
                }
            }
            match state {
                None => rsx! {
                    GraphLoadingPanel {
                        range: range_summary_text(&resolved_start, &resolved_end),
                        sampling: selected_tab.label()
                    }
                },
                Some(Err(error)) => rsx! {
                    InlineStatus { title: panel_title, message: error }
                },
                Some(Ok(data)) => rsx! {
                    if selected_tab == SpendingTab::Tags {
                        SpendingOverTimeChart {
                        spending: data.spending_over_time.clone(),
                        selected: selected.clone(),
                        selected_period: selected_period_value.clone(),
                        bucket_label: selected_bucket.label().to_string(),
                        colors: tag_colors.clone(),
                        onclick: move |period: SpendingPeriodSelection| {
                            let same_period = selected_period().is_some_and(|current| {
                                current.start_date == period.start_date
                                    && current.end_date == period.end_date
                            });
                            selected_period.set(if same_period { None } else { Some(period) });
                            transaction_page.set(0);
                        },
                        onfocussegment: move |sel: SpendingSegmentSelection| {
                            let already_focused = selected_tag() == Some(sel.key.clone())
                                && selected_period().is_some_and(|current| {
                                    current.start_date == sel.period.start_date
                                        && current.end_date == sel.period.end_date
                                });
                            if already_focused {
                                selected_tag.set(None);
                                selected_period.set(None);
                            } else {
                                selected_tag.set(Some(sel.key));
                                selected_period.set(Some(sel.period));
                            }
                            transaction_page.set(0);
                        },
                        onselecttag: move |tag: String| {
                            let next = if selected_tag() == Some(tag.clone()) {
                                None
                            } else {
                                Some(tag)
                            };
                            selected_tag.set(next);
                            transaction_page.set(0);
                        }
                    }
                    div { class: "spending-layout",
                        div { class: "spending-chart-area",
                            SpendingPieChart {
                                tags: tags.clone(),
                                selected: selected.clone(),
                                currency: data.spending.currency.clone(),
                                colors: tag_colors.clone(),
                                onclick: move |tag: String| {
                                    let next = if selected_tag() == Some(tag.clone()) {
                                        None
                                    } else {
                                        Some(tag)
                                    };
                                    selected_tag.set(next);
                                    transaction_page.set(0);
                                }
                            }
                        }
                        div { class: "tag-list",
                            div { class: "spending-total",
                                span { class: "metric-label", "Total" }
                                strong { "{format_full_money(total, &data.spending.currency)}" }
                                small { "{data.spending.transaction_count} transactions / {data.spending.start_date} to {data.spending.end_date}" }
                            }
                            if let Some(period) = period_metric.clone() {
                                div { class: "spending-total selected-total",
                                    span { class: "metric-label", "Period" }
                                    strong { "{period.label}" }
                                    small {
                                        "{format_full_money(period.total, &data.spending.currency)} / {period.transaction_count} transactions / {period.start_date} to {period.end_date}"
                                    }
                                }
                            }
                            if let Some(value) = selected_total {
                                div { class: "spending-total selected-total",
                                    span { class: "metric-label", "Selected" }
                                    strong { "{format_full_money(value, &data.spending.currency)}" }
                                    small { "{selected_label}" }
                                }
                            }
                            for (index, entry) in tags.iter().enumerate() {
                                TagRow {
                                    entry: entry.clone(),
                                    color: spending_tag_color_for(&tag_colors, &entry.key, index),
                                    currency: data.spending.currency.clone(),
                                    selected: selected.as_ref() == Some(&entry.key),
                                    onclick: move |tag: String| {
                                        let next = if selected_tag() == Some(tag.clone()) {
                                            None
                                        } else {
                                            Some(tag)
                                        };
                                        selected_tag.set(next);
                                        transaction_page.set(0);
                                    }
                                }
                            }
                        }
                    }
                    } else {
                        SpendingMatchLists {
                            exact_entries: spending_breakdown_entries(&data.exact_match_spending),
                            close_entries: spending_breakdown_entries(&data.close_match_spending),
                            currency: data.spending.currency.clone(),
                        }
                    }
                    TransactionList {
                        transactions: page_transactions.clone(),
                        currency: data.spending.currency.clone(),
                        range_text: transaction_scope.clone(),
                        sort_field: selected_sort_field,
                        sort_direction: selected_sort_direction,
                        show_ignored,
                        title_filter: title_filter.clone(),
                        selected_keys: selected_keys.clone(),
                        selected_count: selected_filtered_count,
                        page: current_page,
                        page_count,
                        tag_options: tag_options.clone(),
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
                        onselectfiltered: move |_| {
                            let mut next = selected_transaction_keys();
                            for key in &filtered_transaction_keys {
                                next.insert(key.clone());
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
                        ai_busy: ai_busy(),
                        mutation_busy: mutation_busy(),
                        ai_result: ai_result(),
                        tag_targets: selected_tag_targets.clone(),
                        onpromptchange: move |value| ai_prompt.set(value),
                        onairulesubmit: move |_| {
                            let prompt = ai_prompt().trim().to_string();
                            let transactions = selected_ai_transactions.clone();
                            let existing_tags = tag_options.clone();
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
                            ai_busy.set(true);
                            spawn({
                                let mut ai_status = ai_status;
                                let mut ai_result = ai_result;
                                let mut ai_busy = ai_busy;
                                async move {
                                    match suggest_ai_rules(AiRuleSuggestionInput {
                                        prompt,
                                        transactions,
                                        existing_tags,
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
                                    ai_busy.set(false);
                                }
                            });
                        },
                        ontagssave: move |input: SetTransactionTagsInput| {
                            if mutation_busy() {
                                return;
                            }
                            mutation_busy.set(true);
                            tag_update_status.set(Some("Saving tags...".to_string()));
                            spawn({
                                let mut spending = spending;
                                let mut tag_update_status = tag_update_status;
                                let mut mutation_busy = mutation_busy;
                                async move {
                                    match set_transaction_tags(input).await {
                                        Ok(()) => {
                                            tag_update_status.set(Some("Tags saved.".to_string()));
                                            spending.restart();
                                        }
                                        Err(error) => {
                                            tag_update_status.set(Some(error));
                                        }
                                    }
                                    mutation_busy.set(false);
                                }
                            });
                        },
                        ontagsbulksave: move |input: SetTransactionTagsInput| {
                            if mutation_busy() {
                                return;
                            }
                            mutation_busy.set(true);
                            let updated_count = input.transactions.len();
                            tag_update_status.set(Some(format!(
                                "Saving tags for {updated_count} transaction(s)..."
                            )));
                            spawn({
                                let mut spending = spending;
                                let mut tag_update_status = tag_update_status;
                                let mut selected_transaction_keys = selected_transaction_keys;
                                let mut mutation_busy = mutation_busy;
                                async move {
                                    match set_transaction_tags(input).await {
                                        Ok(()) => {
                                            tag_update_status.set(Some(format!(
                                                "Updated {updated_count} transaction(s)."
                                            )));
                                            selected_transaction_keys.set(HashSet::new());
                                            spending.restart();
                                        }
                                        Err(error) => {
                                            tag_update_status.set(Some(error));
                                        }
                                    }
                                    mutation_busy.set(false);
                                }
                            });
                        },
                        oneffectivedatesave: move |input: SetTransactionEffectiveDateInput| {
                            if mutation_busy() {
                                return;
                            }
                            mutation_busy.set(true);
                            tag_update_status.set(Some("Saving date...".to_string()));
                            spawn({
                                let mut spending = spending;
                                let mut tag_update_status = tag_update_status;
                                let mut mutation_busy = mutation_busy;
                                async move {
                                    match set_transaction_effective_date(input).await {
                                        Ok(()) => {
                                            tag_update_status.set(Some("Date saved.".to_string()));
                                            spending.restart();
                                        }
                                        Err(error) => {
                                            tag_update_status.set(Some(error));
                                        }
                                    }
                                    mutation_busy.set(false);
                                }
                            });
                        },
                        onignoresave: move |input: SetTransactionIgnoreInput| {
                            if mutation_busy() {
                                return;
                            }
                            mutation_busy.set(true);
                            let count = input.transactions.len();
                            let clear_selection = count > 1;
                            tag_update_status.set(Some("Updating spending exclusion...".to_string()));
                            spawn({
                                let mut spending = spending;
                                let mut tag_update_status = tag_update_status;
                                let mut selected_transaction_keys = selected_transaction_keys;
                                let mut mutation_busy = mutation_busy;
                                async move {
                                    match set_transaction_ignore(input).await {
                                        Ok(()) => {
                                            tag_update_status.set(Some(if count > 1 {
                                                format!("Updated {count} transaction(s).")
                                            } else {
                                                "Updated.".to_string()
                                            }));
                                            if clear_selection {
                                                selected_transaction_keys.set(HashSet::new());
                                            }
                                            spending.restart();
                                        }
                                        Err(error) => {
                                            tag_update_status.set(Some(error));
                                        }
                                    }
                                    mutation_busy.set(false);
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

#[derive(Clone, Debug)]
struct SpendingBarRect {
    key: String,
    label: String,
    start_date: String,
    end_date: String,
    total: f64,
    value: f64,
    transaction_count: usize,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    color: &'static str,
}

#[derive(Clone, Debug)]
struct SpendingBarHitZone {
    selection: SpendingPeriodSelection,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    tooltip_x: f64,
    tooltip_y: f64,
}

/// Identifies a single category subsection (segment) within one time period.
/// Used to hover or "pin" the tooltip to that category.
#[derive(Clone, Debug, PartialEq)]
struct SpendingSegmentKey {
    key: String,
    start_date: String,
    end_date: String,
}

/// Payload emitted when a segment is clicked to focus the view on a single
/// category within a single time period.
#[derive(Clone, Debug, PartialEq)]
struct SpendingSegmentSelection {
    key: String,
    period: SpendingPeriodSelection,
}

#[component]
fn SpendingOverTimeChart(
    spending: SpendingOutput,
    selected: Option<String>,
    selected_period: Option<SpendingPeriodSelection>,
    bucket_label: String,
    colors: HashMap<String, &'static str>,
    onclick: EventHandler<SpendingPeriodSelection>,
    onfocussegment: EventHandler<SpendingSegmentSelection>,
    onselecttag: EventHandler<String>,
) -> Element {
    let mut hovered_period = use_signal(|| None::<SpendingPeriodSelection>);
    let mut hovered_segment = use_signal(|| None::<SpendingSegmentKey>);
    let mut pinned_segment = use_signal(|| None::<SpendingSegmentKey>);
    let points = spending_over_time_points(&spending);
    let series = spending_over_time_series(&points);
    let narrowed_points = selected
        .as_ref()
        .map(|tag| narrow_spending_points_to_tag(&points, tag));
    let display_points = narrowed_points.as_ref().unwrap_or(&points);
    let visible_points = visible_spending_over_time_points(display_points, &series);

    if visible_points.is_empty() || series.is_empty() {
        return rsx! {
            div { class: "chart-empty spending-over-time-empty",
                strong { "No spending over time" }
                small { "Refresh transactions or adjust the range." }
            }
        };
    }

    let width = 720.0;
    let height = 300.0;
    let padding_left = 68.0;
    let padding_right = 20.0;
    let padding_top = 18.0;
    let padding_bottom = 44.0;
    let plot_width = width - padding_left - padding_right;
    let plot_height = height - padding_top - padding_bottom;
    let max_total = visible_points
        .iter()
        .map(|point| point.total)
        .fold(0.0_f64, f64::max)
        .max(1.0);
    let y_max = max_total * 1.08;
    let count = visible_points.len();
    let slot_width = plot_width / count as f64;
    let gap = (slot_width * 0.18).clamp(3.0, 14.0);
    let bar_width = (slot_width - gap).max(2.0);
    let series_keys = series
        .iter()
        .map(|entry| entry.key.clone())
        .collect::<Vec<_>>();
    let range_total = if selected.is_some() {
        visible_points.iter().map(|point| point.total).sum::<f64>()
    } else {
        parse_money_input(&spending.total).unwrap_or_default().abs()
    };
    let over_time_label = match &selected {
        Some(tag) => format!("Over Time · {tag}"),
        None => "Over Time".to_string(),
    };
    let first_label = visible_points
        .first()
        .map(|point| point.label.clone())
        .unwrap_or_default();
    let last_label = visible_points
        .last()
        .map(|point| point.label.clone())
        .unwrap_or_default();
    let mid_label = format_compact_money(y_max / 2.0, &spending.currency);
    let max_label = format_compact_money(y_max, &spending.currency);
    let total_label = format_full_money(range_total, &spending.currency);
    let mut bar_rects = Vec::new();
    let mut bar_hit_zones = Vec::new();
    for (point_index, point) in visible_points.iter().enumerate() {
        let x = padding_left + point_index as f64 * slot_width + gap / 2.0;
        bar_hit_zones.push(SpendingBarHitZone {
            selection: SpendingPeriodSelection {
                label: point.label.clone(),
                start_date: point.start_date.clone(),
                end_date: point.end_date.clone(),
                total: point.total,
                transaction_count: point.transaction_count,
            },
            x: padding_left + point_index as f64 * slot_width,
            y: padding_top,
            width: slot_width,
            height: plot_height,
            tooltip_x: x + bar_width / 2.0,
            tooltip_y: padding_top + 14.0,
        });
        let values = point
            .segments
            .iter()
            .map(|segment| (segment.key.as_str(), segment))
            .collect::<std::collections::HashMap<_, _>>();
        let mut cumulative = 0.0_f64;
        for (series_index, key) in series_keys.iter().enumerate() {
            if let Some(segment) = values.get(key.as_str()) {
                let y1_value = cumulative + segment.value;
                let y0 = padding_top + ((y_max - cumulative) / y_max) * plot_height;
                let y1 = padding_top + ((y_max - y1_value) / y_max) * plot_height;
                cumulative = y1_value;
                bar_rects.push(SpendingBarRect {
                    key: key.clone(),
                    label: point.label.clone(),
                    start_date: point.start_date.clone(),
                    end_date: point.end_date.clone(),
                    total: point.total,
                    value: segment.value,
                    transaction_count: segment.transaction_count,
                    x,
                    y: y1,
                    width: bar_width,
                    height: (y0 - y1).max(0.6),
                    color: spending_tag_color_for(&colors, key, series_index),
                });
            }
        }
    }
    // A clicked (pinned) segment takes precedence over a hovered segment. Both
    // show the category amount alongside the full bucket total. Empty column
    // space and externally-selected periods continue to show period totals.
    let active_segment = pinned_segment().or_else(&*hovered_segment);
    let tooltip = active_segment
        .as_ref()
        .and_then(|pin| {
            bar_rects
                .iter()
                .find(|rect| {
                    rect.key == pin.key
                        && rect.start_date == pin.start_date
                        && rect.end_date == pin.end_date
                })
                .map(|rect| {
                    (
                        format!("{} · {}", rect.label, rect.key),
                        spending_segment_tooltip_detail(
                            rect.value,
                            rect.total,
                            &spending.currency,
                            &bucket_label,
                        ),
                        rect.x + rect.width / 2.0,
                        padding_top + 14.0,
                    )
                })
        })
        .or_else(|| {
            let tooltip_period = hovered_period().or_else(|| selected_period.clone());
            tooltip_period.as_ref().and_then(|period| {
                bar_hit_zones
                    .iter()
                    .find(|zone| {
                        zone.selection.start_date == period.start_date
                            && zone.selection.end_date == period.end_date
                    })
                    .map(|zone| {
                        (
                            period.label.clone(),
                            format!(
                                "{} / {} tx",
                                format_full_money(period.total, &spending.currency),
                                period.transaction_count
                            ),
                            zone.tooltip_x,
                            zone.tooltip_y,
                        )
                    })
            })
        });

    rsx! {
        div { class: "chart-card spending-over-time-card",
            div { class: "chart-meta",
                div {
                    span { class: "metric-label", "{over_time_label}" }
                    strong { "{total_label}" }
                }
                div {
                    span { class: "metric-label", "Bucket" }
                    strong { "{bucket_label}" }
                }
            }
            svg {
                class: "net-worth-chart spending-bar-chart",
                view_box: "0 0 720 300",
                role: "img",
                line {
                    class: "chart-grid",
                    x1: "{padding_left}",
                    x2: "{width - padding_right}",
                    y1: "{padding_top}",
                    y2: "{padding_top}"
                }
                line {
                    class: "chart-grid",
                    x1: "{padding_left}",
                    x2: "{width - padding_right}",
                    y1: "{padding_top + plot_height / 2.0}",
                    y2: "{padding_top + plot_height / 2.0}"
                }
                line {
                    class: "chart-grid axis",
                    x1: "{padding_left}",
                    x2: "{width - padding_right}",
                    y1: "{padding_top + plot_height}",
                    y2: "{padding_top + plot_height}"
                }
                text {
                    class: "chart-axis-label",
                    x: "8",
                    y: "{padding_top + 4.0}",
                    "{max_label}"
                }
                text {
                    class: "chart-axis-label",
                    x: "8",
                    y: "{padding_top + plot_height / 2.0 + 4.0}",
                    "{mid_label}"
                }
                text {
                    class: "chart-axis-label",
                    x: "8",
                    y: "{padding_top + plot_height + 4.0}",
                    "$0"
                }
                text {
                    class: "chart-axis-label date-label",
                    x: "{padding_left}",
                    y: "{height - 10.0}",
                    "{first_label}"
                }
                text {
                    class: "chart-axis-label date-label end",
                    x: "{width - padding_right}",
                    y: "{height - 10.0}",
                    "{last_label}"
                }
                // Period-wide hit zones render first (behind the bars) so they only
                // catch hover/click in the empty column space above each bar; the
                // segment rects above handle per-category interaction.
                g { class: "spending-bar-hit-layer",
                    for hit in bar_hit_zones {
                        {
                            let selection_for_click = hit.selection.clone();
                            let selection_for_enter = hit.selection.clone();
                            let tooltip_text = format!(
                                "{}: {} / {} transactions / {} to {}",
                                hit.selection.label,
                                format_full_money(hit.selection.total, &spending.currency),
                                hit.selection.transaction_count,
                                hit.selection.start_date,
                                hit.selection.end_date
                            );
                            rsx! {
                                rect {
                                    class: "spending-bar-hit-zone",
                                    x: "{hit.x}",
                                    y: "{hit.y}",
                                    width: "{hit.width}",
                                    height: "{hit.height}",
                                    onmouseenter: move |_| hovered_period.set(Some(selection_for_enter.clone())),
                                    onmouseleave: move |_| hovered_period.set(None),
                                    onclick: move |_| onclick.call(selection_for_click.clone()),
                                    title { "{tooltip_text}" }
                                }
                            }
                        }
                    }
                }
                for rect in bar_rects {
                    {
                        let key = rect.key.clone();
                        let label = rect.label.clone();
                        let start_date = rect.start_date.clone();
                        let end_date = rect.end_date.clone();
                        let total = rect.total;
                        let value = rect.value;
                        let transaction_count = rect.transaction_count;
                        let x = rect.x;
                        let y = rect.y;
                        let width = rect.width;
                        let height = rect.height;
                        let color = rect.color;
                        let selection = SpendingPeriodSelection {
                            label: label.clone(),
                            start_date: start_date.clone(),
                            end_date: end_date.clone(),
                            total,
                            transaction_count,
                        };
                        let is_pinned = pinned_segment().is_some_and(|pin| {
                            pin.key == key
                                && pin.start_date == start_date
                                && pin.end_date == end_date
                        });
                        let selected_class = if selected_period
                            .as_ref()
                            .is_some_and(|period| period.start_date == start_date && period.end_date == end_date)
                        {
                            " selected"
                        } else {
                            ""
                        };
                        let pinned_class = if is_pinned { " pinned" } else { "" };
                        let class = format!("spending-bar-segment{selected_class}{pinned_class}");
                        // Clones for the individual event closures.
                        let selection_for_click = selection.clone();
                        let selection_for_enter = selection.clone();
                        let pin_key = key.clone();
                        let pin_start = start_date.clone();
                        let pin_end = end_date.clone();
                        let hover_key = key.clone();
                        let hover_start = start_date.clone();
                        let hover_end = end_date.clone();
                        rsx! {
                            rect {
                                class: "{class}",
                                x: "{x}",
                                y: "{y}",
                                width: "{width}",
                                height: "{height}",
                                style: "fill: {color};",
                                onmouseenter: move |_| {
                                    hovered_period.set(Some(selection_for_enter.clone()));
                                    hovered_segment.set(Some(SpendingSegmentKey {
                                        key: hover_key.clone(),
                                        start_date: hover_start.clone(),
                                        end_date: hover_end.clone(),
                                    }));
                                },
                                onmouseleave: move |_| {
                                    hovered_period.set(None);
                                    hovered_segment.set(None);
                                },
                                onclick: move |_| {
                                    // Toggle this category's pinned tooltip, and focus the
                                    // view on this category + period.
                                    let already_pinned = pinned_segment().is_some_and(|pin| {
                                        pin.key == pin_key
                                            && pin.start_date == pin_start
                                            && pin.end_date == pin_end
                                    });
                                    pinned_segment.set(if already_pinned {
                                        None
                                    } else {
                                        Some(SpendingSegmentKey {
                                            key: pin_key.clone(),
                                            start_date: pin_start.clone(),
                                            end_date: pin_end.clone(),
                                        })
                                    });
                                    onfocussegment.call(SpendingSegmentSelection {
                                        key: pin_key.clone(),
                                        period: selection_for_click.clone(),
                                    });
                                },
                                title {
                                    "{label}: {format_full_money(total, &spending.currency)} total / {key}: {format_full_money(value, &spending.currency)} ({transaction_count} tx)"
                                }
                            }
                        }
                    }
                }
                if let Some((tooltip_title, tooltip_detail, tooltip_x, tooltip_y)) = tooltip {
                    {
                        let (tooltip_width, tooltip_center_x) = spending_tooltip_layout(
                            &tooltip_title,
                            &tooltip_detail,
                            tooltip_x,
                            width,
                        );
                        let tooltip_left = -tooltip_width / 2.0;
                        rsx! {
                    g { class: "spending-chart-tooltip",
                        transform: "translate({tooltip_center_x}, {tooltip_y})",
                        rect {
                            x: "{tooltip_left}",
                            y: "-11",
                            width: "{tooltip_width}",
                            height: "42",
                            rx: "5"
                        }
                        text {
                            class: "spending-tooltip-title",
                            x: "0",
                            y: "3",
                            "{tooltip_title}"
                        }
                        text {
                            class: "spending-tooltip-detail",
                            x: "0",
                            y: "19",
                            "{tooltip_detail}"
                        }
                    }
                        }
                    }
                }
            }
            div { class: "stacked-legend spending-bar-legend",
                for (index, item) in series.iter().enumerate() {
                    {
                        let color = spending_tag_color_for(&colors, &item.key, index);
                        let key = item.key.clone();
                        let key_for_click = key.clone();
                        let class = if selected.as_ref() == Some(&key) {
                            "stacked-legend-item selected"
                        } else {
                            "stacked-legend-item"
                        };
                        rsx! {
                            button {
                                class: "{class}",
                                onclick: move |_| onselecttag.call(key_for_click.clone()),
                                span {
                                    class: "stacked-legend-swatch",
                                    style: "background: {color};"
                                }
                                span { "{key}" }
                            }
                        }
                    }
                }
            }
        }
    }
}

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
    let total = parse_money_input(&entry.total).unwrap_or_default();

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
            strong { "{format_full_money(total, &currency)}" }
            small { "{entry.transaction_count} tx" }
        }
    }
}

#[component]
fn SpendingMatchLists(
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
    let total = parse_money_input(&entry.total).unwrap_or_default();

    rsx! {
        div { class: "spending-match-row",
            span { class: "spending-match-name", "{entry.key}" }
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
                        selected: true,
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
                textarea {
                    class: "ai-rule-prompt",
                    value: "{ai_prompt}",
                    placeholder: "Ask for a tag, ignore, or rename rule for the selected transactions.",
                    disabled: ai_busy,
                    oninput: move |event| onpromptchange.call(event.value())
                }
                div { class: "ai-rule-actions",
                    ControlButton {
                        selected: true,
                        busy: ai_busy,
                        onclick: move |event| onairulesubmit.call(event),
                        disabled: selected_count == 0 || ai_prompt.trim().is_empty() || ai_busy,
                        if ai_busy { "Asking AI" } else { "Ask AI" }
                    }
                    if let Some(status) = ai_status.clone() {
                        OperationStatus { message: status, busy: ai_busy }
                    }
                }
                if let Some(result) = ai_result.clone() {
                    AiRuleSuggestions { result }
                }
            }
            if has_visible_selection && group_editor_open() {
                Modal {
                    dialog_class: "group-edit-dialog",
                    title: "Group edit",
                    header_actions: rsx! {
                        button {
                            class: "icon-button",
                            title: "Close group edit",
                            onclick: move |_| group_editor_open.set(false),
                            "x"
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
                div { class: "chart-empty transaction-empty",
                    strong { "No matching transactions" }
                    small { "Select another tag or range." }
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
                                                span { class: "tag-pill readonly", "{tag}" }
                                            }
                                        }
                                        if tx.ignored_from_spending {
                                            small { class: "ignored-badge", "Not counted" }
                                        }
                                    }
                                    span { class: "transaction-account-cell", "{tx.account_name}" }
                                    strong { class: "transaction-amount-cell", "{format_transaction_amount(&tx, &currency)}" }
                                    button {
                                        class: "transaction-expand-toggle",
                                        r#type: "button",
                                        title: if is_expanded { "Collapse editor" } else { "Expand to edit" },
                                        onclick: move |event| {
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
                                        if is_expanded { "\u{2304}" } else { "\u{203A}" }
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
                        selected: true,
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
fn GroupTagsEditor(
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
fn TransactionEditorPanel(
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

    let rule_ignored = rule_ignores_spending(&transaction);
    let not_spending_shaped = !is_spending_transaction(&transaction);
    let exclude_checked = rule_ignored || annotation_ignores_spending(&transaction);

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
