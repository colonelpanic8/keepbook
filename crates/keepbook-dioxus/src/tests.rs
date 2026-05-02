use super::logic::*;
use super::*;

fn point(date: &str, value: f64) -> NetWorthDataPoint {
    NetWorthDataPoint {
        date: date.to_string(),
        value,
    }
}

#[test]
fn two_year_range_starts_two_years_before_latest_point() {
    let points = vec![
        point("2022-01-01", 100.0),
        point("2024-04-25", 150.0),
        point("2026-04-25", 200.0),
    ];

    assert_eq!(
        visible_date_range(&points, RangePreset::TwoYears, "", ""),
        ("2024-04-25".to_string(), "2026-04-25".to_string())
    );
}

#[test]
fn two_year_range_clamps_to_earliest_available_point() {
    let points = vec![point("2026-02-03", 100.0), point("2026-04-25", 200.0)];

    assert_eq!(
        visible_date_range(&points, RangePreset::TwoYears, "", ""),
        ("2026-02-03".to_string(), "2026-04-25".to_string())
    );
}

#[test]
fn custom_range_uses_manual_overrides() {
    let points = vec![point("2024-01-01", 100.0), point("2026-04-25", 200.0)];

    assert_eq!(
        visible_date_range(&points, RangePreset::Custom, "2025-01-01", "2025-12-31"),
        ("2025-01-01".to_string(), "2025-12-31".to_string())
    );
}

#[test]
fn short_range_presets_use_expected_start_dates() {
    let points = vec![point("2025-01-01", 100.0), point("2026-04-25", 200.0)];

    assert_eq!(
        visible_date_range(&points, RangePreset::OneMonth, "", ""),
        ("2026-03-25".to_string(), "2026-04-25".to_string())
    );
    assert_eq!(
        visible_date_range(&points, RangePreset::NinetyDays, "", ""),
        ("2026-01-25".to_string(), "2026-04-25".to_string())
    );
    assert_eq!(
        visible_date_range(&points, RangePreset::SixMonths, "", ""),
        ("2025-10-25".to_string(), "2026-04-25".to_string())
    );
}

#[test]
fn default_graph_query_requests_one_year_weekly_history() {
    assert_eq!(
        history_query_string(
            DEFAULT_RANGE_PRESET,
            "",
            "",
            DEFAULT_SAMPLING_GRANULARITY,
            "2026-04-25",
            FilterOverrides::default(),
            None,
        ),
        "granularity=weekly&start=2025-04-25&end=2026-04-25"
    );
}

#[test]
fn graph_defaults_parse_config_values() {
    assert_eq!(range_preset_from_config("2y"), RangePreset::TwoYears);
    assert_eq!(range_preset_from_config("one_month"), RangePreset::OneMonth);
    assert_eq!(
        sampling_granularity_from_config("monthly"),
        SamplingGranularity::Monthly
    );
    assert_eq!(
        sampling_granularity_from_config("not-a-real-value"),
        DEFAULT_SAMPLING_GRANULARITY
    );
}

#[test]
fn auto_graph_query_uses_daily_under_three_months() {
    assert_eq!(
        history_query_string(
            RangePreset::NinetyDays,
            "",
            "",
            SamplingGranularity::Auto,
            "2026-04-25",
            FilterOverrides::default(),
            None,
        ),
        "granularity=daily&start=2026-01-25&end=2026-04-25"
    );
}

#[test]
fn max_graph_query_uses_monthly_without_date_bounds() {
    assert_eq!(
        history_query_string(
            RangePreset::Max,
            "",
            "",
            SamplingGranularity::Auto,
            "2026-04-25",
            FilterOverrides::default(),
            None,
        ),
        "granularity=monthly"
    );
}

#[test]
fn account_graph_query_scopes_history() {
    assert_eq!(
        history_query_string(
            RangePreset::Max,
            "",
            "",
            SamplingGranularity::Auto,
            "2026-04-25",
            FilterOverrides::default(),
            Some("account id"),
        ),
        "granularity=monthly&account=account%20id"
    );
}

#[test]
fn filter_override_query_includes_latent_tax_override() {
    assert_eq!(
        filter_override_query_string(FilterOverrides {
            include_latent_capital_gains_tax: Some(false),
        }),
        "include_latent_capital_gains_tax=false"
    );
}

fn transaction(id: &str, amount: &str, status: &str) -> Transaction {
    Transaction {
        id: id.to_string(),
        account_id: "account-1".to_string(),
        account_name: "Card".to_string(),
        timestamp: "2026-04-25T12:00:00+00:00".to_string(),
        description: "Test transaction".to_string(),
        amount: amount.to_string(),
        status: status.to_string(),
        category: None,
        subcategory: None,
        annotation: None,
        ignored_from_spending: false,
    }
}

#[test]
fn inclusive_transaction_query_requests_ignored_rows() {
    assert_eq!(
        transaction_query_string("2025-04-25", "2026-04-25", true),
        "start=2025-04-25&end=2026-04-25&include_ignored=true"
    );
}

#[test]
fn spending_transaction_marking_flags_rows_not_counted_in_totals() {
    let counted = vec![transaction("counted", "-12.50", "posted")];
    let rows = vec![
        transaction("counted", "-12.50", "posted"),
        transaction("ignored", "-8.00", "posted"),
        transaction("inflow", "9.00", "posted"),
        transaction("pending", "-4.00", "pending"),
    ];

    let marked = mark_transactions_excluded_from_spending(rows, &counted);

    assert!(!marked[0].ignored_from_spending);
    assert!(marked[1].ignored_from_spending);
    assert!(marked[2].ignored_from_spending);
    assert!(marked[3].ignored_from_spending);
}

#[test]
fn spending_transactions_sort_by_amount_in_both_directions() {
    let rows = vec![
        transaction("middle", "-12.50", "posted"),
        transaction("largest", "-40.00", "posted"),
        transaction("smallest", "-3.25", "posted"),
    ];

    let ascending = filtered_transactions(
        &rows,
        None,
        "",
        TransactionSortField::Amount,
        SortDirection::Asc,
        true,
    );
    let descending = filtered_transactions(
        &rows,
        None,
        "",
        TransactionSortField::Amount,
        SortDirection::Desc,
        true,
    );

    assert_eq!(ascending[0].id, "largest");
    assert_eq!(ascending[2].id, "smallest");
    assert_eq!(descending[0].id, "smallest");
    assert_eq!(descending[2].id, "largest");
}

#[test]
fn spending_transactions_sort_by_each_visible_text_field() {
    let mut card = transaction("card", "-12.50", "posted");
    card.account_name = "Card".to_string();
    card.category = Some("Dining".to_string());
    card.subcategory = Some("Restaurants".to_string());
    card.description = "Zulu".to_string();
    card.ignored_from_spending = true;

    let mut bank = transaction("bank", "-8.00", "posted");
    bank.account_name = "Bank".to_string();
    bank.category = Some("Bills".to_string());
    bank.subcategory = Some("Utilities".to_string());
    bank.description = "Alpha".to_string();

    let rows = vec![card, bank];

    assert_eq!(
        filtered_transactions(
            &rows,
            None,
            "",
            TransactionSortField::Description,
            SortDirection::Asc,
            true,
        )[0]
        .id,
        "bank"
    );
    assert_eq!(
        filtered_transactions(
            &rows,
            None,
            "",
            TransactionSortField::Category,
            SortDirection::Asc,
            true,
        )[0]
        .id,
        "bank"
    );
    assert_eq!(
        filtered_transactions(
            &rows,
            None,
            "",
            TransactionSortField::Account,
            SortDirection::Asc,
            true,
        )[0]
        .id,
        "bank"
    );
    assert_eq!(
        filtered_transactions(
            &rows,
            None,
            "",
            TransactionSortField::Counted,
            SortDirection::Asc,
            true,
        )[0]
        .id,
        "bank"
    );
}

#[test]
fn transaction_subcategory_prefers_annotation_value() {
    let mut row = transaction("annotated", "-12.50", "posted");
    row.subcategory = Some("Fallback".to_string());
    row.annotation = Some(TransactionAnnotation {
        description: None,
        category: None,
        subcategory: Some("Coffee".to_string()),
        effective_date: None,
    });

    assert_eq!(transaction_subcategory(&row).as_deref(), Some("Coffee"));
}

#[test]
fn spending_transactions_can_hide_ignored_rows() {
    let visible = transaction("visible", "-12.50", "posted");
    let mut ignored = transaction("ignored", "-8.00", "posted");
    ignored.ignored_from_spending = true;
    let rows = vec![visible, ignored];

    let without_ignored = filtered_transactions(
        &rows,
        None,
        "",
        TransactionSortField::Date,
        SortDirection::Desc,
        false,
    );
    let with_ignored = filtered_transactions(
        &rows,
        None,
        "",
        TransactionSortField::Date,
        SortDirection::Desc,
        true,
    );

    assert_eq!(without_ignored.len(), 1);
    assert_eq!(without_ignored[0].id, "visible");
    assert_eq!(with_ignored.len(), 2);
}

#[test]
fn month_offsets_clamp_to_valid_dates() {
    assert_eq!(offset_months("2026-03-31", 1), "2026-02-28");
    assert_eq!(offset_months("2024-03-31", 1), "2024-02-29");
    assert_eq!(offset_years("2024-02-29", 1), "2023-02-28");
}

#[test]
fn auto_sampling_uses_daily_under_three_months() {
    let points = vec![point("2026-01-26", 100.0), point("2026-04-25", 200.0)];

    assert_eq!(
        resolve_sampling_granularity(SamplingGranularity::Auto, &points),
        SamplingGranularity::Daily
    );
}

#[test]
fn auto_sampling_uses_weekly_for_two_year_ranges() {
    let points = vec![point("2024-04-25", 100.0), point("2026-04-25", 200.0)];

    assert_eq!(
        resolve_sampling_granularity(SamplingGranularity::Auto, &points),
        SamplingGranularity::Weekly
    );
}

#[test]
fn sampled_series_preserves_range_endpoints() {
    let points = vec![
        point("2026-01-01", 100.0),
        point("2026-01-02", 110.0),
        point("2026-01-08", 120.0),
        point("2026-01-09", 130.0),
    ];

    let sampled = sample_data_by_granularity(&points, SamplingGranularity::Weekly);

    assert_eq!(
        sampled.first().map(|point| point.date.as_str()),
        Some("2026-01-01")
    );
    assert_eq!(
        sampled.last().map(|point| point.date.as_str()),
        Some("2026-01-09")
    );
}

#[test]
fn current_net_worth_uses_portfolio_snapshot_total() {
    let snapshot = PortfolioSnapshot {
        as_of_date: "2026-04-25".to_string(),
        currency: "USD".to_string(),
        total_value: "1234.56".to_string(),
        by_account: Vec::new(),
    };

    assert_eq!(current_net_worth_from_snapshot(&snapshot), 1234.56);
}

#[test]
fn account_value_uses_portfolio_snapshot_account_total() {
    let account_summaries = vec![AccountSummary {
        account_id: "empower".to_string(),
        account_name: "Empower Retirement".to_string(),
        connection_name: "Empower".to_string(),
        value_in_base: Some("113738.71".to_string()),
    }];

    assert_eq!(
        account_snapshot_value("empower", &account_summaries),
        Some(113738.71)
    );
    assert_eq!(account_snapshot_value("missing", &account_summaries), None);
}

#[test]
fn money_formatting_uses_usd_symbol() {
    assert_eq!(format_full_money(1571.17, "USD"), "$1,571.17");
    assert_eq!(format_full_money(-1571.17, "usd"), "-$1,571.17");
    assert_eq!(format_full_money(1.999, "USD"), "$2.00");
    assert_eq!(format_compact_money(1571.17, "USD"), "$1.6K");
}

#[test]
fn money_formatting_keeps_unknown_currency_code() {
    assert_eq!(format_full_money(1571.17, "CHF"), "CHF 1,571.17");
}

#[test]
fn portfolio_snapshot_deserializes_virtual_accounts() {
    let snapshot: PortfolioSnapshot = serde_json::from_value(serde_json::json!({
        "as_of_date": "2026-04-26",
        "currency": "USD",
        "total_value": "1882543.57",
        "by_account": [
            {
                "account_id": "acct-1",
                "account_name": "Brokerage",
                "connection_name": "Schwab",
                "value_in_base": "2052806.85"
            },
            {
                "account_id": "virtual:latent_capital_gains_tax",
                "account_name": "Latent Capital Gains Tax",
                "connection_name": "Virtual",
                "value_in_base": "-170263.28"
            }
        ]
    }))
    .expect("snapshot should deserialize");

    let virtual_accounts = virtual_account_summaries(&snapshot);

    assert_eq!(virtual_accounts.len(), 1);
    assert_eq!(virtual_accounts[0].account_name, "Latent Capital Gains Tax");
    assert_eq!(
        virtual_accounts[0].value_in_base.as_deref(),
        Some("-170263.28")
    );
}
