//! Chase synchronizer using the Chase internal API.
//!
//! This synchronizer uses session cookies captured from a browser session
//! to make direct API calls for accounts, balances, and transactions.
//! Login still uses browser automation so the user can complete 2FA.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result};
use chromiumoxide::browser::{Browser, BrowserConfig};
use chromiumoxide::cdp::browser_protocol::network::CookieParam;
use chrono::{NaiveDate, Utc};
use futures::StreamExt;
use secrecy::ExposeSecret;
use serde_json::{Map, Value};

use crate::credentials::{CredentialStore, SessionCache, SessionData, StoredCookie};
use crate::models::{
    Account, Asset, AssetBalance, Connection, ConnectionStatus, Id, LastSync, SyncStatus,
    Transaction, TransactionStatus,
};
use crate::storage::Storage;
use crate::sync::chase::api::{
    max_card_transactions, ActivityAccount, AppDataResponse, CardDetailResponse, ChaseActivity,
    ChaseClient, MortgageDetailResponse, TransactionsResponse, DEFAULT_CARD_TXN_PAGE_SIZE,
};
use crate::sync::{
    AuthStatus, InteractiveAuth, SyncOptions, SyncResult, SyncedAssetBalance, Synchronizer,
    TransactionSyncMode,
};

/// Chase synchronizer using API-based data fetching.
pub struct ChaseSynchronizer {
    connection_id: Id,
    session_cache: SessionCache,
    profile_root: PathBuf,
    credential_store: Option<Box<dyn CredentialStore>>,
}

struct BrowserApiClient {
    _browser: Browser,
    handler_task: tokio::task::JoinHandle<()>,
    page: chromiumoxide::Page,
}

impl Drop for BrowserApiClient {
    fn drop(&mut self) {
        self.handler_task.abort();
    }
}

impl BrowserApiClient {
    async fn connect(profile_dir: &Path, session: &SessionData) -> Result<Self> {
        let (browser, mut handler) = launch_browser(profile_dir, false).await?;
        let handler_task = tokio::spawn(async move { while (handler.next().await).is_some() {} });
        let page = browser.new_page("about:blank").await?;

        page.goto("https://secure.chase.com/web/auth/dashboard")
            .await
            .ok();
        apply_cookies(&page, session).await.ok();
        page.goto("https://secure.chase.com/web/auth/dashboard")
            .await
            .ok();
        ensure_logged_in_with_timeout(&page, Duration::from_secs(30)).await?;

        Ok(Self {
            _browser: browser,
            handler_task,
            page,
        })
    }

    async fn capture_session(&self) -> Result<SessionData> {
        session_from_page(&self.page).await
    }

    async fn post_json(&self, path: &str, body: &str) -> Result<serde_json::Value> {
        browser_fetch_json(&self.page, "POST", path, Some(body)).await
    }

    async fn get_json(&self, path: &str) -> Result<serde_json::Value> {
        browser_fetch_json(&self.page, "GET", path, None).await
    }

    async fn test_auth(&self) -> Result<()> {
        let value = self
            .post_json(
                "/svc/rl/accounts/secure/v1/dashboard/data/list",
                "context=GWM_OVD_NEW_PBM",
            )
            .await?;
        let resp: AppDataResponse =
            serde_json::from_value(value).context("Failed to parse Chase dashboard response")?;
        if resp.code != "SUCCESS" {
            anyhow::bail!(
                "Chase auth test failed in browser context: code={}",
                resp.code
            );
        }
        Ok(())
    }

    async fn get_accounts(&self) -> Result<Vec<ActivityAccount>> {
        let value = self
            .post_json(
                "/svc/rl/accounts/secure/v1/dashboard/data/list",
                "context=GWM_OVD_NEW_PBM",
            )
            .await?;
        let resp: AppDataResponse =
            serde_json::from_value(value).context("Failed to parse Chase dashboard response")?;
        for cached in &resp.cache {
            if cached.url.contains("activity/options/list") {
                if let Some(accounts) = cached.response.get("accounts") {
                    let accounts: Vec<ActivityAccount> =
                        serde_json::from_value(accounts.clone())
                            .context("Failed to parse accounts from dashboard cache")?;
                    return Ok(accounts);
                }
            }
        }
        anyhow::bail!("Could not find accounts in Chase dashboard response");
    }

    async fn get_card_detail(&self, account_id: i64) -> Result<CardDetailResponse> {
        let value = self
            .post_json(
                "/svc/rr/accounts/secure/v2/account/detail/card/list",
                &format!("accountId={account_id}"),
            )
            .await?;
        serde_json::from_value(value).context("Failed to parse Chase card detail")
    }

    async fn get_mortgage_detail(&self, account_id: i64) -> Result<MortgageDetailResponse> {
        let value = self
            .post_json(
                "/svc/rr/accounts/secure/v2/account/detail/mortgage/list",
                &format!("accountId={account_id}"),
            )
            .await?;
        serde_json::from_value(value).context("Failed to parse Chase mortgage detail")
    }

    async fn get_card_transactions(
        &self,
        account_id: i64,
        record_count: u32,
        pagination_key: Option<String>,
    ) -> Result<TransactionsResponse> {
        let mut path = format!(
            "/svc/rr/accounts/secure/gateway/credit-card/transactions/inquiry-maintenance/etu-transactions/v4/accounts/transactions?digital-account-identifier={account_id}&provide-available-statement-indicator=true&record-count={record_count}&sort-order-code=D&sort-key-code=T"
        );
        path.push_str(&crate::sync::chase::api::transaction_date_range_params());

        if let Some(key) = pagination_key {
            path.push_str(&format!("&next-page-key={key}"));
        }

        let value = self.get_json(&path).await?;
        serde_json::from_value(value).context("Failed to parse Chase transactions response")
    }

    async fn get_all_card_transactions(&self, account_id: i64) -> Result<Vec<ChaseActivity>> {
        // The direct HTTP client and browser-backed API client should behave the same
        // for transaction history: paginate until Chase stops, with safety guards.
        //
        // We reuse the shared pagination helper in the API module so fixes apply to both.
        let page_size = crate::sync::chase::api::DEFAULT_CARD_TXN_PAGE_SIZE;
        let max_transactions = crate::sync::chase::api::max_card_transactions();

        crate::sync::chase::api::get_all_card_transactions_paginated(
            "Chase(browser)",
            page_size,
            max_transactions,
            |key| self.get_card_transactions(account_id, page_size, key),
        )
        .await
    }
}

enum ChaseBackend {
    Direct(ChaseClient),
    Browser(BrowserApiClient),
}

impl ChaseBackend {
    async fn get_accounts(&self) -> Result<Vec<ActivityAccount>> {
        match self {
            Self::Direct(client) => client.get_accounts().await,
            Self::Browser(client) => client.get_accounts().await,
        }
    }

    async fn get_card_detail(&self, account_id: i64) -> Result<CardDetailResponse> {
        match self {
            Self::Direct(client) => client.get_card_detail(account_id).await,
            Self::Browser(client) => client.get_card_detail(account_id).await,
        }
    }

    async fn get_mortgage_detail(&self, account_id: i64) -> Result<MortgageDetailResponse> {
        match self {
            Self::Direct(client) => client.get_mortgage_detail(account_id).await,
            Self::Browser(client) => client.get_mortgage_detail(account_id).await,
        }
    }

    async fn get_all_card_transactions(&self, account_id: i64) -> Result<Vec<ChaseActivity>> {
        match self {
            Self::Direct(client) => client.get_all_card_transactions(account_id).await,
            Self::Browser(client) => client.get_all_card_transactions(account_id).await,
        }
    }

    async fn get_card_transactions_page(
        &self,
        account_id: i64,
        record_count: u32,
        pagination_key: Option<String>,
    ) -> Result<TransactionsResponse> {
        match self {
            Self::Direct(client) => {
                client
                    .get_card_transactions(account_id, record_count, pagination_key)
                    .await
            }
            Self::Browser(client) => {
                client
                    .get_card_transactions(account_id, record_count, pagination_key)
                    .await
            }
        }
    }
}

impl ChaseSynchronizer {
    /// Create a new Chase synchronizer for a connection.
    pub async fn from_connection<S: Storage + ?Sized>(
        connection: &Connection,
        storage: &S,
    ) -> Result<Self> {
        let profile_root = default_profile_root()?;
        let credential_store = storage.get_credential_store(connection.id())?;
        Ok(Self {
            connection_id: connection.id().clone(),
            session_cache: SessionCache::new()?,
            profile_root,
            credential_store,
        })
    }

    /// Create a new Chase synchronizer with a custom download dir (back-compat; ignored).
    pub async fn from_connection_with_download_dir<S: Storage + ?Sized>(
        connection: &Connection,
        storage: &S,
        _base_dir: &Path,
    ) -> Result<Self> {
        Self::from_connection(connection, storage).await
    }

    /// Create a synchronizer using an explicit session cache (useful for tests).
    pub fn with_session_cache(
        connection: &Connection,
        session_cache: SessionCache,
    ) -> Result<Self> {
        let profile_root = default_profile_root()?;
        Ok(Self {
            connection_id: connection.id().clone(),
            session_cache,
            profile_root,
            credential_store: None,
        })
    }

    fn session_key(&self) -> String {
        self.connection_id.to_string()
    }

    fn get_session(&self) -> Result<Option<SessionData>> {
        self.session_cache.get(&self.session_key())
    }

    fn ensure_profile_dir(&self) -> Result<PathBuf> {
        let dir = self.profile_root.join(self.connection_id.to_string());
        std::fs::create_dir_all(&dir)
            .with_context(|| format!("Failed to create profile dir: {}", dir.display()))?;
        Ok(dir)
    }

    fn should_autofill_login() -> bool {
        match std::env::var("KEEPBOOK_CHASE_AUTOFILL") {
            Ok(v) => !(v == "0" || v.eq_ignore_ascii_case("false") || v.eq_ignore_ascii_case("no")),
            Err(_) => true,
        }
    }

    fn should_auto_capture_login() -> bool {
        // Default to auto-capture so login can be fully hands-off (aside from 2FA).
        // Set KEEPBOOK_CHASE_AUTO_CAPTURE=0 to force the legacy "press Enter" prompt.
        match std::env::var("KEEPBOOK_CHASE_AUTO_CAPTURE") {
            Ok(v) => !(v == "0" || v.eq_ignore_ascii_case("false") || v.eq_ignore_ascii_case("no")),
            Err(_) => true,
        }
    }

    fn login_timeout() -> Duration {
        // Default to a generous timeout; Chase can require multiple steps.
        let secs = std::env::var("KEEPBOOK_CHASE_LOGIN_TIMEOUT_SECS")
            .ok()
            .and_then(|s| s.trim().parse::<u64>().ok())
            .filter(|n| *n > 0)
            .unwrap_or(600);
        Duration::from_secs(secs)
    }

    fn env_credential(key: &str) -> Option<String> {
        std::env::var(key)
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    }

    async fn get_login_credentials(&self) -> Result<Option<(String, String)>> {
        // Highest priority: explicit environment variables (no disk needed).
        //
        // Note: values may be visible to local process inspection tools; prefer pass-backed
        // credential store where possible.
        let env_user = Self::env_credential("KEEPBOOK_CHASE_USERNAME");
        let env_pass = Self::env_credential("KEEPBOOK_CHASE_PASSWORD");
        if let (Some(u), Some(p)) = (env_user, env_pass) {
            return Ok(Some((u, p)));
        }

        let Some(store) = &self.credential_store else {
            return Ok(None);
        };

        let username = store
            .get("username")
            .await?
            .map(|s| s.expose_secret().to_string());
        let password = store
            .get("password")
            .await?
            .map(|s| s.expose_secret().to_string());

        match (username, password) {
            (Some(u), Some(p)) if !u.trim().is_empty() && !p.is_empty() => Ok(Some((u, p))),
            _ => Ok(None),
        }
    }

    async fn sync_internal(
        &self,
        connection: &mut Connection,
        storage: &dyn Storage,
        options: &SyncOptions,
    ) -> Result<SyncResult> {
        let session = self
            .get_session()?
            .context("No Chase session found. Run `keepbook auth chase login` first.")?;
        if session.cookies.is_empty() && session.cookie_jar.is_empty() {
            anyhow::bail!("Chase session has no cookies. Run `keepbook auth chase login` again.");
        }

        let mut backend = {
            let direct = ChaseClient::new(session.clone())?;
            match direct.test_auth().await {
                Ok(()) => ChaseBackend::Direct(direct),
                Err(err) => {
                    eprintln!(
                        "Chase: direct API auth failed ({err:#}); trying browser API fallback"
                    );
                    let profile_dir = self.ensure_profile_dir()?;
                    let browser = BrowserApiClient::connect(&profile_dir, &session)
                        .await
                        .context("Failed to initialize browser API fallback")?;
                    browser.test_auth().await.context(
                        "Chase session is expired or invalid. Run `keepbook auth chase login`.",
                    )?;
                    ChaseBackend::Browser(browser)
                }
            }
        };

        // Load existing accounts to preserve created_at.
        let existing_accounts = storage.list_accounts().await?;
        let existing_by_id: HashMap<String, Account> = existing_accounts
            .into_iter()
            .filter(|a| a.connection_id == *connection.id())
            .map(|a| (a.id.to_string(), a))
            .collect();

        // Fetch accounts from Chase.
        let chase_accounts = backend.get_accounts().await?;
        eprintln!("Chase: found {} accounts", chase_accounts.len());

        let mut accounts = Vec::new();
        let mut balances: Vec<(Id, Vec<SyncedAssetBalance>)> = Vec::new();
        let mut transactions: Vec<(Id, Vec<Transaction>)> = Vec::new();

        for acct in &chase_accounts {
            let account_id = Id::from_external(&format!("chase:{}:{}", connection.id(), acct.id));

            let created_at = existing_by_id
                .get(&account_id.to_string())
                .map(|a| a.created_at)
                .unwrap_or_else(Utc::now);

            let name = if !acct.nickname.is_empty() {
                format!("{} ({})", acct.nickname, acct.mask)
            } else {
                format!("Chase ({})", acct.mask)
            };

            let is_mortgage = acct.category_type.to_lowercase().contains("mortgage")
                || acct.account_type.to_lowercase().contains("mortgage");
            let is_credit_card = acct.category_type.to_lowercase().contains("card")
                || acct.account_type.to_lowercase().contains("card")
                || acct.account_type.to_lowercase().contains("credit");

            let mut tags = vec!["chase".to_string()];
            if is_credit_card {
                tags.push("credit_card".to_string());
            } else if is_mortgage {
                tags.push("mortgage".to_string());
            } else {
                tags.push(acct.account_type.to_lowercase());
            }

            let mut account = Account::new_with(
                account_id.clone(),
                created_at,
                name,
                connection.id().clone(),
            );
            account.tags = tags;
            account.synchronizer_data = serde_json::json!({
                "chase_account_id": acct.id,
                "mask": acct.mask,
                "category_type": acct.category_type,
                "account_type": acct.account_type,
            });

            // Fetch balance.
            let mut account_balances: Vec<SyncedAssetBalance> = Vec::new();

            if is_mortgage {
                match backend.get_mortgage_detail(acct.id).await {
                    Ok(detail) => {
                        if let Some(ref d) = detail.detail {
                            if let Some(bal) = d.balance {
                                // Mortgages are liabilities; negate so the balance is negative.
                                account_balances.push(SyncedAssetBalance::new(AssetBalance::new(
                                    Asset::currency("USD"),
                                    (-bal).to_string(),
                                )));
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!(
                            "Chase: failed to get mortgage detail for {} ({}): {e:#}",
                            acct.mask, acct.id
                        );
                    }
                }
            } else {
                match backend.get_card_detail(acct.id).await {
                    Ok(detail) => {
                        if let Some(ref card) = detail.detail {
                            if let Some(bal) = card.current_balance {
                                // Credit card balances are amounts owed; negate.
                                account_balances.push(SyncedAssetBalance::new(AssetBalance::new(
                                    Asset::currency("USD"),
                                    (-bal).to_string(),
                                )));
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!(
                            "Chase: failed to get card detail for {} ({}): {e:#}",
                            acct.mask, acct.id
                        );
                    }
                }
            }

            // Fetch transactions for credit card accounts.
            let mut acct_txns: Vec<Transaction> = Vec::new();
            if is_credit_card {
                let activities = match options.transactions {
                    TransactionSyncMode::Full => backend.get_all_card_transactions(acct.id).await,
                    TransactionSyncMode::Auto => {
                        get_card_transactions_auto(
                            &backend,
                            storage,
                            connection.id(),
                            &account_id,
                            acct.id,
                        )
                        .await
                    }
                };

                match activities {
                    Ok(activities) => {
                        eprintln!(
                            "Chase: fetched {} transactions for {} ({})",
                            activities.len(),
                            acct.mask,
                            acct.id
                        );
                        for activity in &activities {
                            if let Some(txn) =
                                chase_activity_to_transaction(activity, connection.id(), acct.id)
                            {
                                acct_txns.push(txn);
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!(
                            "Chase: failed to get transactions for {} ({}): {e:#}",
                            acct.mask, acct.id
                        );
                    }
                }
            }

            accounts.push(account);
            balances.push((account_id.clone(), account_balances));
            transactions.push((account_id, acct_txns));
        }

        // If we had to use browser fallback, refresh cached session cookies from the browser.
        if let ChaseBackend::Browser(browser) = &mut backend {
            if let Ok(session) = browser.capture_session().await {
                let _ = self.session_cache.set(&self.session_key(), &session);
            }
        }

        // Update connection state.
        connection.state.last_sync = Some(LastSync {
            at: Utc::now(),
            status: SyncStatus::Success,
            error: None,
        });
        connection.state.status = ConnectionStatus::Active;
        connection.state.account_ids = accounts.iter().map(|a| a.id.clone()).collect();

        let imported_transactions: u64 = transactions.iter().map(|(_, v)| v.len() as u64).sum();
        let mut data = connection
            .state
            .synchronizer_data
            .as_object()
            .cloned()
            .unwrap_or_default();

        // Clean up old browser-based fields.
        data.remove("download_dir");
        data.remove("downloads");
        data.remove("downloaded_count");

        data.insert(
            "imported_at".to_string(),
            serde_json::Value::String(Utc::now().to_rfc3339()),
        );
        data.insert(
            "imported_accounts".to_string(),
            serde_json::Value::Number((accounts.len() as u64).into()),
        );
        data.insert(
            "imported_transactions".to_string(),
            serde_json::Value::Number(imported_transactions.into()),
        );
        data.insert(
            "method".to_string(),
            serde_json::Value::String("api".to_string()),
        );
        connection.state.synchronizer_data = serde_json::Value::Object(data);

        Ok(SyncResult {
            connection: connection.clone(),
            accounts,
            balances,
            transactions,
        })
    }
}

#[async_trait::async_trait]
impl Synchronizer for ChaseSynchronizer {
    fn name(&self) -> &str {
        "chase"
    }

    async fn sync(&self, connection: &mut Connection, storage: &dyn Storage) -> Result<SyncResult> {
        let options = SyncOptions::default();
        self.sync_internal(connection, storage, &options).await
    }

    async fn sync_with_options(
        &self,
        connection: &mut Connection,
        storage: &dyn Storage,
        options: &SyncOptions,
    ) -> Result<SyncResult> {
        self.sync_internal(connection, storage, options).await
    }

    fn interactive(&mut self) -> Option<&mut dyn InteractiveAuth> {
        Some(self)
    }
}

impl ChaseSynchronizer {
    /// Sync with storage access for future account lookups.
    pub async fn sync_with_storage<S: Storage>(
        &self,
        connection: &mut Connection,
        storage: &S,
    ) -> Result<SyncResult> {
        let options = SyncOptions::default();
        self.sync_internal(connection, storage, &options).await
    }
}

#[async_trait::async_trait]
impl InteractiveAuth for ChaseSynchronizer {
    fn auth_required_for_sync(&self) -> bool {
        true
    }

    async fn check_auth(&self) -> Result<AuthStatus> {
        match self.get_session()? {
            None => Ok(AuthStatus::Missing),
            Some(session) => {
                if session.cookies.is_empty() && session.cookie_jar.is_empty() {
                    return Ok(AuthStatus::Missing);
                }

                if let Some(captured_at) = session.captured_at {
                    let age_secs = Utc::now().timestamp() - captured_at;
                    if age_secs > 7 * 24 * 60 * 60 {
                        return Ok(AuthStatus::Expired {
                            reason: format!("Session is {} hours old", age_secs / 3600),
                        });
                    }
                }

                // Probe the Chase API to verify the session is actually valid.
                // Sessions can be revoked server-side before the 7-day age limit.
                match ChaseClient::new(session) {
                    Ok(client) => match client.test_auth().await {
                        Ok(()) => Ok(AuthStatus::Valid),
                        Err(err) => Ok(AuthStatus::Expired {
                            reason: format!("Session rejected by Chase API: {err:#}"),
                        }),
                    },
                    Err(err) => Ok(AuthStatus::Expired {
                        reason: format!("Failed to build Chase client: {err:#}"),
                    }),
                }
            }
        }
    }

    async fn login(&mut self) -> Result<()> {
        let profile_dir = self.ensure_profile_dir()?;
        cleanup_profile_lock_artifacts(&profile_dir);
        kill_profile_browser_processes(&profile_dir);
        let (browser, mut handler) = launch_browser(&profile_dir, true).await?;
        let handler_task = tokio::spawn(async move { while (handler.next().await).is_some() {} });

        let page = browser.new_page("about:blank").await?;

        // Go directly to a page that will show the login iframe when unauthenticated.
        page.goto("https://secure.chase.com/web/auth/dashboard")
            .await?;

        let auto_capture = Self::should_auto_capture_login();

        if Self::should_autofill_login() {
            match self.get_login_credentials().await {
                Ok(Some((username, password))) => {
                    eprintln!(
                        "Chase: attempting autofill (set KEEPBOOK_CHASE_AUTOFILL=0 to disable)..."
                    );
                    if let Err(err) = autofill_login_iframe(&page, &username, &password).await {
                        eprintln!("Chase: autofill failed (continuing with manual login): {err:#}");
                    }
                }
                Ok(None) => {}
                Err(err) => {
                    eprintln!(
                        "Chase: could not load credentials (continuing with manual login): {err:#}"
                    );
                }
            }
        }

        // Optional: let the user enter an SMS code in the terminal so we can fill it into the
        // browser. This is best-effort and may not work for all Chase flows.
        if std::env::var("KEEPBOOK_CHASE_SMS_CODE").is_ok() {
            if let Err(err) = maybe_prompt_and_fill_sms_code(&page).await {
                eprintln!("Chase: SMS-code assist failed (continuing with manual login): {err:#}");
            }
        }

        if auto_capture {
            let timeout = Self::login_timeout();
            eprintln!(
                "Chase: waiting for login to complete (timeout={}s; set KEEPBOOK_CHASE_LOGIN_TIMEOUT_SECS or KEEPBOOK_CHASE_AUTO_CAPTURE=0 for manual)...",
                timeout.as_secs()
            );
            ensure_logged_in_with_timeout(&page, timeout).await?;
        } else {
            eprintln!("\n========================================");
            eprintln!("Complete Chase login in the browser.");
            eprintln!("When finished, return here and press Enter.");
            eprintln!("========================================\n");

            let mut input = String::new();
            let _ = std::io::stdin().read_line(&mut input);

            // Navigate to dashboard to ensure cookies are set for secure.chase.com.
            page.goto("https://secure.chase.com/web/auth/dashboard")
                .await
                .ok();
            ensure_logged_in_with_timeout(&page, Duration::from_secs(30)).await?;
        }

        // Always land on the secure dashboard before we try to validate API access; otherwise
        // browser-context fetches may be blocked by page origin/CORS even if the user is logged in.
        page.goto("https://secure.chase.com/web/auth/dashboard")
            .await
            .ok();
        ensure_logged_in_with_timeout(&page, Duration::from_secs(30)).await?;

        eprintln!("Capturing cookies...");
        let session = wait_for_valid_api_session(&page).await?;

        self.session_cache.set(&self.session_key(), &session)?;

        eprintln!(
            "Session saved successfully ({} cookies).",
            session.cookie_jar.len().max(session.cookies.len())
        );

        drop(browser);
        handler_task.abort();

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn chase_activity_to_transaction(
    activity: &ChaseActivity,
    connection_id: &Id,
    chase_account_id: i64,
) -> Option<Transaction> {
    let stable_id = activity.stable_id();
    let tx_id = Id::from_external(&format!(
        "chase:{}:{}:{}",
        connection_id, chase_account_id, stable_id
    ));

    let date_str = if activity.is_pending() {
        &activity.transaction_date
    } else {
        activity
            .transaction_post_date
            .as_deref()
            .unwrap_or(&activity.transaction_date)
    };

    let timestamp = NaiveDate::parse_from_str(date_str, "%Y-%m-%d")
        .ok()
        .and_then(|d| d.and_hms_opt(12, 0, 0))
        .map(|dt| dt.and_utc())?;

    let status = if activity.is_pending() {
        TransactionStatus::Pending
    } else {
        TransactionStatus::Posted
    };

    let amount = activity.signed_amount();
    let mut synchronizer_data = Map::new();
    synchronizer_data.insert(
        "chase_account_id".to_string(),
        Value::Number(chase_account_id.into()),
    );
    synchronizer_data.insert("stable_id".to_string(), Value::String(stable_id));
    synchronizer_data.insert(
        "transaction_status".to_string(),
        Value::String(activity.transaction_status_code.clone()),
    );
    synchronizer_data.insert(
        "credit_debit_code".to_string(),
        Value::String(activity.credit_debit_code.clone()),
    );
    synchronizer_data.insert(
        "transaction_date".to_string(),
        Value::String(activity.transaction_date.clone()),
    );
    synchronizer_data.insert(
        "post_date".to_string(),
        activity
            .transaction_post_date
            .clone()
            .map(Value::String)
            .unwrap_or(Value::Null),
    );
    if let Some(v) = &activity.sor_transaction_identifier {
        if !v.trim().is_empty() {
            synchronizer_data.insert(
                "sor_transaction_identifier".to_string(),
                Value::String(v.clone()),
            );
        }
    }
    if let Some(v) = &activity.derived_unique_transaction_identifier {
        if !v.trim().is_empty() {
            synchronizer_data.insert(
                "derived_unique_transaction_identifier".to_string(),
                Value::String(v.clone()),
            );
        }
    }
    if let Some(v) = &activity.transaction_reference_number {
        if !v.trim().is_empty() {
            synchronizer_data.insert(
                "transaction_reference_number".to_string(),
                Value::String(v.clone()),
            );
        }
    }
    if let Some(v) = &activity.etu_standard_transaction_type_name {
        if !v.trim().is_empty() {
            synchronizer_data.insert(
                "etu_standard_transaction_type_name".to_string(),
                Value::String(v.clone()),
            );
        }
    }
    if let Some(v) = &activity.etu_standard_transaction_type_group_name {
        if !v.trim().is_empty() {
            synchronizer_data.insert(
                "etu_standard_transaction_type_group_name".to_string(),
                Value::String(v.clone()),
            );
        }
    }
    if let Some(v) = &activity.etu_standard_expense_category_code {
        if !v.trim().is_empty() {
            synchronizer_data.insert(
                "etu_standard_expense_category_code".to_string(),
                Value::String(v.clone()),
            );
        }
    }
    if let Some(v) = &activity.last4_card_number {
        if !v.trim().is_empty() {
            synchronizer_data.insert("last4_card_number".to_string(), Value::String(v.clone()));
        }
    }
    if let Some(v) = activity.digital_account_identifier {
        synchronizer_data.insert(
            "digital_account_identifier".to_string(),
            Value::Number(v.into()),
        );
    }
    if let Some(details) = &activity.merchant_details {
        if let Some(raw) = &details.raw_merchant_details {
            if let Some(v) = &raw.merchant_dba_name {
                if !v.trim().is_empty() {
                    synchronizer_data
                        .insert("merchant_dba_name".to_string(), Value::String(v.clone()));
                }
            }
            if let Some(v) = &raw.merchant_city_name {
                if !v.trim().is_empty() {
                    synchronizer_data
                        .insert("merchant_city_name".to_string(), Value::String(v.clone()));
                }
            }
            if let Some(v) = &raw.merchant_state_code {
                if !v.trim().is_empty() {
                    synchronizer_data
                        .insert("merchant_state_code".to_string(), Value::String(v.clone()));
                }
            }
            if let Some(v) = &raw.merchant_category_code {
                if !v.trim().is_empty() {
                    synchronizer_data.insert(
                        "merchant_category_code".to_string(),
                        Value::String(v.clone()),
                    );
                }
            }
            if let Some(v) = &raw.merchant_category_name {
                if !v.trim().is_empty() {
                    synchronizer_data.insert(
                        "merchant_category_name".to_string(),
                        Value::String(v.clone()),
                    );
                }
            }
        }

        let enriched_merchant_names: Vec<Value> = details
            .enriched_merchants
            .iter()
            .filter_map(|m| {
                m.merchant_name
                    .as_ref()
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty())
                    .map(|s| Value::String(s.to_string()))
            })
            .collect();
        if !enriched_merchant_names.is_empty() {
            synchronizer_data.insert(
                "enriched_merchant_names".to_string(),
                Value::Array(enriched_merchant_names),
            );
        }

        let enriched_merchant_role_type_codes: Vec<Value> = details
            .enriched_merchants
            .iter()
            .filter_map(|m| m.merchant_role_type_code.map(|v| Value::Number(v.into())))
            .collect();
        if !enriched_merchant_role_type_codes.is_empty() {
            synchronizer_data.insert(
                "enriched_merchant_role_type_codes".to_string(),
                Value::Array(enriched_merchant_role_type_codes),
            );
        }
    }

    Some(Transaction {
        id: tx_id,
        timestamp,
        amount: amount.to_string(),
        asset: Asset::currency(activity.currency_code.as_deref().unwrap_or("USD")).normalized(),
        description: activity.description(),
        status,
        synchronizer_data: Value::Object(synchronizer_data),
        standardized_metadata: None,
    })
    .map(Transaction::backfill_standardized_metadata)
}

fn chase_activity_to_transaction_id(
    connection_id: &Id,
    chase_account_id: i64,
    activity: &ChaseActivity,
) -> Id {
    let stable_id = activity.stable_id();
    Id::from_external(&format!(
        "chase:{}:{}:{}",
        connection_id, chase_account_id, stable_id
    ))
}

fn chase_overlap_stop_threshold() -> usize {
    std::env::var("KEEPBOOK_CHASE_OVERLAP_THRESHOLD")
        .ok()
        .and_then(|s| s.trim().parse::<usize>().ok())
        .filter(|n| *n > 0)
        .unwrap_or(200)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sync::chase::api::{
        ChaseActivity, EnrichedMerchant, MerchantDetails, RawMerchantDetails,
    };

    #[test]
    fn chase_activity_to_transaction_persists_extended_metadata() {
        let activity = ChaseActivity {
            transaction_status_code: "Posted".to_string(),
            transaction_amount: 12.34,
            transaction_date: "2026-02-15".to_string(),
            transaction_post_date: Some("2026-02-16".to_string()),
            sor_transaction_identifier: Some("sor-123".to_string()),
            derived_unique_transaction_identifier: Some("derived-456".to_string()),
            transaction_reference_number: Some("ref-789".to_string()),
            credit_debit_code: "D".to_string(),
            etu_standard_transaction_type_name: Some("Card Purchase".to_string()),
            etu_standard_transaction_type_group_name: Some("Purchases".to_string()),
            etu_standard_expense_category_code: Some("FOOD_AND_DRINK".to_string()),
            currency_code: Some("USD".to_string()),
            merchant_details: Some(MerchantDetails {
                raw_merchant_details: Some(RawMerchantDetails {
                    merchant_dba_name: Some("Coffee Shop".to_string()),
                    merchant_city_name: Some("San Francisco".to_string()),
                    merchant_state_code: Some("CA".to_string()),
                    merchant_category_code: Some("5814".to_string()),
                    merchant_category_name: Some("Fast Food".to_string()),
                }),
                enriched_merchants: vec![EnrichedMerchant {
                    merchant_name: Some("Blue Bottle Coffee".to_string()),
                    merchant_role_type_code: Some(101),
                }],
            }),
            last4_card_number: Some("1234".to_string()),
            digital_account_identifier: Some(987654321),
        };

        let tx = chase_activity_to_transaction(&activity, &Id::from_string("conn-1"), 123)
            .expect("expected transaction to parse");
        let data = tx
            .synchronizer_data
            .as_object()
            .expect("expected synchronizer_data object");

        assert_eq!(
            data.get("etu_standard_expense_category_code")
                .and_then(|v| v.as_str()),
            Some("FOOD_AND_DRINK")
        );
        assert_eq!(
            data.get("merchant_category_code").and_then(|v| v.as_str()),
            Some("5814")
        );
        assert_eq!(
            data.get("merchant_category_name").and_then(|v| v.as_str()),
            Some("Fast Food")
        );
        assert_eq!(
            data.get("merchant_dba_name").and_then(|v| v.as_str()),
            Some("Coffee Shop")
        );
        assert_eq!(
            data.get("enriched_merchant_names"),
            Some(&Value::Array(vec![Value::String(
                "Blue Bottle Coffee".to_string()
            )]))
        );
        assert_eq!(
            data.get("enriched_merchant_role_type_codes"),
            Some(&Value::Array(vec![Value::Number(101.into())]))
        );
        assert_eq!(
            data.get("digital_account_identifier")
                .and_then(|v| v.as_i64()),
            Some(987654321)
        );
    }
}

async fn get_card_transactions_auto(
    backend: &ChaseBackend,
    storage: &dyn Storage,
    connection_id: &Id,
    keepbook_account_id: &Id,
    chase_account_id: i64,
) -> Result<Vec<ChaseActivity>> {
    // If we have no stored transactions for this account, we need a full backfill anyway.
    let existing = storage.get_transactions(keepbook_account_id).await?;
    if existing.is_empty() {
        return backend.get_all_card_transactions(chase_account_id).await;
    }

    let existing_ids: HashSet<Id> = existing.into_iter().map(|t| t.id).collect();

    let page_size = DEFAULT_CARD_TXN_PAGE_SIZE;
    let max_transactions = max_card_transactions();
    let threshold = chase_overlap_stop_threshold();

    let mut all_activities: Vec<ChaseActivity> = Vec::new();
    let mut consecutive_existing: usize = 0;
    let mut pagination_key: Option<String> = None;
    let mut seen_keys: HashSet<String> = HashSet::new();
    let mut pages: usize = 0;
    let max_pages: usize = ((max_transactions / page_size.max(1) as usize).max(1)) + 50;

    loop {
        pages += 1;
        if pages > max_pages {
            eprintln!(
                "Chase: stopping pagination at {} transactions (safety limit: max pages={max_pages}; set KEEPBOOK_CHASE_MAX_TRANSACTIONS to increase overall cap)",
                all_activities.len()
            );
            break;
        }

        let resp = backend
            .get_card_transactions_page(chase_account_id, page_size, pagination_key.clone())
            .await?;

        if resp.activities.is_empty() {
            if resp.more_records_indicator {
                eprintln!(
                    "Chase: got empty transactions page but moreRecordsIndicator=true; stopping to avoid infinite pagination"
                );
            }
            break;
        }

        for activity in resp.activities {
            let id = chase_activity_to_transaction_id(connection_id, chase_account_id, &activity);
            if existing_ids.contains(&id) {
                consecutive_existing += 1;
            } else {
                consecutive_existing = 0;
            }
            all_activities.push(activity);
        }

        if all_activities.len() > max_transactions {
            all_activities.truncate(max_transactions);
            eprintln!(
                "Chase: stopping pagination at {} transactions (safety limit; set KEEPBOOK_CHASE_MAX_TRANSACTIONS to increase)",
                all_activities.len()
            );
            break;
        }

        if consecutive_existing >= threshold {
            eprintln!(
                "Chase: stopping pagination after detecting overlap ({} consecutive existing transactions; set KEEPBOOK_CHASE_OVERLAP_THRESHOLD or use --transactions full)",
                consecutive_existing
            );
            break;
        }

        if !resp.more_records_indicator {
            break;
        }

        match resp.pagination_contextual_text {
            Some(ref key) if !key.is_empty() => {
                if !seen_keys.insert(key.clone()) {
                    eprintln!(
                        "Chase: pagination key repeated; stopping to avoid infinite pagination"
                    );
                    break;
                }
                pagination_key = Some(key.clone());
            }
            _ => break,
        }
    }

    Ok(all_activities)
}

fn default_profile_root() -> Result<PathBuf> {
    let base = dirs::cache_dir().context("Could not find cache directory")?;
    let dir = base.join("keepbook").join("chase").join("profiles");
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("Failed to create profile root: {}", dir.display()))?;
    Ok(dir)
}

async fn ensure_logged_in_with_timeout(
    page: &chromiumoxide::Page,
    timeout: Duration,
) -> Result<()> {
    let check_js = r#"(function() {
      const url = String(window.location && window.location.href || '');
      const txt = (document.body && document.body.innerText || '').toLowerCase();
      const hasSignIn = txt.includes('sign in') || txt.includes('signin') || txt.includes('sign on') || txt.includes('enroll');
      const hasLogout = txt.includes('sign out') || txt.includes('log out') || txt.includes('logout');
      const isSecure = url.includes('secure.chase.com') || url.includes('/web/auth/');
      const loginIframe = document.querySelector('iframe[name=\"logonbox\"], iframe#logonbox');
      const hasLoginIframe = !!loginIframe;
      let iframeSnippet = '';
      if (loginIframe) {
        try {
          const doc = loginIframe.contentDocument || (loginIframe.contentWindow && loginIframe.contentWindow.document);
          if (doc && doc.body) {
            iframeSnippet = String(doc.body.innerText || '').replace(/\s+/g, ' ').trim().slice(0, 160);
          }
        } catch (_) {}
      }
      const isLogonUrl = /\/logon\/|#\/logon\//i.test(url);
      return { url, hasSignIn, hasLogout, isSecure, hasLoginIframe, isLogonUrl, iframeSnippet };
    })()"#;

    let deadline = std::time::Instant::now() + timeout;
    loop {
        let v: serde_json::Value = page.evaluate(check_js).await?.into_value()?;
        let url = v
            .get("url")
            .and_then(|x| x.as_str())
            .unwrap_or_default()
            .to_string();
        let has_sign_in = v.get("hasSignIn").and_then(|x| x.as_bool()).unwrap_or(true);
        let has_logout = v
            .get("hasLogout")
            .and_then(|x| x.as_bool())
            .unwrap_or(false);
        let is_secure = v.get("isSecure").and_then(|x| x.as_bool()).unwrap_or(false);
        let has_login_iframe = v
            .get("hasLoginIframe")
            .and_then(|x| x.as_bool())
            .unwrap_or(true);
        let is_logon_url = v
            .get("isLogonUrl")
            .and_then(|x| x.as_bool())
            .unwrap_or(false);
        let iframe_snippet = v
            .get("iframeSnippet")
            .and_then(|x| x.as_str())
            .unwrap_or_default()
            .to_string();

        if ((is_secure && !has_sign_in) || has_logout) && !has_login_iframe && !is_logon_url {
            return Ok(());
        }

        if std::time::Instant::now() > deadline {
            anyhow::bail!(
                "Chase session does not appear to be logged in (url={url}, has_sign_in={has_sign_in}, has_logout={has_logout}, has_login_iframe={has_login_iframe}, is_logon_url={is_logon_url}, iframe_snippet={iframe_snippet}). Run `keepbook auth chase login` again."
            );
        }

        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

async fn autofill_login_iframe(
    page: &chromiumoxide::Page,
    username: &str,
    password: &str,
) -> Result<()> {
    let creds = serde_json::json!({ "username": username, "password": password });

    let js: String = format!(
        r#"(function(creds) {{
  function fire(el, type) {{
    try {{ el.dispatchEvent(new Event(type, {{ bubbles: true }})); }} catch (_) {{}}
  }}
  function fireMouse(win, el, type) {{
    try {{
      el.dispatchEvent(new win.MouseEvent(type, {{ bubbles: true, cancelable: true, view: win }}));
      return true;
    }} catch (_) {{}}
    return false;
  }}
  const iframeNames = Array.from(document.querySelectorAll('iframe')).map(f => (f.getAttribute('name') || f.id || '').toString()).filter(Boolean).slice(0, 10);
  const iframe = document.querySelector('iframe[name="logonbox"], iframe#logonbox');
  if (!iframe) return {{ ok: false, error: "login iframe not found", iframeNames }};
  const doc = iframe.contentDocument || (iframe.contentWindow && iframe.contentWindow.document);
  if (!doc) return {{ ok: false, error: "iframe document not accessible", iframeNames }};
  const win = doc.defaultView || iframe.contentWindow || window;

  const user = doc.querySelector('#userId-input-field-input') || doc.querySelector('input[name="username"]');
  const pass = doc.querySelector('#password-input-field-input') || doc.querySelector('input[name="password"][type="password"]');
  const btn = doc.querySelector('#signin-button') || doc.querySelector('button[type="submit"]');
  if (!user || !pass || !btn) {{
    return {{
      ok: false,
      error: "missing login controls",
      iframeNames,
      have: {{
        user: !!user,
        pass: !!pass,
        btn: !!btn
      }}
    }};
  }}

  try {{ user.focus(); }} catch (_) {{}}
  user.value = String(creds.username || "");
  fire(user, "input"); fire(user, "change");

  try {{ pass.focus(); }} catch (_) {{}}
  pass.value = String(creds.password || "");
  fire(pass, "input"); fire(pass, "change");

  // Submit. Chase can be picky about event types/context; try multiple strategies.
  let submittedBy = null;
  const form = btn.form || pass.closest('form') || user.closest('form');
  if (form && typeof form.requestSubmit === 'function') {{
    try {{ form.requestSubmit(btn); submittedBy = 'requestSubmit'; }} catch (_) {{}}
  }}
  if (!submittedBy && form && typeof form.submit === 'function') {{
    try {{ form.submit(); submittedBy = 'form.submit'; }} catch (_) {{}}
  }}
  if (!submittedBy) {{
    try {{ btn.focus(); }} catch (_) {{}}
    try {{ btn.click(); submittedBy = 'btn.click'; }} catch (_) {{}}
  }}
  if (!submittedBy) {{
    // Dispatch in the iframe's window context.
    if (fireMouse(win, btn, 'mousedown') || fireMouse(win, btn, 'pointerdown')) {{}}
    if (fireMouse(win, btn, 'mouseup') || fireMouse(win, btn, 'pointerup')) {{}}
    if (fireMouse(win, btn, 'click')) submittedBy = 'dispatch(click)';
  }}
  if (!submittedBy) {{
    // Last resort: press Enter in password field.
    try {{
      pass.focus();
      pass.dispatchEvent(new win.KeyboardEvent('keydown', {{ key: 'Enter', code: 'Enter', keyCode: 13, which: 13, bubbles: true, cancelable: true }}));
      pass.dispatchEvent(new win.KeyboardEvent('keyup', {{ key: 'Enter', code: 'Enter', keyCode: 13, which: 13, bubbles: true, cancelable: true }}));
      submittedBy = 'enter';
    }} catch (_) {{}}
  }}

  return {{ ok: true, submittedBy, btnDisabled: !!btn.disabled }};
}})({})"#,
        creds.to_string()
    );

    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    loop {
        let v: serde_json::Value = page.evaluate(js.clone()).await?.into_value()?;
        let ok = v.get("ok").and_then(|x| x.as_bool()).unwrap_or(false);
        if ok {
            if let Some(by) = v.get("submittedBy").and_then(|x| x.as_str()) {
                eprintln!("Chase: submitted login ({by})");
            }
            return Ok(());
        }

        let err = v
            .get("error")
            .and_then(|x| x.as_str())
            .unwrap_or("unknown error");

        // Chase loads the login iframe lazily and sometimes after redirects.
        // Retry briefly before giving up.
        let retryable = matches!(
            err,
            "login iframe not found" | "iframe document not accessible" | "missing login controls"
        );
        if retryable && std::time::Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(250)).await;
            continue;
        }

        let iframe_names = v
            .get("iframeNames")
            .and_then(|x| x.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .take(10)
                    .collect::<Vec<_>>()
                    .join(",")
            })
            .unwrap_or_default();
        anyhow::bail!("Chase autofill JS failed: {err} (iframes={iframe_names})");
    }
}

async fn maybe_prompt_and_fill_sms_code(page: &chromiumoxide::Page) -> Result<()> {
    // If a verification-code input is visible, prompt the user for the code and fill it.
    // Set KEEPBOOK_CHASE_SMS_CODE=1 to enable this behavior.
    let present_js = r#"(function() {
  function norm(s){ return String(s||'').toLowerCase(); }
  function isVisible(el){
    if (!el || !el.getBoundingClientRect) return false;
    const r = el.getBoundingClientRect();
    return r.width > 20 && r.height > 10;
  }
  function labelFor(doc, el){
    const aria = el.getAttribute('aria-label') || '';
    const ph = el.getAttribute('placeholder') || '';
    if (aria || ph) return aria || ph;
    if (el.id) {
      const lab = doc.querySelector('label[for=\"' + el.id.replace(/\"/g,'') + '\"]');
      if (lab) return lab.textContent || '';
    }
    return (el.name || el.id || '');
  }
  function hasCodeInput(doc){
    const inputs = Array.from(doc.querySelectorAll('input')).filter(isVisible);
    for (const el of inputs) {
      const meta = norm(labelFor(doc, el) + ' ' + (el.id||'') + ' ' + (el.name||''));
      const isCode = meta.includes('code') || meta.includes('passcode') || meta.includes('otp') || meta.includes('verification');
      if (!isCode) continue;
      const t = (el.getAttribute('type') || '').toLowerCase();
      if (t === 'hidden') continue;
      return true;
    }
    return false;
  }

  if (hasCodeInput(document)) return { ok: true };
  const iframes = Array.from(document.querySelectorAll('iframe'));
  for (const fr of iframes) {
    let doc = null;
    try { doc = fr.contentDocument || (fr.contentWindow && fr.contentWindow.document); } catch (_) { doc = null; }
    if (!doc) continue;
    if (hasCodeInput(doc)) return { ok: true };
  }
  return { ok: false };
})()"#;

    // The code input often appears only after submit + redirects; poll briefly.
    let deadline = std::time::Instant::now() + Duration::from_secs(120);
    loop {
        let v: serde_json::Value = page.evaluate(present_js).await?.into_value()?;
        let ok = v.get("ok").and_then(|x| x.as_bool()).unwrap_or(false);
        if ok {
            break;
        }
        if std::time::Instant::now() >= deadline {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }

    eprintln!("\n========================================");
    eprintln!("Chase: enter SMS verification code (or blank to skip):");
    eprintln!("========================================\n");
    let mut code = String::new();
    let _ = std::io::stdin().read_line(&mut code);
    let code = code.trim().to_string();
    if code.is_empty() {
        return Ok(());
    }

    fill_sms_code(page, &code).await?;
    Ok(())
}

async fn fill_sms_code(page: &chromiumoxide::Page, code: &str) -> Result<()> {
    let code = serde_json::json!({ "code": code });
    let js: String = format!(
        r#"(function(payload) {{
  function norm(s){{ return String(s||'').trim().toLowerCase(); }}
  function isVisible(el){{
    if (!el || !el.getBoundingClientRect) return false;
    const r = el.getBoundingClientRect();
    return r.width > 20 && r.height > 10;
  }}
  function fire(el, type) {{
    try {{ el.dispatchEvent(new Event(type, {{ bubbles: true }})); }} catch (_) {{}}
  }}
  function labelFor(doc, el){{
    const aria = el.getAttribute('aria-label') || '';
    const ph = el.getAttribute('placeholder') || '';
    if (aria || ph) return aria || ph;
    if (el.id) {{
      const lab = doc.querySelector('label[for=\"' + el.id.replace(/\"/g,'') + '\"]');
      if (lab) return lab.textContent || '';
    }}
    return (el.name || el.id || '');
  }}
  function score(doc, el) {{
    const meta = norm(labelFor(doc, el) + ' ' + (el.id||'') + ' ' + (el.name||''));
    let s = 0;
    if (meta.includes('otp')) s += 6;
    if (meta.includes('passcode')) s += 6;
    if (meta.includes('verification')) s += 5;
    if (meta.includes('code')) s += 3;
    const t = norm(el.getAttribute('type') || '');
    if (t === 'tel') s += 2;
    return s;
  }}
  function findBest(doc) {{
    const inputs = Array.from(doc.querySelectorAll('input')).filter(isVisible);
    let best = null;
    let bestScore = 0;
    for (const el of inputs) {{
      const meta = norm(labelFor(doc, el) + ' ' + (el.id||'') + ' ' + (el.name||''));
      const isCode = meta.includes('code') || meta.includes('passcode') || meta.includes('otp') || meta.includes('verification');
      if (!isCode) continue;
      const t = norm(el.getAttribute('type') || '');
      if (t === 'hidden') continue;
      const s = score(doc, el);
      if (s > bestScore) {{
        bestScore = s;
        best = el;
      }}
    }}
    return best;
  }}
  function clickSubmit(doc) {{
    const btns = Array.from(doc.querySelectorAll('button,input[type=\"submit\"],input[type=\"button\"],[role=\"button\"],[role=\"link\"]')).filter(isVisible);
    const want = ['verify','continue','next','submit','confirm','done'];
    for (const b of btns) {{
      const txt = norm(b.textContent || b.value || b.getAttribute('aria-label') || '');
      if (!txt) continue;
      if (want.some(w => txt.includes(w))) {{
        try {{ b.click(); return true; }} catch (_) {{}}
        try {{ b.dispatchEvent(new MouseEvent('click', {{bubbles:true, cancelable:true, view:window}})); return true; }} catch (_) {{}}
      }}
    }}
    return false;
  }}
  function tryFill(doc) {{
    const el = findBest(doc);
    if (!el) return false;
    try {{ el.focus(); }} catch (_) {{}}
    el.value = String(payload.code || '');
    fire(el,'input'); fire(el,'change');
    clickSubmit(doc);
    return true;
  }}

  if (tryFill(document)) return {{ ok: true }};
  const iframes = Array.from(document.querySelectorAll('iframe'));
  for (const fr of iframes) {{
    let doc = null;
    try {{ doc = fr.contentDocument || (fr.contentWindow && fr.contentWindow.document); }} catch (_) {{ doc = null; }}
    if (!doc) continue;
    if (tryFill(doc)) return {{ ok: true }};
  }}
  return {{ ok: false, error: 'no code input found' }};
}})({})"#,
        code.to_string()
    );

    let v: serde_json::Value = page.evaluate(js).await?.into_value()?;
    if v.get("ok").and_then(|x| x.as_bool()).unwrap_or(false) {
        return Ok(());
    }
    anyhow::bail!(
        "Could not find a verification-code input (you may need to complete 2FA in the browser)."
    );
}

async fn session_from_page(page: &chromiumoxide::Page) -> Result<SessionData> {
    let cookies = page.get_cookies().await?;

    let mut cookie_map = HashMap::new();
    let mut cookie_jar = Vec::new();
    for cookie in cookies {
        cookie_map.insert(cookie.name.clone(), cookie.value.clone());
        cookie_jar.push(StoredCookie {
            name: cookie.name,
            value: cookie.value,
            domain: cookie.domain,
            path: cookie.path,
            secure: cookie.secure,
            http_only: cookie.http_only,
            same_site: cookie.same_site.map(|s| format!("{s:?}")),
        });
    }

    Ok(SessionData {
        token: None,
        cookies: cookie_map,
        cookie_jar,
        captured_at: Some(Utc::now().timestamp()),
        data: HashMap::new(),
    })
}

async fn wait_for_valid_api_session(page: &chromiumoxide::Page) -> Result<SessionData> {
    // Login redirects and anti-bot checks can complete after the dashboard first appears.
    // Ensure the captured cookie jar can authenticate an API call before persisting it.
    let deadline = std::time::Instant::now() + Duration::from_secs(180);
    loop {
        let session = session_from_page(page).await?;
        // Force a secure origin for browser-context API fetches.
        page.goto("https://secure.chase.com/web/auth/dashboard")
            .await
            .ok();
        match browser_test_auth(page).await {
            Ok(()) => return Ok(session),
            Err(err) => {
                if std::time::Instant::now() >= deadline {
                    anyhow::bail!(
                        "Chase login completed in browser, but API session is not ready: {err}"
                    );
                }
            }
        }

        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

async fn apply_cookies(page: &chromiumoxide::Page, session: &SessionData) -> Result<()> {
    let mut cookies = Vec::new();

    if !session.cookie_jar.is_empty() {
        for c in &session.cookie_jar {
            let mut cookie = CookieParam::new(c.name.clone(), c.value.clone());
            cookie.domain = Some(c.domain.clone());
            cookie.path = Some(c.path.clone());
            cookie.secure = Some(c.secure);
            cookie.http_only = Some(c.http_only);
            cookies.push(cookie);
        }
    } else {
        for (name, value) in &session.cookies {
            let mut cookie = CookieParam::new(name.clone(), value.clone());
            cookie.url = Some("https://www.chase.com".to_string());
            cookies.push(cookie);
        }
    }

    if !cookies.is_empty() {
        page.set_cookies(cookies).await?;
    }

    Ok(())
}

async fn browser_fetch_json(
    page: &chromiumoxide::Page,
    method: &str,
    path: &str,
    body: Option<&str>,
) -> Result<serde_json::Value> {
    let req = serde_json::json!({
        "method": method,
        "path": path,
        "body": body.unwrap_or(""),
    });

    let js = format!(
        r#"(async function(req) {{
  try {{
    const reqId = (globalThis.crypto && typeof globalThis.crypto.randomUUID === 'function')
      ? globalThis.crypto.randomUUID()
      : String(Date.now()) + '-' + String(Math.random()).slice(2);
    const opts = {{
      method: req.method,
      credentials: 'include',
      headers: {{
        'accept': 'application/json, text/plain, */*',
        'x-jpmc-csrf-token': 'NONE',
        'x-jpmc-channel': 'id=C30',
        'x-jpmc-client-request-id': reqId,
        'x-requested-with': 'XMLHttpRequest',
        'referer': 'https://secure.chase.com/web/auth/dashboard',
        'origin': 'https://secure.chase.com'
      }}
    }};
    if (String(req.method || '').toUpperCase() === 'POST') {{
      opts.headers['content-type'] = 'application/x-www-form-urlencoded; charset=UTF-8';
      opts.body = String(req.body || '');
    }}

    const res = await fetch(req.path, opts);
    const text = await res.text();
    return {{ ok: res.ok, status: res.status, text }};
  }} catch (e) {{
    return {{ ok: false, status: 0, text: String((e && e.message) || e || 'fetch failed') }};
  }}
}})({})"#,
        req
    );

    let v: serde_json::Value = page.evaluate(js).await?.into_value()?;
    let ok = v.get("ok").and_then(|x| x.as_bool()).unwrap_or(false);
    let status = v.get("status").and_then(|x| x.as_i64()).unwrap_or(0);
    let text = v
        .get("text")
        .and_then(|x| x.as_str())
        .unwrap_or_default()
        .to_string();

    if !ok {
        anyhow::bail!(
            "Chase browser API request failed (method={method}, path={path}, status={status}): {}",
            text.chars().take(500).collect::<String>()
        );
    }

    serde_json::from_str(&text).with_context(|| {
        format!(
            "Failed to parse Chase browser API JSON (path={path}): {}",
            text.chars().take(200).collect::<String>()
        )
    })
}

async fn browser_test_auth(page: &chromiumoxide::Page) -> Result<()> {
    let value = browser_fetch_json(
        page,
        "POST",
        "/svc/rl/accounts/secure/v1/dashboard/data/list",
        Some("context=GWM_OVD_NEW_PBM"),
    )
    .await?;
    let resp: AppDataResponse =
        serde_json::from_value(value).context("Failed to parse Chase browser auth response")?;
    if resp.code != "SUCCESS" {
        anyhow::bail!("Chase browser auth test failed: code={}", resp.code);
    }
    Ok(())
}

async fn launch_browser(
    profile_dir: &Path,
    show_browser: bool,
) -> Result<(Browser, chromiumoxide::handler::Handler)> {
    let chrome_path = find_chrome().context(
        "Chrome/Chromium not found. Please install Chrome or Chromium to use Chase sync.",
    )?;

    let mut builder = BrowserConfig::builder();
    builder = builder
        .chrome_executable(chrome_path)
        .viewport(None)
        .user_data_dir(profile_dir)
        .arg("--disable-blink-features=AutomationControlled")
        .arg("--disable-infobars")
        .arg("--no-first-run")
        .arg("--no-default-browser-check");
    if show_browser {
        builder = builder.with_head();
    }
    let config = builder
        .build()
        .map_err(|e| anyhow::anyhow!("Failed to configure browser: {e}"))?;

    let (browser, handler) = Browser::launch(config)
        .await
        .context("Failed to launch browser")?;

    Ok((browser, handler))
}

/// Find Chrome/Chromium executable.
fn find_chrome() -> Option<String> {
    if let Ok(output) = std::process::Command::new("which")
        .arg("google-chrome")
        .output()
    {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() {
                return Some(path);
            }
        }
    }

    if let Ok(output) = std::process::Command::new("which").arg("chromium").output() {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() {
                return Some(path);
            }
        }
    }

    let candidates = [
        "/usr/bin/google-chrome",
        "/usr/bin/google-chrome-stable",
        "/usr/bin/chromium",
        "/usr/bin/chromium-browser",
        "/snap/bin/chromium",
        "/run/current-system/sw/bin/google-chrome",
        "/run/current-system/sw/bin/chromium",
        "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
        "/Applications/Chromium.app/Contents/MacOS/Chromium",
    ];

    for candidate in candidates {
        if std::path::Path::new(candidate).exists() {
            return Some(candidate.to_string());
        }
    }
    None
}

fn cleanup_profile_lock_artifacts(profile_dir: &Path) {
    for name in ["SingletonLock", "SingletonSocket"] {
        let path = profile_dir.join(name);
        let _ = std::fs::remove_file(path);
    }
}

fn kill_profile_browser_processes(profile_dir: &Path) {
    let needle = format!("user-data-dir={}", profile_dir.display());
    let output = match std::process::Command::new("pgrep")
        .arg("-f")
        .arg(&needle)
        .output()
    {
        Ok(o) => o,
        Err(_) => return,
    };

    if !output.status.success() {
        return;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        let pid = line.trim();
        if pid.is_empty() {
            continue;
        }
        let _ = std::process::Command::new("kill").arg(pid).status();
    }

    std::thread::sleep(Duration::from_millis(250));
}
