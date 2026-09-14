use super::*;
use crate::models::ConnectionConfig;

fn connection() -> Connection {
    Connection::new(ConnectionConfig {
        name: "Schwab".to_string(),
        synchronizer: "schwab".to_string(),
        credentials: None,
        balance_staleness: None,
    })
}

fn account_with_number(number: serde_json::Value) -> Account {
    let mut account = Account::new("Brokerage", Id::from_string("conn-1"));
    account.synchronizer_data = serde_json::json!({ "account_number": number });
    account
}

#[test]
fn account_numbers_keep_only_their_digits() {
    assert_eq!(
        SchwabSynchronizer::normalized_account_number("...1234"),
        Some("1234".to_string())
    );
    assert_eq!(
        SchwabSynchronizer::normalized_account_number("1234-5678"),
        Some("12345678".to_string())
    );
}

#[test]
fn account_numbers_without_digits_fall_back_to_the_trimmed_text() {
    assert_eq!(
        SchwabSynchronizer::normalized_account_number("  IRA  "),
        Some("IRA".to_string())
    );
    assert_eq!(SchwabSynchronizer::normalized_account_number("   "), None);
    assert_eq!(SchwabSynchronizer::normalized_account_number(""), None);
}

#[test]
fn stored_account_numbers_come_from_synchronizer_data() {
    assert_eq!(
        SchwabSynchronizer::stored_account_number(&account_with_number(serde_json::json!(
            "xxxx-1234"
        ))),
        Some("1234".to_string())
    );
    // A numeric (rather than string) value is not a usable account number.
    assert_eq!(
        SchwabSynchronizer::stored_account_number(&account_with_number(serde_json::json!(1234))),
        None
    );
    assert_eq!(
        SchwabSynchronizer::stored_account_number(&Account::new(
            "Brokerage",
            Id::from_string("conn-1")
        )),
        None
    );
}

#[test]
fn banking_transactions_prefer_the_display_number_over_the_rotating_account_id() {
    assert_eq!(
        SchwabSynchronizer::banking_selected_account_id("...9876", "fallback"),
        "9876"
    );
    assert_eq!(
        SchwabSynchronizer::banking_selected_account_id("", "fallback"),
        "fallback"
    );
    assert_eq!(
        SchwabSynchronizer::banking_selected_account_id("no digits", "fallback"),
        "fallback"
    );
}

fn synchronizer_with_session(
    dir: &tempfile::TempDir,
    connection: &Connection,
    session: Option<SessionData>,
) -> Result<SchwabSynchronizer> {
    let cache = SessionCache::with_path(dir.path())?;
    if let Some(session) = session {
        cache.set(connection.id().as_ref(), &session)?;
    }
    Ok(SchwabSynchronizer::with_session_cache(connection, cache))
}

#[tokio::test]
async fn auth_is_missing_without_a_cached_session() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let connection = connection();
    let sync = synchronizer_with_session(&dir, &connection, None)?;

    assert!(matches!(sync.check_auth().await?, AuthStatus::Missing));
    Ok(())
}

#[tokio::test]
async fn auth_is_missing_when_the_cached_session_has_no_token() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let connection = connection();
    let sync = synchronizer_with_session(
        &dir,
        &connection,
        Some(SessionData::new().with_cookie("sid", "abc")),
    )?;

    assert!(matches!(sync.check_auth().await?, AuthStatus::Missing));
    Ok(())
}

#[tokio::test]
async fn a_day_old_session_is_reported_expired_without_a_network_call() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let connection = connection();
    let mut session = SessionData::new().with_token("stale");
    session.captured_at = Some(Utc::now().timestamp() - 25 * 60 * 60);
    let sync = synchronizer_with_session(&dir, &connection, Some(session))?;

    match sync.check_auth().await? {
        AuthStatus::Expired { reason } => assert_eq!(reason, "Session is 25 hours old"),
        other => panic!("expected an expired session, got {other:?}"),
    }
    Ok(())
}
