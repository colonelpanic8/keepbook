use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::Utc;
use keepbook::app::{default_portfolio_history_granularity, default_portfolio_include_prices};
use keepbook::config::{default_config_path, ResolvedConfig};
use keepbook::storage::{JsonFileStorage, Storage};
use serde::{Deserialize, Serialize};
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

#[derive(Clone)]
pub struct ApiState {
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
            config_path,
            config,
            storage,
        })
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
}

#[derive(Debug, Serialize)]
pub struct OverviewOutput {
    pub config_path: String,
    pub data_dir: String,
    pub reporting_currency: String,
    pub connections: serde_json::Value,
    pub accounts: serde_json::Value,
    pub balances: serde_json::Value,
    pub snapshot: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub history: Option<serde_json::Value>,
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
pub struct HistoryQuery {
    pub currency: Option<String>,
    pub start: Option<String>,
    pub end: Option<String>,
    #[serde(default = "default_portfolio_history_granularity")]
    pub granularity: String,
    #[serde(default = "default_portfolio_include_prices")]
    pub include_prices: bool,
}

#[derive(Debug, Deserialize)]
pub struct OverviewQuery {
    pub history_start: Option<String>,
    pub history_end: Option<String>,
    #[serde(default = "default_portfolio_history_granularity")]
    pub history_granularity: String,
    #[serde(default = "default_portfolio_include_prices")]
    pub include_prices: bool,
    #[serde(default)]
    pub include_history: bool,
}

#[derive(Debug, Serialize)]
struct ErrorOutput {
    error: String,
}

pub struct ApiError(anyhow::Error);

impl From<anyhow::Error> for ApiError {
    fn from(error: anyhow::Error) -> Self {
        Self(error)
    }
}

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

pub fn router(state: ApiState) -> Router {
    Router::new()
        .route("/api/health", get(health))
        .route("/api/config", get(config))
        .route("/api/overview", get(overview))
        .route("/api/connections", get(connections))
        .route("/api/accounts", get(accounts))
        .route("/api/balances", get(balances))
        .route("/api/transactions", get(transactions))
        .route("/api/portfolio/history", get(portfolio_history))
        .route("/api/git/merge-master", post(merge_origin_master))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

pub async fn serve(config_path: impl AsRef<Path>, addr: SocketAddr) -> Result<()> {
    let state = ApiState::load(config_path)?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(%addr, "keepbook API server listening");
    axum::serve(listener, router(state))
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
}

async fn health() -> Json<HealthOutput> {
    Json(HealthOutput { ok: true })
}

async fn config(State(state): State<ApiState>) -> Json<ConfigOutput> {
    Json(ConfigOutput {
        config_path: state.config_path.display().to_string(),
        data_dir: state.config.data_dir.display().to_string(),
        reporting_currency: state.config.reporting_currency.clone(),
    })
}

async fn overview(
    State(state): State<ApiState>,
    Query(query): Query<OverviewQuery>,
) -> Result<Json<OverviewOutput>, ApiError> {
    let connections = keepbook::app::list_connections(state.storage.as_ref()).await?;
    let accounts = keepbook::app::list_accounts(state.storage.as_ref()).await?;
    let balances = keepbook::app::list_balances(state.storage.as_ref(), &state.config).await?;
    let snapshot = keepbook::app::portfolio_snapshot(
        state.storage.clone(),
        &state.config,
        None,
        None,
        "both".to_string(),
        false,
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
        Some(json_value(
            keepbook::app::portfolio_history(
                state.storage.clone(),
                &state.config,
                None,
                history_start,
                history_end,
                query.history_granularity,
                query.include_prices,
            )
            .await?,
        )?)
    } else {
        None
    };

    Ok(Json(OverviewOutput {
        config_path: state.config_path.display().to_string(),
        data_dir: state.config.data_dir.display().to_string(),
        reporting_currency: state.config.reporting_currency.clone(),
        connections: json_value(connections)?,
        accounts: json_value(accounts)?,
        balances: json_value(balances)?,
        snapshot: json_value(snapshot)?,
        history,
    }))
}

async fn connections(State(state): State<ApiState>) -> Result<Json<serde_json::Value>, ApiError> {
    let output = keepbook::app::list_connections(state.storage.as_ref()).await?;
    Ok(Json(json_value(output)?))
}

async fn accounts(State(state): State<ApiState>) -> Result<Json<serde_json::Value>, ApiError> {
    let output = keepbook::app::list_accounts(state.storage.as_ref()).await?;
    Ok(Json(json_value(output)?))
}

async fn balances(State(state): State<ApiState>) -> Result<Json<serde_json::Value>, ApiError> {
    let output = keepbook::app::list_balances(state.storage.as_ref(), &state.config).await?;
    Ok(Json(json_value(output)?))
}

async fn transactions(
    State(state): State<ApiState>,
    Query(query): Query<TransactionQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let output = keepbook::app::list_transactions(
        state.storage.as_ref(),
        query.start,
        query.end,
        query.sort_by_amount,
        !query.include_ignored,
        &state.config,
    )
    .await?;
    Ok(Json(json_value(output)?))
}

async fn portfolio_history(
    State(state): State<ApiState>,
    Query(query): Query<HistoryQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let output = keepbook::app::portfolio_history(
        state.storage.clone(),
        &state.config,
        query.currency,
        query.start,
        query.end,
        query.granularity,
        query.include_prices,
    )
    .await?;
    Ok(Json(json_value(output)?))
}

async fn merge_origin_master(
    State(state): State<ApiState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    keepbook::app::run_preflight(
        &state.config,
        keepbook::app::PreflightOptions {
            merge_origin_master: true,
        },
    )?;
    Ok(Json(json_value(())?))
}

pub fn default_listen_addr() -> SocketAddr {
    SocketAddr::from(([127, 0, 0, 1], 8799))
}

pub fn default_server_config_path() -> PathBuf {
    default_config_path()
}
