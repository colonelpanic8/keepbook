use std::net::SocketAddr;
use std::path::Path;

use anyhow::Result;
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
use serde::Serialize;
#[cfg(feature = "http")]
use tower_http::cors::CorsLayer;
#[cfg(feature = "http")]
use tower_http::trace::TraceLayer;

mod ai_rules;
mod dto;
mod git;
mod settings;
mod state;

pub use ai_rules::{
    AiRuleSuggestionInput, AiRuleSuggestionsOutput, AiRuleToolCallOutput, AiRuleTransactionInput,
};
pub use dto::{
    AddRepositoryInput, ApplicationSettingsInput, ApplicationSettingsOutput, AssetsQuery,
    ConfigOutput, FilteringOutput, GitRemoteSettings, GitRepoState, GitSettingsInput,
    GitSettingsOutput, GitSyncCancelToken, GitSyncInput, GitSyncOutput, HealthOutput,
    HistoryDefaultsOutput, HistoryQuery, LatentCapitalGainsTaxFilterOutput, OverviewOutput,
    OverviewQuery, ProposedTransactionEditsQuery, RecurringTransactionReviewInput,
    RecurringTransactionsQuery, RepositoryOutput, RepositoryRegistryOutput, SpendingQuery,
    SyncConnectionsInput, SyncPricesInput, TransactionEffectiveDateInput,
    TransactionIgnoreBatchInput, TransactionQuery, TransactionSubtagsBatchInput,
    TransactionTagTargetInput, TransactionTagsBatchInput, TraySnapshotOutput,
};
pub use keepbook::app::ReviewedRecurringTransactionOutput;
pub use keepbook::config::WindowDecorationsConfig;
pub use settings::{
    default_listen_addr, default_server_config_path, desktop_start_minimized_to_tray,
    desktop_window_decorations,
};
pub use state::{active_repository_config_path, default_app_config_path, ApiState};

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

#[cfg(feature = "http")]
pub fn router(state: ApiState) -> Router {
    Router::new()
        .route("/api/health", get(health))
        .route("/api/config", get(config))
        .route("/api/repositories", get(repositories).post(add_repository))
        .route("/api/repositories/{id}/activate", post(activate_repository))
        .route("/api/repositories/{id}/remove", post(remove_repository))
        .route("/api/overview", get(overview))
        .route("/api/connections", get(connections))
        .route("/api/accounts", get(accounts))
        .route("/api/balances", get(balances))
        .route("/api/transactions", get(transactions))
        .route("/api/recurring-transactions", get(recurring_transactions))
        .route(
            "/api/recurring-transactions/review",
            post(review_recurring_transaction),
        )
        .route("/api/transactions/tags/batch", post(set_transaction_tags))
        .route(
            "/api/transactions/ignore/batch",
            post(set_transaction_ignore),
        )
        .route(
            "/api/transactions/subtags/batch",
            post(set_transaction_subtags),
        )
        .route(
            "/api/transactions/effective-date",
            post(set_transaction_effective_date),
        )
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
        .route("/api/portfolio/assets", get(portfolio_assets))
        .route(
            "/api/portfolio/stacked-history",
            get(portfolio_stacked_history),
        )
        .route("/api/git/merge-master", post(merge_origin_master))
        .route(
            "/api/git/settings",
            get(git_settings).put(save_git_settings),
        )
        .route(
            "/api/application/settings",
            get(application_settings).put(save_application_settings),
        )
        .route("/api/git/sync", post(sync_git_repo))
        .route("/api/sync/connections", post(sync_connections))
        .route("/api/sync/prices", post(sync_prices))
        .route("/api/reload", post(reload_data))
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
async fn recurring_transactions(
    State(state): State<ApiState>,
    Query(query): Query<RecurringTransactionsQuery>,
) -> Result<Json<Vec<ReviewedRecurringTransactionOutput>>, ApiError> {
    Ok(Json(state.recurring_transactions(query).await?))
}

#[cfg(feature = "http")]
async fn review_recurring_transaction(
    State(state): State<ApiState>,
    Json(input): Json<RecurringTransactionReviewInput>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(state.review_recurring_transaction(input).await?))
}

#[cfg(feature = "http")]
async fn set_transaction_tags(
    State(state): State<ApiState>,
    Json(input): Json<TransactionTagsBatchInput>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(state.set_transaction_tags(input).await?))
}

#[cfg(feature = "http")]
async fn set_transaction_ignore(
    State(state): State<ApiState>,
    Json(input): Json<TransactionIgnoreBatchInput>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(state.set_transaction_ignore(input).await?))
}

#[cfg(feature = "http")]
async fn set_transaction_subtags(
    State(state): State<ApiState>,
    Json(input): Json<TransactionSubtagsBatchInput>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(state.set_transaction_subtags(input).await?))
}

#[cfg(feature = "http")]
async fn set_transaction_effective_date(
    State(state): State<ApiState>,
    Json(input): Json<TransactionEffectiveDateInput>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(state.set_transaction_effective_date(input).await?))
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
async fn portfolio_assets(
    State(state): State<ApiState>,
    Query(query): Query<AssetsQuery>,
) -> Result<Json<keepbook::app::AssetBreakdownOutput>, ApiError> {
    Ok(Json(state.portfolio_assets(query).await?))
}

#[cfg(feature = "http")]
async fn portfolio_stacked_history(
    State(state): State<ApiState>,
    Query(query): Query<HistoryQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(state.portfolio_stacked_history(query).await?))
}

#[cfg(feature = "http")]
async fn merge_origin_master(
    State(state): State<ApiState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(state.merge_origin_master().await?))
}

#[cfg(feature = "http")]
async fn reload_data(State(state): State<ApiState>) -> Result<Json<serde_json::Value>, ApiError> {
    state.reload().await?;
    Ok(Json(serde_json::json!({ "status": "ok" })))
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
async fn application_settings(
    State(state): State<ApiState>,
) -> Result<Json<ApplicationSettingsOutput>, ApiError> {
    Ok(Json(state.application_settings().await?))
}

#[cfg(feature = "http")]
async fn save_application_settings(
    State(state): State<ApiState>,
    Json(input): Json<ApplicationSettingsInput>,
) -> Result<Json<ApplicationSettingsOutput>, ApiError> {
    Ok(Json(state.save_application_settings(input).await?))
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

#[cfg(feature = "http")]
async fn repositories(
    State(state): State<ApiState>,
) -> Result<Json<RepositoryRegistryOutput>, ApiError> {
    Ok(Json(state.repositories().await?))
}

#[cfg(feature = "http")]
async fn add_repository(
    State(state): State<ApiState>,
    Json(input): Json<AddRepositoryInput>,
) -> Result<Json<RepositoryRegistryOutput>, ApiError> {
    Ok(Json(state.add_repository(input).await?))
}

#[cfg(feature = "http")]
async fn activate_repository(
    State(state): State<ApiState>,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<RepositoryRegistryOutput>, ApiError> {
    Ok(Json(state.activate_repository(&id).await?))
}

#[cfg(feature = "http")]
async fn remove_repository(
    State(state): State<ApiState>,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<RepositoryRegistryOutput>, ApiError> {
    Ok(Json(state.remove_repository(&id).await?))
}
