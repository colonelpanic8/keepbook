use super::logic::*;
use super::*;

#[test]
fn navigation_styles_keep_compact_header_sticky_to_the_viewport() {
    assert!(
        APP_CSS.contains(
            "html,\nbody {\n  max-width: 100%;\n  overflow-x: clip;\n}"
        ),
        "the document must clip horizontal overflow without becoming a sticky-positioning container"
    );
    assert!(
        APP_CSS.contains(
            ".shell {\n  max-width: 100%;\n  min-height: 100vh;\n  min-height: 100dvh;\n  overflow-x: clip;\n}"
        ),
        "the shell must not capture the compact header's sticky positioning"
    );
    assert!(
        APP_CSS.contains("position: sticky;\n    top: 0;"),
        "the compact navigation header must remain sticky at the top"
    );
    assert!(
        APP_CSS
            .matches("var(--compact-header-padding-block)")
            .count()
            == 2,
        "compact navigation padding must have one state-independent vertical definition"
    );
    assert!(
        APP_CSS
            .matches("var(--compact-header-padding-inline)")
            .count()
            == 2,
        "compact navigation padding must have one state-independent horizontal definition"
    );
    assert!(
        APP_CSS.contains("height: calc(100dvh - 100%);\n    left: 0;"),
        "the open drawer must derive its height from the unchanged compact header"
    );
}

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
        window_decorations: keepbook_server::WindowDecorationsConfig::Auto,
    }));
    assert!(desktop_window_visible(DesktopStartupOptions {
        start_minimized_to_tray: false,
        window_decorations: keepbook_server::WindowDecorationsConfig::Auto,
    }));
}

#[cfg(feature = "desktop")]
#[test]
fn desktop_window_builder_respects_explicit_window_decoration_setting() {
    let window_builder = desktop_window_builder(DesktopStartupOptions {
        start_minimized_to_tray: false,
        window_decorations: keepbook_server::WindowDecorationsConfig::Hidden,
    });

    assert_eq!(window_builder.window.title, "Keepbook");
    assert!(!window_builder.window.decorations);
    assert!(window_builder.window.visible);

    let window_builder = desktop_window_builder(DesktopStartupOptions {
        start_minimized_to_tray: false,
        window_decorations: keepbook_server::WindowDecorationsConfig::System,
    });
    assert!(window_builder.window.decorations);
}

#[cfg(all(feature = "desktop", target_os = "linux"))]
fn desktop_environment<'a>(
    entries: &'a [(&'a str, &'a str)],
) -> impl FnMut(&str) -> Option<std::ffi::OsString> + 'a {
    move |name| {
        entries
            .iter()
            .find(|(key, _)| *key == name)
            .map(|(_, value)| std::ffi::OsString::from(value))
    }
}

#[cfg(all(feature = "desktop", target_os = "linux"))]
#[test]
fn auto_window_decorations_are_hidden_only_on_hyprland() {
    assert!(should_disable_window_decorations_for(
        keepbook_server::WindowDecorationsConfig::Auto,
        desktop_environment(&[("HYPRLAND_INSTANCE_SIGNATURE", "abc123")]),
    ));
    assert!(should_disable_window_decorations_for(
        keepbook_server::WindowDecorationsConfig::Auto,
        desktop_environment(&[("XDG_CURRENT_DESKTOP", "KDE:Hyprland")]),
    ));
    assert!(!should_disable_window_decorations_for(
        keepbook_server::WindowDecorationsConfig::Auto,
        desktop_environment(&[("XDG_CURRENT_DESKTOP", "KDE")]),
    ));
}

#[cfg(all(feature = "desktop", target_os = "linux"))]
#[test]
fn explicit_window_decoration_overrides_take_precedence() {
    assert!(should_disable_window_decorations_for(
        keepbook_server::WindowDecorationsConfig::Hidden,
        desktop_environment(&[]),
    ));
    assert!(!should_disable_window_decorations_for(
        keepbook_server::WindowDecorationsConfig::System,
        desktop_environment(&[("XDG_CURRENT_DESKTOP", "Hyprland")]),
    ));
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
fn spending_tooltip_expands_for_long_text_and_stays_inside_chart() {
    let title = "2026-07 · Restaurants and entertainment";
    let detail = "$12,345.67 category · $98,765.43 month total";

    let (width, left_center) = spending_tooltip_layout(title, detail, 20.0, 720.0);
    assert!(width > 240.0);
    assert_eq!(left_center - width / 2.0, 8.0);

    let (same_width, right_center) = spending_tooltip_layout(title, detail, 700.0, 720.0);
    assert_eq!(same_width, width);
    assert_eq!(right_center + width / 2.0, 712.0);
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
fn current_snapshot_replaces_utc_tomorrow_history_point_on_local_today() {
    let history = History {
        currency: "USD".to_string(),
        points: vec![
            HistoryPoint {
                date: "2026-07-13".to_string(),
                total_value: "100".to_string(),
                percentage_change_from_previous: None,
            },
            HistoryPoint {
                date: "2026-07-14".to_string(),
                total_value: "110".to_string(),
                percentage_change_from_previous: None,
            },
            HistoryPoint {
                date: "2026-07-15".to_string(),
                total_value: "120".to_string(),
                percentage_change_from_previous: None,
            },
        ],
        summary: None,
    };

    assert_eq!(
        history_data_points_with_current_snapshot(&history, "2026-07-14", 125.0),
        vec![point("2026-07-13", 100.0), point("2026-07-14", 125.0)]
    );
}

#[test]
fn current_snapshot_appends_local_today_when_history_has_no_today_point() {
    let history = History {
        currency: "USD".to_string(),
        points: vec![HistoryPoint {
            date: "2026-07-13".to_string(),
            total_value: "100".to_string(),
            percentage_change_from_previous: None,
        }],
        summary: None,
    };

    assert_eq!(
        history_data_points_with_current_snapshot(&history, "2026-07-14", 125.0),
        vec![point("2026-07-13", 100.0), point("2026-07-14", 125.0)]
    );
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

fn asset_entry(
    asset: serde_json::Value,
    asset_id: &str,
    liability: bool,
    total_amount: &str,
    value_in_base: Option<&str>,
    changes: AssetChanges,
) -> AssetBreakdownEntry {
    AssetBreakdownEntry {
        asset,
        asset_id: asset_id.to_string(),
        liability,
        total_amount: total_amount.to_string(),
        price: None,
        price_date: None,
        price_updated_at: None,
        amount_last_checked_at: None,
        amount_last_changed_at: None,
        value_in_base: value_in_base.map(str::to_string),
        changes,
        holdings: Vec::new(),
    }
}

fn compare_asset_entries(
    a: &AssetBreakdownEntry,
    b: &AssetBreakdownEntry,
    field: AssetSortField,
    direction: SortDirection,
) -> std::cmp::Ordering {
    compare_asset_entries_with_change_metric(a, b, field, direction, false)
}

fn day_change(absolute: &str, percentage: Option<&str>) -> AssetChanges {
    AssetChanges {
        day: Some(AssetChange {
            absolute: absolute.to_string(),
            percentage: percentage.map(str::to_string),
        }),
        ..AssetChanges::default()
    }
}

#[test]
fn assets_query_includes_account_overrides_but_never_latent_tax() {
    assert_eq!(
        assets_query_string(
            FilterOverrides {
                include_latent_capital_gains_tax: Some(true),
                ..FilterOverrides::default()
            },
            false
        ),
        ""
    );
    assert_eq!(
        assets_query_string(FilterOverrides {
            include_latent_capital_gains_tax: Some(true),
            account_portfolio_exclusions: vec![AccountPortfolioExclusionOverride {
                account_id: "acct-1".to_string(),
                exclude_from_portfolio: true,
            }],
        }, false),
        "account_portfolio_overrides=%5B%7B%22account_id%22%3A%22acct-1%22%2C%22exclude_from_portfolio%22%3Atrue%7D%5D"
    );
}

#[test]
fn asset_breakdown_deserializes_with_omitted_optional_fields() {
    let breakdown: AssetBreakdown = serde_json::from_value(serde_json::json!({
        "as_of_date": "2026-07-17",
        "currency": "USD",
        "change_mode": "price_only",
        "total_value": "1000.5",
        "assets": [
            {
                "asset": {"type": "equity", "ticker": "AAPL", "exchange": "NASDAQ"},
                "asset_id": "equity/AAPL",
                "liability": false,
                "total_amount": "10",
                "price": "100.05",
                "price_date": "2026-07-16",
                "price_updated_at": "2026-07-16T21:05:00+00:00",
                "amount_last_checked_at": "2026-07-17T09:00:00+00:00",
                "amount_last_changed_at": "2026-07-15T09:00:00+00:00",
                "value_in_base": "1000.5",
                "changes": {"day": {"absolute": "50", "percentage": "5.26"}},
                "holdings": [
                    {
                        "account_id": "acct-1",
                        "account_name": "Brokerage",
                        "amount": "10",
                        "balance_date": "2026-07-15"
                    }
                ]
            },
            {
                "asset": {"type": "manual_value", "name": "House", "currency": "USD"},
                "asset_id": "manual_value/House",
                "liability": false,
                "total_amount": "1",
                "changes": {},
                "holdings": []
            }
        ]
    }))
    .expect("asset breakdown should deserialize");

    assert_eq!(breakdown.assets.len(), 2);
    let priced = &breakdown.assets[0];
    assert_eq!(priced.price.as_deref(), Some("100.05"));
    assert_eq!(
        priced.price_updated_at.as_deref(),
        Some("2026-07-16T21:05:00+00:00")
    );
    assert_eq!(priced.holdings[0].connection_name, None);
    assert_eq!(priced.holdings[0].value_in_base, None);
    let unpriced = &breakdown.assets[1];
    assert_eq!(unpriced.value_in_base, None);
    assert_eq!(unpriced.changes, AssetChanges::default());
}

#[test]
fn asset_display_names_cover_all_asset_kinds() {
    let currency = serde_json::json!({"type": "currency", "iso_code": "USD"});
    let equity = serde_json::json!({"type": "equity", "ticker": "AAPL", "exchange": "NASDAQ"});
    let crypto = serde_json::json!({"type": "crypto", "symbol": "ETH", "network": "mainnet"});
    let manual = serde_json::json!({"type": "manual_value", "name": "House", "currency": "USD"});
    let unknown = serde_json::json!({"type": "mystery"});

    assert_eq!(asset_display_name(&currency, "currency/USD"), "USD");
    assert_eq!(asset_display_name(&equity, "equity/AAPL"), "AAPL");
    assert_eq!(asset_display_name(&crypto, "crypto/ETH"), "ETH");
    assert_eq!(asset_display_name(&manual, "manual_value/House"), "House");
    assert_eq!(asset_display_name(&unknown, "fallback-id"), "fallback-id");

    assert_eq!(asset_secondary_text(&currency), None);
    assert_eq!(asset_secondary_text(&equity).as_deref(), Some("NASDAQ"));
    assert_eq!(asset_secondary_text(&crypto).as_deref(), Some("mainnet"));
    assert_eq!(
        asset_secondary_text(&serde_json::json!({"type": "equity", "ticker": "AAPL"})),
        None
    );

    assert_eq!(asset_kind_label(&currency), "Currency");
    assert_eq!(asset_kind_label(&equity), "Equity");
    assert_eq!(asset_kind_label(&crypto), "Crypto");
    assert_eq!(asset_kind_label(&manual), "Manual");
    assert_eq!(asset_kind_label(&unknown), "Asset");
}

#[test]
fn signed_percent_uses_signed_money_sign_convention() {
    assert_eq!(format_signed_percent(5.26), "+5.26%");
    assert_eq!(format_signed_percent(-3.1), "-3.1%");
    assert_eq!(format_signed_percent(0.0), "+0%");
}

#[test]
fn change_value_class_keeps_zero_neutral() {
    assert_eq!(change_value_class(4.2), "change-positive");
    assert_eq!(change_value_class(-4.2), "change-negative");
    assert_eq!(change_value_class(0.0), "");
}

#[test]
fn asset_amount_display_trims_trailing_zeros() {
    assert_eq!(format_asset_amount("10.500000"), "10.5");
    assert_eq!(format_asset_amount("-3.0"), "-3");
    assert_eq!(format_asset_amount("not-a-number"), "not-a-number");
}

#[test]
fn asset_expansion_keys_distinguish_liability_rows() {
    let asset = asset_entry(
        serde_json::json!({"type": "currency", "iso_code": "USD"}),
        "currency/USD",
        false,
        "10",
        Some("10"),
        AssetChanges::default(),
    );
    let liability = asset_entry(
        serde_json::json!({"type": "currency", "iso_code": "USD"}),
        "currency/USD",
        true,
        "-4",
        Some("-4"),
        AssetChanges::default(),
    );
    assert_ne!(asset_expansion_key(&asset), asset_expansion_key(&liability));
}

#[test]
fn default_asset_sort_directions_match_field_semantics() {
    assert_eq!(
        default_asset_sort_direction(AssetSortField::Name),
        SortDirection::Asc
    );
    for field in [
        AssetSortField::Amount,
        AssetSortField::AmountChecked,
        AssetSortField::AmountChanged,
        AssetSortField::PriceUpdated,
        AssetSortField::Value,
        AssetSortField::DayChange,
        AssetSortField::WeekChange,
        AssetSortField::MonthChange,
        AssetSortField::YearChange,
    ] {
        assert_eq!(default_asset_sort_direction(field), SortDirection::Desc);
    }
}

#[test]
fn asset_sort_options_round_trip_and_cover_every_field() {
    let expected = [
        AssetSortField::Name,
        AssetSortField::Amount,
        AssetSortField::Value,
        AssetSortField::PriceUpdated,
        AssetSortField::DayChange,
        AssetSortField::WeekChange,
        AssetSortField::MonthChange,
        AssetSortField::YearChange,
        AssetSortField::AmountChecked,
        AssetSortField::AmountChanged,
    ];

    assert_eq!(AssetSortField::OPTIONS, expected);
    for field in AssetSortField::OPTIONS {
        assert_eq!(AssetSortField::from_value(field.value()), Some(field));
        assert!(!field.label().is_empty());
    }
    assert_eq!(AssetSortField::from_value("price"), None);
}

#[test]
fn asset_timestamp_fields_sort_latest_first_and_missing_last() {
    let mut old = asset_entry(
        serde_json::json!({"type": "equity", "ticker": "OLD"}),
        "equity/OLD",
        false,
        "1",
        Some("1"),
        AssetChanges::default(),
    );
    old.price_updated_at = Some("2026-07-10T10:00:00+00:00".to_string());
    let mut recent = asset_entry(
        serde_json::json!({"type": "equity", "ticker": "RECENT"}),
        "equity/RECENT",
        false,
        "1",
        Some("1"),
        AssetChanges::default(),
    );
    recent.price_updated_at = Some("2026-07-20T10:00:00+00:00".to_string());
    let missing = asset_entry(
        serde_json::json!({"type": "currency", "iso_code": "USD"}),
        "currency/USD",
        false,
        "1",
        Some("1"),
        AssetChanges::default(),
    );
    let mut entries = [old, missing, recent];

    entries.sort_by(|a, b| {
        compare_asset_entries_with_change_metric(
            a,
            b,
            AssetSortField::PriceUpdated,
            SortDirection::Desc,
            false,
        )
    });

    assert_eq!(
        entries
            .iter()
            .map(|entry| entry.asset_id.as_str())
            .collect::<Vec<_>>(),
        vec!["equity/RECENT", "equity/OLD", "currency/USD"]
    );
}

#[test]
fn asset_change_sort_switches_between_percentage_and_absolute() {
    let percentage_winner = asset_entry(
        serde_json::json!({"type": "equity", "ticker": "PCT"}),
        "equity/PCT",
        false,
        "1",
        Some("1"),
        day_change("10", Some("20")),
    );
    let absolute_winner = asset_entry(
        serde_json::json!({"type": "equity", "ticker": "ABS"}),
        "equity/ABS",
        false,
        "1",
        Some("1"),
        day_change("100", Some("5")),
    );
    let mut entries = [percentage_winner, absolute_winner];

    entries.sort_by(|a, b| {
        compare_asset_entries_with_change_metric(
            a,
            b,
            AssetSortField::DayChange,
            SortDirection::Desc,
            false,
        )
    });
    assert_eq!(entries[0].asset_id, "equity/PCT");

    entries.sort_by(|a, b| {
        compare_asset_entries_with_change_metric(
            a,
            b,
            AssetSortField::DayChange,
            SortDirection::Desc,
            true,
        )
    });
    assert_eq!(entries[0].asset_id, "equity/ABS");
}

#[test]
fn asset_value_sort_uses_absolute_value_and_keeps_unpriced_last() {
    let mut entries = [
        asset_entry(
            serde_json::json!({"type": "manual_value", "name": "House", "currency": "USD"}),
            "manual_value/House",
            false,
            "1",
            None,
            AssetChanges::default(),
        ),
        asset_entry(
            serde_json::json!({"type": "currency", "iso_code": "USD"}),
            "currency/USD",
            true,
            "-900",
            Some("-900"),
            AssetChanges::default(),
        ),
        asset_entry(
            serde_json::json!({"type": "equity", "ticker": "AAPL"}),
            "equity/AAPL",
            false,
            "5",
            Some("500"),
            AssetChanges::default(),
        ),
    ];

    entries.sort_by(|a, b| compare_asset_entries(a, b, AssetSortField::Value, SortDirection::Desc));
    let descending: Vec<&str> = entries.iter().map(|e| e.asset_id.as_str()).collect();
    assert_eq!(
        descending,
        vec!["currency/USD", "equity/AAPL", "manual_value/House"]
    );

    entries.sort_by(|a, b| compare_asset_entries(a, b, AssetSortField::Value, SortDirection::Asc));
    let ascending: Vec<&str> = entries.iter().map(|e| e.asset_id.as_str()).collect();
    assert_eq!(
        ascending,
        vec!["equity/AAPL", "currency/USD", "manual_value/House"]
    );
}

#[test]
fn asset_change_sort_uses_percentage_and_keeps_missing_last() {
    let mut entries = [
        asset_entry(
            serde_json::json!({"type": "equity", "ticker": "AAA"}),
            "equity/AAA",
            false,
            "1",
            Some("100"),
            AssetChanges::default(),
        ),
        asset_entry(
            serde_json::json!({"type": "equity", "ticker": "BBB"}),
            "equity/BBB",
            false,
            "1",
            Some("100"),
            day_change("120", None),
        ),
        asset_entry(
            serde_json::json!({"type": "equity", "ticker": "CCC"}),
            "equity/CCC",
            false,
            "1",
            Some("100"),
            day_change("-3", Some("-3.1")),
        ),
        asset_entry(
            serde_json::json!({"type": "equity", "ticker": "DDD"}),
            "equity/DDD",
            false,
            "1",
            Some("100"),
            day_change("5", Some("5.26")),
        ),
    ];

    entries.sort_by(|a, b| {
        compare_asset_entries(a, b, AssetSortField::DayChange, SortDirection::Desc)
    });
    let ids: Vec<&str> = entries.iter().map(|e| e.asset_id.as_str()).collect();
    assert_eq!(
        ids,
        vec!["equity/DDD", "equity/CCC", "equity/AAA", "equity/BBB"]
    );
}

#[test]
fn asset_name_sort_is_case_insensitive_with_stable_tie_break() {
    let mut entries = [
        asset_entry(
            serde_json::json!({"type": "equity", "ticker": "msft"}),
            "equity/msft",
            false,
            "1",
            Some("1"),
            AssetChanges::default(),
        ),
        asset_entry(
            serde_json::json!({"type": "equity", "ticker": "AAPL"}),
            "equity/AAPL",
            false,
            "1",
            Some("1"),
            AssetChanges::default(),
        ),
        asset_entry(
            serde_json::json!({"type": "currency", "iso_code": "USD"}),
            "currency/USD",
            true,
            "-1",
            Some("-1"),
            AssetChanges::default(),
        ),
        asset_entry(
            serde_json::json!({"type": "currency", "iso_code": "USD"}),
            "currency/USD",
            false,
            "2",
            Some("2"),
            AssetChanges::default(),
        ),
    ];

    entries.sort_by(|a, b| compare_asset_entries(a, b, AssetSortField::Name, SortDirection::Asc));
    let ids: Vec<(String, bool)> = entries
        .iter()
        .map(|e| (e.asset_id.clone(), e.liability))
        .collect();
    assert_eq!(
        ids,
        vec![
            ("equity/AAPL".to_string(), false),
            ("equity/msft".to_string(), false),
            ("currency/USD".to_string(), false),
            ("currency/USD".to_string(), true),
        ]
    );
}

#[test]
fn assets_query_strings_parse_into_server_assets_query() {
    let empty = serde_urlencoded::from_str::<keepbook_server::AssetsQuery>("")
        .expect("empty assets query should parse");
    assert!(empty.date.is_none());
    assert!(empty.account_portfolio_overrides.is_none());
    assert!(!empty.include_amount_changes);

    let query = assets_query_string(
        FilterOverrides {
            include_latent_capital_gains_tax: None,
            account_portfolio_exclusions: vec![AccountPortfolioExclusionOverride {
                account_id: "acct-1".to_string(),
                exclude_from_portfolio: true,
            }],
        },
        false,
    );
    let parsed = serde_urlencoded::from_str::<keepbook_server::AssetsQuery>(&query)
        .expect("assets query with overrides should parse");
    assert_eq!(
        parsed.account_portfolio_overrides.as_deref(),
        Some(r#"[{"account_id":"acct-1","exclude_from_portfolio":true}]"#)
    );

    let combined = assets_query_string(FilterOverrides::default(), true);
    let parsed = serde_urlencoded::from_str::<keepbook_server::AssetsQuery>(&combined)
        .expect("combined change mode should parse");
    assert!(parsed.include_amount_changes);
}
