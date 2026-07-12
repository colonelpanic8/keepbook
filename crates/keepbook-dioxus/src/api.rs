use super::logic::*;
use super::*;
#[cfg(not(target_arch = "wasm32"))]
use serde::{Deserialize, Serialize};

#[cfg(target_arch = "wasm32")]
use gloo_net::http::Request;
#[cfg(not(target_arch = "wasm32"))]
use std::path::PathBuf;
#[cfg(not(target_arch = "wasm32"))]
use std::sync::OnceLock;

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn run_native_blocking<T, F>(work: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, String> + Send + 'static,
{
    let (sender, receiver) = tokio::sync::oneshot::channel();
    std::thread::spawn(move || {
        let _ = sender.send(work());
    });
    receiver
        .await
        .map_err(|_| "Background operation stopped before returning a result.".to_string())?
}

#[cfg(not(target_arch = "wasm32"))]
async fn run_native_worker<T, F>(future: F) -> Result<T, String>
where
    T: Send + 'static,
    F: std::future::Future<Output = Result<T, String>> + Send + 'static,
{
    run_native_blocking(move || {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| format!("Could not start background operation: {error}"))
            .and_then(|runtime| runtime.block_on(future))
    })
    .await
}

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
    let account_portfolio_overrides = account_filter_override_param(&overrides);
    let state = native_api_state()?.clone();
    run_native_worker(async move {
        let output = state
            .overview(keepbook_server::OverviewQuery {
                history_start: None,
                history_end: None,
                history_granularity: None,
                include_prices: None,
                include_latent_capital_gains_tax: overrides.include_latent_capital_gains_tax,
                account_portfolio_overrides,
                include_history: false,
            })
            .await
            .map_err(|error| format!("Could not load keepbook overview: {error:#}"))?;
        from_native_output(output, "keepbook overview")
    })
    .await
}

pub(crate) async fn fetch_history(query: String) -> Result<History, String> {
    fetch_history_impl(query).await
}

pub(crate) async fn fetch_stacked_history(query: String) -> Result<StackedHistory, String> {
    fetch_stacked_history_impl(query).await
}

pub(crate) async fn fetch_spending_dashboard(
    query: String,
    over_time_query: String,
    exact_match_query: String,
    close_match_query: String,
) -> Result<SpendingDashboardData, String> {
    let (spending, spending_over_time, exact_match_spending, close_match_spending) = futures_util::try_join!(
        fetch_spending_impl(query),
        fetch_spending_impl(over_time_query),
        fetch_spending_impl(exact_match_query),
        fetch_spending_impl(close_match_query),
    )?;
    let tx_query = transaction_query_string(
        &spending.start_date,
        &spending.end_date,
        Some(&spending.tz),
        false,
    );
    let counted_transactions = fetch_transactions_impl(tx_query);
    let all_tx_query = transaction_query_string(
        &spending.start_date,
        &spending.end_date,
        Some(&spending.tz),
        true,
    );
    let all_transactions = fetch_transactions_impl(all_tx_query);
    let (counted_transactions, all_transactions) =
        futures_util::try_join!(counted_transactions, all_transactions)?;
    let transactions =
        mark_transactions_excluded_from_spending(all_transactions, &counted_transactions);
    Ok(SpendingDashboardData {
        spending,
        spending_over_time,
        exact_match_spending,
        close_match_spending,
        transactions,
    })
}

pub(crate) async fn fetch_tray_snapshot() -> Result<TraySnapshot, String> {
    fetch_tray_snapshot_impl().await
}

pub(crate) async fn fetch_git_settings() -> Result<GitSettingsOutput, String> {
    fetch_git_settings_impl().await
}

pub(crate) async fn fetch_application_settings() -> Result<ApplicationSettingsOutput, String> {
    fetch_application_settings_impl().await
}

pub(crate) async fn save_application_settings(
    input: ApplicationSettingsInput,
) -> Result<ApplicationSettingsOutput, String> {
    save_application_settings_impl(input).await
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

pub(crate) async fn reload_data() -> Result<serde_json::Value, String> {
    reload_data_impl().await
}

pub(crate) async fn suggest_ai_rules(
    input: AiRuleSuggestionInput,
) -> Result<AiRuleSuggestionsOutput, String> {
    suggest_ai_rules_impl(input).await
}

pub(crate) async fn set_transaction_tags(input: SetTransactionTagsInput) -> Result<(), String> {
    set_transaction_tags_impl(input).await
}

pub(crate) async fn set_transaction_ignore(input: SetTransactionIgnoreInput) -> Result<(), String> {
    set_transaction_ignore_impl(input).await
}

pub(crate) async fn set_transaction_effective_date(
    input: SetTransactionEffectiveDateInput,
) -> Result<(), String> {
    set_transaction_effective_date_impl(input).await
}

pub(crate) async fn fetch_proposed_transaction_edits(
) -> Result<Vec<ProposedTransactionEdit>, String> {
    fetch_proposed_transaction_edits_impl().await
}

pub(crate) async fn fetch_recurring_transactions(
    query: String,
) -> Result<Vec<RecurringTransaction>, String> {
    fetch_recurring_transactions_impl(query).await
}

pub(crate) async fn review_recurring_transaction(
    input: RecurringTransactionReviewInput,
) -> Result<(), String> {
    review_recurring_transaction_impl(input).await
}

pub(crate) async fn decide_proposed_transaction_edit(
    id: String,
    action: &'static str,
) -> Result<(), String> {
    decide_proposed_transaction_edit_impl(id, action).await
}

#[cfg(target_arch = "wasm32")]
pub(crate) async fn fetch_recurring_transactions_impl(
    query: String,
) -> Result<Vec<RecurringTransaction>, String> {
    let response = Request::get(&format!("{API_BASE}/api/recurring-transactions?{query}"))
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
        .json::<Vec<RecurringTransaction>>()
        .await
        .map_err(|error| format!("Could not decode recurring transactions: {error}"))
}

#[cfg(target_arch = "wasm32")]
pub(crate) async fn review_recurring_transaction_impl(
    input: RecurringTransactionReviewInput,
) -> Result<(), String> {
    let response = Request::post(&format!("{API_BASE}/api/recurring-transactions/review"))
        .json(&input)
        .map_err(|error| format!("Could not encode recurring review: {error}"))?
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

#[cfg(not(target_arch = "wasm32"))]
fn account_filter_override_param(overrides: &FilterOverrides) -> Option<String> {
    if overrides.account_portfolio_exclusions.is_empty() {
        None
    } else {
        serde_json::to_string(&overrides.account_portfolio_exclusions).ok()
    }
}

#[cfg(target_arch = "wasm32")]
pub(crate) async fn fetch_stacked_history_impl(query: String) -> Result<StackedHistory, String> {
    let response = Request::get(&format!("{API_BASE}/api/portfolio/stacked-history?{query}"))
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
        .json::<StackedHistory>()
        .await
        .map_err(|error| format!("Could not decode stacked net worth history: {error}"))
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
pub(crate) async fn fetch_application_settings_impl() -> Result<ApplicationSettingsOutput, String> {
    let response = Request::get(&format!("{API_BASE}/api/application/settings"))
        .send()
        .await
        .map_err(|error| format!("Could not reach application settings: {error}"))?;

    if !response.ok() {
        return Err(format!(
            "keepbook-server returned HTTP {} {}",
            response.status(),
            response.status_text()
        ));
    }

    response
        .json::<ApplicationSettingsOutput>()
        .await
        .map_err(|error| format!("Could not decode application settings: {error}"))
}

#[cfg(target_arch = "wasm32")]
pub(crate) async fn save_application_settings_impl(
    input: ApplicationSettingsInput,
) -> Result<ApplicationSettingsOutput, String> {
    let response = Request::put(&format!("{API_BASE}/api/application/settings"))
        .json(&input)
        .map_err(|error| format!("Could not encode application settings: {error}"))?
        .send()
        .await
        .map_err(|error| format!("Could not save application settings: {error}"))?;

    if !response.ok() {
        return Err(format!(
            "keepbook-server returned HTTP {} {}",
            response.status(),
            response.status_text()
        ));
    }

    response
        .json::<ApplicationSettingsOutput>()
        .await
        .map_err(|error| format!("Could not decode application settings: {error}"))
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
pub(crate) async fn reload_data_impl() -> Result<serde_json::Value, String> {
    let response = Request::post(&format!("{API_BASE}/api/reload"))
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
        .map_err(|error| format!("Could not decode reload result: {error}"))
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
pub(crate) async fn set_transaction_ignore_impl(
    input: SetTransactionIgnoreInput,
) -> Result<(), String> {
    let response = Request::post(&format!("{API_BASE}/api/transactions/ignore/batch"))
        .json(&input)
        .map_err(|error| format!("Could not encode ignore update: {error}"))?
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
pub(crate) async fn set_transaction_effective_date_impl(
    input: SetTransactionEffectiveDateInput,
) -> Result<(), String> {
    let response = Request::post(&format!("{API_BASE}/api/transactions/effective-date"))
        .json(&input)
        .map_err(|error| format!("Could not encode effective date update: {error}"))?
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
pub(crate) async fn fetch_recurring_transactions_impl(
    query: String,
) -> Result<Vec<RecurringTransaction>, String> {
    let query =
        serde_urlencoded::from_str::<keepbook_server::RecurringTransactionsQuery>(&query)
            .map_err(|error| format!("Could not encode recurring transaction query: {error}"))?;
    let state = native_api_state()?.clone();
    run_native_worker(async move {
        let output = state
            .recurring_transactions(query)
            .await
            .map_err(|error| format!("Could not load recurring transactions: {error:#}"))?;
        from_native_output(output, "recurring transactions")
    })
    .await
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn review_recurring_transaction_impl(
    input: RecurringTransactionReviewInput,
) -> Result<(), String> {
    let state = native_api_state()?.clone();
    run_native_worker(async move {
        state
            .review_recurring_transaction(keepbook_server::RecurringTransactionReviewInput {
                status: input.status,
                candidate: keepbook_server::ReviewedRecurringTransactionOutput {
                    candidate_key: input.candidate.candidate_key,
                    review_status: input.candidate.review_status,
                    name: input.candidate.name,
                    normalized_name: input.candidate.normalized_name,
                    status: input.candidate.status,
                    cadence: input.candidate.cadence,
                    estimated_interval_days: input.candidate.estimated_interval_days,
                    estimated_recurring_cost: input.candidate.estimated_recurring_cost,
                    estimated_annual_cost: input.candidate.estimated_annual_cost,
                    confidence: input.candidate.confidence,
                    cadence_score: input.candidate.cadence_score,
                    occurrence_count: input.candidate.occurrence_count,
                    first_seen: input.candidate.first_seen,
                    last_seen: input.candidate.last_seen,
                    next_expected: input.candidate.next_expected,
                    amount: keepbook_server::ReviewedRecurringTransactionAmountOutput {
                        typical: input.candidate.amount.typical,
                        min: input.candidate.amount.min,
                        max: input.candidate.amount.max,
                        asset: input.candidate.amount.asset,
                    },
                    reason_codes: input.candidate.reason_codes,
                    transactions: input
                        .candidate
                        .transactions
                        .into_iter()
                        .map(|occurrence| {
                            keepbook_server::ReviewedRecurringTransactionOccurrenceOutput {
                                id: occurrence.id,
                                account_id: occurrence.account_id,
                                account_name: occurrence.account_name,
                                date: occurrence.date,
                                description: occurrence.description,
                                amount: occurrence.amount,
                            }
                        })
                        .collect(),
                },
            })
            .await
            .map_err(|error| format!("Could not review recurring transaction: {error:#}"))?;
        Ok(())
    })
    .await
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn fetch_history_impl(query: String) -> Result<History, String> {
    let query = serde_urlencoded::from_str::<keepbook_server::HistoryQuery>(&query)
        .map_err(|error| format!("Could not encode history query: {error}"))?;
    let state = native_api_state()?.clone();
    run_native_worker(async move {
        let output = state
            .portfolio_history(query)
            .await
            .map_err(|error| format!("Could not load net worth history: {error:#}"))?;
        from_native_output(output, "net worth history")
    })
    .await
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn fetch_stacked_history_impl(query: String) -> Result<StackedHistory, String> {
    let query = serde_urlencoded::from_str::<keepbook_server::HistoryQuery>(&query)
        .map_err(|error| format!("Could not encode stacked history query: {error}"))?;
    let state = native_api_state()?.clone();
    run_native_worker(async move {
        let output = state
            .portfolio_stacked_history(query)
            .await
            .map_err(|error| format!("Could not load stacked net worth history: {error:#}"))?;
        from_native_output(output, "stacked net worth history")
    })
    .await
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn fetch_spending_impl(query: String) -> Result<SpendingOutput, String> {
    let query = serde_urlencoded::from_str::<keepbook_server::SpendingQuery>(&query)
        .map_err(|error| format!("Could not encode spending query: {error}"))?;
    let state = native_api_state()?.clone();
    run_native_worker(async move {
        let output = state
            .spending(query)
            .await
            .map_err(|error| format!("Could not load spending data: {error:#}"))?;
        from_native_output(output, "spending data")
    })
    .await
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn fetch_transactions_impl(query: String) -> Result<Vec<Transaction>, String> {
    let query = serde_urlencoded::from_str::<keepbook_server::TransactionQuery>(&query)
        .map_err(|error| format!("Could not encode transaction query: {error}"))?;
    let state = native_api_state()?.clone();
    run_native_worker(async move {
        let output = state
            .transactions(query)
            .await
            .map_err(|error| format!("Could not load transactions: {error:#}"))?;
        from_native_output(output, "transactions")
    })
    .await
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn fetch_tray_snapshot_impl() -> Result<TraySnapshot, String> {
    let state = native_api_state()?.clone();
    run_native_worker(async move {
        let output = state
            .tray_snapshot()
            .await
            .map_err(|error| format!("Could not load tray snapshot: {error:#}"))?;
        from_native_output(output, "tray snapshot")
    })
    .await
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn fetch_git_settings_impl() -> Result<GitSettingsOutput, String> {
    let state = native_api_state()?.clone();
    run_native_worker(async move {
        let output = state
            .git_settings()
            .await
            .map_err(|error| format!("Could not load Git settings: {error:#}"))?;
        from_native_output(output, "Git settings")
    })
    .await
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn save_git_settings_impl(
    input: GitSettingsInput,
) -> Result<GitSettingsOutput, String> {
    let state = native_api_state()?.clone();
    run_native_worker(async move {
        let output = state
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
    })
    .await
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn fetch_application_settings_impl() -> Result<ApplicationSettingsOutput, String> {
    let state = native_api_state()?.clone();
    run_native_worker(async move {
        let output = state
            .application_settings()
            .await
            .map_err(|error| format!("Could not load application settings: {error:#}"))?;
        from_native_output(output, "application settings")
    })
    .await
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn save_application_settings_impl(
    input: ApplicationSettingsInput,
) -> Result<ApplicationSettingsOutput, String> {
    let state = native_api_state()?.clone();
    run_native_worker(async move {
        let output = state
            .save_application_settings(keepbook_server::ApplicationSettingsInput {
                start_minimized_to_tray: input.start_minimized_to_tray,
            })
            .await
            .map_err(|error| format!("Could not save application settings: {error:#}"))?;
        from_native_output(output, "application settings")
    })
    .await
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn sync_git_repo_cancelable_impl(
    input: GitSyncInput,
    cancel_handle: GitSyncCancelHandle,
) -> Result<GitSyncOutput, String> {
    let state = native_api_state()?.clone();
    let output = run_native_blocking(move || {
        state
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
            .map_err(|error| format!("Git sync failed: {error:#}"))
    })
    .await?;
    from_native_output(output, "Git sync result")
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn sync_connections_impl(
    input: SyncConnectionsInput,
) -> Result<serde_json::Value, String> {
    let state = native_api_state()?.clone();
    run_native_worker(async move {
        state
            .sync_connections(keepbook_server::SyncConnectionsInput {
                target: input.target,
                if_stale: input.if_stale,
                full_transactions: input.full_transactions,
            })
            .await
            .map_err(|error| format!("Balance refresh failed: {error:#}"))
    })
    .await
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn sync_prices_impl(input: SyncPricesInput) -> Result<serde_json::Value, String> {
    let state = native_api_state()?.clone();
    run_native_worker(async move {
        state
            .sync_prices(keepbook_server::SyncPricesInput {
                scope: Some(input.scope),
                target: input.target,
                force: input.force,
                quote_staleness_seconds: input.quote_staleness_seconds,
            })
            .await
            .map_err(|error| format!("Price refresh failed: {error:#}"))
    })
    .await
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn reload_data_impl() -> Result<serde_json::Value, String> {
    let state = native_api_state()?.clone();
    run_native_worker(async move {
        state
            .reload()
            .await
            .map_err(|error| format!("Data resync failed: {error:#}"))?;
        Ok(serde_json::json!({ "status": "ok" }))
    })
    .await
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn suggest_ai_rules_impl(
    input: AiRuleSuggestionInput,
) -> Result<AiRuleSuggestionsOutput, String> {
    let state = native_api_state()?.clone();
    run_native_worker(async move {
        let output = state
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
                        tag: transaction.tag,
                        subtag: transaction.subtag,
                        ignored_from_spending: transaction.ignored_from_spending,
                    })
                    .collect(),
                existing_tags: input.existing_tags,
            })
            .await
            .map_err(|error| format!("AI rule suggestion failed: {error:#}"))?;
        from_native_output(output, "AI rule suggestions")
    })
    .await
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn set_transaction_tags_impl(
    input: SetTransactionTagsInput,
) -> Result<(), String> {
    let state = native_api_state()?.clone();
    run_native_worker(async move {
        state
            .set_transaction_tags(keepbook_server::TransactionTagsBatchInput {
                transactions: input
                    .transactions
                    .into_iter()
                    .map(|transaction| keepbook_server::TransactionTagTargetInput {
                        account_id: transaction.account_id,
                        transaction_id: transaction.transaction_id,
                    })
                    .collect(),
                tags: input.tags,
                clear_tags: input.clear_tags,
            })
            .await
            .map_err(|error| format!("Tag update failed: {error:#}"))?;
        Ok(())
    })
    .await
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn set_transaction_ignore_impl(
    input: SetTransactionIgnoreInput,
) -> Result<(), String> {
    let state = native_api_state()?.clone();
    run_native_worker(async move {
        state
            .set_transaction_ignore(keepbook_server::TransactionIgnoreBatchInput {
                transactions: input
                    .transactions
                    .into_iter()
                    .map(|transaction| keepbook_server::TransactionTagTargetInput {
                        account_id: transaction.account_id,
                        transaction_id: transaction.transaction_id,
                    })
                    .collect(),
                ignore: input.ignore,
            })
            .await
            .map_err(|error| format!("Ignore update failed: {error:#}"))?;
        Ok(())
    })
    .await
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn set_transaction_effective_date_impl(
    input: SetTransactionEffectiveDateInput,
) -> Result<(), String> {
    let state = native_api_state()?.clone();
    run_native_worker(async move {
        state
            .set_transaction_effective_date(keepbook_server::TransactionEffectiveDateInput {
                account_id: input.account_id,
                transaction_id: input.transaction_id,
                effective_date: input.effective_date,
                clear_effective_date: input.clear_effective_date,
            })
            .await
            .map_err(|error| format!("Effective date update failed: {error:#}"))?;
        Ok(())
    })
    .await
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn fetch_proposed_transaction_edits_impl(
) -> Result<Vec<ProposedTransactionEdit>, String> {
    let state = native_api_state()?.clone();
    run_native_worker(async move {
        let output = state
            .proposed_transaction_edits(keepbook_server::ProposedTransactionEditsQuery {
                include_decided: false,
            })
            .await
            .map_err(|error| format!("Could not load proposed edits: {error:#}"))?;
        from_native_output(output, "proposed edits")
    })
    .await
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn decide_proposed_transaction_edit_impl(
    id: String,
    action: &'static str,
) -> Result<(), String> {
    let state = native_api_state()?.clone();
    run_native_worker(async move {
        let result = match action {
            "approve" => state.approve_proposed_transaction_edit(id).await,
            "reject" => state.reject_proposed_transaction_edit(id).await,
            "remove" => state.remove_proposed_transaction_edit(id).await,
            _ => return Err(format!("Unsupported proposal action: {action}")),
        };
        result
            .map(|_| ())
            .map_err(|error| format!("Could not update proposed edit: {error:#}"))
    })
    .await
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
    if path.contains("/Library/Application Support/keepbook") {
        return android_default_git_data_dir().display().to_string();
    }

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
