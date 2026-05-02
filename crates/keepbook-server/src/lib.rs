use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
#[cfg(feature = "http")]
use axum::extract::{Path as AxumPath, Query, State};
#[cfg(feature = "http")]
use axum::http::StatusCode;
#[cfg(feature = "http")]
use axum::response::{IntoResponse, Response};
#[cfg(feature = "http")]
use axum::routing::{get, post};
#[cfg(feature = "http")]
use axum::{Json, Router};
use chrono::{Local, Utc};
use keepbook::config::{default_config_path, ResolvedConfig};
use keepbook::format::format_base_currency_display;
use keepbook::models::Asset;
use keepbook::storage::{JsonFileStorage, Storage};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use toml_edit::{value, DocumentMut, Item, Table};
#[cfg(feature = "http")]
use tower_http::cors::CorsLayer;
#[cfg(feature = "http")]
use tower_http::trace::TraceLayer;

mod ai_rules;

pub use ai_rules::{
    AiRuleSuggestionInput, AiRuleSuggestionsOutput, AiRuleToolCallOutput, AiRuleTransactionInput,
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

impl ApiState {
    pub fn load(config_path: impl AsRef<Path>) -> Result<Self> {
        let config_path = config_path.as_ref().to_path_buf();
        let config = ResolvedConfig::load_or_default(&config_path)
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

    async fn reload(&self) -> Result<()> {
        let config_path = {
            let inner = self.inner.read().await;
            inner.config_path.clone()
        };
        let config = ResolvedConfig::load_or_default(&config_path)
            .with_context(|| format!("failed to reload config from {}", config_path.display()))?;
        let storage = Arc::new(JsonFileStorage::new(&config.data_dir));
        let mut inner = self.inner.write().await;
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
            history_defaults: history_defaults(&state.config),
            filtering: filtering_output(&state.config, &state.config, None),
        }
    }

    pub async fn overview(&self, query: OverviewQuery) -> Result<OverviewOutput> {
        let state = self.snapshot().await;
        let effective_config =
            config_with_filter_overrides(&state.config, query.include_latent_capital_gains_tax);
        let connections = keepbook::app::list_connections(state.storage.as_ref()).await?;
        let accounts = keepbook::app::list_accounts(state.storage.as_ref()).await?;
        let balances =
            keepbook::app::list_balances(state.storage.as_ref(), &effective_config).await?;
        let snapshot = keepbook::app::portfolio_snapshot(
            state.storage.clone(),
            &effective_config,
            None,
            None,
            "both".to_string(),
            false,
            None,
            None,
            None,
            false,
            true,
            false,
            false,
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
                    state.storage.clone(),
                    &effective_config,
                    None,
                    history_start,
                    history_end,
                    history_granularity,
                    include_prices,
                )
                .await?,
            )?)
        } else {
            None
        };

        Ok(OverviewOutput {
            config_path: state.config_path.display().to_string(),
            data_dir: state.config.data_dir.display().to_string(),
            reporting_currency: state.config.reporting_currency.clone(),
            history_defaults: history_defaults(&state.config),
            filtering: filtering_output(
                &state.config,
                &effective_config,
                query.include_latent_capital_gains_tax,
            ),
            connections: json_value(connections)?,
            accounts: json_value(accounts)?,
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
            None,
            None,
            "account".to_string(),
            false,
            None,
            None,
            None,
            false,
            true,
            false,
            false,
        )
        .await;

        let (total_label, as_of_date, portfolio_breakdown_lines) = match portfolio_result {
            Ok(snapshot) => (
                format_tray_currency(&snapshot.total_value, &snapshot.currency, &state.config),
                snapshot.as_of_date.to_string(),
                build_portfolio_breakdown_lines(&snapshot, &state.config),
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
                query.sort_by_amount,
                !query.include_ignored,
                &state.config,
            )
            .await?,
        )
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
                    group_by: query.group_by.unwrap_or_else(|| "category".to_string()),
                    top: query.top,
                    lookback_days: query.lookback_days.unwrap_or(7),
                    include_noncurrency: query.include_noncurrency,
                    include_empty: query.include_empty,
                },
            )
            .await?,
        )
    }

    pub async fn set_transaction_category(
        &self,
        input: TransactionCategoryInput,
    ) -> Result<serde_json::Value> {
        let state = self.snapshot().await;
        let category = input
            .category
            .map(|category| category.trim().to_string())
            .filter(|category| !category.is_empty());
        keepbook::app::set_transaction_annotation(
            state.storage.as_ref(),
            &state.config,
            &input.account_id,
            &input.transaction_id,
            None,
            false,
            None,
            false,
            category,
            input.clear_category,
            None,
            false,
            Vec::new(),
            false,
            false,
            None,
            false,
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
        let granularity = query
            .granularity
            .unwrap_or_else(|| effective_config.history.portfolio_granularity.clone());
        let include_prices = query
            .include_prices
            .unwrap_or(effective_config.history.include_prices);
        let selection = keepbook::app::resolve_portfolio_history_selection(
            state.storage.as_ref(),
            &effective_config,
            query.account.as_deref(),
            query.connection.as_deref(),
        )
        .await?;
        let output = match selection {
            keepbook::app::PortfolioHistorySelection::Portfolio => {
                keepbook::app::portfolio_history(
                    state.storage.clone(),
                    &effective_config,
                    query.currency,
                    query.start,
                    query.end,
                    granularity,
                    include_prices,
                )
                .await?
            }
            keepbook::app::PortfolioHistorySelection::Accounts(account_ids) => {
                keepbook::app::portfolio_history_for_accounts(
                    state.storage.clone(),
                    &effective_config,
                    query.currency,
                    query.start,
                    query.end,
                    granularity,
                    include_prices,
                    account_ids,
                )
                .await?
            }
            keepbook::app::PortfolioHistorySelection::LatentCapitalGainsTax => {
                keepbook::app::latent_capital_gains_tax_history(
                    state.storage.clone(),
                    &effective_config,
                    query.currency,
                    query.start,
                    query.end,
                    granularity,
                    include_prices,
                )
                .await?
            }
        };
        json_value(output)
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
        Ok(GitSettingsOutput {
            config_path: snapshot.config_path.display().to_string(),
            data_dir: snapshot.config.data_dir.display().to_string(),
            git,
            repo_state: read_git_repo_state(&snapshot.config.data_dir),
        })
    }

    pub async fn save_git_settings(&self, input: GitSettingsInput) -> Result<GitSettingsOutput> {
        let snapshot = self.snapshot().await;
        write_git_settings(&snapshot.config_path, &input)?;
        self.reload().await?;
        self.git_settings().await
    }

    pub async fn sync_git_repo(&self, input: GitSyncInput) -> Result<GitSyncOutput> {
        let snapshot = self.snapshot().await;
        let data_dir = resolve_input_data_dir(&snapshot.config_path, input.data_dir.trim());
        validate_git_data_dir(&data_dir)?;
        prepare_git_ssh_environment(&snapshot.config_path)?;
        let configured_git = load_git_remote_settings(&snapshot.config_path)?;
        let private_key_pem =
            resolve_git_private_key(&snapshot.config_path, &configured_git, &input)?;
        let repo_state = read_git_repo_state(&data_dir);
        let branch = repo_state
            .branch
            .clone()
            .unwrap_or_else(|| non_empty(input.branch.trim(), "master"));
        let remote_url = repo_state
            .remote_url
            .clone()
            .unwrap_or_else(|| build_ssh_remote_url(&input.host, &input.repo, &input.ssh_user));
        sync_git_ssh(&data_dir, &remote_url, &branch, &private_key_pem)?;

        if input.save_settings {
            let ssh_key_path = if input.private_key_pem.trim().is_empty() {
                configured_git.ssh_key_path
            } else {
                Some(persist_git_private_key(
                    &snapshot.config_path,
                    &input.private_key_pem,
                )?)
            };
            write_git_settings(
                &snapshot.config_path,
                &GitSettingsInput {
                    data_dir: input.data_dir.clone(),
                    host: input.host.clone(),
                    repo: input.repo.clone(),
                    branch: input.branch.clone(),
                    ssh_user: input.ssh_user.clone(),
                    ssh_key_path,
                },
            )?;
        }
        self.reload().await?;

        Ok(GitSyncOutput {
            ok: true,
            data_dir: data_dir.display().to_string(),
            remote_url,
            branch,
        })
    }

    pub async fn sync_connections(&self, input: SyncConnectionsInput) -> Result<serde_json::Value> {
        let snapshot = self.snapshot().await;
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

#[derive(Debug, Serialize)]
pub struct HealthOutput {
    pub ok: bool,
}

#[derive(Debug, Serialize)]
pub struct ConfigOutput {
    pub config_path: String,
    pub data_dir: String,
    pub reporting_currency: String,
    pub history_defaults: HistoryDefaultsOutput,
    pub filtering: FilteringOutput,
}

#[derive(Debug, Serialize)]
pub struct HistoryDefaultsOutput {
    pub portfolio_granularity: String,
    pub change_points_granularity: String,
    pub include_prices: bool,
    pub graph_range: String,
    pub graph_granularity: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitRemoteSettings {
    pub host: String,
    pub repo: String,
    pub branch: String,
    pub ssh_user: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ssh_key_path: Option<String>,
}

impl Default for GitRemoteSettings {
    fn default() -> Self {
        Self {
            host: "github.com".to_string(),
            repo: "colonelpanic8/keepbook-data".to_string(),
            branch: "master".to_string(),
            ssh_user: "git".to_string(),
            ssh_key_path: None,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct GitSettingsOutput {
    pub config_path: String,
    pub data_dir: String,
    pub git: GitRemoteSettings,
    pub repo_state: GitRepoState,
}

#[derive(Debug, Deserialize)]
pub struct GitSettingsInput {
    pub data_dir: String,
    pub host: String,
    pub repo: String,
    pub branch: String,
    pub ssh_user: String,
    #[serde(default)]
    pub ssh_key_path: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct GitSyncInput {
    pub data_dir: String,
    pub host: String,
    pub repo: String,
    pub branch: String,
    pub ssh_user: String,
    pub private_key_pem: String,
    #[serde(default)]
    pub save_settings: bool,
}

#[derive(Debug, Serialize)]
pub struct GitSyncOutput {
    pub ok: bool,
    pub data_dir: String,
    pub remote_url: String,
    pub branch: String,
}

#[derive(Debug, Deserialize)]
pub struct SyncConnectionsInput {
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default)]
    pub if_stale: bool,
    #[serde(default)]
    pub full_transactions: bool,
}

#[derive(Debug, Deserialize)]
pub struct SyncPricesInput {
    #[serde(default)]
    pub scope: Option<String>,
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default)]
    pub force: bool,
    #[serde(default)]
    pub quote_staleness_seconds: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GitRepoState {
    pub cloned: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct FilteringOutput {
    pub latent_capital_gains_tax: LatentCapitalGainsTaxFilterOutput,
}

#[derive(Debug, Serialize)]
pub struct LatentCapitalGainsTaxFilterOutput {
    pub configured_enabled: bool,
    pub effective_enabled: bool,
    pub override_enabled: Option<bool>,
    pub rate_configured: bool,
    pub account_name: String,
}

#[derive(Debug, Serialize)]
pub struct OverviewOutput {
    pub config_path: String,
    pub data_dir: String,
    pub reporting_currency: String,
    pub history_defaults: HistoryDefaultsOutput,
    pub filtering: FilteringOutput,
    pub connections: serde_json::Value,
    pub accounts: serde_json::Value,
    pub balances: serde_json::Value,
    pub snapshot: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub history: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
pub struct TraySnapshotOutput {
    pub total_label: String,
    pub as_of_date: String,
    pub history_lines: Vec<String>,
    pub portfolio_breakdown_lines: Vec<String>,
    pub spending_lines: Vec<String>,
    pub transaction_lines: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct TransactionQuery {
    pub start: Option<String>,
    pub end: Option<String>,
    #[serde(default)]
    pub sort_by_amount: bool,
    #[serde(default)]
    pub include_ignored: bool,
}

#[derive(Debug, Deserialize)]
pub struct TransactionCategoryInput {
    pub account_id: String,
    pub transaction_id: String,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub clear_category: bool,
}

#[derive(Debug, Deserialize)]
pub struct SpendingQuery {
    pub currency: Option<String>,
    pub start: Option<String>,
    pub end: Option<String>,
    pub period: Option<String>,
    pub period_alignment: Option<String>,
    pub tz: Option<String>,
    pub week_start: Option<String>,
    pub bucket_days: Option<u64>,
    pub account: Option<String>,
    pub connection: Option<String>,
    pub status: Option<String>,
    pub direction: Option<String>,
    pub group_by: Option<String>,
    pub top: Option<usize>,
    pub lookback_days: Option<u32>,
    #[serde(default)]
    pub include_noncurrency: bool,
    #[serde(default)]
    pub include_empty: bool,
}

#[derive(Debug, Deserialize, Default)]
pub struct ProposedTransactionEditsQuery {
    #[serde(default)]
    pub include_decided: bool,
}

#[derive(Debug, Deserialize)]
pub struct HistoryQuery {
    pub currency: Option<String>,
    pub start: Option<String>,
    pub end: Option<String>,
    pub granularity: Option<String>,
    pub include_prices: Option<bool>,
    pub include_latent_capital_gains_tax: Option<bool>,
    pub account: Option<String>,
    pub connection: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct OverviewQuery {
    pub history_start: Option<String>,
    pub history_end: Option<String>,
    pub history_granularity: Option<String>,
    pub include_prices: Option<bool>,
    pub include_latent_capital_gains_tax: Option<bool>,
    #[serde(default)]
    pub include_history: bool,
}

#[cfg(feature = "http")]
#[derive(Debug, Serialize)]
struct ErrorOutput {
    error: String,
}

#[cfg(feature = "http")]
pub struct ApiError(anyhow::Error);

#[cfg(feature = "http")]
impl From<anyhow::Error> for ApiError {
    fn from(error: anyhow::Error) -> Self {
        Self(error)
    }
}

#[cfg(feature = "http")]
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = Json(ErrorOutput {
            error: self.0.to_string(),
        });
        (StatusCode::INTERNAL_SERVER_ERROR, body).into_response()
    }
}

fn default_history_end() -> String {
    Utc::now().date_naive().to_string()
}

fn json_value<T: Serialize>(value: T) -> Result<serde_json::Value> {
    serde_json::to_value(value).context("failed to encode keepbook API output")
}

fn default_currency_symbol(currency: &str) -> Option<&'static str> {
    match currency.to_ascii_uppercase().as_str() {
        "USD" => Some("$"),
        "EUR" => Some("€"),
        "GBP" => Some("£"),
        "JPY" => Some("¥"),
        _ => None,
    }
}

fn format_tray_currency(value: &str, currency: &str, config: &ResolvedConfig) -> String {
    let dp = config.display.currency_decimals.or(Some(2));
    let symbol = config
        .display
        .currency_symbol
        .as_deref()
        .or_else(|| default_currency_symbol(currency));
    match Decimal::from_str(value) {
        Ok(d) => {
            let formatted = format_base_currency_display(
                d,
                dp,
                config.display.currency_grouping,
                symbol,
                config.display.currency_fixed_decimals,
            );
            if symbol.is_some() {
                formatted
            } else {
                format!("{formatted} {currency}")
            }
        }
        Err(_) => value.to_string(),
    }
}

fn format_history_change_for_tray(percentage_change: Option<&str>) -> String {
    match percentage_change {
        Some("N/A") | None => "N/A".to_string(),
        Some(value) if value.starts_with('-') => format!("{value}%"),
        Some(value) => format!("+{value}%"),
    }
}

fn normalize_spending_windows_days(windows: &[u32]) -> Vec<u32> {
    let mut normalized: Vec<u32> = windows.iter().copied().filter(|days| *days > 0).collect();
    normalized.sort_unstable();
    normalized.dedup();
    normalized
}

fn format_spending_window_label(days: u32) -> String {
    match days {
        365 => "year".to_string(),
        _ if days.is_multiple_of(365) => format!("{} years", days / 365),
        _ => format!("{days}d"),
    }
}

fn last_n_days_range(days: u32) -> (chrono::NaiveDate, chrono::NaiveDate) {
    let end = Local::now().date_naive();
    let start = end - chrono::Duration::days(days.saturating_sub(1) as i64);
    (start, end)
}

fn build_portfolio_breakdown_lines(
    snapshot: &keepbook::portfolio::PortfolioSnapshot,
    config: &ResolvedConfig,
) -> Vec<String> {
    let mut lines = vec![format!(
        "Total: {}",
        format_tray_currency(&snapshot.total_value, &snapshot.currency, config)
    )];

    let Some(accounts) = snapshot.by_account.as_ref() else {
        lines.push("No account breakdown available".to_string());
        return lines;
    };

    if accounts.is_empty() {
        lines.push("No accounts with balances".to_string());
        return lines;
    }

    lines.extend(accounts.iter().map(|account| {
        let value = account
            .value_in_base
            .as_deref()
            .map(|value| format_tray_currency(value, &snapshot.currency, config))
            .unwrap_or_else(|| "unpriced".to_string());
        format!(
            "{} / {}: {}",
            account.connection_name, account.account_name, value
        )
    }));

    lines
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
                        config,
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
            let value = format_tray_currency(&report.total, &report.currency, config);
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

        rows.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        rows.truncate(config.tray.transaction_count);

        Ok(rows
            .iter()
            .map(|row| {
                let date = row.timestamp.with_timezone(&Local).format("%m-%d");
                let currency = match &row.asset {
                    Asset::Currency { iso_code } => iso_code.as_str(),
                    Asset::Equity { ticker, .. } => ticker.as_str(),
                    Asset::Crypto { symbol, .. } => symbol.as_str(),
                };
                let amount = format_tray_currency(&row.amount, currency, config);
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

#[cfg(feature = "http")]
pub fn router(state: ApiState) -> Router {
    Router::new()
        .route("/api/health", get(health))
        .route("/api/config", get(config))
        .route("/api/overview", get(overview))
        .route("/api/connections", get(connections))
        .route("/api/accounts", get(accounts))
        .route("/api/balances", get(balances))
        .route("/api/transactions", get(transactions))
        .route("/api/transactions/category", post(set_transaction_category))
        .route("/api/spending", get(spending))
        .route("/api/tray", get(tray))
        .route(
            "/api/proposed-transaction-edits",
            get(proposed_transaction_edits),
        )
        .route(
            "/api/proposed-transaction-edits/{id}/approve",
            post(approve_proposed_transaction_edit),
        )
        .route(
            "/api/proposed-transaction-edits/{id}/reject",
            post(reject_proposed_transaction_edit),
        )
        .route(
            "/api/proposed-transaction-edits/{id}/remove",
            post(remove_proposed_transaction_edit),
        )
        .route("/api/portfolio/history", get(portfolio_history))
        .route("/api/git/merge-master", post(merge_origin_master))
        .route(
            "/api/git/settings",
            get(git_settings).put(save_git_settings),
        )
        .route("/api/git/sync", post(sync_git_repo))
        .route("/api/sync/connections", post(sync_connections))
        .route("/api/sync/prices", post(sync_prices))
        .route("/api/ai/rules/suggest", post(suggest_ai_rules))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

#[cfg(feature = "http")]
pub async fn serve(config_path: impl AsRef<Path>, addr: SocketAddr) -> Result<()> {
    let state = ApiState::load(config_path)?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(%addr, "keepbook API server listening");
    axum::serve(listener, router(state))
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

#[cfg(feature = "http")]
async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
}

#[cfg(feature = "http")]
async fn health() -> Json<HealthOutput> {
    Json(HealthOutput { ok: true })
}

#[cfg(feature = "http")]
async fn config(State(state): State<ApiState>) -> Json<ConfigOutput> {
    Json(state.config_output().await)
}

#[cfg(feature = "http")]
async fn overview(
    State(state): State<ApiState>,
    Query(query): Query<OverviewQuery>,
) -> Result<Json<OverviewOutput>, ApiError> {
    Ok(Json(state.overview(query).await?))
}

#[cfg(feature = "http")]
async fn connections(State(state): State<ApiState>) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(state.connections().await?))
}

#[cfg(feature = "http")]
async fn accounts(State(state): State<ApiState>) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(state.accounts().await?))
}

#[cfg(feature = "http")]
async fn balances(State(state): State<ApiState>) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(state.balances().await?))
}

#[cfg(feature = "http")]
async fn transactions(
    State(state): State<ApiState>,
    Query(query): Query<TransactionQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(state.transactions(query).await?))
}

#[cfg(feature = "http")]
async fn set_transaction_category(
    State(state): State<ApiState>,
    Json(input): Json<TransactionCategoryInput>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(state.set_transaction_category(input).await?))
}

#[cfg(feature = "http")]
async fn spending(
    State(state): State<ApiState>,
    Query(query): Query<SpendingQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(state.spending(query).await?))
}

#[cfg(feature = "http")]
async fn tray(State(state): State<ApiState>) -> Result<Json<TraySnapshotOutput>, ApiError> {
    Ok(Json(state.tray_snapshot().await?))
}

#[cfg(feature = "http")]
async fn proposed_transaction_edits(
    State(state): State<ApiState>,
    Query(query): Query<ProposedTransactionEditsQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(state.proposed_transaction_edits(query).await?))
}

#[cfg(feature = "http")]
async fn approve_proposed_transaction_edit(
    State(state): State<ApiState>,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(state.approve_proposed_transaction_edit(id).await?))
}

#[cfg(feature = "http")]
async fn reject_proposed_transaction_edit(
    State(state): State<ApiState>,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(state.reject_proposed_transaction_edit(id).await?))
}

#[cfg(feature = "http")]
async fn remove_proposed_transaction_edit(
    State(state): State<ApiState>,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(state.remove_proposed_transaction_edit(id).await?))
}

#[cfg(feature = "http")]
async fn portfolio_history(
    State(state): State<ApiState>,
    Query(query): Query<HistoryQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(state.portfolio_history(query).await?))
}

#[cfg(feature = "http")]
async fn merge_origin_master(
    State(state): State<ApiState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(state.merge_origin_master().await?))
}

#[cfg(feature = "http")]
async fn git_settings(State(state): State<ApiState>) -> Result<Json<GitSettingsOutput>, ApiError> {
    Ok(Json(state.git_settings().await?))
}

#[cfg(feature = "http")]
async fn save_git_settings(
    State(state): State<ApiState>,
    Json(input): Json<GitSettingsInput>,
) -> Result<Json<GitSettingsOutput>, ApiError> {
    Ok(Json(state.save_git_settings(input).await?))
}

#[cfg(feature = "http")]
async fn sync_git_repo(
    State(state): State<ApiState>,
    Json(input): Json<GitSyncInput>,
) -> Result<Json<GitSyncOutput>, ApiError> {
    Ok(Json(state.sync_git_repo(input).await?))
}

#[cfg(feature = "http")]
async fn sync_connections(
    State(state): State<ApiState>,
    Json(input): Json<SyncConnectionsInput>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(state.sync_connections(input).await?))
}

#[cfg(feature = "http")]
async fn sync_prices(
    State(state): State<ApiState>,
    Json(input): Json<SyncPricesInput>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(state.sync_prices(input).await?))
}

#[cfg(feature = "http")]
async fn suggest_ai_rules(
    State(state): State<ApiState>,
    Json(input): Json<AiRuleSuggestionInput>,
) -> Result<Json<AiRuleSuggestionsOutput>, ApiError> {
    Ok(Json(state.suggest_ai_rules(input).await?))
}

fn load_config_doc(config_path: &Path) -> Result<DocumentMut> {
    if config_path.exists() {
        let content = std::fs::read_to_string(config_path)
            .with_context(|| format!("failed to read {}", config_path.display()))?;
        content
            .parse::<DocumentMut>()
            .with_context(|| format!("failed to parse {}", config_path.display()))
    } else {
        Ok(DocumentMut::new())
    }
}

fn load_git_remote_settings(config_path: &Path) -> Result<GitRemoteSettings> {
    let doc = load_config_doc(config_path)?;
    let defaults = GitRemoteSettings::default();
    let git_sync = doc.get("git_sync");
    Ok(GitRemoteSettings {
        host: table_string(git_sync, "host").unwrap_or(defaults.host),
        repo: table_string(git_sync, "repo").unwrap_or(defaults.repo),
        branch: table_string(git_sync, "branch").unwrap_or(defaults.branch),
        ssh_user: table_string(git_sync, "ssh_user").unwrap_or(defaults.ssh_user),
        ssh_key_path: table_string(git_sync, "ssh_key_path"),
    })
}

fn table_string(table: Option<&Item>, key: &str) -> Option<String> {
    table?
        .as_table_like()?
        .get(key)?
        .as_str()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn write_git_settings(config_path: &Path, input: &GitSettingsInput) -> Result<()> {
    let mut doc = load_config_doc(config_path)?;
    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }

    doc["data_dir"] = value(input.data_dir.trim());
    if !doc
        .get("git_sync")
        .is_some_and(|item| item.as_table_like().is_some())
    {
        doc.insert("git_sync", Item::Table(Table::new()));
    }
    doc["git_sync"]["host"] = value(non_empty(input.host.trim(), "github.com"));
    doc["git_sync"]["repo"] = value(input.repo.trim());
    doc["git_sync"]["branch"] = value(non_empty(input.branch.trim(), "master"));
    doc["git_sync"]["ssh_user"] = value(non_empty(input.ssh_user.trim(), "git"));
    if let Some(ssh_key_path) = input
        .ssh_key_path
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        doc["git_sync"]["ssh_key_path"] = value(ssh_key_path);
    }

    std::fs::write(config_path, doc.to_string())
        .with_context(|| format!("failed to write {}", config_path.display()))?;
    Ok(())
}

fn non_empty(value: &str, default: &str) -> String {
    if value.is_empty() {
        default.to_string()
    } else {
        value.to_string()
    }
}

fn resolve_input_data_dir(config_path: &Path, data_dir: &str) -> PathBuf {
    let path = PathBuf::from(data_dir);
    if path.is_absolute() {
        path
    } else {
        config_path
            .parent()
            .map(|parent| parent.join(path.clone()))
            .unwrap_or(path)
    }
}

fn validate_git_data_dir(data_dir: &Path) -> Result<()> {
    if data_dir.parent().is_none() {
        anyhow::bail!(
            "Git data directory cannot be a filesystem root: {}",
            data_dir.display()
        );
    }

    Ok(())
}

fn read_git_repo_state(data_dir: &Path) -> GitRepoState {
    let Ok(repo) = git2::Repository::open(data_dir) else {
        return GitRepoState {
            cloned: false,
            remote_url: None,
            branch: None,
            commit: None,
        };
    };

    let remote_url = repo
        .find_remote("origin")
        .ok()
        .and_then(|remote| remote.url().map(ToString::to_string));
    let head = repo.head().ok();
    let branch = head
        .as_ref()
        .filter(|head| head.is_branch())
        .and_then(|head| head.shorthand().map(ToString::to_string));
    let commit = head
        .as_ref()
        .and_then(|head| head.peel_to_commit().ok())
        .map(|commit| commit.id().to_string());

    GitRepoState {
        cloned: true,
        remote_url,
        branch,
        commit,
    }
}

fn prepare_git_ssh_environment(config_path: &Path) -> Result<()> {
    let Some(config_dir) = config_path.parent() else {
        return Ok(());
    };

    let ssh_dir = config_dir.join(".ssh");
    std::fs::create_dir_all(&ssh_dir)
        .with_context(|| format!("failed to create {}", ssh_dir.display()))?;

    let known_hosts = ssh_dir.join("known_hosts");
    if !known_hosts.exists() {
        std::fs::write(&known_hosts, "")
            .with_context(|| format!("failed to create {}", known_hosts.display()))?;
    }

    if cfg!(target_os = "android") || std::env::var_os("HOME").is_none() {
        std::env::set_var("HOME", config_dir);
    }

    Ok(())
}

fn default_git_ssh_key_path(config_path: &Path) -> Result<PathBuf> {
    let Some(config_dir) = config_path.parent() else {
        anyhow::bail!("cannot resolve SSH key path without a config directory");
    };

    Ok(config_dir.join(".ssh").join("keepbook_sync_key"))
}

fn persist_git_private_key(config_path: &Path, private_key_pem: &str) -> Result<String> {
    let key_path = default_git_ssh_key_path(config_path)?;
    let Some(parent) = key_path.parent() else {
        anyhow::bail!(
            "cannot resolve SSH key directory for {}",
            key_path.display()
        );
    };

    std::fs::create_dir_all(parent)
        .with_context(|| format!("failed to create {}", parent.display()))?;

    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .mode(0o600)
            .open(&key_path)
            .with_context(|| format!("failed to open {}", key_path.display()))?;
        file.write_all(private_key_pem.trim_end().as_bytes())
            .with_context(|| format!("failed to write {}", key_path.display()))?;
        file.write_all(b"\n")
            .with_context(|| format!("failed to finalize {}", key_path.display()))?;
        std::fs::set_permissions(&key_path, std::fs::Permissions::from_mode(0o600))
            .with_context(|| format!("failed to set permissions on {}", key_path.display()))?;
    }

    #[cfg(not(unix))]
    {
        std::fs::write(&key_path, format!("{}\n", private_key_pem.trim_end()))
            .with_context(|| format!("failed to write {}", key_path.display()))?;
    }

    Ok(key_path.display().to_string())
}

fn resolve_git_private_key(
    config_path: &Path,
    settings: &GitRemoteSettings,
    input: &GitSyncInput,
) -> Result<String> {
    let inline_key = input.private_key_pem.trim();
    if !inline_key.is_empty() {
        return Ok(input.private_key_pem.clone());
    }

    let Some(key_path) = settings.ssh_key_path.as_deref() else {
        anyhow::bail!("SSH private key is empty and no saved SSH key path is configured");
    };

    let key_path = resolve_config_relative_path(config_path, key_path);
    std::fs::read_to_string(&key_path)
        .with_context(|| format!("failed to read saved SSH key {}", key_path.display()))
        .and_then(|contents| {
            if contents.trim().is_empty() {
                anyhow::bail!("saved SSH key {} is empty", key_path.display());
            }
            Ok(contents)
        })
}

fn activate_age_identity_from_git_settings(config_path: &Path) -> Result<()> {
    let settings = load_git_remote_settings(config_path)?;
    let Some(ssh_key_path) = settings
        .ssh_key_path
        .as_deref()
        .map(str::trim)
        .filter(|path| !path.is_empty())
    else {
        return Ok(());
    };

    let resolved = resolve_config_relative_path(config_path, ssh_key_path);
    std::env::set_var("KEEPBOOK_CREDENTIALS_AGE_IDENTITY_PATH", resolved);
    Ok(())
}

fn resolve_config_relative_path(config_path: &Path, path: &str) -> PathBuf {
    let path = expand_home_path(path);
    if path.is_absolute() {
        path
    } else {
        config_path
            .parent()
            .map(|parent| parent.join(path.clone()))
            .unwrap_or(path)
    }
}

fn expand_home_path(path: &str) -> PathBuf {
    if path == "~" {
        return std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(path));
    }

    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home).join(rest);
        }
    }

    PathBuf::from(path)
}

fn log_git_sync_event(message: impl AsRef<str>) {
    let message = message.as_ref();
    tracing::info!("{message}");

    #[cfg(target_os = "android")]
    eprintln!("{message}");
}

fn normalize_repo_path(repo: &str) -> String {
    let repo = repo.trim();
    if repo.ends_with(".git") {
        repo.to_string()
    } else {
        format!("{repo}.git")
    }
}

fn build_ssh_remote_url(host: &str, repo: &str, ssh_user: &str) -> String {
    let repo = repo.trim();
    if is_explicit_git_remote(repo) {
        return repo.to_string();
    }

    let repo = normalize_repo_path(repo);
    let host = host.trim();
    let ssh_user = non_empty(ssh_user.trim(), "git");

    if host.contains("://") || host.contains(':') {
        let host = host.strip_prefix("ssh://").unwrap_or(host);
        format!("ssh://{ssh_user}@{host}/{repo}")
    } else {
        format!("{ssh_user}@{host}:{repo}")
    }
}

fn is_explicit_git_remote(remote: &str) -> bool {
    remote.contains("://") || (remote.contains('@') && remote.contains(':'))
}

fn sync_git_ssh(
    data_dir: &Path,
    remote_url: &str,
    branch: &str,
    private_key_pem: &str,
) -> Result<()> {
    use git2::{build::CheckoutBuilder, build::RepoBuilder, Cred, FetchOptions, RemoteCallbacks};
    use git2::{Repository, ResetType};

    fn fetch_options(ssh_user: &str, private_key_pem: &str) -> FetchOptions<'static> {
        let ssh_user = ssh_user.to_string();
        let private_key_pem = private_key_pem.to_string();
        let mut callbacks = RemoteCallbacks::new();
        callbacks.credentials(move |_url, username_from_url, _allowed| {
            let username = username_from_url.unwrap_or(&ssh_user);
            Cred::ssh_key_from_memory(username, None, &private_key_pem, None)
        });
        callbacks.certificate_check(|_cert, _host| Ok(git2::CertificateCheckStatus::CertificateOk));

        let mut fetch_options = FetchOptions::new();
        fetch_options.remote_callbacks(callbacks);
        fetch_options
    }

    let ssh_user = remote_url
        .split('@')
        .next()
        .and_then(|prefix| prefix.rsplit(['/', ':']).next())
        .filter(|value| !value.is_empty())
        .unwrap_or("git")
        .to_string();

    let is_existing_repo = data_dir.join(".git").exists();
    log_git_sync_event(format!(
        "Keepbook git sync {} {remote_url} branch {branch} in {}",
        if is_existing_repo {
            "fetching"
        } else {
            "cloning"
        },
        data_dir.display()
    ));

    let repo = if is_existing_repo {
        let repo = Repository::open(data_dir)
            .with_context(|| format!("failed to open git repository {}", data_dir.display()))?;
        match repo.find_remote("origin") {
            Ok(remote) => {
                if remote.url() != Some(remote_url) {
                    repo.remote_set_url("origin", remote_url)?;
                }
            }
            Err(_) => {
                repo.remote("origin", remote_url)?;
            }
        }
        repo
    } else {
        if let Some(parent) = data_dir.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        let mut builder = RepoBuilder::new();
        builder.branch(branch);
        builder.fetch_options(fetch_options(&ssh_user, private_key_pem));
        builder
            .clone(remote_url, data_dir)
            .with_context(|| format!("failed to clone {remote_url} into {}", data_dir.display()))?
    };

    {
        let mut remote = repo.find_remote("origin")?;
        let mut options = fetch_options(&ssh_user, private_key_pem);
        let refspec = format!("refs/heads/{branch}:refs/remotes/origin/{branch}");
        remote
            .fetch(&[refspec.as_str()], Some(&mut options), None)
            .with_context(|| format!("failed to fetch origin/{branch}"))?;
    }

    let remote_ref = format!("refs/remotes/origin/{branch}");
    let obj = repo
        .revparse_single(&remote_ref)
        .with_context(|| format!("failed to resolve {remote_ref}"))?;
    let commit = obj.peel_to_commit()?;
    let local_ref = format!("refs/heads/{branch}");
    if repo.find_reference(&local_ref).is_err() {
        repo.branch(branch, &commit, true)?;
    }
    repo.set_head(&local_ref)?;
    repo.checkout_head(Some(CheckoutBuilder::new().force()))?;
    repo.reset(commit.as_object(), ResetType::Hard, None)?;
    log_git_sync_event(format!(
        "Keepbook git sync checked out {branch} at {} in {}",
        commit.id(),
        data_dir.display()
    ));

    Ok(())
}

pub fn default_listen_addr() -> SocketAddr {
    SocketAddr::from(([127, 0, 0, 1], 8799))
}

pub fn default_server_config_path() -> PathBuf {
    default_config_path()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[cfg(unix)]
    #[test]
    fn validate_git_data_dir_rejects_filesystem_root() {
        let error = validate_git_data_dir(Path::new("/")).expect_err("root should be rejected");
        assert!(error.to_string().contains("filesystem root"));
    }

    #[test]
    fn validate_git_data_dir_accepts_nested_path() {
        validate_git_data_dir(Path::new("/tmp/keepbook-data"))
            .expect("nested path should be valid");
    }

    #[test]
    fn load_git_remote_settings_ignores_non_table_git_sync() -> Result<()> {
        let config_path = unique_test_config_path("load-git-non-table");
        write_test_config(&config_path, "git_sync = \"invalid\"\n")?;

        let settings = load_git_remote_settings(&config_path)?;
        let defaults = GitRemoteSettings::default();

        assert_eq!(settings.host, defaults.host);
        assert_eq!(settings.repo, defaults.repo);
        assert_eq!(settings.branch, defaults.branch);
        assert_eq!(settings.ssh_user, defaults.ssh_user);
        assert_eq!(settings.ssh_key_path, defaults.ssh_key_path);
        remove_test_config(config_path);
        Ok(())
    }

    #[test]
    fn write_git_settings_creates_missing_git_sync_table() -> Result<()> {
        let config_path = unique_test_config_path("write-git-missing-table");
        write_test_config(&config_path, "data_dir = \"./old-data\"\n")?;

        write_git_settings(
            &config_path,
            &GitSettingsInput {
                data_dir: "/tmp/keepbook-data".to_string(),
                host: "github.com".to_string(),
                repo: "colonelpanic8/keepbook-data".to_string(),
                branch: "master".to_string(),
                ssh_user: "git".to_string(),
                ssh_key_path: Some(".ssh/keepbook_sync_key".to_string()),
            },
        )?;

        let settings = load_git_remote_settings(&config_path)?;
        assert_eq!(settings.host, "github.com");
        assert_eq!(settings.repo, "colonelpanic8/keepbook-data");
        assert_eq!(settings.branch, "master");
        assert_eq!(settings.ssh_user, "git");
        assert_eq!(
            settings.ssh_key_path.as_deref(),
            Some(".ssh/keepbook_sync_key")
        );
        let content = std::fs::read_to_string(&config_path)?;
        assert!(content.contains("[git_sync]"));
        remove_test_config(config_path);
        Ok(())
    }

    fn unique_test_config_path(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after Unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "keepbook-server-{name}-{}-{nanos}/keepbook.toml",
            std::process::id()
        ))
    }

    fn write_test_config(path: &Path, contents: &str) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, contents)?;
        Ok(())
    }

    fn remove_test_config(path: PathBuf) {
        if let Some(parent) = path.parent() {
            let _ = std::fs::remove_dir_all(parent);
        }
    }
}
