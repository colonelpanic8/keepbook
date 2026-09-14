use super::*;

fn asset_string_field<'a>(asset: &'a serde_json::Value, key: &str) -> Option<&'a str> {
    asset
        .get(key)
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

/// Primary display name for an asset breakdown row: currency ISO code, equity
/// ticker, crypto symbol, or manual-value name, falling back to the asset id.
pub(crate) fn asset_display_name(asset: &serde_json::Value, fallback: &str) -> String {
    let name = match asset_string_field(asset, "type") {
        Some("currency") => asset_string_field(asset, "iso_code"),
        Some("equity") => asset_string_field(asset, "ticker"),
        Some("crypto") => asset_string_field(asset, "symbol"),
        Some("manual_value") => asset_string_field(asset, "name"),
        _ => None,
    };
    name.unwrap_or(fallback).to_string()
}

/// Secondary detail for an asset row: an equity's exchange or a crypto
/// asset's network, when present.
pub(crate) fn asset_secondary_text(asset: &serde_json::Value) -> Option<String> {
    match asset_string_field(asset, "type") {
        Some("equity") => asset_string_field(asset, "exchange"),
        Some("crypto") => asset_string_field(asset, "network"),
        _ => None,
    }
    .map(str::to_string)
}

pub(crate) fn asset_kind_label(asset: &serde_json::Value) -> &'static str {
    match asset_string_field(asset, "type") {
        Some("currency") => "Currency",
        Some("equity") => "Equity",
        Some("crypto") => "Crypto",
        Some("manual_value") => "Manual",
        _ => "Asset",
    }
}

/// Display form of an asset holding amount: rounded and trimmed like the rest
/// of the app's decimal output, keeping the raw text when unparseable.
pub(crate) fn format_asset_amount(amount: &str) -> String {
    format_decimal_text(amount, 4).unwrap_or_else(|| amount.to_string())
}

/// Expansion-state key for an asset breakdown row. Asset and liability rows
/// for the same asset are distinct rows, so the flag is part of the key.
pub(crate) fn asset_expansion_key(entry: &AssetBreakdownEntry) -> String {
    format!(
        "{}:{}",
        entry.asset_id,
        if entry.liability {
            "liability"
        } else {
            "asset"
        }
    )
}

pub(crate) fn default_asset_sort_direction(field: AssetSortField) -> SortDirection {
    match field {
        AssetSortField::Name => SortDirection::Asc,
        AssetSortField::Amount
        | AssetSortField::AmountChecked
        | AssetSortField::AmountChanged
        | AssetSortField::PriceUpdated
        | AssetSortField::Value
        | AssetSortField::DayChange
        | AssetSortField::WeekChange
        | AssetSortField::MonthChange
        | AssetSortField::YearChange => SortDirection::Desc,
    }
}

pub(crate) fn asset_period_change(
    entry: &AssetBreakdownEntry,
    field: AssetSortField,
) -> Option<&AssetChange> {
    match field {
        AssetSortField::DayChange => entry.changes.day.as_ref(),
        AssetSortField::WeekChange => entry.changes.week.as_ref(),
        AssetSortField::MonthChange => entry.changes.month.as_ref(),
        AssetSortField::YearChange => entry.changes.year.as_ref(),
        AssetSortField::Name
        | AssetSortField::Amount
        | AssetSortField::AmountChecked
        | AssetSortField::AmountChanged
        | AssetSortField::PriceUpdated
        | AssetSortField::Value => None,
    }
}

fn asset_sort_metric(
    entry: &AssetBreakdownEntry,
    field: AssetSortField,
    use_absolute_changes: bool,
) -> Option<f64> {
    match field {
        AssetSortField::Name => None,
        AssetSortField::Amount => parse_money_input(&entry.total_amount),
        AssetSortField::AmountChecked => {
            asset_timestamp_sort_metric(entry.amount_last_checked_at.as_deref())
        }
        AssetSortField::AmountChanged => {
            asset_timestamp_sort_metric(entry.amount_last_changed_at.as_deref())
        }
        AssetSortField::PriceUpdated => {
            asset_timestamp_sort_metric(entry.price_updated_at.as_deref())
        }
        AssetSortField::Value => entry
            .value_in_base
            .as_deref()
            .and_then(parse_money_input)
            .map(f64::abs),
        AssetSortField::DayChange
        | AssetSortField::WeekChange
        | AssetSortField::MonthChange
        | AssetSortField::YearChange => asset_period_change(entry, field).and_then(|change| {
            if use_absolute_changes {
                parse_money_input(&change.absolute)
            } else {
                change.percentage.as_deref().and_then(parse_money_input)
            }
        }),
    }
}

/// RFC 3339 app timestamps are UTC (`+00:00`), so their compact numeric form
/// preserves chronological ordering without target-specific date libraries.
fn asset_timestamp_sort_metric(timestamp: Option<&str>) -> Option<f64> {
    timestamp.map(|value| {
        value
            .bytes()
            .filter(u8::is_ascii_digit)
            .take(17)
            .fold(0_f64, |metric, digit| {
                metric * 10.0 + f64::from(digit - b'0')
            })
    })
}

pub(crate) fn format_asset_timestamp(timestamp: Option<&str>) -> String {
    let Some(value) = timestamp else {
        return "—".to_string();
    };
    if value.len() >= 16 && value.as_bytes().get(10) == Some(&b'T') {
        format!("{} {} UTC", &value[..10], &value[11..16])
    } else {
        value.to_string()
    }
}

/// Missing metrics (unpriced rows, absent change periods) always sort last,
/// regardless of direction.
fn compare_metrics_missing_last(
    left: Option<f64>,
    right: Option<f64>,
    direction: SortDirection,
) -> std::cmp::Ordering {
    match (left, right) {
        (Some(left), Some(right)) => {
            let ordering = left
                .partial_cmp(&right)
                .unwrap_or(std::cmp::Ordering::Equal);
            match direction {
                SortDirection::Asc => ordering,
                SortDirection::Desc => ordering.reverse(),
            }
        }
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    }
}

pub(crate) fn compare_asset_entries_with_change_metric(
    a: &AssetBreakdownEntry,
    b: &AssetBreakdownEntry,
    field: AssetSortField,
    direction: SortDirection,
    use_absolute_changes: bool,
) -> std::cmp::Ordering {
    let ordering = match field {
        AssetSortField::Name => {
            let ordering = compare_case_insensitive(
                &asset_display_name(&a.asset, &a.asset_id),
                &asset_display_name(&b.asset, &b.asset_id),
            );
            match direction {
                SortDirection::Asc => ordering,
                SortDirection::Desc => ordering.reverse(),
            }
        }
        _ => compare_metrics_missing_last(
            asset_sort_metric(a, field, use_absolute_changes),
            asset_sort_metric(b, field, use_absolute_changes),
            direction,
        ),
    };
    ordering
        .then_with(|| a.asset_id.cmp(&b.asset_id))
        .then_with(|| a.liability.cmp(&b.liability))
}
