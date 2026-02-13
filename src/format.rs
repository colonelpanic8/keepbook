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

