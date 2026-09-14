//! Parsing of the date, range, and interval arguments the portfolio commands
//! accept, plus the calendar arithmetic those forms need.

use anyhow::{Context, Result};
use chrono::{Datelike, Duration, NaiveDate};

#[derive(Debug, Clone, Copy)]
pub(super) enum PriceHistoryInterval {
    Daily,
    Weekly,
    Monthly,
    Yearly,
}

impl PriceHistoryInterval {
    pub(super) fn parse(value: &str) -> Result<Self> {
        match value.to_lowercase().as_str() {
            "daily" => Ok(Self::Daily),
            "weekly" => Ok(Self::Weekly),
            "monthly" => Ok(Self::Monthly),
            "yearly" | "annual" | "annually" => Ok(Self::Yearly),
            _ => anyhow::bail!(
                "Invalid interval: {value}. Use: daily, weekly, monthly, yearly, annual"
            ),
        }
    }

    pub(super) fn as_str(&self) -> &'static str {
        match self {
            Self::Daily => "daily",
            Self::Weekly => "weekly",
            Self::Monthly => "monthly",
            Self::Yearly => "yearly",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) enum DateRangeBound {
    Start,
    End,
}

impl DateRangeBound {
    fn label(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::End => "end",
        }
    }
}

pub(super) fn parse_portfolio_date_bound(
    value: &str,
    bound: DateRangeBound,
    anchor_date: NaiveDate,
) -> Result<NaiveDate> {
    let value = value.trim();
    if value.eq_ignore_ascii_case("today") {
        return Ok(anchor_date);
    }

    if let Ok(date) = NaiveDate::parse_from_str(value, "%Y-%m-%d") {
        return Ok(date);
    }

    if value.len() == 4 && value.chars().all(|c| c.is_ascii_digit()) {
        let year = value.parse::<i32>()?;
        return match bound {
            DateRangeBound::Start => NaiveDate::from_ymd_opt(year, 1, 1)
                .with_context(|| format!("Invalid {} date: {value}", bound.label())),
            DateRangeBound::End => Ok(year_end(year)),
        };
    }

    if value.len() == 7
        && value.as_bytes()[4] == b'-'
        && value[..4].chars().all(|c| c.is_ascii_digit())
        && value[5..].chars().all(|c| c.is_ascii_digit())
    {
        let year = value[..4].parse::<i32>()?;
        let month = value[5..].parse::<u32>()?;
        let first_day = NaiveDate::from_ymd_opt(year, month, 1)
            .with_context(|| format!("Invalid {} date: {value}", bound.label()))?;
        return match bound {
            DateRangeBound::Start => Ok(first_day),
            DateRangeBound::End => Ok(month_end(first_day)),
        };
    }

    if let Some(date) = parse_relative_portfolio_date(value, anchor_date)? {
        return Ok(date);
    }

    anyhow::bail!(
        "Invalid {} date: {value}. Use YYYY-MM-DD, YYYY-MM, YYYY, today, or relative offsets like -1y, -3m, -2w, -10d",
        bound.label()
    )
}

fn parse_relative_portfolio_date(value: &str, anchor_date: NaiveDate) -> Result<Option<NaiveDate>> {
    let Some(sign) = value.chars().next().filter(|c| *c == '-' || *c == '+') else {
        return Ok(None);
    };
    if value.len() < 3 {
        return Ok(None);
    }

    let unit = value
        .chars()
        .last()
        .map(|c| c.to_ascii_lowercase())
        .unwrap_or_default();
    let amount = &value[1..value.len() - unit.len_utf8()];
    if amount.is_empty() || !amount.chars().all(|c| c.is_ascii_digit()) {
        return Ok(None);
    }

    let magnitude = amount.parse::<i32>()?;
    let signed = if sign == '-' { -magnitude } else { magnitude };

    let date = match unit {
        'd' => anchor_date
            .checked_add_signed(Duration::days(signed as i64))
            .with_context(|| format!("Relative date out of range: {value}"))?,
        'w' => anchor_date
            .checked_add_signed(Duration::weeks(signed as i64))
            .with_context(|| format!("Relative date out of range: {value}"))?,
        'm' => shift_months_clamped(anchor_date, signed),
        'y' => {
            let months = signed
                .checked_mul(12)
                .with_context(|| format!("Relative date out of range: {value}"))?;
            shift_months_clamped(anchor_date, months)
        }
        _ => return Ok(None),
    };

    Ok(Some(date))
}

pub(super) fn advance_interval_date(date: NaiveDate, interval: PriceHistoryInterval) -> NaiveDate {
    match interval {
        PriceHistoryInterval::Daily => date + Duration::days(1),
        PriceHistoryInterval::Weekly => date + Duration::days(7),
        PriceHistoryInterval::Monthly => next_month_end(date),
        PriceHistoryInterval::Yearly => next_year_end(date),
    }
}

pub(super) fn align_start_date(date: NaiveDate, interval: PriceHistoryInterval) -> NaiveDate {
    match interval {
        PriceHistoryInterval::Monthly => month_end(date),
        PriceHistoryInterval::Yearly => year_end(date.year()),
        _ => date,
    }
}

fn next_year_end(date: NaiveDate) -> NaiveDate {
    year_end(date.year() + 1)
}

fn year_end(year: i32) -> NaiveDate {
    NaiveDate::from_ymd_opt(year, 12, 31).expect("valid year end")
}

fn next_month_end(date: NaiveDate) -> NaiveDate {
    let (year, month) = if date.month() == 12 {
        (date.year() + 1, 1)
    } else {
        (date.year(), date.month() + 1)
    };
    let day = days_in_month(year, month);
    NaiveDate::from_ymd_opt(year, month, day).expect("valid next month end")
}

fn month_end(date: NaiveDate) -> NaiveDate {
    let day = days_in_month(date.year(), date.month());
    NaiveDate::from_ymd_opt(date.year(), date.month(), day).expect("valid month end")
}

fn days_in_month(year: i32, month: u32) -> u32 {
    let (next_year, next_month) = if month == 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    };
    let first_next = NaiveDate::from_ymd_opt(next_year, next_month, 1).expect("valid next month");
    let last = first_next - Duration::days(1);
    last.day()
}

fn shift_months_clamped(date: NaiveDate, months: i32) -> NaiveDate {
    let month_index = date.year() * 12 + date.month0() as i32 + months;
    let year = month_index.div_euclid(12);
    let month0 = month_index.rem_euclid(12) as u32;
    let month = month0 + 1;
    let day = date.day().min(days_in_month(year, month));
    NaiveDate::from_ymd_opt(year, month, day).expect("valid shifted month")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RelativeDateUnit {
    Day,
    Week,
    Month,
    Year,
}

impl RelativeDateUnit {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "d" | "day" | "days" => Some(Self::Day),
            "w" | "week" | "weeks" => Some(Self::Week),
            "m" | "mo" | "mon" | "month" | "months" => Some(Self::Month),
            "y" | "yr" | "year" | "years" => Some(Self::Year),
            _ => None,
        }
    }

    fn shift(self, anchor_date: NaiveDate, count: i32) -> NaiveDate {
        match self {
            Self::Day => anchor_date + Duration::days(count as i64),
            Self::Week => anchor_date + Duration::days((count * 7) as i64),
            Self::Month => shift_months_clamped(anchor_date, count),
            Self::Year => shift_months_clamped(anchor_date, count * 12),
        }
    }
}

fn parse_count_and_unit(value: &str) -> Option<(usize, RelativeDateUnit)> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }

    let digit_count = trimmed.chars().take_while(|c| c.is_ascii_digit()).count();
    if digit_count > 0 {
        let count = trimmed[..digit_count].parse::<usize>().ok()?;
        let unit = RelativeDateUnit::parse(trimmed[digit_count..].trim())?;
        return Some((count, unit));
    }

    let mut parts = trimmed.split_whitespace();
    let count = parts.next()?.parse::<usize>().ok()?;
    let unit = RelativeDateUnit::parse(parts.next()?)?;
    if parts.next().is_some() {
        return None;
    }
    Some((count, unit))
}

fn parse_history_spec_entry(anchor_date: NaiveDate, spec: &str) -> Result<Vec<NaiveDate>> {
    let normalized = spec.trim().to_lowercase();
    if normalized.is_empty() {
        anyhow::bail!("history spec entry must not be empty");
    }
    if normalized == "today" {
        return Ok(vec![anchor_date]);
    }

    for prefix in ["each of the last ", "last "] {
        if let Some(rest) = normalized.strip_prefix(prefix) {
            let (count, unit) = parse_count_and_unit(rest)
                .with_context(|| format!("Invalid history spec entry: {spec}"))?;
            if count == 0 {
                anyhow::bail!("history spec entry must use a positive count: {spec}");
            }
            return Ok((0..count)
                .map(|offset| unit.shift(anchor_date, -(offset as i32)))
                .collect());
        }
    }

    let rest = normalized.strip_suffix(" ago").unwrap_or(&normalized);
    let (count, unit) = parse_count_and_unit(rest)
        .with_context(|| format!("Invalid history spec entry: {spec}"))?;
    if count == 0 {
        anyhow::bail!("history spec entry must use a positive count: {spec}");
    }
    Ok(vec![unit.shift(anchor_date, -(count as i32))])
}

pub(super) fn history_spec_dates(
    anchor_date: NaiveDate,
    history_spec: &[String],
) -> Result<Vec<NaiveDate>> {
    let mut dates = Vec::new();
    for spec in history_spec {
        dates.extend(parse_history_spec_entry(anchor_date, spec)?);
    }
    dates.sort_unstable();
    dates.dedup();
    Ok(dates)
}
