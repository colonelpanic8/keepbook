//! Formatting helpers for tray/menu surfaces.
//!
//! Both the sync daemon's ksni tray and the server's tray endpoints render the
//! same text, so these live here rather than being duplicated per consumer.

use std::str::FromStr;

use rust_decimal::Decimal;

use crate::config::DisplayConfig;
use crate::format::{currency_symbol, format_base_currency_display};
use crate::portfolio::PortfolioSnapshot;

pub fn format_tray_currency(value: &str, currency: &str, display: &DisplayConfig) -> String {
    // The tray is a UI surface: default to sane currency rounding even when the
    // global config doesn't set `display.currency_decimals`.
    let dp = display.currency_decimals.or(Some(2));
    let symbol = display
        .currency_symbol
        .as_deref()
        .or_else(|| currency_symbol(currency));
    match Decimal::from_str(value) {
        Ok(d) => {
            let formatted = format_base_currency_display(
                d,
                dp,
                display.currency_grouping,
                symbol,
                display.currency_fixed_decimals,
            );
            if symbol.is_some() {
                formatted
            } else {
                format!("{formatted} {currency}")
            }
        }
        Err(_) => value.to_string(),
    }
}

pub fn format_history_change_for_tray(percentage_change: Option<&str>) -> String {
    match percentage_change {
        Some("N/A") | None => "N/A".to_string(),
        Some(value) if value.starts_with('-') => format!("{value}%"),
        Some(value) => format!("+{value}%"),
    }
}

pub fn normalize_spending_windows_days(windows: &[u32]) -> Vec<u32> {
    let mut normalized: Vec<u32> = windows.iter().copied().filter(|days| *days > 0).collect();
    normalized.sort_unstable();
    normalized.dedup();
    normalized
}

pub fn format_spending_window_label(days: u32) -> String {
    match days {
        365 => "year".to_string(),
        _ if days.is_multiple_of(365) => format!("{} years", days / 365),
        _ => format!("{days}d"),
    }
}

pub fn build_portfolio_breakdown_lines(
    snapshot: &PortfolioSnapshot,
    display: &DisplayConfig,
) -> Vec<String> {
    let mut lines = vec![format!(
        "Total: {}",
        format_tray_currency(&snapshot.total_value, &snapshot.currency, display)
    )];

    let Some(accounts) = snapshot.by_account.as_ref() else {
        lines.push("No account breakdown available".to_string());
        return lines;
    };

    if accounts.is_empty() {
        lines.push("No accounts with balances".to_string());
        return lines;
    }

    lines.extend(accounts.iter().map(|account| {
        let value = account
            .value_in_base
            .as_deref()
            .map(|value| format_tray_currency(value, &snapshot.currency, display))
            .unwrap_or_else(|| "unpriced".to_string());
        format!(
            "{} / {}: {}",
            account.connection_name, account.account_name, value
        )
    }));

    lines
}

#[cfg(test)]
#[path = "../../tests/unit/app/tray_tests.rs"]
mod tray_tests;
