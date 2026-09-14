pub(crate) fn parse_money_input(value: &str) -> Option<f64> {
    let cleaned = value
        .chars()
        .filter(|ch| !matches!(ch, '$' | ',' | ' '))
        .collect::<String>();
    if cleaned.is_empty() {
        None
    } else {
        cleaned
            .parse::<f64>()
            .ok()
            .filter(|value| value.is_finite())
    }
}

pub(crate) fn format_input_number(value: f64) -> String {
    format_number(value, 2)
}

pub(crate) fn format_compact_money(value: f64, currency: &str) -> String {
    let abs = value.abs();
    let (scaled, suffix) = if abs >= 1_000_000_000.0 {
        (value / 1_000_000_000.0, "B")
    } else if abs >= 1_000_000.0 {
        (value / 1_000_000.0, "M")
    } else if abs >= 1_000.0 {
        (value / 1_000.0, "K")
    } else {
        (value, "")
    };
    format_money_display(scaled, currency, 1, suffix)
}

pub(crate) fn format_full_money(value: f64, currency: &str) -> String {
    format_money_display(value, currency, 2, "")
}

fn format_money_display(value: f64, currency: &str, decimals: usize, suffix: &str) -> String {
    let rounded = format!("{:.*}", decimals, value.abs());
    let amount = match rounded.split_once('.') {
        Some((integer, fraction)) => {
            format!(
                "{}.{fraction}{suffix}",
                format_digit_string_with_commas(integer)
            )
        }
        None => format!("{}{suffix}", format_digit_string_with_commas(&rounded)),
    };

    apply_currency_display(&amount, currency, value < 0.0)
}

fn apply_currency_display(amount: &str, currency: &str, negative: bool) -> String {
    let sign = if negative { "-" } else { "" };
    match currency_display_symbol(currency) {
        Some(symbol) => format!("{sign}{symbol}{amount}"),
        None => {
            let currency = currency.trim();
            if currency.is_empty() {
                format!("{sign}{amount}")
            } else {
                format!("{} {sign}{amount}", currency.to_uppercase())
            }
        }
    }
}

fn currency_display_symbol(currency: &str) -> Option<&'static str> {
    match currency.trim().to_ascii_uppercase().as_str() {
        "USD" | "US DOLLAR" | "UNITED STATES DOLLAR" | "DOLLAR" => Some("$"),
        _ => None,
    }
}

fn format_digit_string_with_commas(digits: &str) -> String {
    let mut formatted = String::new();
    for (index, ch) in digits.chars().rev().enumerate() {
        if index > 0 && index % 3 == 0 {
            formatted.push(',');
        }
        formatted.push(ch);
    }
    formatted.chars().rev().collect()
}

pub(crate) fn format_number(value: f64, decimals: usize) -> String {
    let mut formatted = format!("{value:.decimals$}");
    if formatted.contains('.') {
        while formatted.ends_with('0') {
            formatted.pop();
        }
        if formatted.ends_with('.') {
            formatted.pop();
        }
    }
    formatted
}

/// Canonical decimal text as produced by the app layer: an optional sign, then
/// integer and fraction digit runs. Keeping money in this form lets the UI
/// render app values without an f64 round trip.
struct DecimalText<'a> {
    negative: bool,
    integer: &'a str,
    fraction: &'a str,
}

fn parse_decimal_text(value: &str) -> Option<DecimalText<'_>> {
    let trimmed = value.trim();
    let (negative, digits) = match trimmed.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, trimmed.strip_prefix('+').unwrap_or(trimmed)),
    };
    let (integer, fraction) = digits.split_once('.').unwrap_or((digits, ""));
    if integer.is_empty() && fraction.is_empty() {
        return None;
    }
    if !integer.bytes().all(|byte| byte.is_ascii_digit())
        || !fraction.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    Some(DecimalText {
        negative,
        integer,
        fraction,
    })
}

/// Round the digits of a [`DecimalText`] to `decimals` places, half away from
/// zero, returning the integer and fraction digit runs.
fn round_decimal_digits(value: &DecimalText<'_>, decimals: usize) -> (String, String) {
    let mut digits: Vec<u8> = value
        .integer
        .bytes()
        .chain(value.fraction.bytes())
        .map(|byte| byte - b'0')
        .collect();
    let keep = value.integer.len() + decimals;
    let round_up = digits.get(keep).is_some_and(|digit| *digit >= 5);
    digits.truncate(keep);
    digits.resize(keep, 0);
    if round_up {
        let mut index = digits.len();
        loop {
            if index == 0 {
                digits.insert(0, 1);
                break;
            }
            index -= 1;
            if digits[index] == 9 {
                digits[index] = 0;
            } else {
                digits[index] += 1;
                break;
            }
        }
    }

    let split = digits.len() - decimals;
    let render = |run: &[u8]| run.iter().map(|digit| (digit + b'0') as char).collect();
    let integer: String = render(&digits[..split]);
    let fraction: String = render(&digits[split..]);
    let trimmed = integer.trim_start_matches('0');
    (
        if trimmed.is_empty() {
            "0".to_string()
        } else {
            trimmed.to_string()
        },
        fraction,
    )
}

fn is_zero_digits(integer: &str, fraction: &str) -> bool {
    integer.bytes().chain(fraction.bytes()).all(|b| b == b'0')
}

/// Round app decimal text to `decimals` places and strip trailing fraction
/// zeros: the string-domain counterpart of [`format_number`].
pub(crate) fn format_decimal_text(value: &str, decimals: usize) -> Option<String> {
    let parsed = parse_decimal_text(value)?;
    let (integer, fraction) = round_decimal_digits(&parsed, decimals);
    let fraction = fraction.trim_end_matches('0');
    let sign = if parsed.negative && !is_zero_digits(&integer, fraction) {
        "-"
    } else {
        ""
    };
    Some(if fraction.is_empty() {
        format!("{sign}{integer}")
    } else {
        format!("{sign}{integer}.{fraction}")
    })
}

/// Currency text for app decimal money, matching [`format_full_money`] without
/// the f64 round trip. `None` when the text is not a decimal.
pub(crate) fn format_money_text(value: &str, currency: &str) -> Option<String> {
    let parsed = parse_decimal_text(value)?;
    Some(money_text(&parsed, currency, parsed.negative))
}

/// [`format_money_text`] on an amount's magnitude, dropping the sign.
fn format_absolute_money_text(value: &str, currency: &str) -> Option<String> {
    let parsed = parse_decimal_text(value)?;
    Some(money_text(&parsed, currency, false))
}

fn money_text(parsed: &DecimalText<'_>, currency: &str, negative: bool) -> String {
    let (integer, fraction) = round_decimal_digits(parsed, 2);
    let amount = format!("{}.{fraction}", format_digit_string_with_commas(&integer));
    apply_currency_display(&amount, currency, negative)
}

/// Money range text (`$1.00–$2.00`) over two app decimal amounts' magnitudes,
/// ordered smallest first and collapsed to one amount when both render the
/// same. `None` when either text is not a decimal.
pub(crate) fn format_absolute_money_range_text(
    left: &str,
    right: &str,
    currency: &str,
) -> Option<String> {
    let left_text = format_absolute_money_text(left, currency)?;
    let right_text = format_absolute_money_text(right, currency)?;
    let (low, high) =
        if compare_money_text_magnitude(left, right).is_some_and(std::cmp::Ordering::is_gt) {
            (right_text, left_text)
        } else {
            (left_text, right_text)
        };
    Some(if low == high {
        low
    } else {
        format!("{low}\u{2013}{high}")
    })
}

/// Orders two app decimal amounts by magnitude, ignoring sign.
fn compare_money_text_magnitude(left: &str, right: &str) -> Option<std::cmp::Ordering> {
    let left = parse_decimal_text(left)?;
    let right = parse_decimal_text(right)?;
    let left_integer = left.integer.trim_start_matches('0');
    let right_integer = right.integer.trim_start_matches('0');
    Some(
        left_integer
            .len()
            .cmp(&right_integer.len())
            .then_with(|| left_integer.cmp(right_integer))
            .then_with(|| compare_fraction_digits(left.fraction, right.fraction)),
    )
}

fn compare_fraction_digits(left: &str, right: &str) -> std::cmp::Ordering {
    let digit = |run: &str, index: usize| run.as_bytes().get(index).copied().unwrap_or(b'0');
    (0..left.len().max(right.len()))
        .map(|index| digit(left, index).cmp(&digit(right, index)))
        .find(|ordering| ordering.is_ne())
        .unwrap_or(std::cmp::Ordering::Equal)
}

/// [`format_money_text`] with an explicit `+` on non-negative amounts.
pub(crate) fn format_signed_money_text(value: &str, currency: &str) -> Option<String> {
    let formatted = format_money_text(value, currency)?;
    Some(if parse_decimal_text(value)?.negative {
        formatted
    } else {
        format!("+{formatted}")
    })
}

/// Signed percentage text for app decimal percentages, using the same sign
/// convention as [`format_signed_money_text`]: non-negative values get a `+`.
pub(crate) fn format_signed_percent_text(value: &str) -> Option<String> {
    let formatted = format_decimal_text(value, 2)?;
    Some(if formatted.starts_with('-') {
        format!("{formatted}%")
    } else {
        format!("+{formatted}%")
    })
}

/// Gain/loss coloring class for a change readout that always takes a side, the
/// way the chart's range change does: zero reads as a gain.
pub(crate) fn signed_change_class_text(value: &str) -> &'static str {
    if change_value_class_text(value) == "change-negative" {
        "change-negative"
    } else {
        "change-positive"
    }
}

/// Compact currency text (`$1.6K`) for app decimal money, matching
/// [`format_compact_money`] without the f64 round trip.
pub(crate) fn format_compact_money_text(value: &str, currency: &str) -> Option<String> {
    let parsed = parse_decimal_text(value)?;
    let integer = parsed.integer.trim_start_matches('0');
    let (shift, suffix) = match integer.len() {
        digits if digits > 9 => (9, "B"),
        digits if digits > 6 => (6, "M"),
        digits if digits > 3 => (3, "K"),
        _ => (0, ""),
    };
    let split = integer.len() - shift;
    let scaled_integer = &integer[..split];
    let scaled_fraction = format!("{}{}", &integer[split..], parsed.fraction);
    let (integer, fraction) = round_decimal_digits(
        &DecimalText {
            negative: parsed.negative,
            integer: scaled_integer,
            fraction: &scaled_fraction,
        },
        1,
    );
    let amount = format!(
        "{}.{fraction}{suffix}",
        format_digit_string_with_commas(&integer)
    );
    Some(apply_currency_display(&amount, currency, parsed.negative))
}

/// Gain/loss coloring class for app decimal text. Zero stays neutral.
pub(crate) fn change_value_class_text(value: &str) -> &'static str {
    match parse_decimal_text(value) {
        Some(parsed) if is_zero_digits(parsed.integer, parsed.fraction) => "",
        Some(parsed) if parsed.negative => "change-negative",
        Some(_) => "change-positive",
        None => "",
    }
}

pub(crate) fn enabled_label(value: bool) -> &'static str {
    if value {
        "Included"
    } else {
        "Excluded"
    }
}

pub(crate) fn compare_case_insensitive(a: &str, b: &str) -> std::cmp::Ordering {
    a.to_lowercase()
        .cmp(&b.to_lowercase())
        .then_with(|| a.cmp(b))
}
