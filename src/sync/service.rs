use std::sync::Arc;

use crate::clock::{Clock, SystemClock};
use crate::config::RefreshConfig;
use crate::git::{try_auto_commit, AutoCommitOutcome};
use crate::market_data::MarketDataService;
use crate::models::Connection;
use crate::staleness::{check_balance_staleness_at, resolve_balance_staleness};
use crate::storage::{find_account, find_connection, Storage};
use anyhow::{Context, Result};

use super::{
    AuthStatus, InteractiveAuth, PriceRefreshResult, SyncOrchestrator, SyncWithPricesResult,
};
use super::{DefaultSynchronizerFactory, SynchronizerFactory};

pub trait AuthPrompter: Send + Sync {
    fn confirm_login(&self, prompt: &str) -> Result<bool>;
}

#[derive(Debug, Clone)]
pub struct FixedAuthPrompter {
    allow: bool,
}

impl FixedAuthPrompter {
    pub fn allow() -> Self {
        Self { allow: true }
    }

    pub fn deny() -> Self {
        Self { allow: false }
    }
}

impl AuthPrompter for FixedAuthPrompter {
    fn confirm_login(&self, _prompt: &str) -> Result<bool> {
        Ok(self.allow)
    }
}

pub trait AutoCommitter: Send + Sync {
    fn maybe_commit(&self, action: &str);
}

#[derive(Debug, Clone, Default)]
pub struct NoopAutoCommitter;

impl AutoCommitter for NoopAutoCommitter {
    fn maybe_commit(&self, _action: &str) {}
}

#[derive(Debug, Clone)]
pub struct GitAutoCommitter {
    data_dir: std::path::PathBuf,
    auto_commit: bool,
    auto_push: bool,
}

impl GitAutoCommitter {
    pub fn new(
        data_dir: impl Into<std::path::PathBuf>,
        auto_commit: bool,
        auto_push: bool,
    ) -> Self {
        Self {
            data_dir: data_dir.into(),
            auto_commit,
            auto_push,
        }
    }
}

impl AutoCommitter for GitAutoCommitter {
    fn maybe_commit(&self, action: &str) {
        if !self.auto_commit {
            return;
        }

        match try_auto_commit(&self.data_dir, action, self.auto_push) {
            Ok(AutoCommitOutcome::Committed) => {
                tracing::info!("Auto-committed keepbook data");
            }
            Ok(AutoCommitOutcome::SkippedNoChanges) => {
                tracing::debug!("Auto-commit skipped: no changes");
            }
            Ok(AutoCommitOutcome::SkippedNotRepo { reason }) => {
                tracing::warn!("Auto-commit enabled but skipped: {reason}");
            }
            Err(err) => {
                tracing::warn!(error = %err, "Auto-commit failed");
            }
        }
    }
}

pub struct SyncContext {
    pub storage: Arc<dyn Storage>,
    pub market_data: MarketDataService,
    pub reporting_currency: String,
    pub auth_prompter: Arc<dyn AuthPrompter>,
    pub auto_committer: Arc<dyn AutoCommitter>,
    pub synchronizer_factory: Arc<dyn SynchronizerFactory>,
    pub clock: Arc<dyn Clock>,
}

impl SyncContext {
    pub fn new(
        storage: Arc<dyn Storage>,
        market_data: MarketDataService,
        reporting_currency: String,
    ) -> Self {
        Self {
            storage,
            market_data,
            reporting_currency,
            auth_prompter: Arc::new(FixedAuthPrompter::deny()),
            auto_committer: Arc::new(NoopAutoCommitter),
            synchronizer_factory: Arc::new(DefaultSynchronizerFactory::new(None)),
            clock: Arc::new(SystemClock),
        }
    }

    pub fn with_auth_prompter(mut self, prompter: Arc<dyn AuthPrompter>) -> Self {
        self.auth_prompter = prompter;
        self
    }

    pub fn with_auto_committer(mut self, committer: Arc<dyn AutoCommitter>) -> Self {
        self.auto_committer = committer;
        self
    }

    pub fn with_factory(mut self, factory: Arc<dyn SynchronizerFactory>) -> Self {
        self.synchronizer_factory = factory;
        self
    }

    pub fn with_clock(mut self, clock: Arc<dyn Clock>) -> Self {
        // Keep time-sensitive behavior consistent across orchestrator and market data.
        self.market_data = self.market_data.with_clock(clock.clone());
        self.clock = clock;
        self
    }
}

#[derive(Debug)]
pub enum SyncOutcome {
    Synced {
        report: SyncWithPricesResult,
    },
    SkippedManual {
        connection: Connection,
    },
    SkippedNotStale {
        connection: Connection,
    },
    AuthRequired {
        connection: Connection,
        error: String,
    },
    Failed {
        connection: Connection,
        error: String,
    },
}

pub struct SyncService {
    storage: Arc<dyn Storage>,
    orchestrator: SyncOrchestrator,
    auth_prompter: Arc<dyn AuthPrompter>,
    auto_committer: Arc<dyn AutoCommitter>,
    factory: Arc<dyn SynchronizerFactory>,
    clock: Arc<dyn Clock>,
}

impl SyncService {
    pub fn new(context: SyncContext) -> Self {
        let storage = context.storage.clone();
        let orchestrator = SyncOrchestrator::new(
            storage.clone(),
            context.market_data,
            context.reporting_currency,
        )
        .with_clock(context.clock.clone());

        Self {
            storage,
            orchestrator,
            auth_prompter: context.auth_prompter,
            auto_committer: context.auto_committer,
            factory: context.synchronizer_factory,
            clock: context.clock,
        }
    }

    pub async fn sync_connection(&self, id_or_name: &str) -> Result<SyncOutcome> {
        let connection = find_connection(self.storage.as_ref(), id_or_name)
            .await?
            .context(format!("Connection not found: {id_or_name}"))?;
        self.sync_connection_internal(connection, id_or_name).await
    }

    pub async fn sync_connection_if_stale(
        &self,
        id_or_name: &str,
        refresh: &RefreshConfig,
    ) -> Result<SyncOutcome> {
        let connection = find_connection(self.storage.as_ref(), id_or_name)
            .await?
            .context(format!("Connection not found: {id_or_name}"))?;
        let threshold = resolve_balance_staleness(None, &connection, refresh);
        let check = check_balance_staleness_at(&connection, threshold, self.clock.now());

        if !check.is_stale {
            return Ok(SyncOutcome::SkippedNotStale { connection });
        }

        self.sync_connection_internal(connection, id_or_name).await
    }

    pub async fn sync_all(&self) -> Result<Vec<SyncOutcome>> {
        let connections = self.storage.list_connections().await?;
        let mut results = Vec::with_capacity(connections.len());

        for connection in connections {
            let id_or_name = connection.id().to_string();
            match self
                .sync_connection_internal(connection.clone(), &id_or_name)
                .await
            {
                Ok(outcome) => results.push(outcome),
                Err(err) => results.push(SyncOutcome::Failed {
                    connection,
                    error: format!("{err:#}"),
                }),
            }
        }

        self.auto_committer.maybe_commit("sync all");

        Ok(results)
    }

    pub async fn sync_all_if_stale(&self, refresh: &RefreshConfig) -> Result<Vec<SyncOutcome>> {
        let connections = self.storage.list_connections().await?;
        let mut results = Vec::with_capacity(connections.len());

        for connection in connections {
            let threshold = resolve_balance_staleness(None, &connection, refresh);
            let check = check_balance_staleness_at(&connection, threshold, self.clock.now());

            if !check.is_stale {
                results.push(SyncOutcome::SkippedNotStale { connection });
                continue;
            }

            let id_or_name = connection.id().to_string();
            match self
                .sync_connection_internal(connection.clone(), &id_or_name)
                .await
            {
                Ok(outcome) => results.push(outcome),
                Err(err) => results.push(SyncOutcome::Failed {
                    connection,
                    error: format!("{err:#}"),
                }),
            }
        }

        self.auto_committer.maybe_commit("sync all");

        Ok(results)
    }

    /// Refresh prices only (no balance sync), for all accounts in storage.
    pub async fn sync_prices_all(&self, force: bool) -> Result<PriceRefreshResult> {
        let date = self.clock.today();
        let result = self
            .orchestrator
            .refresh_all_valuation_prices(date, force)
            .await?;

        let label = if force {
            "sync prices all (force)"
        } else {
            "sync prices all"
        };
        self.auto_committer.maybe_commit(label);
        Ok(result)
    }

    /// Refresh prices only (no balance sync), for all accounts in a connection.
    pub async fn sync_prices_connection(
        &self,
        id_or_name: &str,
        force: bool,
    ) -> Result<PriceRefreshResult> {
        let connection = find_connection(self.storage.as_ref(), id_or_name)
            .await?
            .context(format!("Connection not found: {id_or_name}"))?;
        let date = self.clock.today();
        let result = self
            .orchestrator
            .refresh_connection_valuation_prices(connection.id(), date, force)
            .await?;

        let label = if force {
            format!("sync prices connection {id_or_name} (force)")
        } else {
            format!("sync prices connection {id_or_name}")
        };
        self.auto_committer.maybe_commit(&label);
        Ok(result)
    }

    /// Refresh prices only (no balance sync), for a single account.
    pub async fn sync_prices_account(
        &self,
        id_or_name: &str,
        force: bool,
    ) -> Result<PriceRefreshResult> {
        let account = find_account(self.storage.as_ref(), id_or_name)
            .await?
            .context(format!("Account not found: {id_or_name}"))?;
        let date = self.clock.today();
        let result = self
            .orchestrator
            .refresh_account_valuation_prices(&account.id, date, force)
            .await?;

        let label = if force {
            format!("sync prices account {id_or_name} (force)")
        } else {
            format!("sync prices account {id_or_name}")
        };
        self.auto_committer.maybe_commit(&label);
        Ok(result)
    }

    pub async fn login(
        &self,
        synchronizer_name: &str,
        id_or_name: Option<&str>,
    ) -> Result<Connection> {
        let connections = self.storage.list_connections().await?;
        let matching: Vec<_> = connections
            .into_iter()
            .filter(|c| c.config.synchronizer == synchronizer_name)
            .collect();

        let connection = match (id_or_name, matching.len()) {
            (Some(id_or_name), _) => find_connection(self.storage.as_ref(), id_or_name)
                .await?
                .filter(|c| c.config.synchronizer == synchronizer_name)
                .context(format!(
                    "{} connection not found: {}",
                    synchronizer_name, id_or_name
                ))?,
            (None, 1) => matching.into_iter().next().unwrap(),
            (None, 0) => {
                anyhow::bail!("No {} connections found", synchronizer_name);
            }
            (None, n) => {
                let names: Vec<_> = matching.iter().map(|c| &c.config.name).collect();
                anyhow::bail!(
                    "Multiple {} connections found ({n}). Specify one: {names:?}",
                    synchronizer_name
                );
            }
        };

        let mut synchronizer = self
            .factory
            .create(&connection, self.storage.as_ref())
            .await?;
        let interactive = synchronizer
            .interactive()
            .context("Synchronizer does not support interactive login")?;
        interactive.login().await?;

        Ok(connection)
    }

    async fn sync_connection_internal(
        &self,
        mut connection: Connection,
        action_label: &str,
    ) -> Result<SyncOutcome> {
        if connection.config.synchronizer == "manual" {
            return Ok(SyncOutcome::SkippedManual { connection });
        }

        let mut synchronizer = self
            .factory
            .create(&connection, self.storage.as_ref())
            .await?;

        if let Some(interactive) = synchronizer.interactive() {
            if let Some(outcome) = self
                .ensure_interactive_auth(&connection, interactive)
                .await?
            {
                return Ok(outcome);
            }
        }

        let report = self
            .orchestrator
            .sync_with_prices(synchronizer.as_ref(), &mut connection, false)
            .await?;

        self.auto_committer
            .maybe_commit(&format!("sync connection {action_label}"));

        Ok(SyncOutcome::Synced { report })
    }

    async fn ensure_interactive_auth(
        &self,
        connection: &Connection,
        synchronizer: &mut dyn InteractiveAuth,
    ) -> Result<Option<SyncOutcome>> {
        match synchronizer.check_auth().await? {
            AuthStatus::Valid => Ok(None),
            AuthStatus::Missing => {
                if !synchronizer.auth_required_for_sync() {
                    return Ok(None);
                }
                let prompt = format!("No {} session found. Run login now?", synchronizer.name());
                if self.auth_prompter.confirm_login(&prompt)? {
                    match synchronizer.login().await {
                        Ok(()) => Ok(None),
                        Err(err) => Ok(Some(SyncOutcome::AuthRequired {
                            connection: connection.clone(),
                            error: format!("Interactive login failed: {err:#}"),
                        })),
                    }
                } else {
                    Ok(Some(SyncOutcome::AuthRequired {
                        connection: connection.clone(),
                        error: "No session available".to_string(),
                    }))
                }
            }
            AuthStatus::Expired { reason } => {
                if !synchronizer.auth_required_for_sync() {
                    return Ok(None);
                }
                let prompt = format!(
                    "{} session expired ({reason}). Run login now?",
                    synchronizer.name()
                );
                if self.auth_prompter.confirm_login(&prompt)? {
                    match synchronizer.login().await {
                        Ok(()) => Ok(None),
                        Err(err) => Ok(Some(SyncOutcome::AuthRequired {
                            connection: connection.clone(),
                            error: format!("Interactive login failed: {err:#}"),
                        })),
                    }
                } else {
                    Ok(Some(SyncOutcome::AuthRequired {
                        connection: connection.clone(),
                        error: format!("Session expired: {reason}"),
                    }))
                }
            }
        }
    }
}
