use super::*;
use ksni::Tray;
use notify::event::{AccessKind, CreateKind, ModifyKind, RemoveKind};

#[test]
fn compute_next_delay_without_jitter_is_constant() {
    let interval = Duration::from_secs(1800);
    let jitter = Duration::ZERO;
    let delay = compute_next_delay(interval, jitter);
    assert_eq!(delay, interval);
}

#[test]
fn compute_next_delay_with_jitter_stays_in_range() {
    let interval = Duration::from_secs(600);
    let jitter = Duration::from_secs(120);

    for _ in 0..100 {
        let delay = compute_next_delay(interval, jitter);
        assert!(delay >= Duration::from_secs(480));
        assert!(delay <= Duration::from_secs(720));
    }
}

#[test]
fn parse_nonzero_duration_rejects_zero() {
    assert!(parse_nonzero_duration_arg("0s").is_err());
    assert_eq!(
        parse_nonzero_duration_arg("30s").expect("duration should parse"),
        Duration::from_secs(30)
    );
}

#[test]
fn fs_event_filter_includes_state_mutations() {
    assert!(should_refresh_for_fs_event_kind(&EventKind::Any));
    assert!(should_refresh_for_fs_event_kind(&EventKind::Create(
        CreateKind::Any
    )));
    assert!(should_refresh_for_fs_event_kind(&EventKind::Modify(
        ModifyKind::Any
    )));
    assert!(should_refresh_for_fs_event_kind(&EventKind::Remove(
        RemoveKind::Any
    )));
}

#[test]
fn fs_event_filter_excludes_access_events() {
    assert!(!should_refresh_for_fs_event_kind(&EventKind::Access(
        AccessKind::Any
    )));
}

#[test]
fn parse_sync_counts_handles_mixed_results() {
    let value = serde_json::json!({
        "total": 4,
        "results": [
            {"success": true},
            {"success": true, "skipped": true, "reason": "manual"},
            {"success": true, "skipped": true, "reason": "not stale"},
            {"success": false, "error": "boom"}
        ]
    });

    let counts = parse_sync_counts(&value);
    assert_eq!(counts.total, 4);
    assert_eq!(counts.synced, 1);
    assert_eq!(counts.skipped_manual, 1);
    assert_eq!(counts.skipped_not_stale, 1);
    assert_eq!(counts.failed, 1);
}

#[test]
fn format_tray_currency_uses_usd_symbol_by_default() {
    let display = keepbook::config::DisplayConfig::default();
    let formatted = format_tray_currency("1234.5", "USD", &display);
    assert_eq!(formatted, "$1234.5");
}

#[test]
fn format_tray_currency_appends_unknown_currency_code() {
    let display = keepbook::config::DisplayConfig::default();
    let formatted = format_tray_currency("1234.5", "CHF", &display);
    assert_eq!(formatted, "1234.5 CHF");
}

#[test]
fn format_history_change_for_tray_defaults_to_na() {
    assert_eq!(format_history_change_for_tray(None), "N/A");
    assert_eq!(format_history_change_for_tray(Some("N/A")), "N/A");
}

#[test]
fn format_history_change_for_tray_adds_sign_and_percent() {
    assert_eq!(format_history_change_for_tray(Some("3.25")), "+3.25%");
    assert_eq!(format_history_change_for_tray(Some("-1.50")), "-1.50%");
}

#[test]
fn recent_spending_is_not_rendered_as_submenu() {
    let (cmd_tx, _cmd_rx) = mpsc::unbounded_channel();
    let state = KeepbookTrayState {
        spending_lines: vec!["Last 7d: $42 (3 txns)".to_string()],
        ..KeepbookTrayState::default()
    };
    let tray = KeepbookTray::new(state, cmd_tx);

    let menu = tray.menu();
    assert!(
        menu.iter().any(|item| {
            matches!(
                item,
                MenuItem::Standard(StandardItem { label, .. }) if label == "Recent Spending"
            )
        }),
        "expected top-level standard item with label 'Recent Spending'"
    );
    assert!(
        !menu.iter().any(|item| {
            matches!(
                item,
                MenuItem::SubMenu(SubMenu { label, .. }) if label == "Recent Spending"
            )
        }),
        "did not expect 'Recent Spending' to be rendered as a submenu"
    );
}

#[test]
fn normalize_spending_windows_days_sorts_dedupes_and_drops_zero() {
    assert_eq!(
        normalize_spending_windows_days(&[30, 0, 365, 7, 30]),
        vec![7, 30, 365]
    );
}

#[test]
fn format_spending_window_label_uses_year_for_365_days() {
    assert_eq!(format_spending_window_label(7), "7d");
    assert_eq!(format_spending_window_label(365), "year");
    assert_eq!(format_spending_window_label(730), "2 years");
}

#[test]
fn dioxus_app_action_is_rendered_top_level() {
    let (cmd_tx, _cmd_rx) = mpsc::unbounded_channel();
    let tray = KeepbookTray::new(KeepbookTrayState::default(), cmd_tx);

    let menu = tray.menu();
    assert!(menu.iter().any(|item| {
        matches!(
            item,
            MenuItem::Standard(StandardItem { label, .. })
                if label == "Open Dioxus App"
        )
    }));
}

#[test]
fn build_portfolio_breakdown_lines_formats_account_values() {
    let mut config =
        ResolvedConfig::load_or_default(Path::new("/tmp/keepbook-test/keepbook.toml")).unwrap();
    config.display = keepbook::config::DisplayConfig {
        currency_decimals: Some(2),
        currency_grouping: true,
        currency_symbol: Some("$".to_string()),
        currency_fixed_decimals: true,
    };
    let snapshot = keepbook::portfolio::PortfolioSnapshot {
        as_of_date: NaiveDate::from_ymd_opt(2026, 4, 24).unwrap(),
        currency: "USD".to_string(),
        total_value: "1250".to_string(),
        total_cost_basis: None,
        total_unrealized_gain: None,
        prospective_capital_gains_tax: None,
        valuation_scenario: None,
        by_asset: None,
        by_account: Some(vec![
            keepbook::portfolio::AccountSummary {
                account_id: "acct-1".to_string(),
                account_name: "Checking".to_string(),
                connection_name: "Bank".to_string(),
                value_in_base: Some("1000".to_string()),
            },
            keepbook::portfolio::AccountSummary {
                account_id: "acct-2".to_string(),
                account_name: "Brokerage".to_string(),
                connection_name: "Broker".to_string(),
                value_in_base: None,
            },
        ]),
    };

    let lines = build_portfolio_breakdown_lines(&snapshot, &config);

    assert_eq!(lines[0], "Total: $1,250.00");
    assert_eq!(lines[1], "Bank / Checking: $1,000.00");
    assert_eq!(lines[2], "Broker / Brokerage: unpriced");
}

#[test]
fn portfolio_breakdown_is_rendered_as_submenu() {
    let (cmd_tx, _cmd_rx) = mpsc::unbounded_channel();
    let state = KeepbookTrayState {
        portfolio_breakdown_lines: vec![
            "Total: $42.00".to_string(),
            "Bank / Checking: $42.00".to_string(),
        ],
        ..KeepbookTrayState::default()
    };
    let tray = KeepbookTray::new(state, cmd_tx);

    let menu = tray.menu();
    assert!(menu.iter().any(|item| {
        matches!(
            item,
            MenuItem::SubMenu(SubMenu { label, submenu, .. })
                if label == "Portfolio Breakdown" && submenu.len() == 2
        )
    }));
}
