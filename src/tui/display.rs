//! Sorting, plus the string and style formatting of transactions and history points.

use std::cmp::Ordering;
use std::str::FromStr;

use chrono::{DateTime, NaiveDate, Utc};
use ratatui::style::{Color, Style};
use rust_decimal::Decimal;

use crate::app::transaction_tag_rules::{TransactionTagMatcher, TransactionTagRuleInput};
use crate::app::{HistoryPoint, TransactionOutput};
use crate::config::ResolvedConfig;
use crate::format::{currency_symbol, format_base_currency_display};

use super::state::{AppState, SortMode};

pub(super) fn compare_transactions(
    left: &TransactionOutput,
    right: &TransactionOutput,
    sort: SortMode,
) -> Ordering {
    match sort {
        SortMode::DateDesc => timestamp_order(left, right).reverse(),
        SortMode::DateAsc => timestamp_order(left, right),
        SortMode::AmountAsc => amount_order(left, right),
        SortMode::AmountDesc => amount_order(left, right).reverse(),
    }
}

fn timestamp_order(left: &TransactionOutput, right: &TransactionOutput) -> Ordering {
    let l = timestamp_sort_key(left);
    let r = timestamp_sort_key(right);
    l.cmp(&r)
        .then_with(|| left.id.cmp(&right.id))
        .then_with(|| left.account_id.cmp(&right.account_id))
}

fn amount_order(left: &TransactionOutput, right: &TransactionOutput) -> Ordering {
    let left_amount = Decimal::from_str(&left.amount);
    let right_amount = Decimal::from_str(&right.amount);
    match (left_amount, right_amount) {
        (Ok(l), Ok(r)) => l
            .cmp(&r)
            .then_with(|| timestamp_order(left, right).reverse()),
        (Err(_), Ok(_)) => Ordering::Greater,
        (Ok(_), Err(_)) => Ordering::Less,
        (Err(_), Err(_)) => left.amount.cmp(&right.amount),
    }
}

fn timestamp_sort_key(tx: &TransactionOutput) -> i64 {
    DateTime::parse_from_rfc3339(&tx.timestamp)
        .map(|dt| dt.timestamp_millis())
        .unwrap_or(i64::MIN)
}

pub(super) fn transaction_date(tx: &TransactionOutput) -> Option<NaiveDate> {
    tx.timestamp
        .get(..10)
        .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
}

pub(super) fn net_worth_point_date(point: &HistoryPoint) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(&point.date, "%Y-%m-%d")
        .ok()
        .or_else(|| {
            point
                .timestamp
                .get(..10)
                .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
        })
}

pub(super) fn net_worth_timestamp_sort_key(point: &HistoryPoint) -> i64 {
    DateTime::parse_from_rfc3339(&point.timestamp)
        .map(|dt| dt.timestamp_millis())
        .unwrap_or(i64::MIN)
}

pub(super) fn transaction_date_string(tx: &TransactionOutput) -> String {
    tx.timestamp
        .get(..10)
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| tx.timestamp.clone())
}

pub(super) fn resolved_transaction_tag(
    tx: &TransactionOutput,
    matcher: &TransactionTagMatcher,
) -> Option<String> {
    let annotation_tag = tx
        .annotation
        .as_ref()
        .and_then(|ann| ann.tags.as_ref())
        .and_then(|tags| tags.first())
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    if annotation_tag.is_some() {
        return annotation_tag;
    }

    let effective_tag = tx
        .tags
        .first()
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let effective_subtag = tx
        .subtags
        .first()
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let rule_tag = matcher
        .match_tag(&TransactionTagRuleInput {
            account_id: &tx.account_id,
            account_name: &tx.account_name,
            description: &tx.description,
            tag: effective_tag.unwrap_or(""),
            subtag: effective_subtag.unwrap_or(""),
            status: &tx.status,
            amount: &tx.amount,
        })
        .map(ToOwned::to_owned);
    if rule_tag.is_some() {
        return rule_tag;
    }

    effective_tag.map(ToOwned::to_owned)
}

pub(super) fn transaction_tag_string(
    tx: &TransactionOutput,
    matcher: &TransactionTagMatcher,
) -> String {
    resolved_transaction_tag(tx, matcher).unwrap_or_else(|| "-".to_string())
}

pub(super) fn transaction_amount_string(tx: &TransactionOutput, config: &ResolvedConfig) -> String {
    let Ok(amount) = Decimal::from_str(&tx.amount) else {
        return tx.amount.clone();
    };
    if transaction_asset_is_reporting_currency(tx, &config.reporting_currency) {
        format_base_currency_display(
            amount,
            config.display.currency_decimals,
            config.display.currency_grouping,
            config
                .display
                .currency_symbol
                .as_deref()
                .or_else(|| currency_symbol(&config.reporting_currency)),
            config.display.currency_fixed_decimals,
        )
    } else {
        amount.normalize().to_string()
    }
}

fn transaction_asset_is_reporting_currency(
    tx: &TransactionOutput,
    reporting_currency: &str,
) -> bool {
    let Some(obj) = tx.asset.as_object() else {
        return false;
    };
    let is_currency = obj.get("type").and_then(|v| v.as_str()) == Some("currency");
    if !is_currency {
        return false;
    }
    let Some(iso_code) = obj.get("iso_code").and_then(|v| v.as_str()) else {
        return false;
    };
    normalize_currency_code_for_display(iso_code) == reporting_currency.trim().to_uppercase()
}

#[derive(Debug, Clone)]
struct SpendingWindowSummary {
    days: u32,
    total: Decimal,
    transaction_count: usize,
}

fn spending_windows_from_config(config: &ResolvedConfig) -> Vec<u32> {
    let mut windows: Vec<u32> = config
        .tray
        .spending_windows_days
        .iter()
        .copied()
        .filter(|days| *days > 0)
        .collect();
    if windows.is_empty() {
        windows.extend([7, 30, 90]);
    }
    windows.sort_unstable();
    windows.dedup();
    windows
}

fn summarize_spending_windows(
    transactions: &[TransactionOutput],
    reporting_currency: &str,
    windows_days: &[u32],
    today: NaiveDate,
) -> Vec<SpendingWindowSummary> {
    let mut summaries: Vec<SpendingWindowSummary> = windows_days
        .iter()
        .copied()
        .map(|days| SpendingWindowSummary {
            days,
            total: Decimal::ZERO,
            transaction_count: 0,
        })
        .collect();

    if summaries.is_empty() {
        return summaries;
    }

    for tx in transactions {
        if transaction_annotation_ignores_spending(tx.annotation.as_ref()) {
            continue;
        }
        if !transaction_asset_is_reporting_currency(tx, reporting_currency) {
            continue;
        }
        let Some(date) = transaction_date(tx) else {
            continue;
        };
        let age_days = (today - date).num_days();
        if age_days < 0 {
            continue;
        }
        let Ok(amount) = Decimal::from_str(&tx.amount) else {
            continue;
        };
        if amount >= Decimal::ZERO {
            continue;
        }

        let spend_amount = -amount;
        for summary in &mut summaries {
            if age_days <= summary.days as i64 {
                summary.total += spend_amount;
                summary.transaction_count += 1;
            }
        }
    }

    summaries
}

fn transaction_annotation_ignores_spending(
    annotation: Option<&crate::app::TransactionAnnotationOutput>,
) -> bool {
    annotation.is_some_and(|ann| {
        ann.ignore_spending == Some(true)
            || ann.tags.as_ref().is_some_and(|tags| {
                tags.iter()
                    .any(|tag| crate::models::tag_ignores_spending(tag))
            })
    })
}

pub(super) fn transaction_spending_summary_line(
    app_state: &AppState,
    config: &ResolvedConfig,
) -> String {
    let windows = spending_windows_from_config(config);
    let summaries = summarize_spending_windows(
        &app_state.all_transactions,
        &config.reporting_currency,
        &windows,
        Utc::now().date_naive(),
    );
    if summaries.is_empty() {
        return "no windows configured".to_string();
    }

    let max_windows = 4usize;
    let shown = summaries.len().min(max_windows);
    let mut parts: Vec<String> = Vec::with_capacity(shown + 1);
    for summary in summaries.iter().take(shown) {
        let total = format_base_currency_display(
            summary.total,
            config.display.currency_decimals,
            config.display.currency_grouping,
            config
                .display
                .currency_symbol
                .as_deref()
                .or_else(|| currency_symbol(&config.reporting_currency)),
            config.display.currency_fixed_decimals,
        );
        parts.push(format!(
            "{}d: {} ({} txns)",
            summary.days, total, summary.transaction_count
        ));
    }
    if summaries.len() > shown {
        parts.push(format!("+{} more", summaries.len() - shown));
    }
    parts.join(" | ")
}

pub(super) fn net_worth_time_string(point: &HistoryPoint) -> String {
    point
        .timestamp
        .get(11..19)
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| point.timestamp.clone())
}

pub(super) fn net_worth_trigger_count(point: &HistoryPoint) -> String {
    point
        .change_triggers
        .as_ref()
        .map(|triggers| triggers.len().to_string())
        .unwrap_or_else(|| "-".to_string())
}

pub(super) fn net_worth_delta_style(delta: &Option<String>) -> Style {
    let Some(delta_value) = delta.as_ref() else {
        return Style::default();
    };

    let parsed = Decimal::from_str(delta_value);
    match parsed {
        Ok(value) if value < Decimal::ZERO => Style::default().fg(Color::Red),
        Ok(value) if value > Decimal::ZERO => Style::default().fg(Color::Green),
        _ => Style::default(),
    }
}

pub(super) fn asset_label(asset: &serde_json::Value) -> String {
    let Some(obj) = asset.as_object() else {
        return asset.to_string();
    };
    let Some(kind) = obj.get("type").and_then(|v| v.as_str()) else {
        return asset.to_string();
    };
    match kind {
        "currency" => obj
            .get("iso_code")
            .and_then(|v| v.as_str())
            .map(normalize_currency_code_for_display)
            .unwrap_or_else(|| "currency".to_string()),
        "equity" => obj
            .get("symbol")
            .and_then(|v| v.as_str())
            .map(|s| format!("equity:{s}"))
            .unwrap_or_else(|| "equity".to_string()),
        "crypto" => {
            let symbol = obj.get("symbol").and_then(|v| v.as_str()).unwrap_or("?");
            if let Some(network) = obj.get("network").and_then(|v| v.as_str()) {
                format!("crypto:{network}:{symbol}")
            } else {
                format!("crypto:{symbol}")
            }
        }
        _ => asset.to_string(),
    }
}

fn normalize_currency_code_for_display(value: &str) -> String {
    let trimmed = value.trim();
    match trimmed {
        "840" => "USD".to_string(),
        _ => trimmed.to_uppercase(),
    }
}

#[cfg(test)]
#[path = "../../tests/unit/tui/display_tests.rs"]
mod display_tests;
