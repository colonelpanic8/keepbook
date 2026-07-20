use super::*;
use std::collections::HashSet;

#[component]
pub(super) fn AssetsView(filter_overrides: FilterOverrides) -> Element {
    let refresh_epoch = use_context::<Signal<u64>>();
    let mut include_amount_changes = use_signal(|| false);
    let mut show_absolute_changes = use_signal(|| false);
    let mut breakdown = use_resource(move || {
        let _refresh_epoch = refresh_epoch();
        let current_filter_overrides = filter_overrides.clone();
        let current_include_amount_changes = include_amount_changes();
        async move {
            fetch_assets(assets_query_string(
                current_filter_overrides,
                current_include_amount_changes,
            ))
            .await
        }
    });
    let mut sort_field = use_signal(|| AssetSortField::Value);
    let mut sort_direction = use_signal(|| SortDirection::Desc);
    let mut expanded_assets = use_signal(HashSet::<String>::new);

    let selected_sort_field = sort_field();
    let selected_sort_direction = sort_direction();
    let breakdown_state = breakdown.cloned();

    rsx! {
        match breakdown_state {
            None => rsx! {
                Panel {
                    title: "Assets",
                    BackendActivity { message: "Waiting on backend asset data" }
                }
            },
            Some(Err(error)) => rsx! {
                InlineStatus { title: "Assets", message: error }
            },
            Some(Ok(data)) => {
                let currency = data.currency.clone();
                let mut entries = data.assets.clone();
                entries.sort_by(|a, b| {
                    compare_asset_entries_with_change_metric(
                        a,
                        b,
                        selected_sort_field,
                        selected_sort_direction,
                        show_absolute_changes(),
                    )
                });
                let liability_count = entries.iter().filter(|entry| entry.liability).count();
                let asset_count = entries.len() - liability_count;
                let total_value = parse_money_input(&data.total_value).unwrap_or_default();
                let expanded = expanded_assets();

                rsx! {
                    section { class: "summary-grid",
                        MetricCard {
                            label: "Total value",
                            value: format_full_money(total_value, &currency),
                            detail: format!("As of {}", data.as_of_date)
                        }
                        MetricCard {
                            label: "Assets",
                            value: asset_count.to_string(),
                            detail: "Distinct holdings".to_string()
                        }
                        MetricCard {
                            label: "Liabilities",
                            value: liability_count.to_string(),
                            detail: "Negative holdings".to_string()
                        }
                    }
                    Panel {
                        title: "Assets",
                        subtitle: if data.change_mode == "price_and_amount" {
                            format!("As of {} · Changes include price and amount", data.as_of_date)
                        } else {
                            format!("As of {} · Price changes only", data.as_of_date)
                        },
                        actions: rsx! {
                            div { class: "settings-actions inline-actions",
                                label { class: "compact-check",
                                    input {
                                        r#type: "checkbox",
                                        checked: include_amount_changes(),
                                        onchange: move |event| {
                                            include_amount_changes.set(event.checked());
                                            breakdown.restart();
                                        }
                                    }
                                    span { "Include amount changes" }
                                }
                                label { class: "compact-check",
                                    input {
                                        r#type: "checkbox",
                                        checked: show_absolute_changes(),
                                        onchange: move |event| show_absolute_changes.set(event.checked())
                                    }
                                    span { "Absolute changes" }
                                }
                            }
                        },
                        if entries.is_empty() {
                            div { class: "chart-empty",
                                strong { "No assets" }
                                small { "Refresh balances to populate the asset breakdown." }
                            }
                        } else {
                            div { class: "data-table assets-table",
                                div { class: "table-head",
                                    AssetSortHeader {
                                        label: "Asset".to_string(),
                                        field: AssetSortField::Name,
                                        selected_field: selected_sort_field,
                                        direction: selected_sort_direction,
                                        onsortfieldchange: move |field| sort_field.set(field),
                                        onsortdirectionchange: move |direction| sort_direction.set(direction),
                                    }
                                    AssetSortHeader {
                                        label: "Amount".to_string(),
                                        field: AssetSortField::Amount,
                                        selected_field: selected_sort_field,
                                        direction: selected_sort_direction,
                                        onsortfieldchange: move |field| sort_field.set(field),
                                        onsortdirectionchange: move |direction| sort_direction.set(direction),
                                    }
                                    AssetSortHeader {
                                        label: "Amount checked".to_string(),
                                        field: AssetSortField::AmountChecked,
                                        selected_field: selected_sort_field,
                                        direction: selected_sort_direction,
                                        onsortfieldchange: move |field| sort_field.set(field),
                                        onsortdirectionchange: move |direction| sort_direction.set(direction),
                                    }
                                    AssetSortHeader {
                                        label: "Amount changed".to_string(),
                                        field: AssetSortField::AmountChanged,
                                        selected_field: selected_sort_field,
                                        direction: selected_sort_direction,
                                        onsortfieldchange: move |field| sort_field.set(field),
                                        onsortdirectionchange: move |direction| sort_direction.set(direction),
                                    }
                                    span { "Price" }
                                    AssetSortHeader {
                                        label: "Price updated".to_string(),
                                        field: AssetSortField::PriceUpdated,
                                        selected_field: selected_sort_field,
                                        direction: selected_sort_direction,
                                        onsortfieldchange: move |field| sort_field.set(field),
                                        onsortdirectionchange: move |direction| sort_direction.set(direction),
                                    }
                                    AssetSortHeader {
                                        label: format!("Value ({currency})"),
                                        field: AssetSortField::Value,
                                        selected_field: selected_sort_field,
                                        direction: selected_sort_direction,
                                        onsortfieldchange: move |field| sort_field.set(field),
                                        onsortdirectionchange: move |direction| sort_direction.set(direction),
                                    }
                                    AssetSortHeader {
                                        label: "1D".to_string(),
                                        field: AssetSortField::DayChange,
                                        selected_field: selected_sort_field,
                                        direction: selected_sort_direction,
                                        onsortfieldchange: move |field| sort_field.set(field),
                                        onsortdirectionchange: move |direction| sort_direction.set(direction),
                                    }
                                    AssetSortHeader {
                                        label: "1W".to_string(),
                                        field: AssetSortField::WeekChange,
                                        selected_field: selected_sort_field,
                                        direction: selected_sort_direction,
                                        onsortfieldchange: move |field| sort_field.set(field),
                                        onsortdirectionchange: move |direction| sort_direction.set(direction),
                                    }
                                    AssetSortHeader {
                                        label: "1M".to_string(),
                                        field: AssetSortField::MonthChange,
                                        selected_field: selected_sort_field,
                                        direction: selected_sort_direction,
                                        onsortfieldchange: move |field| sort_field.set(field),
                                        onsortdirectionchange: move |direction| sort_direction.set(direction),
                                    }
                                    AssetSortHeader {
                                        label: "1Y".to_string(),
                                        field: AssetSortField::YearChange,
                                        selected_field: selected_sort_field,
                                        direction: selected_sort_direction,
                                        onsortfieldchange: move |field| sort_field.set(field),
                                        onsortdirectionchange: move |direction| sort_direction.set(direction),
                                    }
                                    span { "" }
                                }
                                for entry in entries {
                                    AssetRow {
                                        key: "{asset_expansion_key(&entry)}",
                                        entry: entry.clone(),
                                        currency: currency.clone(),
                                        expanded: expanded.contains(&asset_expansion_key(&entry)),
                                        show_absolute_changes: show_absolute_changes(),
                                        ontoggle: move |key: String| {
                                            let mut next = expanded_assets();
                                            if !next.insert(key.clone()) {
                                                next.remove(&key);
                                            }
                                            expanded_assets.set(next);
                                        },
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn AssetRow(
    entry: AssetBreakdownEntry,
    currency: String,
    expanded: bool,
    show_absolute_changes: bool,
    ontoggle: EventHandler<String>,
) -> Element {
    let name = asset_display_name(&entry.asset, &entry.asset_id);
    let kind = asset_kind_label(&entry.asset);
    let detail_text = match asset_secondary_text(&entry.asset) {
        Some(secondary) => format!("{kind} · {secondary}"),
        None => kind.to_string(),
    };
    let amount = format_asset_amount(&entry.total_amount);
    let amount_checked = format_asset_timestamp(entry.amount_last_checked_at.as_deref());
    let amount_changed = format_asset_timestamp(entry.amount_last_changed_at.as_deref());
    let price = entry
        .price
        .as_deref()
        .and_then(parse_money_input)
        .map(|value| format_full_money(value, &currency))
        .unwrap_or_else(|| "—".to_string());
    let price_title = entry
        .price_date
        .as_deref()
        .map(|date| format!("Price as of {date}"))
        .unwrap_or_default();
    let price_updated = format_asset_timestamp(entry.price_updated_at.as_deref());
    let value = entry
        .value_in_base
        .as_deref()
        .and_then(parse_money_input)
        .map(|value| format_full_money(value, &currency))
        .unwrap_or_else(|| "—".to_string());
    let row_class = if expanded {
        "table-row asset-row expanded"
    } else {
        "table-row asset-row"
    };
    let row_toggle_key = asset_expansion_key(&entry);
    let chevron_toggle_key = row_toggle_key.clone();
    let holdings = entry.holdings.clone();

    rsx! {
        div {
            class: "{row_class}",
            title: if expanded { "Hide accounts" } else { "Show accounts" },
            onclick: move |_| ontoggle.call(row_toggle_key.clone()),
            div { class: "asset-name-cell",
                div { class: "asset-name-line",
                    strong { "{name}" }
                    if entry.liability {
                        span { class: "status liability-status", "Liability" }
                    }
                }
                small { "{detail_text}" }
            }
            span { "{amount}" }
            span { title: "{entry.amount_last_checked_at.clone().unwrap_or_default()}", "{amount_checked}" }
            span { title: "{entry.amount_last_changed_at.clone().unwrap_or_default()}", "{amount_changed}" }
            span { title: "{price_title}", "{price}" }
            span { title: "{entry.price_updated_at.clone().unwrap_or_default()}", "{price_updated}" }
            strong { "{value}" }
            {asset_change_cell(&entry, AssetSortField::DayChange, &currency, show_absolute_changes)}
            {asset_change_cell(&entry, AssetSortField::WeekChange, &currency, show_absolute_changes)}
            {asset_change_cell(&entry, AssetSortField::MonthChange, &currency, show_absolute_changes)}
            {asset_change_cell(&entry, AssetSortField::YearChange, &currency, show_absolute_changes)}
            button {
                class: "transaction-expand-toggle",
                r#type: "button",
                title: if expanded { "Hide accounts" } else { "Show accounts" },
                onclick: move |event| {
                    event.stop_propagation();
                    ontoggle.call(chevron_toggle_key.clone());
                },
                if expanded { "\u{2304}" } else { "\u{203A}" }
            }
        }
        if expanded {
            for holding in holdings {
                {
                    let holding_amount = format_asset_amount(&holding.amount);
                    let holding_value = holding
                        .value_in_base
                        .as_deref()
                        .and_then(parse_money_input)
                        .map(|value| format_full_money(value, &currency))
                        .unwrap_or_else(|| "—".to_string());
                    let connection = holding.connection_name.clone().unwrap_or_default();
                    rsx! {
                        div { class: "table-row asset-holding-row",
                            div { class: "asset-name-cell",
                                span { "{holding.account_name}" }
                                if !connection.is_empty() {
                                    small { "{connection}" }
                                }
                            }
                            span { "{holding_amount}" }
                            span { "{holding_value}" }
                            small { "Balance {holding.balance_date}" }
                        }
                    }
                }
            }
        }
    }
}

/// One trailing-period change cell. Percentage changes are colored by sign
/// with the absolute change as a tooltip; new positions without a comparable
/// past value show the signed absolute change instead; missing periods show
/// an em dash.
fn asset_change_cell(
    entry: &AssetBreakdownEntry,
    field: AssetSortField,
    currency: &str,
    show_absolute: bool,
) -> Element {
    let Some(change) = asset_period_change(entry, field) else {
        return rsx! {
            span { class: "asset-change-cell", "—" }
        };
    };
    let absolute = parse_money_input(&change.absolute).unwrap_or_default();
    let absolute_text = format_signed_money(absolute, currency);
    if show_absolute {
        let percentage_title = change
            .percentage
            .as_deref()
            .and_then(parse_money_input)
            .map(format_signed_percent)
            .unwrap_or_else(|| "No comparable percentage".to_string());
        return rsx! {
            span {
                class: "asset-change-cell {change_value_class(absolute)}",
                title: "{percentage_title}",
                "{absolute_text}"
            }
        };
    }
    match change.percentage.as_deref().and_then(parse_money_input) {
        Some(percent) => rsx! {
            span {
                class: "asset-change-cell {change_value_class(percent)}",
                title: "{absolute_text}",
                "{format_signed_percent(percent)}"
            }
        },
        None => rsx! {
            span {
                class: "asset-change-cell {change_value_class(absolute)}",
                title: "No prior value for this period",
                "{absolute_text}"
            }
        },
    }
}

#[component]
fn AssetSortHeader(
    label: String,
    field: AssetSortField,
    selected_field: AssetSortField,
    direction: SortDirection,
    onsortfieldchange: EventHandler<AssetSortField>,
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
        default_asset_sort_direction(field)
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
