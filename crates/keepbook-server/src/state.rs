use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use chrono::{Local, Utc};
use keepbook::app::tray::{
    build_portfolio_breakdown_lines, format_history_change_for_tray, format_spending_window_label,
    format_tray_currency, normalize_spending_windows_days,
};
use keepbook::app::ReviewedRecurringTransactionOutput;
use keepbook::config::ResolvedConfig;
use keepbook::credentials::CredentialStore;
use keepbook::format::currency_symbol;
use keepbook::models::{
    Account, AccountConfig, Asset, BalanceSnapshot, Connection, ConnectionConfig, Id,
    ProposedTransactionEdit, RecurringTransactionReview, Transaction, TransactionAnnotationPatch,
};
use keepbook::repositories::{self, RepositoryDeclaration, RepositoryEntry, RepositoryRegistry};
use keepbook::storage::{JsonFileStorage, Storage};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::ai_rules::{self, AiRuleSuggestionInput, AiRuleSuggestionsOutput};
use crate::dto::{
    AddRepositoryInput, ApplicationSettingsInput, ApplicationSettingsOutput, AssetsQuery,
    ConfigOutput, FilteringOutput, GitSettingsInput, GitSettingsOutput, GitSyncCancelToken,
    GitSyncInput, GitSyncOutput, HistoryDefaultsOutput, HistoryQuery,
    LatentCapitalGainsTaxFilterOutput, OverviewOutput, OverviewQuery,
    ProposedTransactionEditsQuery, RecurringTransactionReviewInput, RecurringTransactionsQuery,
    RepositoryOutput, RepositoryRegistryOutput, SpendingQuery, SyncConnectionsInput,
    SyncPricesInput, TransactionEffectiveDateInput, TransactionIgnoreBatchInput, TransactionQuery,
    TransactionSubtagsBatchInput, TransactionTagsBatchInput, TraySnapshotOutput,
};
use crate::git::{
    activate_age_identity_from_git_settings, build_ssh_remote_url, load_git_remote_settings,
    normalize_remote_url_for_ssh, persist_git_private_key, prepare_git_ssh_environment,
    read_git_repo_state, resolve_git_private_key, resolve_input_data_dir, sync_git_ssh,
    validate_git_data_dir, with_default_desktop_ssh_key_path, write_git_settings,
};
use crate::settings::{
    desktop_start_minimized_to_tray, desktop_window_decorations, load_api_config, non_empty,
    write_application_settings,
};

#[derive(Clone)]
pub struct ApiState {
    inner: Arc<RwLock<ApiStateInner>>,
}

struct ApiStateInner {
    config_path: PathBuf,
    config: ResolvedConfig,
    storage: Arc<dyn Storage>,
}

#[derive(Clone)]
struct ApiSnapshot {
    config_path: PathBuf,
    config: ResolvedConfig,
    storage: Arc<dyn Storage>,
}

struct AccountConfigOverrideStorage {
    inner: Arc<dyn Storage>,
    exclude_from_portfolio: HashMap<String, bool>,
}

impl AccountConfigOverrideStorage {
    fn wrap(inner: Arc<dyn Storage>, overrides: HashMap<String, bool>) -> Arc<dyn Storage> {
        if overrides.is_empty() {
            inner
        } else {
            Arc::new(Self {
                inner,
                exclude_from_portfolio: overrides,
            })
        }
    }
}

#[async_trait::async_trait]
impl Storage for AccountConfigOverrideStorage {
    fn get_credential_store(&self, connection_id: &Id) -> Result<Option<Box<dyn CredentialStore>>> {
        self.inner.get_credential_store(connection_id)
    }

    fn get_account_config(&self, account_id: &Id) -> Result<Option<AccountConfig>> {
        let stored = self.inner.get_account_config(account_id)?;
        if let Some(excluded) = self.exclude_from_portfolio.get(account_id.as_str()) {
            let mut config = stored.unwrap_or_default();
            config.exclude_from_portfolio = Some(*excluded);
            return Ok(Some(config));
        }
        Ok(stored)
    }

    async fn list_connections(&self) -> Result<Vec<Connection>> {
        self.inner.list_connections().await
    }

    async fn get_connection(&self, id: &Id) -> Result<Option<Connection>> {
        self.inner.get_connection(id).await
    }

    async fn save_connection(&self, conn: &Connection) -> Result<()> {
        self.inner.save_connection(conn).await
    }

    async fn delete_connection(&self, id: &Id) -> Result<bool> {
        self.inner.delete_connection(id).await
    }

    async fn save_connection_config(&self, id: &Id, config: &ConnectionConfig) -> Result<()> {
        self.inner.save_connection_config(id, config).await
    }

    async fn list_accounts(&self) -> Result<Vec<Account>> {
        self.inner.list_accounts().await
    }

    async fn get_account(&self, id: &Id) -> Result<Option<Account>> {
        self.inner.get_account(id).await
    }

    async fn save_account(&self, account: &Account) -> Result<()> {
        self.inner.save_account(account).await
    }

    async fn delete_account(&self, id: &Id) -> Result<bool> {
        self.inner.delete_account(id).await
    }

    async fn save_account_config(&self, id: &Id, config: &AccountConfig) -> Result<()> {
        self.inner.save_account_config(id, config).await
    }

    async fn get_balance_snapshots(&self, account_id: &Id) -> Result<Vec<BalanceSnapshot>> {
        self.inner.get_balance_snapshots(account_id).await
    }

    async fn append_balance_snapshot(
        &self,
        account_id: &Id,
        snapshot: &BalanceSnapshot,
    ) -> Result<()> {
        self.inner
            .append_balance_snapshot(account_id, snapshot)
            .await
    }

    async fn get_latest_balance_snapshot(
        &self,
        account_id: &Id,
    ) -> Result<Option<BalanceSnapshot>> {
        self.inner.get_latest_balance_snapshot(account_id).await
    }

    async fn get_latest_balances(&self) -> Result<Vec<(Id, BalanceSnapshot)>> {
        self.inner.get_latest_balances().await
    }

    async fn get_latest_balances_for_connection(
        &self,
        connection_id: &Id,
    ) -> Result<Vec<(Id, BalanceSnapshot)>> {
        self.inner
            .get_latest_balances_for_connection(connection_id)
            .await
    }

    async fn get_transactions(&self, account_id: &Id) -> Result<Vec<Transaction>> {
        self.inner.get_transactions(account_id).await
    }

    async fn get_transactions_raw(&self, account_id: &Id) -> Result<Vec<Transaction>> {
        self.inner.get_transactions_raw(account_id).await
    }

    async fn append_transactions(&self, account_id: &Id, txns: &[Transaction]) -> Result<()> {
        self.inner.append_transactions(account_id, txns).await
    }

    async fn get_transaction_annotation_patches(
        &self,
        account_id: &Id,
    ) -> Result<Vec<TransactionAnnotationPatch>> {
        self.inner
            .get_transaction_annotation_patches(account_id)
            .await
    }

    async fn append_transaction_annotation_patches(
        &self,
        account_id: &Id,
        patches: &[TransactionAnnotationPatch],
    ) -> Result<()> {
        self.inner
            .append_transaction_annotation_patches(account_id, patches)
            .await
    }

    async fn get_proposed_transaction_edits(&self) -> Result<Vec<ProposedTransactionEdit>> {
        self.inner.get_proposed_transaction_edits().await
    }

    async fn append_proposed_transaction_edits(
        &self,
        edits: &[ProposedTransactionEdit],
    ) -> Result<()> {
        self.inner.append_proposed_transaction_edits(edits).await
    }

    async fn get_recurring_transaction_reviews(&self) -> Result<Vec<RecurringTransactionReview>> {
        self.inner.get_recurring_transaction_reviews().await
    }

    async fn append_recurring_transaction_reviews(
        &self,
        reviews: &[RecurringTransactionReview],
    ) -> Result<()> {
        self.inner
            .append_recurring_transaction_reviews(reviews)
            .await
    }
}

impl ApiState {
    pub fn load(config_path: impl AsRef<Path>) -> Result<Self> {
        let config_path = config_path.as_ref().to_path_buf();
        let config = load_api_config(&config_path)
            .with_context(|| format!("failed to load config from {}", config_path.display()))?;
        let storage = Arc::new(JsonFileStorage::new(&config.data_dir));

        Ok(Self {
            inner: Arc::new(RwLock::new(ApiStateInner {
                config_path,
                config,
                storage,
            })),
        })
    }

    async fn snapshot(&self) -> ApiSnapshot {
        let inner = self.inner.read().await;
        ApiSnapshot {
            config_path: inner.config_path.clone(),
            config: inner.config.clone(),
            storage: inner.storage.clone(),
        }
    }

    fn snapshot_blocking(&self) -> ApiSnapshot {
        let inner = self.inner.blocking_read();
        ApiSnapshot {
            config_path: inner.config_path.clone(),
            config: inner.config.clone(),
            storage: inner.storage.clone(),
        }
    }

    pub async fn reload(&self) -> Result<()> {
        let config_path = {
            let inner = self.inner.read().await;
            inner.config_path.clone()
        };
        let config = load_api_config(&config_path)
            .with_context(|| format!("failed to reload config from {}", config_path.display()))?;
        let storage = Arc::new(JsonFileStorage::new(&config.data_dir));
        let mut inner = self.inner.write().await;
        inner.config = config;
        inner.storage = storage;
        Ok(())
    }

    pub async fn switch_config(&self, config_path: impl AsRef<Path>) -> Result<()> {
        let config_path = config_path.as_ref().to_path_buf();
        let config = load_api_config(&config_path)
            .with_context(|| format!("failed to load config from {}", config_path.display()))?;
        let storage = Arc::new(JsonFileStorage::new(&config.data_dir));
        let mut inner = self.inner.write().await;
        inner.config_path = config_path;
        inner.config = config;
        inner.storage = storage;
        Ok(())
    }

    fn reload_blocking(&self) -> Result<()> {
        let config_path = {
            let inner = self.inner.blocking_read();
            inner.config_path.clone()
        };
        let config = load_api_config(&config_path)
            .with_context(|| format!("failed to reload config from {}", config_path.display()))?;
        let storage = Arc::new(JsonFileStorage::new(&config.data_dir));
        let mut inner = self.inner.blocking_write();
        inner.config = config;
        inner.storage = storage;
        Ok(())
    }

    pub async fn config_output(&self) -> ConfigOutput {
        let state = self.snapshot().await;
        ConfigOutput {
            config_path: state.config_path.display().to_string(),
            data_dir: state.config.data_dir.display().to_string(),
            reporting_currency: state.config.reporting_currency.clone(),
            reporting_currency_symbol: reporting_currency_symbol(&state.config),
            history_defaults: history_defaults(&state.config),
            filtering: filtering_output(&state.config, &state.config, None),
        }
    }

    pub async fn repositories(&self) -> Result<RepositoryRegistryOutput> {
        let snapshot = self.snapshot().await;
        let app_config_path = default_app_config_path();
        let mut registry = repositories::load_registry(&app_config_path)?;
        if registry.repositories.is_empty() {
            let entry = bootstrap_repository_entry(&snapshot, &registry.repositories)?;
            registry = repositories::add_resolved_local_repository(&app_config_path, &entry)?;
        }
        repository_registry_output(&registry)
    }

    pub async fn add_repository(
        &self,
        input: AddRepositoryInput,
    ) -> Result<RepositoryRegistryOutput> {
        let snapshot = self.snapshot().await;
        let app_config_path = default_app_config_path();
        let mut registry = repositories::load_registry(&app_config_path)?;
        if registry.repositories.is_empty() {
            let entry = bootstrap_repository_entry(&snapshot, &registry.repositories)?;
            registry = repositories::add_resolved_local_repository(&app_config_path, &entry)?;
        }

        let path = repositories::absolute_repository_path(&input.path)?;
        let remote = input.remote.trim();
        anyhow::ensure!(!remote.is_empty(), "Git remote is required");
        anyhow::ensure!(
            !registry.repositories.iter().any(|entry| entry.path == path),
            "A repository at {} is already registered",
            path.display()
        );

        let name = input.name.trim();
        let name = if name.is_empty() {
            repositories::repository_name_from_path(&path)
        } else {
            name.to_string()
        };
        let id = repositories::unique_repository_id(&name, &registry.repositories);
        repositories::add_local_repository(
            &app_config_path,
            RepositoryDeclaration {
                id,
                name,
                remote: remote.to_string(),
                branch: non_empty(input.branch.trim(), "master").to_string(),
                path: Some(path),
                config_path: None,
            },
        )
        .and_then(|registry| repository_registry_output(&registry))
    }

    pub async fn activate_repository(
        &self,
        repository_id: &str,
    ) -> Result<RepositoryRegistryOutput> {
        let app_config_path = default_app_config_path();
        let registry = repositories::load_registry(&app_config_path)?;
        let entry = registry
            .repositories
            .iter()
            .find(|entry| entry.id == repository_id)
            .cloned()
            .with_context(|| format!("Unknown Keepbook repository: {repository_id}"))?;
        anyhow::ensure!(
            entry.config_path.is_file(),
            "Repository {} is not ready: {} does not exist",
            entry.name,
            entry.config_path.display()
        );

        self.switch_config(&entry.config_path).await?;
        let registry = repositories::set_active_repository(&app_config_path, &entry.id)?;
        repository_registry_output(&registry)
    }

    pub async fn remove_repository(&self, repository_id: &str) -> Result<RepositoryRegistryOutput> {
        let app_config_path = default_app_config_path();
        let registry = repositories::remove_local_repository(&app_config_path, repository_id)?;
        repository_registry_output(&registry)
    }

    pub async fn overview(&self, query: OverviewQuery) -> Result<OverviewOutput> {
        let state = self.snapshot().await;
        let effective_config =
            config_with_filter_overrides(&state.config, query.include_latent_capital_gains_tax);
        let storage = storage_with_account_overrides(
            state.storage.clone(),
            &query.account_portfolio_overrides,
        )?;
        let connections = keepbook::app::list_connections(storage.as_ref()).await?;
        let accounts = keepbook::app::list_accounts(storage.as_ref()).await?;
        let balances = keepbook::app::list_balances(storage.as_ref(), &effective_config).await?;
        let snapshot = keepbook::app::portfolio_snapshot(
            storage.clone(),
            &effective_config,
            keepbook::app::PortfolioSnapshotRequest {
                group_by: "both".to_string(),
                offline: true,
                ..Default::default()
            },
        )
        .await?;
        let history = if query.include_history {
            let history_start = query.history_start;
            let history_end = query.history_end.or_else(|| Some(default_history_end()));
            let history_granularity = query
                .history_granularity
                .unwrap_or_else(|| effective_config.history.portfolio_granularity.clone());
            let include_prices = query
                .include_prices
                .unwrap_or(effective_config.history.include_prices);
            Some(json_value(
                keepbook::app::portfolio_history(
                    storage.clone(),
                    &effective_config,
                    None,
                    history_start,
                    history_end,
                    history_granularity,
                    include_prices,
                    false,
                )
                .await?,
            )?)
        } else {
            None
        };

        let account_totals = keepbook::app::account_totals(storage.as_ref(), &snapshot).await?;

        Ok(OverviewOutput {
            config_path: state.config_path.display().to_string(),
            data_dir: state.config.data_dir.display().to_string(),
            reporting_currency: state.config.reporting_currency.clone(),
            reporting_currency_symbol: reporting_currency_symbol(&state.config),
            history_defaults: history_defaults(&state.config),
            filtering: filtering_output(
                &state.config,
                &effective_config,
                query.include_latent_capital_gains_tax,
            ),
            connections: json_value(connections)?,
            accounts: json_value(accounts)?,
            account_totals: json_value(account_totals)?,
            balances: json_value(balances)?,
            snapshot: json_value(snapshot)?,
            history,
        })
    }

    pub async fn tray_snapshot(&self) -> Result<TraySnapshotOutput> {
        let state = self.snapshot().await;
        let portfolio_result = keepbook::app::portfolio_snapshot(
            state.storage.clone(),
            &state.config,
            keepbook::app::PortfolioSnapshotRequest {
                group_by: "account".to_string(),
                offline: true,
                ..Default::default()
            },
        )
        .await;

        let (total_label, as_of_date, portfolio_breakdown_lines) = match portfolio_result {
            Ok(snapshot) => (
                format_tray_currency(
                    &snapshot.total_value,
                    &snapshot.currency,
                    &state.config.display,
                ),
                snapshot.as_of_date.to_string(),
                build_portfolio_breakdown_lines(&snapshot, &state.config.display),
            ),
            Err(err) => (
                "unavailable".to_string(),
                "unavailable".to_string(),
                vec![format!("Portfolio unavailable: {err}")],
            ),
        };
        let history_lines = tray_history_lines(state.storage.clone(), &state.config).await;
        let spending_lines = tray_spending_lines(state.storage.clone(), &state.config).await;
        let transaction_lines = tray_transaction_lines(state.storage, &state.config).await;

        Ok(TraySnapshotOutput {
            total_label,
            as_of_date,
            history_lines,
            portfolio_breakdown_lines,
            spending_lines,
            transaction_lines,
        })
    }

    pub async fn connections(&self) -> Result<serde_json::Value> {
        let state = self.snapshot().await;
        json_value(keepbook::app::list_connections(state.storage.as_ref()).await?)
    }

    pub async fn accounts(&self) -> Result<serde_json::Value> {
        let state = self.snapshot().await;
        json_value(keepbook::app::list_accounts(state.storage.as_ref()).await?)
    }

    pub async fn balances(&self) -> Result<serde_json::Value> {
        let state = self.snapshot().await;
        json_value(keepbook::app::list_balances(state.storage.as_ref(), &state.config).await?)
    }

    pub async fn transactions(&self, query: TransactionQuery) -> Result<serde_json::Value> {
        let state = self.snapshot().await;
        json_value(
            keepbook::app::list_transactions(
                state.storage.as_ref(),
                query.start,
                query.end,
                query.tz,
                query.sort_by_amount,
                !query.include_ignored,
                &state.config,
            )
            .await?,
        )
    }

    pub async fn recurring_transactions(
        &self,
        query: RecurringTransactionsQuery,
    ) -> Result<Vec<ReviewedRecurringTransactionOutput>> {
        let state = self.snapshot().await;
        keepbook::app::list_reviewed_recurring_transactions(
            state.storage.as_ref(),
            keepbook::app::RecurringTransactionsOptions {
                start: query.start,
                end: query.end,
                include_ignored: query.include_ignored,
                include_possible: query.include_possible,
                min_confidence: query.min_confidence.unwrap_or(0.70),
            },
            query.include_dismissed,
            &state.config,
        )
        .await
    }

    pub async fn review_recurring_transaction(
        &self,
        input: RecurringTransactionReviewInput,
    ) -> Result<serde_json::Value> {
        let state = self.snapshot().await;
        let status = match input.status.as_str() {
            "verified" => keepbook::models::RecurringTransactionReviewStatus::Verified,
            "dismissed" => keepbook::models::RecurringTransactionReviewStatus::Dismissed,
            other => anyhow::bail!("unknown recurring transaction review status: {other}"),
        };
        keepbook::app::set_recurring_transaction_review(
            state.storage.as_ref(),
            &state.config,
            input.candidate.candidate_key,
            status,
            &input.candidate.candidate,
        )
        .await
    }

    pub async fn spending(&self, query: SpendingQuery) -> Result<serde_json::Value> {
        let state = self.snapshot().await;
        json_value(
            keepbook::app::spending_report(
                state.storage.as_ref(),
                &state.config,
                keepbook::app::SpendingReportOptions {
                    currency: query.currency,
                    start: query.start,
                    end: query.end,
                    period: query.period.unwrap_or_else(|| "range".to_string()),
                    period_alignment: query
                        .period_alignment
                        .or_else(|| Some("calendar".to_string())),
                    tz: query.tz,
                    week_start: query.week_start,
                    bucket: query
                        .bucket_days
                        .map(|days| Duration::from_secs(days.saturating_mul(86_400))),
                    account: query.account,
                    connection: query.connection,
                    status: query.status.unwrap_or_else(|| "posted".to_string()),
                    direction: query.direction.unwrap_or_else(|| "outflow".to_string()),
                    group_by: query.group_by.unwrap_or_else(|| "tag".to_string()),
                    top: query.top,
                    lookback_days: query.lookback_days.unwrap_or(7),
                    include_noncurrency: query.include_noncurrency,
                    include_empty: query.include_empty,
                },
            )
            .await?,
        )
    }

    pub async fn set_transaction_tags(
        &self,
        input: TransactionTagsBatchInput,
    ) -> Result<serde_json::Value> {
        let state = self.snapshot().await;
        let targets = input
            .transactions
            .into_iter()
            .map(|transaction| (transaction.account_id, transaction.transaction_id))
            .collect::<Vec<_>>();
        keepbook::app::set_transaction_tags(
            state.storage.as_ref(),
            &state.config,
            targets,
            input.tags,
            input.clear_tags,
        )
        .await
    }

    pub async fn set_transaction_ignore(
        &self,
        input: TransactionIgnoreBatchInput,
    ) -> Result<serde_json::Value> {
        let state = self.snapshot().await;
        let targets = input
            .transactions
            .into_iter()
            .map(|transaction| (transaction.account_id, transaction.transaction_id))
            .collect::<Vec<_>>();
        keepbook::app::set_transaction_ignore(
            state.storage.as_ref(),
            &state.config,
            targets,
            input.ignore,
        )
        .await
    }

    pub async fn set_transaction_subtags(
        &self,
        input: TransactionSubtagsBatchInput,
    ) -> Result<serde_json::Value> {
        let state = self.snapshot().await;
        let targets = input
            .transactions
            .into_iter()
            .map(|transaction| (transaction.account_id, transaction.transaction_id))
            .collect::<Vec<_>>();
        keepbook::app::set_transaction_subtags(
            state.storage.as_ref(),
            &state.config,
            targets,
            input.subtags,
            input.clear_subtags,
        )
        .await
    }

    pub async fn set_transaction_effective_date(
        &self,
        input: TransactionEffectiveDateInput,
    ) -> Result<serde_json::Value> {
        let state = self.snapshot().await;
        keepbook::app::set_transaction_annotation(
            state.storage.as_ref(),
            &state.config,
            &input.account_id,
            &input.transaction_id,
            keepbook::app::TransactionAnnotationInput {
                effective_date: input.effective_date,
                clear_effective_date: input.clear_effective_date,
                ..Default::default()
            },
        )
        .await
    }

    pub async fn proposed_transaction_edits(
        &self,
        query: ProposedTransactionEditsQuery,
    ) -> Result<serde_json::Value> {
        let state = self.snapshot().await;
        json_value(
            keepbook::app::list_proposed_transaction_edits(
                state.storage.as_ref(),
                query.include_decided,
            )
            .await?,
        )
    }

    pub async fn approve_proposed_transaction_edit(&self, id: String) -> Result<serde_json::Value> {
        let state = self.snapshot().await;
        keepbook::app::approve_proposed_transaction_edit(state.storage.as_ref(), &state.config, &id)
            .await
    }

    pub async fn reject_proposed_transaction_edit(&self, id: String) -> Result<serde_json::Value> {
        let state = self.snapshot().await;
        keepbook::app::reject_proposed_transaction_edit(state.storage.as_ref(), &state.config, &id)
            .await
    }

    pub async fn remove_proposed_transaction_edit(&self, id: String) -> Result<serde_json::Value> {
        let state = self.snapshot().await;
        keepbook::app::remove_proposed_transaction_edit(state.storage.as_ref(), &state.config, &id)
            .await
    }

    pub async fn portfolio_history(&self, query: HistoryQuery) -> Result<serde_json::Value> {
        let state = self.snapshot().await;
        let effective_config =
            config_with_filter_overrides(&state.config, query.include_latent_capital_gains_tax);
        let storage = storage_with_account_overrides(
            state.storage.clone(),
            &query.account_portfolio_overrides,
        )?;
        let granularity = query
            .granularity
            .unwrap_or_else(|| effective_config.history.portfolio_granularity.clone());
        let include_prices = query
            .include_prices
            .unwrap_or(effective_config.history.include_prices);
        let include_current = query.include_current.unwrap_or(false);
        let selection = keepbook::app::resolve_portfolio_history_selection(
            storage.as_ref(),
            &effective_config,
            query.account.as_deref(),
            query.connection.as_deref(),
        )
        .await?;
        let output = match selection {
            keepbook::app::PortfolioHistorySelection::Portfolio => {
                keepbook::app::portfolio_history(
                    storage.clone(),
                    &effective_config,
                    query.currency,
                    query.start,
                    query.end,
                    granularity,
                    include_prices,
                    include_current,
                )
                .await?
            }
            keepbook::app::PortfolioHistorySelection::Accounts(account_ids) => {
                keepbook::app::portfolio_history_for_accounts(
                    storage.clone(),
                    &effective_config,
                    query.currency,
                    query.start,
                    query.end,
                    granularity,
                    include_prices,
                    include_current,
                    account_ids,
                )
                .await?
            }
            keepbook::app::PortfolioHistorySelection::LatentCapitalGainsTax => {
                keepbook::app::latent_capital_gains_tax_history(
                    storage.clone(),
                    &effective_config,
                    query.currency,
                    query.start,
                    query.end,
                    granularity,
                    include_prices,
                    include_current,
                )
                .await?
            }
        };
        json_value(output)
    }

    pub async fn portfolio_assets(
        &self,
        query: AssetsQuery,
    ) -> Result<keepbook::app::AssetBreakdownOutput> {
        let state = self.snapshot().await;
        let storage = storage_with_account_overrides(
            state.storage.clone(),
            &query.account_portfolio_overrides,
        )?;
        keepbook::app::portfolio_assets(
            storage,
            &state.config,
            query.date,
            query.include_amount_changes,
        )
        .await
    }

    pub async fn portfolio_stacked_history(
        &self,
        query: HistoryQuery,
    ) -> Result<serde_json::Value> {
        let state = self.snapshot().await;
        let effective_config =
            config_with_filter_overrides(&state.config, query.include_latent_capital_gains_tax);
        let storage = storage_with_account_overrides(
            state.storage.clone(),
            &query.account_portfolio_overrides,
        )?;
        let granularity = query
            .granularity
            .unwrap_or_else(|| effective_config.history.portfolio_granularity.clone());
        let include_prices = query
            .include_prices
            .unwrap_or(effective_config.history.include_prices);
        let include_current = query.include_current.unwrap_or(false);
        json_value(
            keepbook::app::portfolio_stacked_history(
                storage,
                &effective_config,
                query.currency,
                query.start,
                query.end,
                granularity,
                include_prices,
                include_current,
            )
            .await?,
        )
    }

    pub async fn merge_origin_master(&self) -> Result<serde_json::Value> {
        let state = self.snapshot().await;
        keepbook::app::run_preflight(
            &state.config,
            keepbook::app::PreflightOptions {
                merge_origin_master: true,
                pull_remote: false,
            },
        )?;
        json_value(())
    }

    pub async fn git_settings(&self) -> Result<GitSettingsOutput> {
        let snapshot = self.snapshot().await;
        let git = load_git_remote_settings(&snapshot.config_path)?;
        prepare_git_ssh_environment(&snapshot.config_path)?;
        let git = with_default_desktop_ssh_key_path(&snapshot.config_path, git);
        Ok(GitSettingsOutput {
            config_path: snapshot.config_path.display().to_string(),
            data_dir: snapshot.config.data_dir.display().to_string(),
            git,
            repo_state: read_git_repo_state(&snapshot.config.data_dir),
        })
    }

    pub async fn application_settings(&self) -> Result<ApplicationSettingsOutput> {
        let snapshot = self.snapshot().await;
        Ok(ApplicationSettingsOutput {
            config_path: snapshot.config_path.display().to_string(),
            start_minimized_to_tray: desktop_start_minimized_to_tray(&snapshot.config_path)?,
            window_decorations: desktop_window_decorations(&snapshot.config_path)?
                .as_str()
                .to_string(),
        })
    }

    pub async fn save_application_settings(
        &self,
        input: ApplicationSettingsInput,
    ) -> Result<ApplicationSettingsOutput> {
        let snapshot = self.snapshot().await;
        write_application_settings(&snapshot.config_path, &input)?;
        self.reload().await?;
        self.application_settings().await
    }

    pub async fn save_git_settings(&self, input: GitSettingsInput) -> Result<GitSettingsOutput> {
        let snapshot = self.snapshot().await;
        write_git_settings(&snapshot.config_path, &input)?;
        self.reload().await?;
        self.git_settings().await
    }

    pub async fn sync_git_repo(&self, input: GitSyncInput) -> Result<GitSyncOutput> {
        self.sync_git_repo_with_cancel(input, GitSyncCancelToken::default())
            .await
    }

    pub async fn sync_git_repo_with_cancel(
        &self,
        input: GitSyncInput,
        cancel_token: GitSyncCancelToken,
    ) -> Result<GitSyncOutput> {
        let snapshot = self.snapshot().await;
        let output = self.sync_git_repo_from_snapshot(input, cancel_token, snapshot)?;
        self.reload().await?;
        Ok(output)
    }

    pub fn sync_git_repo_blocking_with_cancel(
        &self,
        input: GitSyncInput,
        cancel_token: GitSyncCancelToken,
    ) -> Result<GitSyncOutput> {
        let snapshot = self.snapshot_blocking();
        let output = self.sync_git_repo_from_snapshot(input, cancel_token, snapshot)?;
        self.reload_blocking()?;
        Ok(output)
    }

    fn sync_git_repo_from_snapshot(
        &self,
        input: GitSyncInput,
        cancel_token: GitSyncCancelToken,
        snapshot: ApiSnapshot,
    ) -> Result<GitSyncOutput> {
        let data_dir = resolve_input_data_dir(&snapshot.config_path, input.data_dir.trim());
        validate_git_data_dir(&data_dir)?;
        prepare_git_ssh_environment(&snapshot.config_path)?;
        let configured_git = load_git_remote_settings(&snapshot.config_path)?;
        let auth_git =
            with_default_desktop_ssh_key_path(&snapshot.config_path, configured_git.clone());
        let private_key_pem = resolve_git_private_key(&snapshot.config_path, &auth_git, &input)?;
        let repo_state = read_git_repo_state(&data_dir);
        let branch = repo_state
            .branch
            .clone()
            .unwrap_or_else(|| non_empty(input.branch.trim(), "master"));
        let remote_url = repo_state
            .remote_url
            .clone()
            .unwrap_or_else(|| build_ssh_remote_url(&input.host, &input.repo, &input.ssh_user));
        let remote_url = normalize_remote_url_for_ssh(&remote_url, &input.ssh_user);
        sync_git_ssh(
            &data_dir,
            &remote_url,
            &branch,
            &private_key_pem,
            &cancel_token,
        )?;

        if input.save_settings {
            let saved_ssh_key_path = if !input.private_key_pem.trim().is_empty() {
                Some(persist_git_private_key(
                    &snapshot.config_path,
                    &input.private_key_pem,
                )?)
            } else {
                auth_git.ssh_key_path.clone()
            };
            write_git_settings(
                &snapshot.config_path,
                &GitSettingsInput {
                    data_dir: input.data_dir.clone(),
                    host: input.host.clone(),
                    repo: input.repo.clone(),
                    branch: input.branch.clone(),
                    ssh_user: input.ssh_user.clone(),
                    ssh_key_path: saved_ssh_key_path,
                },
            )?;
        }
        Ok(GitSyncOutput {
            ok: true,
            data_dir: data_dir.display().to_string(),
            remote_url,
            branch,
        })
    }

    pub async fn sync_connections(&self, input: SyncConnectionsInput) -> Result<serde_json::Value> {
        let snapshot = self.snapshot().await;
        prepare_git_ssh_environment(&snapshot.config_path)?;
        activate_age_identity_from_git_settings(&snapshot.config_path)?;
        std::env::set_var("KEEPBOOK_NONINTERACTIVE", "1");
        let transactions = if input.full_transactions {
            keepbook::sync::TransactionSyncMode::Full
        } else {
            keepbook::sync::TransactionSyncMode::Auto
        };

        match input
            .target
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            Some(target) => {
                if input.if_stale {
                    keepbook::app::sync_connection_if_stale(
                        snapshot.storage,
                        &snapshot.config,
                        target,
                        transactions,
                    )
                    .await
                } else {
                    keepbook::app::sync_connection(
                        snapshot.storage,
                        &snapshot.config,
                        target,
                        transactions,
                    )
                    .await
                }
            }
            None => {
                if input.if_stale {
                    keepbook::app::sync_all_if_stale(
                        snapshot.storage,
                        &snapshot.config,
                        transactions,
                    )
                    .await
                } else {
                    keepbook::app::sync_all(snapshot.storage, &snapshot.config, transactions).await
                }
            }
        }
    }

    pub async fn sync_prices(&self, input: SyncPricesInput) -> Result<serde_json::Value> {
        let snapshot = self.snapshot().await;
        prepare_git_ssh_environment(&snapshot.config_path)?;
        activate_age_identity_from_git_settings(&snapshot.config_path)?;
        let target = input
            .target
            .as_deref()
            .map(str::trim)
            .filter(|target| !target.is_empty());
        let scope_name = input.scope.as_deref().unwrap_or("all").trim();
        let scope = match scope_name {
            "" | "all" => keepbook::app::SyncPricesScopeArg::All,
            "connection" => {
                let Some(target) = target else {
                    anyhow::bail!("price sync connection scope requires target");
                };
                keepbook::app::SyncPricesScopeArg::Connection(Some(target))
            }
            "account" => {
                let Some(target) = target else {
                    anyhow::bail!("price sync account scope requires target");
                };
                keepbook::app::SyncPricesScopeArg::Account(Some(target))
            }
            other => anyhow::bail!("unknown price sync scope: {other}"),
        };

        keepbook::app::sync_prices(
            snapshot.storage,
            &snapshot.config,
            scope,
            input.force,
            input.quote_staleness_seconds.map(Duration::from_secs),
        )
        .await
    }

    pub async fn suggest_ai_rules(
        &self,
        input: AiRuleSuggestionInput,
    ) -> Result<AiRuleSuggestionsOutput> {
        let snapshot = self.snapshot().await;
        ai_rules::suggest_rules(&snapshot.config_path, &snapshot.config, input).await
    }
}

#[derive(Debug, Deserialize)]
struct AccountPortfolioExclusionOverride {
    account_id: String,
    exclude_from_portfolio: bool,
}

fn default_history_end() -> String {
    Utc::now().date_naive().to_string()
}

fn json_value<T: Serialize>(value: T) -> Result<serde_json::Value> {
    serde_json::to_value(value).context("failed to encode keepbook API output")
}

fn reporting_currency_symbol(config: &ResolvedConfig) -> Option<String> {
    config
        .display
        .currency_symbol
        .as_deref()
        .or_else(|| currency_symbol(&config.reporting_currency))
        .map(str::to_string)
}

fn last_n_days_range(days: u32) -> (chrono::NaiveDate, chrono::NaiveDate) {
    let end = Local::now().date_naive();
    let start = end - chrono::Duration::days(days.saturating_sub(1) as i64);
    (start, end)
}

async fn tray_history_lines(storage: Arc<dyn Storage>, config: &ResolvedConfig) -> Vec<String> {
    match keepbook::app::portfolio_recent_history(
        storage,
        config,
        None,
        true,
        Local::now().date_naive(),
    )
    .await
    {
        Ok(history_points) => {
            let mut lines: Vec<String> = history_points
                .iter()
                .rev()
                .take(config.tray.history_points)
                .map(|point| {
                    let value = format_tray_currency(
                        &point.total_value,
                        &config.reporting_currency,
                        &config.display,
                    );
                    let percentage_change = format_history_change_for_tray(
                        point.percentage_change_from_previous.as_deref(),
                    );
                    format!("{}: {} ({} vs prev)", point.date, value, percentage_change)
                })
                .collect();

            if lines.is_empty() {
                lines.push("No portfolio history available".to_string());
            }
            lines
        }
        Err(err) => vec![format!("History unavailable: {err}")],
    }
}

async fn tray_spending_line_for_days(
    storage: &Arc<dyn Storage>,
    config: &ResolvedConfig,
    days: u32,
) -> String {
    let label = format_spending_window_label(days);
    let (start, end) = last_n_days_range(days);
    let opts = keepbook::app::SpendingReportOptions {
        currency: None,
        start: Some(start.format("%Y-%m-%d").to_string()),
        end: Some(end.format("%Y-%m-%d").to_string()),
        period: "range".to_string(),
        period_alignment: None,
        tz: None,
        week_start: None,
        bucket: None,
        account: None,
        connection: None,
        status: "posted".to_string(),
        direction: "outflow".to_string(),
        group_by: "none".to_string(),
        top: None,
        lookback_days: 7,
        include_noncurrency: false,
        include_empty: false,
    };

    match keepbook::app::spending_report(storage.as_ref(), config, opts).await {
        Ok(report) => {
            let value = format_tray_currency(&report.total, &report.currency, &config.display);
            let tx_label = if report.transaction_count == 1 {
                "txn"
            } else {
                "txns"
            };
            format!(
                "Last {label}: {} ({} {})",
                value, report.transaction_count, tx_label
            )
        }
        Err(_) => format!("Last {label}: unavailable"),
    }
}

async fn tray_spending_lines(storage: Arc<dyn Storage>, config: &ResolvedConfig) -> Vec<String> {
    let windows = normalize_spending_windows_days(&config.tray.spending_windows_days);
    let mut lines = Vec::with_capacity(windows.len().max(1));
    for days in windows {
        lines.push(tray_spending_line_for_days(&storage, config, days).await);
    }
    if lines.is_empty() {
        lines.push("No spending windows configured".to_string());
    }
    lines
}

async fn tray_transaction_lines(storage: Arc<dyn Storage>, config: &ResolvedConfig) -> Vec<String> {
    if config.tray.transaction_count == 0 {
        return vec!["Transaction display disabled".to_string()];
    }

    let result: Result<Vec<String>> = async {
        let connections = storage.list_connections().await?;
        let accounts = storage.list_accounts().await?;
        let conn_name_by_id: HashMap<String, String> = connections
            .iter()
            .map(|c| (c.id().to_string(), c.config.name.clone()))
            .collect();
        let account_conn_name: HashMap<String, String> = accounts
            .iter()
            .map(|a| {
                let conn_name = conn_name_by_id
                    .get(&a.connection_id.to_string())
                    .cloned()
                    .unwrap_or_else(|| "Unknown".to_string());
                (a.id.to_string(), conn_name)
            })
            .collect();

        struct TxRow {
            timestamp: chrono::DateTime<chrono::Utc>,
            source: String,
            amount: String,
            description: String,
            asset: Asset,
        }

        let cutoff = chrono::Utc::now() - chrono::Duration::days(30);
        let mut rows = Vec::new();
        for account in &accounts {
            let txns = storage.get_transactions(&account.id).await?;
            let source = account_conn_name
                .get(&account.id.to_string())
                .cloned()
                .unwrap_or_else(|| "Unknown".to_string());

            for tx in txns {
                if tx.timestamp < cutoff {
                    continue;
                }
                rows.push(TxRow {
                    timestamp: tx.timestamp,
                    source: source.clone(),
                    amount: tx.amount,
                    description: tx.description,
                    asset: tx.asset,
                });
            }
        }

        rows.sort_by_key(|row| std::cmp::Reverse(row.timestamp));
        rows.truncate(config.tray.transaction_count);

        Ok(rows
            .iter()
            .map(|row| {
                let date = row.timestamp.with_timezone(&Local).format("%m-%d");
                let currency = match &row.asset {
                    Asset::Currency { iso_code } => iso_code.as_str(),
                    Asset::ManualValue { currency, .. } => currency.as_str(),
                    Asset::Equity { ticker, .. } => ticker.as_str(),
                    Asset::Crypto { symbol, .. } => symbol.as_str(),
                };
                let amount = format_tray_currency(&row.amount, currency, &config.display);
                let desc = if row.description.chars().count() > 30 {
                    let truncated: String = row.description.chars().take(27).collect();
                    format!("{truncated}...")
                } else {
                    row.description.clone()
                };
                format!("{} | {} | {} | {}", date, row.source, amount, desc)
            })
            .collect())
    }
    .await;

    match result {
        Ok(lines) if lines.is_empty() => vec!["No transactions in last 30 days".to_string()],
        Ok(lines) => lines,
        Err(err) => vec![format!("Transactions unavailable: {err}")],
    }
}

fn history_defaults(config: &ResolvedConfig) -> HistoryDefaultsOutput {
    HistoryDefaultsOutput {
        portfolio_granularity: config.history.portfolio_granularity.clone(),
        change_points_granularity: config.history.change_points_granularity.clone(),
        include_prices: config.history.include_prices,
        graph_range: config.history.graph_range.clone(),
        graph_granularity: config.history.graph_granularity.clone(),
    }
}

fn config_with_filter_overrides(
    base: &ResolvedConfig,
    include_latent_capital_gains_tax: Option<bool>,
) -> ResolvedConfig {
    let mut config = base.clone();
    if let Some(enabled) = include_latent_capital_gains_tax {
        config.portfolio.latent_capital_gains_tax.enabled = enabled;
    }
    config
}

fn storage_with_account_overrides(
    storage: Arc<dyn Storage>,
    account_portfolio_overrides: &Option<String>,
) -> Result<Arc<dyn Storage>> {
    let storage = AccountConfigOverrideStorage::wrap(
        storage,
        account_portfolio_exclusion_overrides(account_portfolio_overrides)?,
    );
    Ok(storage)
}

fn account_portfolio_exclusion_overrides(
    account_portfolio_overrides: &Option<String>,
) -> Result<HashMap<String, bool>> {
    let mut overrides = HashMap::new();
    let Some(raw_overrides) = account_portfolio_overrides
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(overrides);
    };
    let parsed: Vec<AccountPortfolioExclusionOverride> =
        serde_json::from_str(raw_overrides).context("invalid account_portfolio_overrides")?;
    for override_entry in parsed {
        let account_id = override_entry.account_id.trim();
        if !account_id.is_empty() {
            overrides.insert(
                account_id.to_string(),
                override_entry.exclude_from_portfolio,
            );
        }
    }
    Ok(overrides)
}

fn filtering_output(
    base: &ResolvedConfig,
    effective: &ResolvedConfig,
    include_latent_capital_gains_tax: Option<bool>,
) -> FilteringOutput {
    let configured = &base.portfolio.latent_capital_gains_tax;
    let effective = &effective.portfolio.latent_capital_gains_tax;

    FilteringOutput {
        latent_capital_gains_tax: LatentCapitalGainsTaxFilterOutput {
            configured_enabled: configured.enabled,
            effective_enabled: effective.enabled,
            override_enabled: include_latent_capital_gains_tax,
            rate_configured: effective.rate.is_some(),
            account_name: effective.account_name.clone(),
        },
    }
}

pub fn default_app_config_path() -> PathBuf {
    repositories::default_app_config_path()
}

pub fn active_repository_config_path(fallback: impl AsRef<Path>) -> Result<PathBuf> {
    repositories::active_repository_config_path(&default_app_config_path(), fallback)
}

fn bootstrap_repository_entry(
    snapshot: &ApiSnapshot,
    repositories: &[RepositoryEntry],
) -> Result<RepositoryEntry> {
    let git = load_git_remote_settings(&snapshot.config_path)?;
    let repo_state = read_git_repo_state(&snapshot.config.data_dir);
    let remote = repo_state
        .remote_url
        .unwrap_or_else(|| build_ssh_remote_url(&git.host, &git.repo, &git.ssh_user));
    let name = repositories::repository_name_from_path(&snapshot.config.data_dir);
    let id = repositories::unique_repository_id(&name, repositories);
    Ok(RepositoryEntry {
        id,
        name,
        path: snapshot.config.data_dir.clone(),
        config_path: snapshot.config_path.clone(),
        remote,
        branch: git.branch,
        managed: false,
    })
}

fn repository_registry_output(registry: &RepositoryRegistry) -> Result<RepositoryRegistryOutput> {
    let active_repository = registry.active_repository.clone();
    let repositories = registry
        .repositories
        .iter()
        .map(|entry| {
            let state = read_git_repo_state(&entry.path);
            RepositoryOutput {
                id: entry.id.clone(),
                name: entry.name.clone(),
                path: entry.path.display().to_string(),
                config_path: entry.config_path.display().to_string(),
                remote: entry.remote.clone(),
                branch: entry.branch.clone(),
                active: active_repository.as_deref() == Some(entry.id.as_str()),
                cloned: state.cloned && entry.config_path.is_file(),
                commit: state.commit,
                managed: entry.managed,
            }
        })
        .collect();
    Ok(RepositoryRegistryOutput {
        config_path: registry.manifest_path.display().to_string(),
        device_config_path: registry.device_config_path.display().to_string(),
        active_repository,
        repositories,
    })
}

#[cfg(test)]
#[path = "../tests/unit/state_tests.rs"]
mod state_tests;
