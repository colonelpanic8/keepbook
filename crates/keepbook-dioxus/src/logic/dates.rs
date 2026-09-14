use super::*;

pub(crate) fn range_summary_text(start: &str, end: &str) -> String {
    match (start.is_empty(), end.is_empty()) {
        (false, false) => format!("{start} to {end}"),
        (false, true) => format!("{start} onward"),
        (true, false) => format!("through {end}"),
        (true, true) => "All available dates".to_string(),
    }
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn current_date_string() -> String {
    let date = js_sys::Date::new_0();
    format!(
        "{:04}-{:02}-{:02}",
        date.get_full_year(),
        date.get_month() + 1,
        date.get_date()
    )
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn current_date_string() -> String {
    chrono::Local::now().date_naive().to_string()
}

pub(crate) fn offset_years(date: &str, years: i32) -> String {
    offset_months(date, years * 12)
}

pub(crate) fn offset_months(date: &str, months: i32) -> String {
    let Some((year, month, day)) = parse_ymd(date) else {
        return date.to_string();
    };

    let month_index = year * 12 + month as i32 - 1 - months;
    let new_year = month_index.div_euclid(12);
    let new_month = month_index.rem_euclid(12) as u32 + 1;
    let new_day = day.min(days_in_month(new_year, new_month));
    format!("{new_year:04}-{new_month:02}-{new_day:02}")
}

pub(crate) fn offset_days(date: &str, days: i64) -> String {
    let Some((year, month, day)) = parse_ymd(date) else {
        return date.to_string();
    };
    civil_from_days(days_from_civil(year, month, day) - days)
}

pub(crate) fn sampling_bucket(date: &str, granularity: SamplingGranularity) -> String {
    match granularity {
        SamplingGranularity::Weekly => parse_ymd(date)
            .map(|(year, month, day)| {
                let day_number = days_from_civil(year, month, day);
                format!("week-{}", day_number.div_euclid(7))
            })
            .unwrap_or_else(|| date.to_string()),
        SamplingGranularity::Monthly => date.get(..7).unwrap_or(date).to_string(),
        SamplingGranularity::Yearly => date.get(..4).unwrap_or(date).to_string(),
        SamplingGranularity::Auto | SamplingGranularity::Daily => date.to_string(),
    }
}

pub(crate) fn days_between(start: &str, end: &str) -> Option<i64> {
    let (start_year, start_month, start_day) = parse_ymd(start)?;
    let (end_year, end_month, end_day) = parse_ymd(end)?;
    Some(
        days_from_civil(end_year, end_month, end_day)
            - days_from_civil(start_year, start_month, start_day),
    )
}

pub(crate) fn parse_ymd(date: &str) -> Option<(i32, u32, u32)> {
    let mut parts = date.split('-');
    let year = parts.next()?.parse::<i32>().ok()?;
    let month = parts.next()?.parse::<u32>().ok()?;
    let day = parts.next()?.parse::<u32>().ok()?;
    if parts.next().is_some() || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    Some((year, month, day))
}

pub(crate) fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 30,
    }
}

pub(crate) fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

pub(crate) fn days_from_civil(year: i32, month: u32, day: u32) -> i64 {
    let year = year - if month <= 2 { 1 } else { 0 };
    let era = (year as i64).div_euclid(400);
    let yoe = year as i64 - era * 400;
    let month = month as i64;
    let doy = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

pub(crate) fn civil_from_days(days: i64) -> String {
    let days = days + 719_468;
    let era = days.div_euclid(146_097);
    let doe = days - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096).div_euclid(365);
    let mut year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2).div_euclid(153);
    let day = doy - (153 * mp + 2).div_euclid(5) + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    year += if month <= 2 { 1 } else { 0 };
    format!("{year:04}-{month:02}-{day:02}")
}
