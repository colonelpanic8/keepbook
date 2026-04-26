use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::Arc;

use anyhow::{Context, Result};
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::Utc;
use keepbook::config::{default_config_path, ResolvedConfig};
use keepbook::storage::{JsonFileStorage, Storage};
use serde::{Deserialize, Serialize};
use tempfile::NamedTempFile;
use tokio::sync::RwLock;
use toml_edit::{value, DocumentMut, Item, Table};
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

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
}

impl Default for GitRemoteSettings {
    fn default() -> Self {
        Self {
            host: "github.com".to_string(),
            repo: "colonelpanic8/keepbook-data".to_string(),
            branch: "master".to_string(),
            ssh_user: "git".to_string(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct GitSettingsOutput {
    pub config_path: String,
    pub data_dir: String,
    pub git: GitRemoteSettings,
}

#[derive(Debug, Deserialize)]
pub struct GitSettingsInput {
    pub data_dir: String,
    pub host: String,
    pub repo: String,
    pub branch: String,
    pub ssh_user: String,
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

#[derive(Debug, Serialize)]
pub struct OverviewOutput {
    pub config_path: String,
    pub data_dir: String,
    pub reporting_currency: String,
    pub history_defaults: HistoryDefaultsOutput,
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
    pub granularity: Option<String>,
    pub include_prices: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct OverviewQuery {
    pub history_start: Option<String>,
    pub history_end: Option<String>,
    pub history_granularity: Option<String>,
    pub include_prices: Option<bool>,
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

fn history_defaults(config: &ResolvedConfig) -> HistoryDefaultsOutput {
    HistoryDefaultsOutput {
        portfolio_granularity: config.history.portfolio_granularity.clone(),
        change_points_granularity: config.history.change_points_granularity.clone(),
        include_prices: config.history.include_prices,
        graph_range: config.history.graph_range.clone(),
        graph_granularity: config.history.graph_granularity.clone(),
    }
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
        .route(
            "/api/git/settings",
            get(git_settings).put(save_git_settings),
        )
        .route("/api/git/sync", post(sync_git_repo))
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
    let state = state.snapshot().await;
    Json(ConfigOutput {
        config_path: state.config_path.display().to_string(),
        data_dir: state.config.data_dir.display().to_string(),
        reporting_currency: state.config.reporting_currency.clone(),
        history_defaults: history_defaults(&state.config),
    })
}

async fn overview(
    State(state): State<ApiState>,
    Query(query): Query<OverviewQuery>,
) -> Result<Json<OverviewOutput>, ApiError> {
    let state = state.snapshot().await;
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
        let history_granularity = query
            .history_granularity
            .unwrap_or_else(|| state.config.history.portfolio_granularity.clone());
        let include_prices = query
            .include_prices
            .unwrap_or(state.config.history.include_prices);
        Some(json_value(
            keepbook::app::portfolio_history(
                state.storage.clone(),
                &state.config,
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

    Ok(Json(OverviewOutput {
        config_path: state.config_path.display().to_string(),
        data_dir: state.config.data_dir.display().to_string(),
        reporting_currency: state.config.reporting_currency.clone(),
        history_defaults: history_defaults(&state.config),
        connections: json_value(connections)?,
        accounts: json_value(accounts)?,
        balances: json_value(balances)?,
        snapshot: json_value(snapshot)?,
        history,
    }))
}

async fn connections(State(state): State<ApiState>) -> Result<Json<serde_json::Value>, ApiError> {
    let state = state.snapshot().await;
    let output = keepbook::app::list_connections(state.storage.as_ref()).await?;
    Ok(Json(json_value(output)?))
}

async fn accounts(State(state): State<ApiState>) -> Result<Json<serde_json::Value>, ApiError> {
    let state = state.snapshot().await;
    let output = keepbook::app::list_accounts(state.storage.as_ref()).await?;
    Ok(Json(json_value(output)?))
}

async fn balances(State(state): State<ApiState>) -> Result<Json<serde_json::Value>, ApiError> {
    let state = state.snapshot().await;
    let output = keepbook::app::list_balances(state.storage.as_ref(), &state.config).await?;
    Ok(Json(json_value(output)?))
}

async fn transactions(
    State(state): State<ApiState>,
    Query(query): Query<TransactionQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let state = state.snapshot().await;
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
    let state = state.snapshot().await;
    let granularity = query
        .granularity
        .unwrap_or_else(|| state.config.history.portfolio_granularity.clone());
    let include_prices = query
        .include_prices
        .unwrap_or(state.config.history.include_prices);
    let output = keepbook::app::portfolio_history(
        state.storage.clone(),
        &state.config,
        query.currency,
        query.start,
        query.end,
        granularity,
        include_prices,
    )
    .await?;
    Ok(Json(json_value(output)?))
}

async fn merge_origin_master(
    State(state): State<ApiState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let state = state.snapshot().await;
    keepbook::app::run_preflight(
        &state.config,
        keepbook::app::PreflightOptions {
            merge_origin_master: true,
        },
    )?;
    Ok(Json(json_value(())?))
}

async fn git_settings(State(state): State<ApiState>) -> Result<Json<GitSettingsOutput>, ApiError> {
    let snapshot = state.snapshot().await;
    let git = load_git_remote_settings(&snapshot.config_path)?;
    Ok(Json(GitSettingsOutput {
        config_path: snapshot.config_path.display().to_string(),
        data_dir: snapshot.config.data_dir.display().to_string(),
        git,
    }))
}

async fn save_git_settings(
    State(state): State<ApiState>,
    Json(input): Json<GitSettingsInput>,
) -> Result<Json<GitSettingsOutput>, ApiError> {
    let snapshot = state.snapshot().await;
    write_git_settings(&snapshot.config_path, &input)?;
    state.reload().await?;
    git_settings(State(state)).await
}

async fn sync_git_repo(
    State(state): State<ApiState>,
    Json(input): Json<GitSyncInput>,
) -> Result<Json<GitSyncOutput>, ApiError> {
    if input.private_key_pem.trim().is_empty() {
        return Err(anyhow::anyhow!("SSH private key is empty").into());
    }

    let snapshot = state.snapshot().await;
    if input.save_settings {
        write_git_settings(
            &snapshot.config_path,
            &GitSettingsInput {
                data_dir: input.data_dir.clone(),
                host: input.host.clone(),
                repo: input.repo.clone(),
                branch: input.branch.clone(),
                ssh_user: input.ssh_user.clone(),
            },
        )?;
        state.reload().await?;
    }

    let data_dir = resolve_input_data_dir(&snapshot.config_path, input.data_dir.trim());
    let branch = non_empty(input.branch.trim(), "master");
    let remote_url = build_ssh_remote_url(&input.host, &input.repo, &input.ssh_user);
    sync_git_ssh(&data_dir, &remote_url, &branch, &input.private_key_pem)?;
    state.reload().await?;

    Ok(Json(GitSyncOutput {
        ok: true,
        data_dir: data_dir.display().to_string(),
        remote_url,
        branch,
    }))
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
    })
}

fn table_string(table: Option<&Item>, key: &str) -> Option<String> {
    table?
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
    if !doc["git_sync"].is_table() {
        doc["git_sync"] = Item::Table(Table::new());
    }
    doc["git_sync"]["host"] = value(non_empty(input.host.trim(), "github.com"));
    doc["git_sync"]["repo"] = value(input.repo.trim());
    doc["git_sync"]["branch"] = value(non_empty(input.branch.trim(), "master"));
    doc["git_sync"]["ssh_user"] = value(non_empty(input.ssh_user.trim(), "git"));

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
    if repo.contains("://") {
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

fn sync_git_ssh(
    data_dir: &Path,
    remote_url: &str,
    branch: &str,
    private_key_pem: &str,
) -> Result<()> {
    let mut key_file = NamedTempFile::new().context("failed to create temporary SSH key file")?;
    std::io::Write::write_all(&mut key_file, private_key_pem.as_bytes())
        .context("failed to write temporary SSH key")?;
    std::io::Write::write_all(&mut key_file, b"\n").context("failed to terminate SSH key")?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(key_file.path(), std::fs::Permissions::from_mode(0o600))
            .context("failed to restrict temporary SSH key permissions")?;
    }

    let ssh_command = format!(
        "ssh -i {} -o IdentitiesOnly=yes -o StrictHostKeyChecking=accept-new",
        key_file.path().display()
    );

    if data_dir.join(".git").exists() {
        run_git(
            data_dir,
            &["remote", "set-url", "origin", remote_url],
            &ssh_command,
        )?;
        run_git(data_dir, &["fetch", "origin", branch], &ssh_command)?;
        run_git(
            data_dir,
            &["checkout", "-B", branch, &format!("origin/{branch}")],
            &ssh_command,
        )?;
        run_git(
            data_dir,
            &["reset", "--hard", &format!("origin/{branch}")],
            &ssh_command,
        )?;
    } else {
        if let Some(parent) = data_dir.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        let data_dir_arg = data_dir.display().to_string();
        run_git(
            Path::new("."),
            &["clone", "--branch", branch, remote_url, &data_dir_arg],
            &ssh_command,
        )?;
    }

    Ok(())
}

fn run_git(dir: &Path, args: &[&str], ssh_command: &str) -> Result<Output> {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_SSH_COMMAND", ssh_command)
        .output()
        .with_context(|| format!("failed to run git {}", args.join(" ")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git {} failed: {}", args.join(" "), stderr.trim());
    }

    Ok(output)
}

pub fn default_listen_addr() -> SocketAddr {
    SocketAddr::from(([127, 0, 0, 1], 8799))
}

pub fn default_server_config_path() -> PathBuf {
    default_config_path()
}
