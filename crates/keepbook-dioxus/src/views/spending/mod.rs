mod editor;
mod matches;
mod over_time;
mod pie;
mod transactions;

use super::*;
use crate::api::{
    fetch_spending_dashboard, set_transaction_effective_date, set_transaction_ignore,
    set_transaction_tags, suggest_ai_rules,
};
use std::collections::HashSet;

use editor::*;
use matches::*;
use over_time::*;
use pie::*;
use transactions::*;

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
        .and_then(|data| format_money_text(&data.spending.total, &data.spending.currency))
        .unwrap_or_default();
    let selected_total = selected.as_ref().zip(loaded).and_then(|(tag, data)| {
        tags.iter()
            .find(|entry| &entry.key == tag)
            .and_then(|entry| format_money_text(&entry.total, &data.spending.currency))
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
                        series: tags.clone(),
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
                                strong { "{total}" }
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
                                    strong { "{value}" }
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
