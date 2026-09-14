use super::*;
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
