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
