use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use anyhow::Result;
use keepbook::market_data::{MarketDataService, NullMarketDataStore};
use keepbook::models::{Connection, ConnectionConfig};
use keepbook::storage::{JsonFileStorage, Storage};
use keepbook::sync::{
    AuthStatus, FixedAuthPrompter, NoopAutoCommitter, SyncContext, SyncOutcome, SyncResult,
    SyncService, Synchronizer, SynchronizerFactory,
};
use tempfile::TempDir;

struct MockState {
    login_called: AtomicBool,
    sync_called: AtomicBool,
    auth_status: AuthStatus,
}

struct InteractiveMock {
    state: Arc<MockState>,
}

#[async_trait::async_trait]
impl Synchronizer for InteractiveMock {
    fn name(&self) -> &str {
        "mock"
    }

    async fn sync(
        &self,
        connection: &mut Connection,
        _storage: &dyn keepbook::storage::Storage,
    ) -> Result<SyncResult> {
        self.state.sync_called.store(true, Ordering::SeqCst);
        Ok(SyncResult {
            connection: connection.clone(),
            accounts: Vec::new(),
            balances: Vec::new(),
            transactions: Vec::new(),
        })
    }

    fn interactive(&mut self) -> Option<&mut dyn keepbook::sync::InteractiveAuth> {
        Some(self)
    }
}

#[async_trait::async_trait]
impl keepbook::sync::InteractiveAuth for InteractiveMock {
    async fn check_auth(&self) -> Result<AuthStatus> {
        Ok(self.state.auth_status.clone())
    }

    async fn login(&mut self) -> Result<()> {
        self.state.login_called.store(true, Ordering::SeqCst);
        Ok(())
    }
}

struct TestFactory {
    state: Arc<MockState>,
}

#[async_trait::async_trait]
impl SynchronizerFactory for TestFactory {
    async fn create(
        &self,
        _connection: &Connection,
        _storage: &dyn keepbook::storage::Storage,
    ) -> Result<Box<dyn Synchronizer>> {
        Ok(Box::new(InteractiveMock {
            state: self.state.clone(),
        }))
    }
}

async fn setup_service(
    storage: &JsonFileStorage,
    prompter: FixedAuthPrompter,
    state: Arc<MockState>,
) -> SyncService {
    let market_data = MarketDataService::new(Arc::new(NullMarketDataStore), None);
    let context = SyncContext::new(
        Arc::new(storage.clone()) as Arc<dyn Storage>,
        market_data,
        "USD".to_string(),
    )
    .with_auth_prompter(Arc::new(prompter))
    .with_auto_committer(Arc::new(NoopAutoCommitter))
    .with_factory(Arc::new(TestFactory { state }));

    SyncService::new(context)
}

async fn persist_connection(storage: &JsonFileStorage, connection: &Connection) -> Result<()> {
    storage
        .save_connection_config(connection.id(), &connection.config)
        .await?;
    storage.save_connection(connection).await?;
    Ok(())
}

#[tokio::test]
async fn sync_service_auth_decline_skips_sync() -> Result<()> {
    let dir = TempDir::new()?;
    let storage = JsonFileStorage::new(dir.path());

    let connection = Connection::new(ConnectionConfig {
        name: "Mock".to_string(),
        synchronizer: "mock".to_string(),
        credentials: None,
        balance_staleness: None,
    });
    persist_connection(&storage, &connection).await?;

    let state = Arc::new(MockState {
        login_called: AtomicBool::new(false),
        sync_called: AtomicBool::new(false),
        auth_status: AuthStatus::Missing,
    });

    let service = setup_service(&storage, FixedAuthPrompter::deny(), state.clone()).await;
    let outcome = service.sync_connection(connection.id().as_ref()).await?;

    match outcome {
        SyncOutcome::AuthRequired { .. } => {}
        other => anyhow::bail!("unexpected outcome: {:?}", other),
    }

    assert!(!state.login_called.load(Ordering::SeqCst));
    assert!(!state.sync_called.load(Ordering::SeqCst));

    Ok(())
}

#[tokio::test]
async fn sync_service_auth_accepts_and_syncs() -> Result<()> {
    let dir = TempDir::new()?;
    let storage = JsonFileStorage::new(dir.path());

    let connection = Connection::new(ConnectionConfig {
        name: "Mock".to_string(),
        synchronizer: "mock".to_string(),
        credentials: None,
        balance_staleness: None,
    });
    persist_connection(&storage, &connection).await?;

    let state = Arc::new(MockState {
        login_called: AtomicBool::new(false),
        sync_called: AtomicBool::new(false),
        auth_status: AuthStatus::Missing,
    });

    let service = setup_service(&storage, FixedAuthPrompter::allow(), state.clone()).await;
    let outcome = service.sync_connection(connection.id().as_ref()).await?;

    match outcome {
        SyncOutcome::Synced { .. } => {}
        other => anyhow::bail!("unexpected outcome: {:?}", other),
    }

    assert!(state.login_called.load(Ordering::SeqCst));
    assert!(state.sync_called.load(Ordering::SeqCst));

    Ok(())
}
