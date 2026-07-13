use super::logic::*;
use super::*;

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn native_worker_completes_off_the_ui_thread() {
    let ui_thread = std::thread::current().id();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("test runtime should start");
    let worker_thread = runtime
        .block_on(crate::api::run_native_blocking(|| {
            Ok(std::thread::current().id())
        }))
        .expect("worker should return its result");

    assert_ne!(worker_thread, ui_thread);
}

fn point(date: &str, value: f64) -> NetWorthDataPoint {
    NetWorthDataPoint {
        date: date.to_string(),
        value,
    }
}

fn active_series(key: &str, label: &str) -> ActiveStackedSeries {
    ActiveStackedSeries {
        key: key.to_string(),
        label: label.to_string(),
        account_id: None,
        series_type: "account_asset".to_string(),
    }
}

fn stacked_point(date: &str, total: f64, components: &[(&str, f64)]) -> StackedHistoryDataPoint {
    StackedHistoryDataPoint {
        date: date.to_string(),
        total,
        components: components
            .iter()
            .map(|(series_key, value)| StackedValue {
                series_key: (*series_key).to_string(),
                value: *value,
            })
            .collect(),
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
        "granularity=weekly&start=2025-04-25"
    );
}

#[test]
fn preset_graph_queries_omit_end_bound() {
    // Presets mean "until now"; sending a local-timezone end date used to trim
    // change points stamped with tomorrow's UTC date off the end of charts.
    assert_eq!(
        requested_history_date_range(RangePreset::OneYear, "", "", "2026-04-25"),
        (Some("2025-04-25".to_string()), None)
    );
    assert_eq!(
        requested_history_date_range(RangePreset::Custom, "2026-01-01", "", "2026-04-25"),
        (Some("2026-01-01".to_string()), None)
    );
    assert_eq!(
        requested_history_date_range(
            RangePreset::Custom,
            "2026-01-01",
            "2026-02-01",
            "2026-04-25"
        ),
        (
            Some("2026-01-01".to_string()),
            Some("2026-02-01".to_string())
        )
    );
}

#[test]
fn spending_date_range_keeps_end_bound() {
    assert_eq!(
        requested_spending_date_range(RangePreset::OneYear, "", "", "2026-04-25"),
        (
            Some("2025-04-25".to_string()),
            Some("2026-04-25".to_string())
        )
    );
    assert_eq!(
        requested_spending_date_range(RangePreset::Custom, "2026-01-01", "", "2026-04-25"),
        (
            Some("2026-01-01".to_string()),
            Some("2026-04-25".to_string())
        )
    );
    assert_eq!(
        requested_spending_date_range(RangePreset::Max, "", "", "2026-04-25"),
        (None, None)
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
        "granularity=daily&start=2026-01-25"
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
fn coalesce_minor_stacked_series_groups_series_below_visible_threshold() {
    let series = vec![
        active_series("cash", "Cash"),
        active_series("brokerage", "Brokerage"),
        active_series("tiny", "Tiny"),
    ];
    let data = vec![
        stacked_point(
            "2026-01-01",
            100.0,
            &[("cash", 60.0), ("brokerage", 39.0), ("tiny", 1.0)],
        ),
        stacked_point(
            "2026-02-01",
            200.0,
            &[("cash", 100.0), ("brokerage", 98.0), ("tiny", 2.0)],
        ),
    ];

    let (coalesced_data, coalesced_series) = coalesce_minor_stacked_series(&data, &series, 2.0);

    assert_eq!(
        coalesced_series
            .iter()
            .map(|series| series.label.as_str())
            .collect::<Vec<_>>(),
        vec!["Cash", "Brokerage", "Other <2%"]
    );
    assert_eq!(
        coalesced_data[1]
            .components
            .iter()
            .find(|component| component.series_key == "__other_minor_contributions")
            .map(|component| component.value),
        Some(2.0)
    );
}

#[test]
fn coalesce_minor_stacked_series_keeps_series_material_anywhere_in_visible_range() {
    let series = vec![
        active_series("cash", "Cash"),
        active_series("volatile", "Volatile"),
    ];
    let data = vec![
        stacked_point("2026-01-01", 100.0, &[("cash", 96.0), ("volatile", 4.0)]),
        stacked_point("2026-02-01", 200.0, &[("cash", 198.0), ("volatile", 2.0)]),
    ];

    let (coalesced_data, coalesced_series) = coalesce_minor_stacked_series(&data, &series, 2.0);

    assert_eq!(coalesced_data, data);
    assert_eq!(coalesced_series, series);
}

#[test]
fn default_spending_queries_request_one_year_monthly_total() {
    assert_eq!(
        spending_query_string(
            DEFAULT_SPENDING_RANGE_PRESET,
            "",
            "",
            "2026-04-25",
            "USD",
        ),
        "period=range&group_by=tag&direction=outflow&status=posted&currency=USD&start=2025-04-25&end=2026-04-25"
    );
    assert_eq!(
        spending_over_time_query_string(
            DEFAULT_SPENDING_RANGE_PRESET,
            "",
            "",
            "2026-04-25",
            "USD",
            DEFAULT_SPENDING_BUCKET,
        ),
        "period=monthly&period_alignment=calendar&group_by=tag&direction=outflow&status=posted&include_empty=true&currency=USD&start=2025-04-25&end=2026-04-25"
    );
}

#[cfg(feature = "desktop")]
#[test]
fn desktop_start_minimized_to_tray_starts_window_hidden() {
    assert!(!desktop_window_visible(DesktopStartupOptions {
        start_minimized_to_tray: true,
    }));
    assert!(desktop_window_visible(DesktopStartupOptions {
        start_minimized_to_tray: false,
    }));
}

#[cfg(feature = "desktop")]
#[test]
fn desktop_window_builder_removes_native_window_chrome() {
    let window_builder = desktop_window_builder(DesktopStartupOptions {
        start_minimized_to_tray: false,
    });

    assert_eq!(window_builder.window.title, "Keepbook");
    assert!(!window_builder.window.decorations);
    assert!(window_builder.window.visible);
}

#[test]
fn spending_over_time_query_requests_bucketed_tag_breakdowns() {
    assert_eq!(
        spending_over_time_query_string(
            RangePreset::NinetyDays,
            "",
            "",
            "2026-04-25",
            "USD",
            SpendingBucket::Weekly,
        ),
        "period=weekly&period_alignment=calendar&group_by=tag&direction=outflow&status=posted&include_empty=true&currency=USD&start=2026-01-25&end=2026-04-25"
    );
}

#[test]
fn spending_over_time_points_preserve_bucket_breakdowns() {
    let spending = SpendingOutput {
        currency: "USD".to_string(),
        tz: "local".to_string(),
        start_date: "2026-01-01".to_string(),
        end_date: "2026-02-28".to_string(),
        period: "monthly".to_string(),
        total: "60".to_string(),
        transaction_count: 3,
        periods: vec![
            SpendingPeriod {
                start_date: "2026-01-01".to_string(),
                end_date: "2026-01-31".to_string(),
                total: "40".to_string(),
                transaction_count: 2,
                breakdown: vec![
                    SpendingBreakdownEntry {
                        key: "food".to_string(),
                        total: "25".to_string(),
                        transaction_count: 1,
                    },
                    SpendingBreakdownEntry {
                        key: "untagged".to_string(),
                        total: "15".to_string(),
                        transaction_count: 1,
                    },
                ],
            },
            SpendingPeriod {
                start_date: "2026-02-01".to_string(),
                end_date: "2026-02-28".to_string(),
                total: "20".to_string(),
                transaction_count: 1,
                breakdown: vec![SpendingBreakdownEntry {
                    key: "food".to_string(),
                    total: "20".to_string(),
                    transaction_count: 1,
                }],
            },
        ],
        skipped_transaction_count: 0,
        missing_price_transaction_count: 0,
        missing_fx_transaction_count: 0,
    };

    let points = spending_over_time_points(&spending);
    let series = spending_over_time_series(&points);

    assert_eq!(points[0].label, "2026-01");
    assert_eq!(points[0].total, 40.0);
    assert_eq!(points[0].segments[0].key, "food");
    assert_eq!(points[0].segments[1].key, "Untagged");
    assert_eq!(series[0].key, "food");
    assert_eq!(series[0].total, "45");
}

#[test]
fn spending_over_time_visible_points_keep_empty_periods() {
    let spending = SpendingOutput {
        currency: "USD".to_string(),
        tz: "local".to_string(),
        start_date: "2026-05-01".to_string(),
        end_date: "2026-06-04".to_string(),
        period: "monthly".to_string(),
        total: "10".to_string(),
        transaction_count: 1,
        periods: vec![
            SpendingPeriod {
                start_date: "2026-05-01".to_string(),
                end_date: "2026-05-31".to_string(),
                total: "10".to_string(),
                transaction_count: 1,
                breakdown: vec![SpendingBreakdownEntry {
                    key: "Food".to_string(),
                    total: "10".to_string(),
                    transaction_count: 1,
                }],
            },
            SpendingPeriod {
                start_date: "2026-06-01".to_string(),
                end_date: "2026-06-04".to_string(),
                total: "0".to_string(),
                transaction_count: 0,
                breakdown: vec![],
            },
        ],
        skipped_transaction_count: 0,
        missing_price_transaction_count: 0,
        missing_fx_transaction_count: 0,
    };

    let points = spending_over_time_points(&spending);
    let series = spending_over_time_series(&points);
    let visible_points = visible_spending_over_time_points(&points, &series);

    assert_eq!(visible_points.len(), 2);
    assert_eq!(visible_points[1].label, "2026-06");
    assert_eq!(visible_points[1].total, 0.0);
}

#[test]
fn spending_segment_tooltip_shows_category_and_bucket_totals() {
    assert_eq!(
        spending_segment_tooltip_detail(12.5, 80.0, "USD", "Monthly"),
        "$12.50 category · $80.00 month total"
    );
    assert_eq!(
        spending_segment_tooltip_detail(12.5, 80.0, "USD", "Weekly"),
        "$12.50 category · $80.00 week total"
    );
}

#[test]
fn narrow_spending_points_to_tag_keeps_only_selected_tag_totals() {
    let spending = SpendingOutput {
        currency: "USD".to_string(),
        tz: "local".to_string(),
        start_date: "2026-01-01".to_string(),
        end_date: "2026-02-28".to_string(),
        period: "monthly".to_string(),
        total: "60".to_string(),
        transaction_count: 3,
        periods: vec![
            SpendingPeriod {
                start_date: "2026-01-01".to_string(),
                end_date: "2026-01-31".to_string(),
                total: "40".to_string(),
                transaction_count: 2,
                breakdown: vec![
                    SpendingBreakdownEntry {
                        key: "food".to_string(),
                        total: "25".to_string(),
                        transaction_count: 1,
                    },
                    SpendingBreakdownEntry {
                        key: "travel".to_string(),
                        total: "15".to_string(),
                        transaction_count: 1,
                    },
                ],
            },
            SpendingPeriod {
                start_date: "2026-02-01".to_string(),
                end_date: "2026-02-28".to_string(),
                total: "20".to_string(),
                transaction_count: 1,
                breakdown: vec![SpendingBreakdownEntry {
                    key: "travel".to_string(),
                    total: "20".to_string(),
                    transaction_count: 1,
                }],
            },
        ],
        skipped_transaction_count: 0,
        missing_price_transaction_count: 0,
        missing_fx_transaction_count: 0,
    };

    let points = spending_over_time_points(&spending);
    let narrowed = narrow_spending_points_to_tag(&points, "food");

    assert_eq!(narrowed.len(), 2);
    assert_eq!(narrowed[0].label, "2026-01");
    assert_eq!(narrowed[0].total, 25.0);
    assert_eq!(narrowed[0].transaction_count, 1);
    assert_eq!(narrowed[0].segments.len(), 1);
    assert_eq!(narrowed[0].segments[0].key, "food");
    assert_eq!(narrowed[1].label, "2026-02");
    assert_eq!(narrowed[1].total, 0.0);
    assert_eq!(narrowed[1].transaction_count, 0);
    assert!(narrowed[1].segments.is_empty());
}

#[test]
fn filter_override_query_includes_latent_tax_override() {
    assert_eq!(
        filter_override_query_string(FilterOverrides {
            include_latent_capital_gains_tax: Some(false),
            ..FilterOverrides::default()
        }),
        "include_latent_capital_gains_tax=false"
    );
}

#[test]
fn filter_override_query_includes_account_overrides() {
    assert_eq!(
        filter_override_query_string(FilterOverrides {
            account_portfolio_exclusions: vec![
                AccountPortfolioExclusionOverride {
                    account_id: "taxable account".to_string(),
                    exclude_from_portfolio: true,
                },
                AccountPortfolioExclusionOverride {
                    account_id: "retirement".to_string(),
                    exclude_from_portfolio: false,
                },
            ],
            ..FilterOverrides::default()
        }),
        "account_portfolio_overrides=%5B%7B%22account_id%22%3A%22retirement%22%2C%22exclude_from_portfolio%22%3Afalse%7D%2C%7B%22account_id%22%3A%22taxable%20account%22%2C%22exclude_from_portfolio%22%3Atrue%7D%5D"
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
        tags: Vec::new(),
        subtags: Vec::new(),
        annotation: None,
        ignored_from_spending: false,
    }
}

#[test]
fn inclusive_transaction_query_requests_ignored_rows() {
    assert_eq!(
        transaction_query_string("2025-04-25", "2026-04-25", Some("local"), true),
        "start=2025-04-25&end=2026-04-25&tz=local&include_ignored=true"
    );
}

#[test]
fn transaction_query_omits_empty_timezone() {
    assert_eq!(
        transaction_query_string("2025-04-25", "2026-04-25", None, true),
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
        None,
        "",
        TransactionSortField::Amount,
        SortDirection::Asc,
        true,
    );
    let descending = filtered_transactions(
        &rows,
        None,
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
    card.tags = vec!["Dining".to_string()];
    card.subtags = vec!["Restaurants".to_string()];
    card.description = "Zulu".to_string();
    card.ignored_from_spending = true;

    let mut bank = transaction("bank", "-8.00", "posted");
    bank.account_name = "Bank".to_string();
    bank.tags = vec!["Bills".to_string()];
    bank.subtags = vec!["Utilities".to_string()];
    bank.description = "Alpha".to_string();

    let rows = vec![card, bank];

    assert_eq!(
        filtered_transactions(
            &rows,
            None,
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
            None,
            "",
            TransactionSortField::Tag,
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

fn annotation(tags: Option<Vec<&str>>, ignore_spending: Option<bool>) -> TransactionAnnotation {
    TransactionAnnotation {
        description: None,
        tags: tags.map(|tags| tags.into_iter().map(str::to_string).collect()),
        subtags: None,
        effective_date: None,
        ignore_spending,
    }
}

#[test]
fn annotation_ignore_detects_explicit_flag() {
    let mut row = transaction("flagged", "-12.50", "posted");
    row.annotation = Some(annotation(None, Some(true)));
    assert!(annotation_ignores_spending(&row));

    row.annotation = Some(annotation(None, Some(false)));
    assert!(!annotation_ignores_spending(&row));
}

#[test]
fn annotation_ignore_detects_ignore_spending_tags() {
    for tag in [
        "ignore_spending",
        "ignore-spending",
        "ignore:spending",
        "IGNORE_SPENDING",
    ] {
        let mut row = transaction("tagged", "-12.50", "posted");
        row.annotation = Some(annotation(Some(vec![tag]), None));
        assert!(annotation_ignores_spending(&row), "tag {tag} should ignore");
    }

    let mut row = transaction("dining", "-12.50", "posted");
    row.annotation = Some(annotation(Some(vec!["Dining"]), None));
    assert!(!annotation_ignores_spending(&row));
}

#[test]
fn visible_tags_hide_ignore_spending_control_tags() {
    let mut row = transaction("tagged", "-12.50", "posted");
    row.annotation = Some(annotation(Some(vec!["Dining", "ignore_spending"]), None));
    assert_eq!(visible_transaction_tags(&row), vec!["Dining".to_string()]);
}

#[test]
fn annotation_ignore_false_without_annotation() {
    let row = transaction("plain", "-12.50", "posted");
    assert!(!annotation_ignores_spending(&row));
    assert!(!rule_ignores_spending(&row));
}

#[test]
fn rule_ignore_distinguishes_config_level_exclusions() {
    // Excluded overall, but not via annotation -> config rule.
    let mut rule = transaction("rule", "-12.50", "posted");
    rule.ignored_from_spending = true;
    assert!(rule_ignores_spending(&rule));
    assert!(!annotation_ignores_spending(&rule));

    // Excluded overall AND via annotation -> not a rule-level exclusion.
    let mut annotated = transaction("annotated", "-12.50", "posted");
    annotated.ignored_from_spending = true;
    annotated.annotation = Some(annotation(None, Some(true)));
    assert!(!rule_ignores_spending(&annotated));
    assert!(annotation_ignores_spending(&annotated));

    // Excluded only because it is not spending-shaped -> not a rule-level exclusion.
    let mut credit = transaction("credit", "12.50", "posted");
    credit.ignored_from_spending = true;
    assert!(!rule_ignores_spending(&credit));

    let mut pending = transaction("pending", "-12.50", "pending");
    pending.ignored_from_spending = true;
    assert!(!rule_ignores_spending(&pending));
}

#[test]
fn transaction_subtag_prefers_annotation_value() {
    let mut row = transaction("annotated", "-12.50", "posted");
    row.subtags = vec!["Fallback".to_string()];
    row.annotation = Some(TransactionAnnotation {
        description: None,
        tags: None,
        subtags: Some(vec!["Coffee".to_string()]),
        effective_date: None,
        ignore_spending: None,
    });

    assert_eq!(
        transaction_subtags(&row).into_iter().next().as_deref(),
        Some("Coffee")
    );
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
        None,
        "",
        TransactionSortField::Date,
        SortDirection::Desc,
        false,
    );
    let with_ignored = filtered_transactions(
        &rows,
        None,
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
fn spending_transactions_filter_untagged_bucket() {
    let untagged = transaction("untagged", "-12.50", "posted");
    let mut tagged = transaction("tagged", "-8.00", "posted");
    tagged.tags = vec!["Dining".to_string()];
    let rows = vec![untagged, tagged];

    let filtered = filtered_transactions(
        &rows,
        Some("untagged"),
        None,
        "",
        TransactionSortField::Date,
        SortDirection::Desc,
        true,
    );

    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].id, "untagged");
}

#[test]
fn spending_transactions_filter_by_selected_period() {
    let mut january = transaction("january", "-12.50", "posted");
    january.timestamp = "2026-01-15T12:00:00+00:00".to_string();
    january.tags = vec!["Dining".to_string()];
    let mut february = transaction("february", "-8.00", "posted");
    february.timestamp = "2026-02-01T12:00:00+00:00".to_string();
    february.tags = vec!["Dining".to_string()];
    let rows = vec![january, february];

    let filtered = filtered_transactions(
        &rows,
        None,
        Some(("2026-01-01", "2026-01-31")),
        "",
        TransactionSortField::Date,
        SortDirection::Desc,
        true,
    );

    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].id, "january");
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
