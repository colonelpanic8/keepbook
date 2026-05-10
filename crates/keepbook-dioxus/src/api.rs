use super::logic::*;
use super::*;
#[cfg(not(target_arch = "wasm32"))]
use serde::{Deserialize, Serialize};

#[cfg(target_arch = "wasm32")]
use gloo_net::http::Request;
#[cfg(not(target_arch = "wasm32"))]
use std::path::PathBuf;
#[cfg(not(target_arch = "wasm32"))]
use std::sync::{mpsc, Arc, Mutex, OnceLock};
#[cfg(not(target_arch = "wasm32"))]
use std::task::Poll;

pub(crate) async fn fetch_overview(overrides: FilterOverrides) -> Result<Overview, String> {
    fetch_overview_impl(overrides).await
}

#[cfg(target_arch = "wasm32")]
pub(crate) async fn fetch_overview_impl(overrides: FilterOverrides) -> Result<Overview, String> {
    let query = filter_override_query_string(overrides);
    let url = if query.is_empty() {
        format!("{API_BASE}/api/overview")
    } else {
        format!("{API_BASE}/api/overview?{query}")
    };
    let response = Request::get(&url)
        .send()
        .await
        .map_err(|error| format!("Could not reach keepbook-server at {API_BASE}: {error}"))?;

    if !response.ok() {
        return Err(format!(
            "keepbook-server returned HTTP {} {}",
            response.status(),
            response.status_text()
        ));
    }

    response
        .json::<Overview>()
        .await
        .map_err(|error| format!("Could not decode keepbook overview: {error}"))
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn fetch_overview_impl(overrides: FilterOverrides) -> Result<Overview, String> {
    let output = native_api_state()?
        .overview(keepbook_server::OverviewQuery {
            history_start: None,
            history_end: None,
            history_granularity: None,
            include_prices: None,
            include_latent_capital_gains_tax: overrides.include_latent_capital_gains_tax,
            include_history: false,
        })
        .await
        .map_err(|error| format!("Could not load keepbook overview: {error:#}"))?;
    from_native_output(output, "keepbook overview")
}

pub(crate) async fn fetch_history(query: String) -> Result<History, String> {
    fetch_history_impl(query).await
}

pub(crate) async fn fetch_spending_dashboard(
    query: String,
) -> Result<SpendingDashboardData, String> {
    let spending = fetch_spending_impl(query).await?;
    let tx_query = transaction_query_string(
        &spending.start_date,
        &spending.end_date,
        Some(&spending.tz),
        false,
    );
    let counted_transactions = fetch_transactions_impl(tx_query).await?;
    let all_tx_query = transaction_query_string(
        &spending.start_date,
        &spending.end_date,
        Some(&spending.tz),
        true,
    );
    let transactions = mark_transactions_excluded_from_spending(
        fetch_transactions_impl(all_tx_query).await?,
        &counted_transactions,
    );
    Ok(SpendingDashboardData {
        spending,
        transactions,
    })
}

pub(crate) async fn fetch_tray_snapshot() -> Result<TraySnapshot, String> {
    fetch_tray_snapshot_impl().await
}

pub(crate) async fn fetch_git_settings() -> Result<GitSettingsOutput, String> {
    fetch_git_settings_impl().await
}

pub(crate) async fn save_git_settings(
    input: GitSettingsInput,
) -> Result<GitSettingsOutput, String> {
    save_git_settings_impl(input).await
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) type GitSyncCancelHandle = keepbook_server::GitSyncCancelToken;

#[cfg(target_arch = "wasm32")]
#[derive(Clone, Default)]
pub(crate) struct GitSyncCancelHandle;

#[cfg(target_arch = "wasm32")]
impl GitSyncCancelHandle {
    pub(crate) fn cancel(&self) {}
}

pub(crate) fn new_git_sync_cancel_handle() -> GitSyncCancelHandle {
    GitSyncCancelHandle::default()
}

pub(crate) async fn sync_git_repo_cancelable(
    input: GitSyncInput,
    cancel_handle: GitSyncCancelHandle,
) -> Result<GitSyncOutput, String> {
    sync_git_repo_cancelable_impl(input, cancel_handle).await
}

pub(crate) async fn sync_connections(
    input: SyncConnectionsInput,
) -> Result<serde_json::Value, String> {
    sync_connections_impl(input).await
}

pub(crate) async fn sync_prices(input: SyncPricesInput) -> Result<serde_json::Value, String> {
    sync_prices_impl(input).await
}

pub(crate) async fn suggest_ai_rules(
    input: AiRuleSuggestionInput,
) -> Result<AiRuleSuggestionsOutput, String> {
    suggest_ai_rules_impl(input).await
}

pub(crate) async fn set_transaction_tags(input: SetTransactionTagsInput) -> Result<(), String> {
    set_transaction_tags_impl(input).await
}

pub(crate) async fn fetch_proposed_transaction_edits(
) -> Result<Vec<ProposedTransactionEdit>, String> {
    fetch_proposed_transaction_edits_impl().await
}

pub(crate) async fn decide_proposed_transaction_edit(
    id: String,
    action: &'static str,
) -> Result<(), String> {
    decide_proposed_transaction_edit_impl(id, action).await
}

#[cfg(target_arch = "wasm32")]
pub(crate) async fn fetch_history_impl(query: String) -> Result<History, String> {
    let response = Request::get(&format!("{API_BASE}/api/portfolio/history?{query}"))
        .send()
        .await
        .map_err(|error| format!("Could not reach keepbook-server at {API_BASE}: {error}"))?;

    if !response.ok() {
        return Err(format!(
            "keepbook-server returned HTTP {} {}",
            response.status(),
            response.status_text()
        ));
    }

    response
        .json::<History>()
        .await
        .map_err(|error| format!("Could not decode net worth history: {error}"))
}

#[cfg(target_arch = "wasm32")]
pub(crate) async fn fetch_spending_impl(query: String) -> Result<SpendingOutput, String> {
    let response = Request::get(&format!("{API_BASE}/api/spending?{query}"))
        .send()
        .await
        .map_err(|error| format!("Could not reach keepbook-server at {API_BASE}: {error}"))?;

    if !response.ok() {
        return Err(format!(
            "keepbook-server returned HTTP {} {}",
            response.status(),
            response.status_text()
        ));
    }

    response
        .json::<SpendingOutput>()
        .await
        .map_err(|error| format!("Could not decode spending data: {error}"))
}

#[cfg(target_arch = "wasm32")]
pub(crate) async fn fetch_transactions_impl(query: String) -> Result<Vec<Transaction>, String> {
    let response = Request::get(&format!("{API_BASE}/api/transactions?{query}"))
        .send()
        .await
        .map_err(|error| format!("Could not reach keepbook-server at {API_BASE}: {error}"))?;

    if !response.ok() {
        return Err(format!(
            "keepbook-server returned HTTP {} {}",
            response.status(),
            response.status_text()
        ));
    }

    response
        .json::<Vec<Transaction>>()
        .await
        .map_err(|error| format!("Could not decode transactions: {error}"))
}

#[cfg(target_arch = "wasm32")]
pub(crate) async fn fetch_tray_snapshot_impl() -> Result<TraySnapshot, String> {
    let response = Request::get(&format!("{API_BASE}/api/tray"))
        .send()
        .await
        .map_err(|error| format!("Could not reach keepbook-server at {API_BASE}: {error}"))?;

    if !response.ok() {
        return Err(format!(
            "keepbook-server returned HTTP {} {}",
            response.status(),
            response.status_text()
        ));
    }

    response
        .json::<TraySnapshot>()
        .await
        .map_err(|error| format!("Could not decode tray snapshot: {error}"))
}

#[cfg(target_arch = "wasm32")]
pub(crate) async fn fetch_git_settings_impl() -> Result<GitSettingsOutput, String> {
    let response = Request::get(&format!("{API_BASE}/api/git/settings"))
        .send()
        .await
        .map_err(|error| format!("Could not reach keepbook-server at {API_BASE}: {error}"))?;

    if !response.ok() {
        return Err(format!(
            "keepbook-server returned HTTP {} {}",
            response.status(),
            response.status_text()
        ));
    }

    response
        .json::<GitSettingsOutput>()
        .await
        .map_err(|error| format!("Could not decode Git settings: {error}"))
}

#[cfg(target_arch = "wasm32")]
pub(crate) async fn save_git_settings_impl(
    input: GitSettingsInput,
) -> Result<GitSettingsOutput, String> {
    let response = Request::put(&format!("{API_BASE}/api/git/settings"))
        .json(&input)
        .map_err(|error| format!("Could not encode Git settings: {error}"))?
        .send()
        .await
        .map_err(|error| format!("Could not reach keepbook-server at {API_BASE}: {error}"))?;

    if !response.ok() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        return Err(format!("keepbook-server returned HTTP {status}: {text}"));
    }

    response
        .json::<GitSettingsOutput>()
        .await
        .map_err(|error| format!("Could not decode Git settings: {error}"))
}

#[cfg(target_arch = "wasm32")]
pub(crate) async fn sync_git_repo_impl(input: GitSyncInput) -> Result<GitSyncOutput, String> {
    let response = Request::post(&format!("{API_BASE}/api/git/sync"))
        .json(&input)
        .map_err(|error| format!("Could not encode Git sync request: {error}"))?
        .send()
        .await
        .map_err(|error| format!("Could not reach keepbook-server at {API_BASE}: {error}"))?;

    if !response.ok() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        return Err(format!("keepbook-server returned HTTP {status}: {text}"));
    }

    response
        .json::<GitSyncOutput>()
        .await
        .map_err(|error| format!("Could not decode Git sync result: {error}"))
}

#[cfg(target_arch = "wasm32")]
pub(crate) async fn sync_git_repo_cancelable_impl(
    input: GitSyncInput,
    _cancel_handle: GitSyncCancelHandle,
) -> Result<GitSyncOutput, String> {
    sync_git_repo_impl(input).await
}

#[cfg(target_arch = "wasm32")]
pub(crate) async fn sync_connections_impl(
    input: SyncConnectionsInput,
) -> Result<serde_json::Value, String> {
    let response = Request::post(&format!("{API_BASE}/api/sync/connections"))
        .json(&input)
        .map_err(|error| format!("Could not encode balance refresh request: {error}"))?
        .send()
        .await
        .map_err(|error| format!("Could not reach keepbook-server at {API_BASE}: {error}"))?;

    if !response.ok() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        return Err(format!("keepbook-server returned HTTP {status}: {text}"));
    }

    response
        .json::<serde_json::Value>()
        .await
        .map_err(|error| format!("Could not decode balance refresh result: {error}"))
}

#[cfg(target_arch = "wasm32")]
pub(crate) async fn sync_prices_impl(input: SyncPricesInput) -> Result<serde_json::Value, String> {
    let response = Request::post(&format!("{API_BASE}/api/sync/prices"))
        .json(&input)
        .map_err(|error| format!("Could not encode price refresh request: {error}"))?
        .send()
        .await
        .map_err(|error| format!("Could not reach keepbook-server at {API_BASE}: {error}"))?;

    if !response.ok() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        return Err(format!("keepbook-server returned HTTP {status}: {text}"));
    }

    response
        .json::<serde_json::Value>()
        .await
        .map_err(|error| format!("Could not decode price refresh result: {error}"))
}

#[cfg(target_arch = "wasm32")]
pub(crate) async fn suggest_ai_rules_impl(
    input: AiRuleSuggestionInput,
) -> Result<AiRuleSuggestionsOutput, String> {
    let response = Request::post(&format!("{API_BASE}/api/ai/rules/suggest"))
        .json(&input)
        .map_err(|error| format!("Could not encode AI rule request: {error}"))?
        .send()
        .await
        .map_err(|error| format!("Could not reach keepbook-server at {API_BASE}: {error}"))?;

    if !response.ok() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        return Err(format!("keepbook-server returned HTTP {status}: {text}"));
    }

    response
        .json::<AiRuleSuggestionsOutput>()
        .await
        .map_err(|error| format!("Could not decode AI rule suggestions: {error}"))
}

#[cfg(target_arch = "wasm32")]
pub(crate) async fn set_transaction_tags_impl(
    input: SetTransactionTagsInput,
) -> Result<(), String> {
    let response = Request::post(&format!("{API_BASE}/api/transactions/tags/batch"))
        .json(&input)
        .map_err(|error| format!("Could not encode tag update: {error}"))?
        .send()
        .await
        .map_err(|error| format!("Could not reach keepbook-server at {API_BASE}: {error}"))?;

    if !response.ok() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        return Err(format!("keepbook-server returned HTTP {status}: {text}"));
    }

    Ok(())
}

#[cfg(target_arch = "wasm32")]
pub(crate) async fn fetch_proposed_transaction_edits_impl(
) -> Result<Vec<ProposedTransactionEdit>, String> {
    let response = Request::get(&format!("{API_BASE}/api/proposed-transaction-edits"))
        .send()
        .await
        .map_err(|error| format!("Could not reach keepbook-server at {API_BASE}: {error}"))?;

    if !response.ok() {
        return Err(format!(
            "keepbook-server returned HTTP {} {}",
            response.status(),
            response.status_text()
        ));
    }

    response
        .json::<Vec<ProposedTransactionEdit>>()
        .await
        .map_err(|error| format!("Could not decode proposed edits: {error}"))
}

#[cfg(target_arch = "wasm32")]
pub(crate) async fn decide_proposed_transaction_edit_impl(
    id: String,
    action: &'static str,
) -> Result<(), String> {
    let response = Request::post(&format!(
        "{API_BASE}/api/proposed-transaction-edits/{id}/{action}"
    ))
    .send()
    .await
    .map_err(|error| format!("Could not reach keepbook-server at {API_BASE}: {error}"))?;

    if !response.ok() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        return Err(format!("keepbook-server returned HTTP {status}: {text}"));
    }

    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn fetch_history_impl(query: String) -> Result<History, String> {
    let query = serde_urlencoded::from_str::<keepbook_server::HistoryQuery>(&query)
        .map_err(|error| format!("Could not encode history query: {error}"))?;
    let output = native_api_state()?
        .portfolio_history(query)
        .await
        .map_err(|error| format!("Could not load net worth history: {error:#}"))?;
    from_native_output(output, "net worth history")
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn fetch_spending_impl(query: String) -> Result<SpendingOutput, String> {
    let query = serde_urlencoded::from_str::<keepbook_server::SpendingQuery>(&query)
        .map_err(|error| format!("Could not encode spending query: {error}"))?;
    let output = native_api_state()?
        .spending(query)
        .await
        .map_err(|error| format!("Could not load spending data: {error:#}"))?;
    from_native_output(output, "spending data")
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn fetch_transactions_impl(query: String) -> Result<Vec<Transaction>, String> {
    let query = serde_urlencoded::from_str::<keepbook_server::TransactionQuery>(&query)
        .map_err(|error| format!("Could not encode transaction query: {error}"))?;
    let output = native_api_state()?
        .transactions(query)
        .await
        .map_err(|error| format!("Could not load transactions: {error:#}"))?;
    from_native_output(output, "transactions")
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn fetch_tray_snapshot_impl() -> Result<TraySnapshot, String> {
    let output = native_api_state()?
        .tray_snapshot()
        .await
        .map_err(|error| format!("Could not load tray snapshot: {error:#}"))?;
    from_native_output(output, "tray snapshot")
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn fetch_git_settings_impl() -> Result<GitSettingsOutput, String> {
    let output = native_api_state()?
        .git_settings()
        .await
        .map_err(|error| format!("Could not load Git settings: {error:#}"))?;
    from_native_output(output, "Git settings")
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn save_git_settings_impl(
    input: GitSettingsInput,
) -> Result<GitSettingsOutput, String> {
    let output = native_api_state()?
        .save_git_settings(keepbook_server::GitSettingsInput {
            data_dir: input.data_dir,
            host: input.host,
            repo: input.repo,
            branch: input.branch,
            ssh_user: input.ssh_user,
            ssh_key_path: input.ssh_key_path,
        })
        .await
        .map_err(|error| format!("Could not save Git settings: {error:#}"))?;
    from_native_output(output, "Git settings")
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn sync_git_repo_cancelable_impl(
    input: GitSyncInput,
    cancel_handle: GitSyncCancelHandle,
) -> Result<GitSyncOutput, String> {
    let state = native_api_state()?.clone();
    let (sender, receiver) = mpsc::sync_channel(1);
    let waker = Arc::new(Mutex::new(None::<std::task::Waker>));
    let worker_waker = Arc::clone(&waker);
    std::thread::spawn(move || {
        let output = state
            .sync_git_repo_blocking_with_cancel(
                keepbook_server::GitSyncInput {
                    data_dir: input.data_dir,
                    host: input.host,
                    repo: input.repo,
                    branch: input.branch,
                    ssh_user: input.ssh_user,
                    private_key_pem: input.private_key_pem,
                    save_settings: input.save_settings,
                },
                cancel_handle,
            )
            .map_err(|error| format!("Git sync failed: {error:#}"));
        let _ = sender.send(output);
        if let Ok(mut waker) = worker_waker.lock() {
            if let Some(waker) = waker.take() {
                waker.wake();
            }
        }
    });

    let output = std::future::poll_fn(move |cx| match receiver.try_recv() {
        Ok(result) => Poll::Ready(result),
        Err(mpsc::TryRecvError::Empty) => {
            if let Ok(mut waker) = waker.lock() {
                *waker = Some(cx.waker().clone());
            }
            Poll::Pending
        }
        Err(mpsc::TryRecvError::Disconnected) => Poll::Ready(Err(
            "Git sync worker stopped before returning a result.".to_string(),
        )),
    })
    .await?;
    from_native_output(output, "Git sync result")
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn sync_connections_impl(
    input: SyncConnectionsInput,
) -> Result<serde_json::Value, String> {
    native_api_state()?
        .sync_connections(keepbook_server::SyncConnectionsInput {
            target: input.target,
            if_stale: input.if_stale,
            full_transactions: input.full_transactions,
        })
        .await
        .map_err(|error| format!("Balance refresh failed: {error:#}"))
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn sync_prices_impl(input: SyncPricesInput) -> Result<serde_json::Value, String> {
    native_api_state()?
        .sync_prices(keepbook_server::SyncPricesInput {
            scope: Some(input.scope),
            target: input.target,
            force: input.force,
            quote_staleness_seconds: input.quote_staleness_seconds,
        })
        .await
        .map_err(|error| format!("Price refresh failed: {error:#}"))
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn suggest_ai_rules_impl(
    input: AiRuleSuggestionInput,
) -> Result<AiRuleSuggestionsOutput, String> {
    let output = native_api_state()?
        .suggest_ai_rules(keepbook_server::AiRuleSuggestionInput {
            prompt: input.prompt,
            transactions: input
                .transactions
                .into_iter()
                .map(|transaction| keepbook_server::AiRuleTransactionInput {
                    id: transaction.id,
                    account_id: transaction.account_id,
                    account_name: transaction.account_name,
                    timestamp: transaction.timestamp,
                    description: transaction.description,
                    amount: transaction.amount,
                    status: transaction.status,
                    category: transaction.category,
                    subcategory: transaction.subcategory,
                    ignored_from_spending: transaction.ignored_from_spending,
                })
                .collect(),
            existing_categories: input.existing_categories,
        })
        .await
        .map_err(|error| format!("AI rule suggestion failed: {error:#}"))?;
    from_native_output(output, "AI rule suggestions")
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn set_transaction_tags_impl(
    input: SetTransactionTagsInput,
) -> Result<(), String> {
    native_api_state()?
        .set_transaction_tags(keepbook_server::TransactionTagsBatchInput {
            transactions: input
                .transactions
                .into_iter()
                .map(
                    |transaction| keepbook_server::TransactionCategoryTargetInput {
                        account_id: transaction.account_id,
                        transaction_id: transaction.transaction_id,
                    },
                )
                .collect(),
            tags: input.tags,
            clear_tags: input.clear_tags,
        })
        .await
        .map_err(|error| format!("Tag update failed: {error:#}"))?;
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn fetch_proposed_transaction_edits_impl(
) -> Result<Vec<ProposedTransactionEdit>, String> {
    let output = native_api_state()?
        .proposed_transaction_edits(keepbook_server::ProposedTransactionEditsQuery {
            include_decided: false,
        })
        .await
        .map_err(|error| format!("Could not load proposed edits: {error:#}"))?;
    from_native_output(output, "proposed edits")
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn decide_proposed_transaction_edit_impl(
    id: String,
    action: &'static str,
) -> Result<(), String> {
    let state = native_api_state()?;
    let result = match action {
        "approve" => state.approve_proposed_transaction_edit(id).await,
        "reject" => state.reject_proposed_transaction_edit(id).await,
        "remove" => state.remove_proposed_transaction_edit(id).await,
        _ => return Err(format!("Unsupported proposal action: {action}")),
    };
    result
        .map(|_| ())
        .map_err(|error| format!("Could not update proposed edit: {error:#}"))
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn native_api_state() -> Result<&'static keepbook_server::ApiState, String> {
    static STATE: OnceLock<keepbook_server::ApiState> = OnceLock::new();
    if let Some(state) = STATE.get() {
        return Ok(state);
    }

    let state = keepbook_server::ApiState::load(native_config_path())
        .map_err(|error| format!("Could not initialize local keepbook API: {error:#}"))?;
    let _ = STATE.set(state);
    STATE
        .get()
        .ok_or_else(|| "Could not initialize local keepbook API".to_string())
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "android"))]
pub(crate) fn android_app_files_dir() -> PathBuf {
    PathBuf::from(ANDROID_PACKAGE_DATA_DIR).join("files")
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "android"))]
pub(crate) fn android_default_git_data_dir() -> PathBuf {
    android_app_files_dir().join("keepbook-data")
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "android"))]
pub(crate) fn normalize_android_app_data_path(path: String) -> String {
    let legacy_prefix = "/data/data/org.colonelpanic.keepbook.dioxus";
    path.strip_prefix(legacy_prefix)
        .map(|suffix| format!("{ANDROID_PACKAGE_DATA_DIR}{suffix}"))
        .unwrap_or(path)
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "android"))]
pub(crate) fn native_config_path() -> PathBuf {
    let files_dir = android_app_files_dir();
    if let Err(error) = std::fs::create_dir_all(&files_dir) {
        eprintln!(
            "Could not create Android keepbook files dir {}: {error}",
            files_dir.display()
        );
    }

    android_default_git_data_dir().join("keepbook.toml")
}

#[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
pub(crate) fn native_config_path() -> PathBuf {
    keepbook_server::default_server_config_path()
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "android"))]
pub(crate) fn recommended_data_dir() -> Option<String> {
    Some(android_default_git_data_dir().display().to_string())
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "android"))]
pub(crate) fn normalize_git_data_dir_for_client(path: String) -> String {
    normalize_android_app_data_path(path)
}

#[cfg(any(target_arch = "wasm32", not(target_os = "android")))]
pub(crate) fn normalize_git_data_dir_for_client(path: String) -> String {
    path
}

#[cfg(any(target_arch = "wasm32", not(target_os = "android")))]
pub(crate) fn recommended_data_dir() -> Option<String> {
    None
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn from_native_output<T, U>(output: U, label: &str) -> Result<T, String>
where
    T: for<'de> Deserialize<'de>,
    U: Serialize,
{
    serde_json::from_value(
        serde_json::to_value(output)
            .map_err(|error| format!("Could not encode {label}: {error}"))?,
    )
    .map_err(|error| format!("Could not decode {label}: {error}"))
}
