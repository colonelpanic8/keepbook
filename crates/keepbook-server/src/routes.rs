use std::net::SocketAddr;
use std::path::Path;

use anyhow::Result;
use axum::extract::{Path as AxumPath, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use keepbook::app::ReviewedRecurringTransactionOutput;
use serde::Serialize;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

use crate::ai_rules::{AiRuleSuggestionInput, AiRuleSuggestionsOutput};
use crate::dto::{
    AddRepositoryInput, ApplicationSettingsInput, ApplicationSettingsOutput, AssetsQuery,
    ConfigOutput, GitSettingsInput, GitSettingsOutput, GitSyncInput, GitSyncOutput, HealthOutput,
    HistoryQuery, OverviewOutput, OverviewQuery, ProposedTransactionEditsQuery,
    RecurringTransactionReviewInput, RecurringTransactionsQuery, RepositoryRegistryOutput,
    SpendingQuery, SyncConnectionsInput, SyncPricesInput, TransactionEffectiveDateInput,
    TransactionIgnoreBatchInput, TransactionQuery, TransactionSubtagsBatchInput,
    TransactionTagsBatchInput, TraySnapshotOutput,
};
use crate::state::ApiState;

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
    Json(state.config_output().await)
}

async fn overview(
    State(state): State<ApiState>,
    Query(query): Query<OverviewQuery>,
) -> Result<Json<OverviewOutput>, ApiError> {
    Ok(Json(state.overview(query).await?))
}

async fn connections(State(state): State<ApiState>) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(state.connections().await?))
}

async fn accounts(State(state): State<ApiState>) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(state.accounts().await?))
}

async fn balances(State(state): State<ApiState>) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(state.balances().await?))
}

async fn transactions(
    State(state): State<ApiState>,
    Query(query): Query<TransactionQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(state.transactions(query).await?))
}

async fn recurring_transactions(
    State(state): State<ApiState>,
    Query(query): Query<RecurringTransactionsQuery>,
) -> Result<Json<Vec<ReviewedRecurringTransactionOutput>>, ApiError> {
    Ok(Json(state.recurring_transactions(query).await?))
}

async fn review_recurring_transaction(
    State(state): State<ApiState>,
    Json(input): Json<RecurringTransactionReviewInput>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(state.review_recurring_transaction(input).await?))
}

async fn set_transaction_tags(
    State(state): State<ApiState>,
    Json(input): Json<TransactionTagsBatchInput>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(state.set_transaction_tags(input).await?))
}

async fn set_transaction_ignore(
    State(state): State<ApiState>,
    Json(input): Json<TransactionIgnoreBatchInput>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(state.set_transaction_ignore(input).await?))
}

async fn set_transaction_subtags(
    State(state): State<ApiState>,
    Json(input): Json<TransactionSubtagsBatchInput>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(state.set_transaction_subtags(input).await?))
}

async fn set_transaction_effective_date(
    State(state): State<ApiState>,
    Json(input): Json<TransactionEffectiveDateInput>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(state.set_transaction_effective_date(input).await?))
}

async fn spending(
    State(state): State<ApiState>,
    Query(query): Query<SpendingQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(state.spending(query).await?))
}

async fn tray(State(state): State<ApiState>) -> Result<Json<TraySnapshotOutput>, ApiError> {
    Ok(Json(state.tray_snapshot().await?))
}

async fn proposed_transaction_edits(
    State(state): State<ApiState>,
    Query(query): Query<ProposedTransactionEditsQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(state.proposed_transaction_edits(query).await?))
}

async fn approve_proposed_transaction_edit(
    State(state): State<ApiState>,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(state.approve_proposed_transaction_edit(id).await?))
}

async fn reject_proposed_transaction_edit(
    State(state): State<ApiState>,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(state.reject_proposed_transaction_edit(id).await?))
}

async fn remove_proposed_transaction_edit(
    State(state): State<ApiState>,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(state.remove_proposed_transaction_edit(id).await?))
}

async fn portfolio_history(
    State(state): State<ApiState>,
    Query(query): Query<HistoryQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(state.portfolio_history(query).await?))
}

async fn portfolio_assets(
    State(state): State<ApiState>,
    Query(query): Query<AssetsQuery>,
) -> Result<Json<keepbook::app::AssetBreakdownOutput>, ApiError> {
    Ok(Json(state.portfolio_assets(query).await?))
}

async fn portfolio_stacked_history(
    State(state): State<ApiState>,
    Query(query): Query<HistoryQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(state.portfolio_stacked_history(query).await?))
}

async fn merge_origin_master(
    State(state): State<ApiState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(state.merge_origin_master().await?))
}

async fn reload_data(State(state): State<ApiState>) -> Result<Json<serde_json::Value>, ApiError> {
    state.reload().await?;
    Ok(Json(serde_json::json!({ "status": "ok" })))
}

async fn git_settings(State(state): State<ApiState>) -> Result<Json<GitSettingsOutput>, ApiError> {
    Ok(Json(state.git_settings().await?))
}

async fn save_git_settings(
    State(state): State<ApiState>,
    Json(input): Json<GitSettingsInput>,
) -> Result<Json<GitSettingsOutput>, ApiError> {
    Ok(Json(state.save_git_settings(input).await?))
}

async fn application_settings(
    State(state): State<ApiState>,
) -> Result<Json<ApplicationSettingsOutput>, ApiError> {
    Ok(Json(state.application_settings().await?))
}

async fn save_application_settings(
    State(state): State<ApiState>,
    Json(input): Json<ApplicationSettingsInput>,
) -> Result<Json<ApplicationSettingsOutput>, ApiError> {
    Ok(Json(state.save_application_settings(input).await?))
}

async fn sync_git_repo(
    State(state): State<ApiState>,
    Json(input): Json<GitSyncInput>,
) -> Result<Json<GitSyncOutput>, ApiError> {
    Ok(Json(state.sync_git_repo(input).await?))
}

async fn sync_connections(
    State(state): State<ApiState>,
    Json(input): Json<SyncConnectionsInput>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(state.sync_connections(input).await?))
}

async fn sync_prices(
    State(state): State<ApiState>,
    Json(input): Json<SyncPricesInput>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(state.sync_prices(input).await?))
}

async fn suggest_ai_rules(
    State(state): State<ApiState>,
    Json(input): Json<AiRuleSuggestionInput>,
) -> Result<Json<AiRuleSuggestionsOutput>, ApiError> {
    Ok(Json(state.suggest_ai_rules(input).await?))
}

async fn repositories(
    State(state): State<ApiState>,
) -> Result<Json<RepositoryRegistryOutput>, ApiError> {
    Ok(Json(state.repositories().await?))
}

async fn add_repository(
    State(state): State<ApiState>,
    Json(input): Json<AddRepositoryInput>,
) -> Result<Json<RepositoryRegistryOutput>, ApiError> {
    Ok(Json(state.add_repository(input).await?))
}

async fn activate_repository(
    State(state): State<ApiState>,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<RepositoryRegistryOutput>, ApiError> {
    Ok(Json(state.activate_repository(&id).await?))
}

async fn remove_repository(
    State(state): State<ApiState>,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<RepositoryRegistryOutput>, ApiError> {
    Ok(Json(state.remove_repository(&id).await?))
}
