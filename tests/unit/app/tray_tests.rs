use super::*;

use chrono::NaiveDate;

use crate::portfolio::AccountSummary;

#[test]
fn format_tray_currency_uses_usd_symbol_by_default() {
    let display = DisplayConfig::default();
    let formatted = format_tray_currency("1234.5", "USD", &display);
    assert_eq!(formatted, "$1234.5");
}

#[test]
fn format_tray_currency_appends_unknown_currency_code() {
    let display = DisplayConfig::default();
    let formatted = format_tray_currency("1234.5", "CHF", &display);
    assert_eq!(formatted, "1234.5 CHF");
}

#[test]
fn format_history_change_for_tray_defaults_to_na() {
    assert_eq!(format_history_change_for_tray(None), "N/A");
    assert_eq!(format_history_change_for_tray(Some("N/A")), "N/A");
}

#[test]
fn format_history_change_for_tray_adds_sign_and_percent() {
    assert_eq!(format_history_change_for_tray(Some("3.25")), "+3.25%");
    assert_eq!(format_history_change_for_tray(Some("-1.50")), "-1.50%");
}

#[test]
fn normalize_spending_windows_days_sorts_dedupes_and_drops_zero() {
    assert_eq!(
        normalize_spending_windows_days(&[30, 0, 365, 7, 30]),
        vec![7, 30, 365]
    );
}

#[test]
fn format_spending_window_label_uses_year_for_365_days() {
    assert_eq!(format_spending_window_label(7), "7d");
    assert_eq!(format_spending_window_label(365), "year");
    assert_eq!(format_spending_window_label(730), "2 years");
}

#[test]
fn build_portfolio_breakdown_lines_formats_account_values() {
    let display = DisplayConfig {
        currency_decimals: Some(2),
        currency_grouping: true,
        currency_symbol: Some("$".to_string()),
        currency_fixed_decimals: true,
    };
    let snapshot = PortfolioSnapshot {
        as_of_date: NaiveDate::from_ymd_opt(2026, 4, 24).unwrap(),
        currency: "USD".to_string(),
        total_value: "1250".to_string(),
        total_cost_basis: None,
        total_unrealized_gain: None,
        prospective_capital_gains_tax: None,
        valuation_scenario: None,
        by_asset: None,
        by_account: Some(vec![
            AccountSummary {
                account_id: "acct-1".to_string(),
                account_name: "Checking".to_string(),
                connection_name: "Bank".to_string(),
                value_in_base: Some("1000".to_string()),
            },
            AccountSummary {
                account_id: "acct-2".to_string(),
                account_name: "Brokerage".to_string(),
                connection_name: "Broker".to_string(),
                value_in_base: None,
            },
        ]),
        valuation_issues: Vec::new(),
    };

    let lines = build_portfolio_breakdown_lines(&snapshot, &display);

    assert_eq!(lines[0], "Total: $1,250.00");
    assert_eq!(lines[1], "Bank / Checking: $1,000.00");
    assert_eq!(lines[2], "Broker / Brokerage: unpriced");
}
