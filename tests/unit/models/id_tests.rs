use super::*;

#[test]
fn test_from_external_is_deterministic() {
    let first = Id::from_external("schwab-account-123");
    let second = Id::from_external("schwab-account-123");
    assert_eq!(first, second);
}

#[test]
fn test_from_external_differs_for_different_inputs() {
    let first = Id::from_external("schwab-account-123");
    let second = Id::from_external("schwab-account-456");
    assert_ne!(first, second);
}

#[test]
fn test_from_external_is_path_safe() {
    let id = Id::from_external("weird/account/value");
    assert!(!id.as_str().contains('/'));
}

#[test]
fn test_from_string_keeps_value() {
    let id = Id::from_string("account-id-123");
    assert_eq!(id.as_str(), "account-id-123");
}

#[test]
fn test_from_string_checked_rejects_unsafe_values() {
    assert!(Id::from_string_checked("../escape").is_err());
    assert!(Id::from_string_checked("..").is_err());
    assert!(Id::from_string_checked(".").is_err());
    assert!(Id::from_string_checked("foo/bar").is_err());
    assert!(Id::from_string_checked("foo\\bar").is_err());
    assert!(Id::from_string_checked("bad\0id").is_err());
}
