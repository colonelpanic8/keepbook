use super::*;
use std::str::FromStr;

#[test]
fn currency_symbol_maps_codes_and_names() {
    assert_eq!(currency_symbol("USD"), Some("$"));
    assert_eq!(currency_symbol("us-dollar"), Some("$"));
    assert_eq!(currency_symbol("Euro"), Some("€"));
    assert_eq!(currency_symbol("GBP"), Some("£"));
    assert_eq!(currency_symbol("unknown"), None);
}

#[test]
fn format_base_currency_display_defaults_match_numeric_format() {
    let d = Decimal::from_str("1234.500").unwrap();
    assert_eq!(
        format_base_currency_display(d, None, false, None, false),
        format_base_currency_value(d, None)
    );
}

#[test]
fn format_base_currency_display_groups_and_symbols() {
    let d = Decimal::from_str("1234567.5").unwrap();
    assert_eq!(
        format_base_currency_display(d, Some(2), true, Some("$"), true),
        "$1,234,567.50"
    );
}

#[test]
fn format_base_currency_display_negative_sign_precedes_symbol() {
    let d = Decimal::from_str("-1234.5").unwrap();
    assert_eq!(
        format_base_currency_display(d, Some(2), true, Some("$"), true),
        "-$1,234.50"
    );
}
