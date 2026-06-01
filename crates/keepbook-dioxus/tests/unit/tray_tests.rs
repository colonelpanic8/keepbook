use super::*;

#[test]
fn overlay_status_tracks_runtime_state() {
    let runtime = TrayRuntime::default();
    assert_eq!(
        overlay_status_from_runtime(&runtime, None),
        TrayOverlayStatus::Idle
    );

    let runtime = TrayRuntime {
        status_text: "Refreshing prices...".to_string(),
        ..TrayRuntime::default()
    };
    assert_eq!(
        overlay_status_from_runtime(&runtime, None),
        TrayOverlayStatus::Syncing
    );

    let runtime = TrayRuntime {
        status_text: "Error: Price sync failed".to_string(),
        ..TrayRuntime::default()
    };
    assert_eq!(
        overlay_status_from_runtime(&runtime, None),
        TrayOverlayStatus::Error
    );

    let runtime = TrayRuntime::default();
    assert_eq!(
        overlay_status_from_runtime(&runtime, Some(&Err("offline".to_string()))),
        TrayOverlayStatus::Error
    );
}

#[test]
fn tray_left_click_activates_window_instead_of_menu() {
    assert!(!<KeepbookTrayItem as ksni::Tray>::MENU_ON_ACTIVATE);
}
