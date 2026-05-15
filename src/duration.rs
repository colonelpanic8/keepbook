//! Duration parsing utilities for human-readable durations like "14d", "24h".

use std::time::Duration;

use anyhow::{Context, Result};
use serde::{de, Deserialize, Deserializer};

/// Parse a duration string like "14d", "24h", "30m", "60s".
///
/// Supported units:
/// - `d` - days (24 hours)
/// - `h` - hours
/// - `m` - minutes
/// - `s` - seconds
///
/// The input is case-insensitive and whitespace is trimmed.
///
/// # Examples
///
/// ```
/// use keepbook::duration::parse_duration;
/// use std::time::Duration;
///
/// assert_eq!(parse_duration("14d").unwrap(), Duration::from_secs(14 * 24 * 60 * 60));
/// assert_eq!(parse_duration("24h").unwrap(), Duration::from_secs(24 * 60 * 60));
/// assert_eq!(parse_duration("30m").unwrap(), Duration::from_secs(30 * 60));
/// assert_eq!(parse_duration("60s").unwrap(), Duration::from_secs(60));
/// ```
pub fn parse_duration(s: &str) -> Result<Duration> {
    let s = s.trim().to_lowercase();
    let (num, unit) = if s.ends_with('d') {
        (s.trim_end_matches('d'), "d")
    } else if s.ends_with('h') {
        (s.trim_end_matches('h'), "h")
    } else if s.ends_with('m') {
        (s.trim_end_matches('m'), "m")
    } else if s.ends_with('s') {
        (s.trim_end_matches('s'), "s")
    } else {
        anyhow::bail!("Duration must end with d, h, m, or s");
    };

    let num: u64 = num.parse().with_context(|| "Invalid number in duration")?;

    let secs = match unit {
        "d" => num
            .checked_mul(24 * 60 * 60)
            .context("Duration is too large")?,
        "h" => num.checked_mul(60 * 60).context("Duration is too large")?,
        "m" => num.checked_mul(60).context("Duration is too large")?,
        "s" => num,
        _ => unreachable!(),
    };

    Ok(Duration::from_secs(secs))
}

/// Format a duration to a human-readable string.
///
/// Uses the largest appropriate unit (days, hours, minutes, or seconds).
/// For durations that don't divide evenly, uses the largest unit and rounds down.
///
/// # Examples
///
/// ```
/// use keepbook::duration::format_duration;
/// use std::time::Duration;
///
/// assert_eq!(format_duration(Duration::from_secs(14 * 24 * 60 * 60)), "14d");
/// assert_eq!(format_duration(Duration::from_secs(24 * 60 * 60)), "1d");
/// assert_eq!(format_duration(Duration::from_secs(2 * 60 * 60)), "2h");
/// assert_eq!(format_duration(Duration::from_secs(30 * 60)), "30m");
/// assert_eq!(format_duration(Duration::from_secs(45)), "45s");
/// ```
pub fn format_duration(d: Duration) -> String {
    let secs = d.as_secs();

    const SECS_PER_DAY: u64 = 24 * 60 * 60;
    const SECS_PER_HOUR: u64 = 60 * 60;
    const SECS_PER_MINUTE: u64 = 60;

    if secs >= SECS_PER_DAY && secs.is_multiple_of(SECS_PER_DAY) {
        format!("{}d", secs / SECS_PER_DAY)
    } else if secs >= SECS_PER_HOUR && secs.is_multiple_of(SECS_PER_HOUR) {
        format!("{}h", secs / SECS_PER_HOUR)
    } else if secs >= SECS_PER_MINUTE && secs.is_multiple_of(SECS_PER_MINUTE) {
        format!("{}m", secs / SECS_PER_MINUTE)
    } else {
        format!("{secs}s")
    }
}

/// Serde deserializer for duration strings.
///
/// Use with `#[serde(deserialize_with = "deserialize_duration")]`.
///
/// # Example
///
/// ```ignore
/// use serde::Deserialize;
/// use std::time::Duration;
/// use keepbook::duration::deserialize_duration;
///
/// #[derive(Deserialize)]
/// struct Config {
///     #[serde(deserialize_with = "deserialize_duration")]
///     timeout: Duration,
/// }
/// ```
pub fn deserialize_duration<'de, D>(deserializer: D) -> Result<Duration, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    parse_duration(&s).map_err(de::Error::custom)
}

/// Serde deserializer for optional duration strings.
///
/// Use with `#[serde(default, deserialize_with = "deserialize_duration_opt")]`.
///
/// # Example
///
/// ```ignore
/// use serde::Deserialize;
/// use std::time::Duration;
/// use keepbook::duration::deserialize_duration_opt;
///
/// #[derive(Deserialize)]
/// struct Config {
///     #[serde(default, deserialize_with = "deserialize_duration_opt")]
///     timeout: Option<Duration>,
/// }
/// ```
pub fn deserialize_duration_opt<'de, D>(deserializer: D) -> Result<Option<Duration>, D::Error>
where
    D: Deserializer<'de>,
{
    let opt: Option<String> = Option::deserialize(deserializer)?;
    match opt {
        Some(s) => parse_duration(&s).map(Some).map_err(de::Error::custom),
        None => Ok(None),
    }
}

#[cfg(test)]
#[path = "../tests/unit/duration_tests.rs"]
mod duration_tests;
