use super::*;
use ksni::Tray;
use tokio::sync::mpsc;

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
