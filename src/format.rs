use rust_decimal::{Decimal, RoundingStrategy};

/// Format a value denominated in the output/base currency.
///
/// - When `currency_decimals` is set, the value is rounded (half away from zero)
///   to that many decimal places before formatting.
/// - Trailing zeros are stripped (`Decimal::normalize()`), matching the CLI's
///   existing decimal formatting rules.
pub fn format_base_currency_value(value: Decimal, currency_decimals: Option<u32>) -> String {
    let rounded = match currency_decimals {
        Some(dp) => value.round_dp_with_strategy(dp, RoundingStrategy::MidpointAwayFromZero),
        None => value,
    };
    rounded.normalize().to_string()
}

fn group_int_digits(int_part: &str) -> String {
    // Insert commas every 3 digits, preserving any leading zeros.
    let mut out = String::with_capacity(int_part.len() + int_part.len() / 3);
    let bytes = int_part.as_bytes();
    let len = bytes.len();
    for (i, ch) in int_part.chars().enumerate() {
        out.push(ch);
        let remaining = len.saturating_sub(i + 1);
        if remaining > 0 && remaining % 3 == 0 {
            out.push(',');
        }
    }
    out
}

fn pad_fraction_to_dp(s: &str, dp: u32) -> String {
    if dp == 0 {
        return s
            .split_once('.')
            .map(|(i, _)| i.to_string())
            .unwrap_or_else(|| s.to_string());
    }

    let (int_part, frac_part) = match s.split_once('.') {
        Some((i, f)) => (i, f),
        None => (s, ""),
    };

    let mut out = String::with_capacity(int_part.len() + 1 + dp as usize);
    out.push_str(int_part);
    out.push('.');

    let mut written = 0usize;
    for ch in frac_part.chars().take(dp as usize) {
        out.push(ch);
        written += 1;
    }
    while written < dp as usize {
        out.push('0');
        written += 1;
    }

    out
}

fn group_number_string(s: &str) -> String {
    let (int_part, frac_part) = match s.split_once('.') {
        Some((i, f)) => (i, Some(f)),
        None => (s, None),
    };
    let grouped = group_int_digits(int_part);
    match frac_part {
        Some(f) if !f.is_empty() => format!("{grouped}.{f}"),
        _ => grouped,
    }
}

/// Format a base-currency value for human display.
///
/// This does **not** change any canonical JSON numeric string fields. It is
/// intended for UI surfaces (tray, frontend, etc.).
///
/// Options:
/// - `currency_decimals`: rounding precision (half away from zero)
/// - `currency_grouping`: enable thousands separators (`,`)
/// - `currency_symbol`: optional prefix (e.g. `$`, `€`)
/// - `currency_fixed_decimals`: when true and `currency_decimals` is set,
///   pad/truncate to exactly that many decimal places
pub fn format_base_currency_display(
    value: Decimal,
    currency_decimals: Option<u32>,
    currency_grouping: bool,
    currency_symbol: Option<&str>,
    currency_fixed_decimals: bool,
) -> String {
    let rounded = match currency_decimals {
        Some(dp) => value.round_dp_with_strategy(dp, RoundingStrategy::MidpointAwayFromZero),
        None => value,
    };

    let negative = rounded.is_sign_negative() && !rounded.is_zero();
    let abs = rounded.abs();

    // Start from normalized form so the default stays aligned with the CLI's
    // canonical numeric formatting.
    let mut s = abs.normalize().to_string();
    if currency_fixed_decimals {
        if let Some(dp) = currency_decimals {
            s = pad_fraction_to_dp(&s, dp);
        }
    }
    if currency_grouping {
        s = group_number_string(&s);
    }

    let mut out = String::new();
    if negative {
        out.push('-');
    }
    if let Some(sym) = currency_symbol {
        out.push_str(sym);
    }
    out.push_str(&s);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

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
}
