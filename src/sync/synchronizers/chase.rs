//! Chase synchronizer using the Chase internal API.
//!
//! This synchronizer uses session cookies captured from a browser session
//! to make direct API calls for accounts, balances, and transactions.
//! Login still uses browser automation so the user can complete 2FA.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result};
use chrono::{NaiveDate, Utc};
use secrecy::ExposeSecret;
use serde_json::{Map, Value};

use crate::credentials::{CredentialStore, SessionCache, SessionData};
use crate::models::{
    Account, Asset, AssetBalance, Connection, ConnectionStatus, Id, LastSync, SyncStatus,
    Transaction, TransactionStatus,
};
use crate::storage::Storage;
use crate::sync::chase::api::{
    max_card_transactions, ActivityAccount, CardDetailResponse, ChaseActivity, ChaseClient,
    MortgageDetailResponse, TransactionsResponse, DEFAULT_CARD_TXN_PAGE_SIZE,
};
use crate::sync::chase::browser::{
    autofill_login_iframe, default_profile_root, ensure_logged_in_with_timeout,
    maybe_prompt_and_fill_sms_code, open_login_browser, wait_for_valid_api_session,
    BrowserApiClient,
};
use crate::sync::{
    AccountBalances, AccountListing, AuthStatus, InteractiveAuth, SyncOptions, SyncResult,
    SyncedAssetBalance, Synchronizer, TransactionSyncMode,
};

/// Chase synchronizer using API-based data fetching.
pub struct ChaseSynchronizer {
    connection_id: Id,
    session_cache: SessionCache,
    profile_root: PathBuf,
    credential_store: Option<Box<dyn CredentialStore>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ChaseAccountKind {
    CreditCard,
    Mortgage,
    Other,
}

#[allow(clippy::large_enum_variant)]
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
        let mut balances: Vec<(Id, AccountBalances)> = Vec::new();
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

            let account_kind = chase_account_kind(acct);
            let is_credit_card = matches!(account_kind, ChaseAccountKind::CreditCard);

            let mut tags = vec!["chase".to_string()];
            match account_kind {
                ChaseAccountKind::CreditCard => tags.push("credit_card".to_string()),
                ChaseAccountKind::Mortgage => tags.push("mortgage".to_string()),
                ChaseAccountKind::Other => tags.push(acct.account_type.to_lowercase()),
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

            match account_kind {
                ChaseAccountKind::Mortgage => match backend.get_mortgage_detail(acct.id).await {
                    Ok(detail) => {
                        if let Some(ref d) = detail.detail {
                            if let Some(bal) = d.balance {
                                // Mortgages are liabilities; negate so they reduce net worth.
                                account_balances.push(SyncedAssetBalance::new(AssetBalance::new(
                                    Asset::currency("USD"),
                                    liability_balance_amount(bal),
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
                },
                ChaseAccountKind::CreditCard => match backend.get_card_detail(acct.id).await {
                    Ok(detail) => {
                        if let Some(ref card) = detail.detail {
                            if let Some(bal) = card.current_balance {
                                // Credit card balances are amounts owed; negate so they reduce
                                // net worth, while preserving the sign of overpayments.
                                account_balances.push(SyncedAssetBalance::new(AssetBalance::new(
                                    Asset::currency("USD"),
                                    liability_balance_amount(bal),
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
                },
                ChaseAccountKind::Other => {
                    // Chase uses account-type-specific detail endpoints. Until we add an endpoint
                    // for deposit accounts, avoid routing non-liabilities through the credit-card
                    // detail path.
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
            // Chase reports holdings through account-type-specific detail
            // endpoints. When one fails, or the account kind has no detail
            // endpoint yet, nothing was learned about the balance.
            let unavailable_reason = match account_kind {
                ChaseAccountKind::Other => format!(
                    "Chase balances are only fetched for cards and mortgages; account {} was skipped",
                    acct.mask
                ),
                ChaseAccountKind::Mortgage | ChaseAccountKind::CreditCard => {
                    format!("Chase returned no balance for account {}", acct.mask)
                }
            };
            balances.push((
                account_id.clone(),
                AccountBalances::snapshot_or_unavailable(account_balances, unavailable_reason),
            ));
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
            account_listing: AccountListing::Complete,
            balances,
            transactions,
        })
    }
}

fn chase_account_kind(acct: &ActivityAccount) -> ChaseAccountKind {
    let category_type = acct.category_type.to_lowercase();
    let account_type = acct.account_type.to_lowercase();

    if category_type.contains("mortgage") || account_type.contains("mortgage") {
        ChaseAccountKind::Mortgage
    } else if category_type.contains("card")
        || account_type.contains("card")
        || account_type.contains("credit")
    {
        ChaseAccountKind::CreditCard
    } else {
        ChaseAccountKind::Other
    }
}

fn liability_balance_amount(balance: f64) -> String {
    (-balance).to_string()
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
        let (browser, handler_task, page) = open_login_browser(&profile_dir).await?;

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
        "chase:{connection_id}:{chase_account_id}:{stable_id}"
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

    Some(
        Transaction {
            id: tx_id,
            timestamp,
            amount: amount.to_string(),
            asset: Asset::currency(activity.currency_code.as_deref().unwrap_or("USD")).normalized(),
            description: activity.description(),
            status,
            synchronizer_data: Value::Object(synchronizer_data),
            standardized_metadata: None,
        }
        .backfill_standardized_metadata(),
    )
}

fn chase_activity_to_transaction_id(
    connection_id: &Id,
    chase_account_id: i64,
    activity: &ChaseActivity,
) -> Id {
    let stable_id = activity.stable_id();
    Id::from_external(&format!(
        "chase:{connection_id}:{chase_account_id}:{stable_id}"
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
#[allow(clippy::items_after_test_module)]
#[path = "../../../tests/unit/sync/synchronizers/chase_tests.rs"]
mod chase_tests;

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
                "Chase: stopping pagination after detecting overlap ({consecutive_existing} consecutive existing transactions; set KEEPBOOK_CHASE_OVERLAP_THRESHOLD or use --transactions full)"
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
