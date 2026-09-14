use std::sync::atomic::{AtomicUsize, Ordering};

use chrono::{TimeZone, Utc};

use super::*;
use crate::clock::FixedClock;
use crate::market_data::{MarketDataService, NullMarketDataStore};
use crate::models::{ConnectionConfig, LastSync, SyncStatus};
use crate::storage::MemoryStorage;
use crate::sync::{AccountListing, SyncResult, Synchronizer};

#[derive(Default)]
struct Calls {
    check_auth: AtomicUsize,
    login: AtomicUsize,
    sync: AtomicUsize,
}

struct MockSpec {
    name: &'static str,
    auth_status: Option<AuthStatus>,
    auth_required_for_sync: bool,
    login_fails: bool,
    sync_fails: bool,
    calls: Arc<Calls>,
}

impl MockSpec {
    fn new(auth_status: Option<AuthStatus>) -> Self {
        Self {
            name: "mock",
            auth_status,
            auth_required_for_sync: true,
            login_fails: false,
            sync_fails: false,
            calls: Arc::new(Calls::default()),
        }
    }
}

struct Mock {
    spec: Arc<MockSpec>,
}

#[async_trait::async_trait]
impl Synchronizer for Mock {
    fn name(&self) -> &str {
        self.spec.name
    }

    async fn sync(
        &self,
        connection: &mut Connection,
        _storage: &dyn Storage,
    ) -> Result<SyncResult> {
        self.spec.calls.sync.fetch_add(1, Ordering::SeqCst);
        if self.spec.sync_fails {
            anyhow::bail!("mock sync failed");
        }
        Ok(SyncResult {
            connection: connection.clone(),
            accounts: Vec::new(),
            account_listing: AccountListing::Complete,
            balances: Vec::new(),
            transactions: Vec::new(),
        })
    }

    fn interactive(&mut self) -> Option<&mut dyn InteractiveAuth> {
        self.spec.auth_status.as_ref()?;
        Some(self)
    }
}

#[async_trait::async_trait]
impl InteractiveAuth for Mock {
    fn auth_required_for_sync(&self) -> bool {
        self.spec.auth_required_for_sync
    }

    async fn check_auth(&self) -> Result<AuthStatus> {
        self.spec.calls.check_auth.fetch_add(1, Ordering::SeqCst);
        Ok(self
            .spec
            .auth_status
            .clone()
            .expect("check_auth on a non-interactive mock"))
    }

    async fn login(&mut self) -> Result<()> {
        self.spec.calls.login.fetch_add(1, Ordering::SeqCst);
        if self.spec.login_fails {
            anyhow::bail!("mock login failed");
        }
        Ok(())
    }
}

struct MockFactory {
    spec: Arc<MockSpec>,
    fail_create: bool,
}

#[async_trait::async_trait]
impl SynchronizerFactory for MockFactory {
    async fn create(
        &self,
        _connection: &Connection,
        _storage: &dyn Storage,
    ) -> Result<Box<dyn Synchronizer>> {
        if self.fail_create {
            anyhow::bail!("no synchronizer for this connection");
        }
        Ok(Box::new(Mock {
            spec: self.spec.clone(),
        }))
    }
}

struct CountingCommitter {
    actions: std::sync::Mutex<Vec<String>>,
}

impl AutoCommitter for CountingCommitter {
    fn maybe_commit(&self, action: &str) -> Result<()> {
        self.actions.lock().unwrap().push(action.to_string());
        Ok(())
    }
}

fn connection(name: &str, synchronizer: &str) -> Connection {
    Connection::new(ConnectionConfig {
        name: name.to_string(),
        synchronizer: synchronizer.to_string(),
        credentials: None,
        balance_staleness: None,
    })
}

fn now() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 3, 1, 12, 0, 0).unwrap()
}

struct Harness {
    storage: Arc<MemoryStorage>,
    service: SyncService,
    spec: Arc<MockSpec>,
    committer: Arc<CountingCommitter>,
}

fn harness(spec: MockSpec, prompter: FixedAuthPrompter, fail_create: bool) -> Harness {
    let storage = Arc::new(MemoryStorage::new());
    let spec = Arc::new(spec);
    let committer = Arc::new(CountingCommitter {
        actions: std::sync::Mutex::new(Vec::new()),
    });
    let market_data = MarketDataService::new(Arc::new(NullMarketDataStore), None);
    let context = SyncContext::new(
        storage.clone() as Arc<dyn Storage>,
        market_data,
        "USD".to_string(),
    )
    .with_auth_prompter(Arc::new(prompter))
    .with_auto_committer(committer.clone())
    .with_factory(Arc::new(MockFactory {
        spec: spec.clone(),
        fail_create,
    }))
    .with_clock(Arc::new(FixedClock::new(now())));

    Harness {
        storage,
        service: SyncService::new(context),
        spec,
        committer,
    }
}

#[tokio::test]
async fn a_valid_session_syncs_without_prompting() -> Result<()> {
    let h = harness(
        MockSpec::new(Some(AuthStatus::Valid)),
        FixedAuthPrompter::deny(),
        false,
    );
    let connection = connection("Mock", "mock");
    h.storage.save_connection(&connection).await?;

    let outcome = h.service.sync_connection(connection.id().as_str()).await?;

    assert!(matches!(outcome, SyncOutcome::Synced { .. }));
    assert_eq!(h.spec.calls.login.load(Ordering::SeqCst), 0);
    assert_eq!(h.spec.calls.sync.load(Ordering::SeqCst), 1);
    assert_eq!(
        *h.committer.actions.lock().unwrap(),
        vec![format!("sync connection {}", connection.id())]
    );
    Ok(())
}

#[tokio::test]
async fn declining_login_for_an_expired_session_reports_the_reason() -> Result<()> {
    let h = harness(
        MockSpec::new(Some(AuthStatus::Expired {
            reason: "token rotated".to_string(),
        })),
        FixedAuthPrompter::deny(),
        false,
    );
    let connection = connection("Mock", "mock");
    h.storage.save_connection(&connection).await?;

    match h.service.sync_connection(connection.id().as_str()).await? {
        SyncOutcome::AuthRequired { error, .. } => {
            assert_eq!(error, "Session expired: token rotated");
        }
        other => anyhow::bail!("unexpected outcome: {other:?}"),
    }

    assert_eq!(h.spec.calls.login.load(Ordering::SeqCst), 0);
    assert_eq!(h.spec.calls.sync.load(Ordering::SeqCst), 0);
    Ok(())
}

#[tokio::test]
async fn a_synchronizer_that_logs_in_during_sync_is_never_prompted() -> Result<()> {
    let mut spec = MockSpec::new(Some(AuthStatus::Missing));
    spec.auth_required_for_sync = false;
    let h = harness(spec, FixedAuthPrompter::deny(), false);
    let connection = connection("Mock", "mock");
    h.storage.save_connection(&connection).await?;

    let outcome = h.service.sync_connection(connection.id().as_str()).await?;

    assert!(matches!(outcome, SyncOutcome::Synced { .. }));
    assert_eq!(h.spec.calls.check_auth.load(Ordering::SeqCst), 1);
    assert_eq!(h.spec.calls.login.load(Ordering::SeqCst), 0);
    Ok(())
}

#[tokio::test]
async fn manual_connections_are_skipped_before_a_synchronizer_is_built() -> Result<()> {
    let h = harness(MockSpec::new(None), FixedAuthPrompter::deny(), true);
    let connection = connection("Cash", "manual");
    h.storage.save_connection(&connection).await?;

    let outcome = h.service.sync_connection(connection.id().as_str()).await?;

    assert!(matches!(outcome, SyncOutcome::SkippedManual { .. }));
    assert_eq!(h.spec.calls.sync.load(Ordering::SeqCst), 0);
    assert!(h.committer.actions.lock().unwrap().is_empty());
    Ok(())
}

#[tokio::test]
async fn sync_all_records_a_failure_per_connection_and_still_commits() -> Result<()> {
    let mut spec = MockSpec::new(None);
    spec.sync_fails = true;
    let h = harness(spec, FixedAuthPrompter::deny(), false);
    h.storage
        .save_connection(&connection("One", "mock"))
        .await?;
    h.storage
        .save_connection(&connection("Two", "manual"))
        .await?;

    let outcomes = h.service.sync_all().await?;

    assert_eq!(outcomes.len(), 2);
    assert_eq!(
        outcomes
            .iter()
            .filter(|o| matches!(o, SyncOutcome::Failed { error, .. } if error.contains("mock sync failed")))
            .count(),
        1
    );
    assert_eq!(
        outcomes
            .iter()
            .filter(|o| matches!(o, SyncOutcome::SkippedManual { .. }))
            .count(),
        1
    );
    assert_eq!(*h.committer.actions.lock().unwrap(), vec!["sync all"]);
    Ok(())
}

#[tokio::test]
async fn a_connection_synced_recently_enough_is_left_alone() -> Result<()> {
    let h = harness(MockSpec::new(None), FixedAuthPrompter::deny(), false);
    let mut connection = connection("Mock", "mock");
    connection.state.last_sync = Some(LastSync {
        at: now() - chrono::Duration::hours(1),
        status: SyncStatus::Success,
        error: None,
    });
    h.storage.save_connection(&connection).await?;

    let refresh = RefreshConfig {
        balance_staleness: std::time::Duration::from_secs(24 * 60 * 60),
        ..Default::default()
    };

    let outcome = h
        .service
        .sync_connection_if_stale(connection.id().as_str(), &refresh)
        .await?;
    assert!(matches!(outcome, SyncOutcome::SkippedNotStale { .. }));
    assert_eq!(h.spec.calls.sync.load(Ordering::SeqCst), 0);

    let outcomes = h.service.sync_all_if_stale(&refresh).await?;
    assert_eq!(outcomes.len(), 1);
    assert!(matches!(outcomes[0], SyncOutcome::SkippedNotStale { .. }));
    Ok(())
}

#[tokio::test]
async fn a_connection_past_its_staleness_threshold_is_synced() -> Result<()> {
    let h = harness(MockSpec::new(None), FixedAuthPrompter::deny(), false);
    let mut connection = connection("Mock", "mock");
    connection.state.last_sync = Some(LastSync {
        at: now() - chrono::Duration::days(30),
        status: SyncStatus::Success,
        error: None,
    });
    h.storage.save_connection(&connection).await?;

    let outcome = h
        .service
        .sync_connection_if_stale(connection.id().as_str(), &RefreshConfig::default())
        .await?;

    assert!(matches!(outcome, SyncOutcome::Synced { .. }));
    assert_eq!(h.spec.calls.sync.load(Ordering::SeqCst), 1);
    Ok(())
}

#[tokio::test]
async fn login_resolves_the_only_connection_for_a_synchronizer() -> Result<()> {
    let h = harness(
        MockSpec::new(Some(AuthStatus::Missing)),
        FixedAuthPrompter::deny(),
        false,
    );
    let mock = connection("Mock", "mock");
    h.storage.save_connection(&mock).await?;
    h.storage
        .save_connection(&connection("Cash", "manual"))
        .await?;

    let resolved = h.service.login("mock", None).await?;

    assert_eq!(resolved.id(), mock.id());
    assert_eq!(h.spec.calls.login.load(Ordering::SeqCst), 1);
    Ok(())
}

#[tokio::test]
async fn login_needs_a_name_when_a_synchronizer_has_several_connections() -> Result<()> {
    let h = harness(
        MockSpec::new(Some(AuthStatus::Missing)),
        FixedAuthPrompter::deny(),
        false,
    );
    let first = connection("Mock One", "mock");
    let second = connection("Mock Two", "mock");
    h.storage.save_connection(&first).await?;
    h.storage.save_connection(&second).await?;

    let err = h.service.login("mock", None).await.unwrap_err();
    assert!(
        err.to_string()
            .starts_with("Multiple mock connections found (2). Specify one:"),
        "unexpected error: {err}"
    );

    let resolved = h.service.login("mock", Some(second.id().as_str())).await?;
    assert_eq!(resolved.id(), second.id());
    Ok(())
}

#[tokio::test]
async fn login_rejects_an_unknown_or_mismatched_connection() -> Result<()> {
    let h = harness(
        MockSpec::new(Some(AuthStatus::Missing)),
        FixedAuthPrompter::deny(),
        false,
    );
    let manual = connection("Cash", "manual");
    h.storage.save_connection(&manual).await?;

    let err = h.service.login("mock", None).await.unwrap_err();
    assert_eq!(err.to_string(), "No mock connections found");

    let err = h
        .service
        .login("mock", Some(manual.id().as_str()))
        .await
        .unwrap_err();
    assert_eq!(
        err.to_string(),
        format!("mock connection not found: {}", manual.id())
    );
    Ok(())
}
