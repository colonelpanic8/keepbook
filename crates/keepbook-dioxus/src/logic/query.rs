use super::*;

impl FilterOverrides {
    pub(crate) fn account_exclude_override(&self, account_id: &str) -> Option<bool> {
        self.account_portfolio_exclusions
            .iter()
            .find(|override_entry| override_entry.account_id == account_id)
            .map(|override_entry| override_entry.exclude_from_portfolio)
    }

    pub(crate) fn with_account_exclude_override(
        mut self,
        account_id: String,
        exclude_from_portfolio: bool,
    ) -> Self {
        if let Some(override_entry) = self
            .account_portfolio_exclusions
            .iter_mut()
            .find(|override_entry| override_entry.account_id == account_id)
        {
            override_entry.exclude_from_portfolio = exclude_from_portfolio;
        } else {
            self.account_portfolio_exclusions
                .push(AccountPortfolioExclusionOverride {
                    account_id,
                    exclude_from_portfolio,
                });
        }
        self
    }

    pub(crate) fn without_account_exclude_override(mut self, account_id: &str) -> Self {
        self.account_portfolio_exclusions
            .retain(|override_entry| override_entry.account_id != account_id);
        self
    }
}

pub(crate) fn range_preset_from_config(value: &str) -> RangePreset {
    match normalize_config_key(value).as_str() {
        "1m" | "1month" | "month" | "onemonth" => RangePreset::OneMonth,
        "90d" | "90days" | "ninetydays" => RangePreset::NinetyDays,
        "6m" | "6months" | "sixmonths" => RangePreset::SixMonths,
        "1y" | "1year" | "year" | "oneyear" => RangePreset::OneYear,
        "2y" | "2years" | "twoyears" => RangePreset::TwoYears,
        "max" | "all" => RangePreset::Max,
        _ => DEFAULT_RANGE_PRESET,
    }
}

pub(crate) fn sampling_granularity_from_config(value: &str) -> SamplingGranularity {
    match normalize_config_key(value).as_str() {
        "auto" => SamplingGranularity::Auto,
        "daily" | "day" => SamplingGranularity::Daily,
        "weekly" | "week" => SamplingGranularity::Weekly,
        "monthly" | "month" => SamplingGranularity::Monthly,
        "yearly" | "annual" | "annually" | "year" => SamplingGranularity::Yearly,
        _ => DEFAULT_SAMPLING_GRANULARITY,
    }
}

pub(crate) fn normalize_config_key(value: &str) -> String {
    value
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(|ch| ch.to_lowercase())
        .collect()
}

pub(crate) fn history_query_string(
    preset: RangePreset,
    start_override: &str,
    end_override: &str,
    selected_sampling: SamplingGranularity,
    today: &str,
    filter_overrides: FilterOverrides,
    account: Option<&str>,
) -> String {
    let (start, end) = requested_history_date_range(preset, start_override, end_override, today);
    // The request itself omits the end bound so the server never trims freshly
    // synced change points on local/UTC date mismatches, but auto-granularity
    // selection still assumes the range runs through today.
    let granularity = history_request_granularity(
        selected_sampling,
        start.as_deref(),
        end.as_deref().or(Some(today)),
    );
    let mut params = vec![format!(
        "granularity={}",
        query_encode_component(granularity)
    )];

    if let Some(start) = start {
        push_query_param(&mut params, "start", &start);
    }
    if let Some(end) = end {
        push_query_param(&mut params, "end", &end);
    }
    if let Some(account) = account.filter(|account| !account.is_empty()) {
        push_query_param(&mut params, "account", account);
    }
    append_filter_override_params(&mut params, filter_overrides);

    params.join("&")
}

pub(crate) fn spending_query_string(
    preset: RangePreset,
    start_override: &str,
    end_override: &str,
    today: &str,
    currency: &str,
) -> String {
    spending_group_query_string(
        preset,
        start_override,
        end_override,
        today,
        currency,
        "tag",
        None,
    )
}

pub(crate) fn spending_description_query_string(
    preset: RangePreset,
    start_override: &str,
    end_override: &str,
    today: &str,
    currency: &str,
    group_by: &str,
    top: usize,
) -> String {
    spending_group_query_string(
        preset,
        start_override,
        end_override,
        today,
        currency,
        group_by,
        Some(top),
    )
}

fn spending_group_query_string(
    preset: RangePreset,
    start_override: &str,
    end_override: &str,
    today: &str,
    currency: &str,
    group_by: &str,
    top: Option<usize>,
) -> String {
    let (start, end) = requested_spending_date_range(preset, start_override, end_override, today);
    let mut params = vec![
        "period=range".to_string(),
        format!("group_by={}", query_encode_component(group_by)),
        "direction=outflow".to_string(),
        "status=posted".to_string(),
    ];
    if let Some(top) = top {
        params.push(format!("top={top}"));
    }
    push_query_param(&mut params, "currency", currency);
    if let Some(start) = start {
        push_query_param(&mut params, "start", &start);
    }
    if let Some(end) = end {
        push_query_param(&mut params, "end", &end);
    }
    params.join("&")
}

pub(crate) fn spending_over_time_query_string(
    preset: RangePreset,
    start_override: &str,
    end_override: &str,
    today: &str,
    currency: &str,
    bucket: SpendingBucket,
) -> String {
    let (start, end) = requested_spending_date_range(preset, start_override, end_override, today);
    let mut params = vec![
        format!("period={}", bucket.query_value()),
        "period_alignment=calendar".to_string(),
        "group_by=tag".to_string(),
        "direction=outflow".to_string(),
        "status=posted".to_string(),
        "include_empty=true".to_string(),
    ];
    push_query_param(&mut params, "currency", currency);
    if let Some(start) = start {
        push_query_param(&mut params, "start", &start);
    }
    if let Some(end) = end {
        push_query_param(&mut params, "end", &end);
    }
    params.join("&")
}

/// Query string for the asset breakdown endpoint. Assets only honor the
/// account include/exclude overrides; the latent-tax override is a synthetic
/// account and never applies to per-asset rows.
pub(crate) fn assets_query_string(
    overrides: FilterOverrides,
    include_amount_changes: bool,
) -> String {
    let mut params = Vec::new();
    if include_amount_changes {
        params.push("include_amount_changes=true".to_string());
    }
    append_filter_override_params(
        &mut params,
        FilterOverrides {
            include_latent_capital_gains_tax: None,
            account_portfolio_exclusions: overrides.account_portfolio_exclusions,
        },
    );
    params.join("&")
}

#[cfg(any(target_arch = "wasm32", test))]
pub(crate) fn filter_override_query_string(overrides: FilterOverrides) -> String {
    let mut params = Vec::new();
    append_filter_override_params(&mut params, overrides);
    params.join("&")
}

pub(crate) fn append_filter_override_params(params: &mut Vec<String>, overrides: FilterOverrides) {
    if let Some(enabled) = overrides.include_latent_capital_gains_tax {
        push_query_param(
            params,
            "include_latent_capital_gains_tax",
            bool_query_value(enabled),
        );
    }
    let mut account_overrides = overrides.account_portfolio_exclusions;
    account_overrides.sort_by(|a, b| a.account_id.cmp(&b.account_id));
    if !account_overrides.is_empty() {
        if let Ok(encoded) = serde_json::to_string(&account_overrides) {
            push_query_param(params, "account_portfolio_overrides", &encoded);
        }
    }
}

pub(crate) fn bool_query_value(value: bool) -> &'static str {
    if value {
        "true"
    } else {
        "false"
    }
}

pub(crate) fn push_query_param(params: &mut Vec<String>, key: &str, value: &str) {
    params.push(format!("{key}={}", query_encode_component(value)));
}

pub(crate) fn query_encode_component(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.as_bytes() {
        match *byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(*byte as char);
            }
            other => encoded.push_str(&format!("%{other:02X}")),
        }
    }
    encoded
}

pub(crate) fn requested_history_date_range(
    preset: RangePreset,
    start_override: &str,
    end_override: &str,
    today: &str,
) -> (Option<String>, Option<String>) {
    if preset == RangePreset::Custom {
        return (
            non_empty_string(start_override),
            non_empty_string(end_override),
        );
    }

    // Presets mean "from the computed start until now", so no end bound is
    // sent. Sending `today` computed from the local timezone used to trim
    // change points stamped with tomorrow's UTC date off the end of charts.
    let start = match preset {
        RangePreset::OneMonth => Some(offset_months(today, 1)),
        RangePreset::NinetyDays => Some(offset_days(today, 90)),
        RangePreset::SixMonths => Some(offset_months(today, 6)),
        RangePreset::OneYear => Some(offset_years(today, 1)),
        RangePreset::TwoYears => Some(offset_years(today, 2)),
        RangePreset::Max | RangePreset::Custom => None,
    };

    (start, None)
}

pub(crate) fn requested_spending_date_range(
    preset: RangePreset,
    start_override: &str,
    end_override: &str,
    today: &str,
) -> (Option<String>, Option<String>) {
    let (start, end) = requested_history_date_range(preset, start_override, end_override, today);
    // Spending queries keep their explicit end bound (today for presets, and a
    // today fallback for custom ranges); only Max stays unbounded.
    if preset == RangePreset::Max {
        (start, end)
    } else {
        (start, end.or_else(|| Some(today.to_string())))
    }
}

pub(crate) fn non_empty_string(value: &str) -> Option<String> {
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

pub(crate) fn history_request_granularity(
    selected: SamplingGranularity,
    start: Option<&str>,
    end: Option<&str>,
) -> &'static str {
    if selected != SamplingGranularity::Auto {
        return selected.query_value();
    }

    match (start, end) {
        (Some(start), Some(end)) => match days_between(start, end) {
            Some(days) if days < 93 => SamplingGranularity::Daily.query_value(),
            Some(days) if days > 365 * 3 => SamplingGranularity::Monthly.query_value(),
            Some(_) => SamplingGranularity::Weekly.query_value(),
            None => SamplingGranularity::Daily.query_value(),
        },
        _ => SamplingGranularity::Monthly.query_value(),
    }
}

pub(crate) fn transaction_query_string(
    start: &str,
    end: &str,
    tz: Option<&str>,
    include_ignored: bool,
) -> String {
    let mut params = Vec::new();
    if !start.trim().is_empty() {
        push_query_param(&mut params, "start", start);
    }
    if !end.trim().is_empty() {
        push_query_param(&mut params, "end", end);
    }
    if let Some(tz) = tz.filter(|tz| !tz.trim().is_empty()) {
        push_query_param(&mut params, "tz", tz);
    }
    if include_ignored {
        push_query_param(&mut params, "include_ignored", "true");
    }
    params.join("&")
}
