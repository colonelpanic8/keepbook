//! Schwab synchronizer with browser-based authentication.
//!
//! This synchronizer uses Chrome DevTools Protocol for automated session capture
//! and Schwab's internal APIs for data fetching.

use std::collections::HashMap;
use std::time::Duration;

use anyhow::{Context, Result};
use chromiumoxide::browser::{Browser, BrowserConfig};
use chromiumoxide::cdp::js_protocol::runtime::EvaluateParams;
use chrono::Utc;
use futures::StreamExt;
use secrecy::ExposeSecret;

use crate::credentials::{CredentialStore, SessionCache, SessionData};
use crate::market_data::{AssetId, PriceKind, PricePoint};
use crate::models::{
    Account, Asset, AssetBalance, Connection, ConnectionStatus, Id, LastSync, SyncStatus,
};
use crate::storage::Storage;
use crate::sync::schwab::{
    parse_banking_transactions_rows, parse_brokerage_transactions_rows, Position, SchwabClient,
    TransactionHistoryTimeFrame,
};
use crate::sync::{
    AuthStatus, InteractiveAuth, SyncOptions, SyncResult, SyncedAssetBalance, Synchronizer,
    TransactionSyncMode,
};

const SCHWAB_LOGIN_URL: &str = "https://client.schwab.com/Login/SignOn/CustomerCenterLogin.aspx";

/// Schwab synchronizer with browser-based authentication.
pub struct SchwabSynchronizer {
    connection_id: Id,
    session_cache: SessionCache,
    credential_store: Option<Box<dyn CredentialStore>>,
}

impl SchwabSynchronizer {
    fn banking_selected_account_id(
        account_number_display_full: &str,
        fallback_account_id: &str,
    ) -> String {
        let digits: String = account_number_display_full
            .chars()
            .filter(|c| c.is_ascii_digit())
            .collect();
        if digits.is_empty() {
            fallback_account_id.to_string()
        } else {
            digits
        }
    }

    /// Create a new Schwab synchronizer for a connection.
    pub async fn from_connection<S: Storage + ?Sized>(
        connection: &Connection,
        storage: &S,
    ) -> Result<Self> {
        let session_cache = SessionCache::new()?;
        let credential_store = storage.get_credential_store(connection.id())?;

        Ok(Self {
            connection_id: connection.id().clone(),
            session_cache,
            credential_store,
        })
    }

    /// Create a synchronizer using an explicit session cache (useful for tests).
    pub fn with_session_cache(connection: &Connection, session_cache: SessionCache) -> Self {
        Self {
            connection_id: connection.id().clone(),
            session_cache,
            credential_store: None,
        }
    }

    fn session_key(&self) -> String {
        self.connection_id.to_string()
    }

    fn get_session(&self) -> Result<Option<SessionData>> {
        self.session_cache.get(&self.session_key())
    }

    fn should_autofill_login() -> bool {
        match std::env::var("KEEPBOOK_SCHWAB_AUTOFILL") {
            Ok(v) => !(v == "0" || v.eq_ignore_ascii_case("false") || v.eq_ignore_ascii_case("no")),
            Err(_) => true,
        }
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
        let env_user = Self::env_credential("KEEPBOOK_SCHWAB_USERNAME");
        let env_pass = Self::env_credential("KEEPBOOK_SCHWAB_PASSWORD");
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

    async fn sync_internal<S: Storage + ?Sized>(
        &self,
        connection: &mut Connection,
        storage: &S,
        options: &SyncOptions,
    ) -> Result<SyncResult> {
        // Load session
        let session = self
            .get_session()?
            .context("No session found. Run login first.")?;

        // Load existing accounts to preserve created_at
        let existing_accounts = storage.list_accounts().await?;
        let existing_by_id: HashMap<String, Account> = existing_accounts
            .into_iter()
            .filter(|a| a.connection_id == *connection.id())
            .map(|a| (a.id.to_string(), a))
            .collect();

        // Create client and fetch data
        let client = SchwabClient::new(session)?;

        let accounts_resp = client.get_accounts().await?;
        let positions_resp = client.get_positions().await?;
        let history_accounts = match client.get_transaction_history_brokerage_accounts().await {
            Ok(v) => v,
            Err(err) => {
                eprintln!(
                    "Schwab: transaction-history account metadata unavailable, falling back to /Account ids: {err:#}"
                );
                Vec::new()
            }
        };
        let mut history_account_ids_by_name: HashMap<String, String> = HashMap::new();
        let mut lone_history_account_id: Option<String> = None;
        for acct in &history_accounts {
            let id = acct.id.trim();
            if id.is_empty() {
                continue;
            }
            let key = acct.nick_name.trim().to_ascii_lowercase();
            if !key.is_empty() {
                history_account_ids_by_name.insert(key, id.to_string());
            }
        }
        if history_accounts.len() == 1 {
            let only_id = history_accounts[0].id.trim();
            if !only_id.is_empty() {
                lone_history_account_id = Some(only_id.to_string());
            }
        }

        // Collect all positions into a flat list
        let all_positions: Vec<Position> = positions_resp
            .security_groupings
            .into_iter()
            .flat_map(|g| g.positions)
            .collect();

        // Build sync result
        let mut accounts = Vec::new();
        let mut balances: Vec<(Id, Vec<SyncedAssetBalance>)> = Vec::new();
        let mut transactions: Vec<(Id, Vec<crate::models::Transaction>)> = Vec::new();

        for schwab_account in accounts_resp.accounts {
            // Use Schwab's account_id to generate a stable, filesystem-safe ID
            let account_id = Id::from_external(&schwab_account.account_id);

            // Preserve created_at from existing account if it exists
            let created_at = existing_by_id
                .get(&account_id.to_string())
                .map(|a| a.created_at)
                .unwrap_or_else(Utc::now);

            let account_name = if schwab_account.nick_name.is_empty() {
                schwab_account.default_name.clone()
            } else {
                schwab_account.nick_name.clone()
            };

            let account = Account {
                id: account_id.clone(),
                name: account_name.clone(),
                connection_id: connection.id().clone(),
                tags: vec![
                    "schwab".to_string(),
                    schwab_account.account_type.to_lowercase(),
                ],
                created_at,
                active: true,
                synchronizer_data: serde_json::json!({
                    "account_number": schwab_account.account_number_display_full,
                }),
            };

            let mut account_balances: Vec<SyncedAssetBalance> = vec![];

            // For brokerage accounts, use individual positions (skip CASH position, use balances.cash)
            // For non-brokerage (bank) accounts, use the total balance
            if schwab_account.is_brokerage {
                // Add equity positions
                for position in &all_positions {
                    // Skip CASH position - we'll use balances.cash instead
                    if position.default_symbol == "CASH" {
                        continue;
                    }

                    let asset = Asset::equity(&position.default_symbol);
                    let asset_balance =
                        AssetBalance::new(asset.clone(), position.quantity.to_string())
                            .with_cost_basis(position.cost.to_string());

                    let price_point = PricePoint {
                        asset_id: AssetId::from_asset(&asset),
                        as_of_date: Utc::now().date_naive(),
                        timestamp: Utc::now(),
                        price: position.price.to_string(),
                        quote_currency: "USD".to_string(),
                        kind: PriceKind::Close,
                        source: "schwab".to_string(),
                    };
                    account_balances
                        .push(SyncedAssetBalance::new(asset_balance).with_price(price_point));
                }

                // Add actual cash balance from account balances (not from CASH position)
                if let Some(bal) = &schwab_account.balances {
                    if let Some(cash) = bal.cash {
                        account_balances.push(SyncedAssetBalance::new(AssetBalance::new(
                            Asset::currency("USD"),
                            cash.to_string(),
                        )));
                    }
                }
            } else if let Some(bal) = &schwab_account.balances {
                // Non-brokerage accounts (bank/checking): store total balance as USD
                account_balances.push(SyncedAssetBalance::new(AssetBalance::new(
                    Asset::currency("USD"),
                    bal.balance.to_string(),
                )));
            }

            let existing_txns = storage.get_transactions(&account_id).await?;
            let time_frame = match options.transactions {
                TransactionSyncMode::Full => TransactionHistoryTimeFrame::All,
                TransactionSyncMode::Auto => {
                    if existing_txns.is_empty() {
                        TransactionHistoryTimeFrame::All
                    } else {
                        TransactionHistoryTimeFrame::Last6Months
                    }
                }
            };

            if schwab_account.is_brokerage {
                let tx_account_id = history_account_ids_by_name
                    .get(&account_name.trim().to_ascii_lowercase())
                    .cloned()
                    .or_else(|| lone_history_account_id.clone())
                    .unwrap_or_else(|| schwab_account.account_id.clone());

                let history_rows = client
                    .get_brokerage_transactions(&tx_account_id, &account_name, time_frame)
                    .await
                    .with_context(|| {
                        format!(
                            "Failed to fetch Schwab transactions for account {} (transaction-history id {})",
                            schwab_account.account_id, tx_account_id
                        )
                    })?;

                if !history_rows.is_empty() {
                    let parsed = parse_brokerage_transactions_rows(&account_id, &history_rows)
                        .with_context(|| {
                            format!(
                                "Failed to parse Schwab transactions for account {}",
                                schwab_account.account_id
                            )
                        })?;
                    if !parsed.transactions.is_empty() {
                        transactions.push((account_id.clone(), parsed.transactions));
                    }
                }
            } else {
                let bank_tx_account_id = Self::banking_selected_account_id(
                    &schwab_account.account_number_display_full,
                    &schwab_account.account_id,
                );
                let bank_nickname = if schwab_account.default_name.trim().is_empty() {
                    account_name.as_str()
                } else {
                    schwab_account.default_name.trim()
                };

                let history_rows = match client
                    .get_banking_transactions(&bank_tx_account_id, bank_nickname, time_frame)
                    .await
                {
                    Ok(rows) => rows,
                    Err(err) if time_frame == TransactionHistoryTimeFrame::All => {
                        eprintln!(
                            "Schwab: banking transaction-history does not support timeFrame=All for account {} (banking id {}), retrying Last6Months: {err:#}",
                            schwab_account.account_id, bank_tx_account_id
                        );
                        match client
                            .get_banking_transactions(
                                &bank_tx_account_id,
                                bank_nickname,
                                TransactionHistoryTimeFrame::Last6Months,
                            )
                            .await
                        {
                            Ok(rows) => rows,
                            Err(retry_err) => {
                                eprintln!(
                                    "Schwab: transaction-history fetch unavailable for non-brokerage account {} (banking id {}): {retry_err:#}",
                                    schwab_account.account_id, bank_tx_account_id
                                );
                                Vec::new()
                            }
                        }
                    }
                    Err(err) => {
                        eprintln!(
                            "Schwab: transaction-history fetch unavailable for non-brokerage account {} (banking id {}): {err:#}",
                            schwab_account.account_id, bank_tx_account_id
                        );
                        Vec::new()
                    }
                };

                if !history_rows.is_empty() {
                    match parse_banking_transactions_rows(&account_id, &history_rows) {
                        Ok(parsed) => {
                            if !parsed.transactions.is_empty() {
                                transactions.push((account_id.clone(), parsed.transactions));
                            }
                        }
                        Err(err) => {
                            eprintln!(
                                "Schwab: failed to parse banking transaction-history rows for account {}: {err:#}",
                                schwab_account.account_id
                            );
                        }
                    }
                }
            }

            accounts.push(account);
            balances.push((account_id, account_balances));
        }

        // Update connection state
        connection.state.account_ids = accounts.iter().map(|a| a.id.clone()).collect();
        connection.state.last_sync = Some(LastSync {
            at: Utc::now(),
            status: SyncStatus::Success,
            error: None,
        });
        connection.state.status = ConnectionStatus::Active;

        Ok(SyncResult {
            connection: connection.clone(),
            accounts,
            balances,
            transactions,
        })
    }
}

#[async_trait::async_trait]
impl Synchronizer for SchwabSynchronizer {
    fn name(&self) -> &str {
        "schwab"
    }

    async fn sync(&self, connection: &mut Connection, storage: &dyn Storage) -> Result<SyncResult> {
        self.sync_internal(connection, storage, &SyncOptions::default())
            .await
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

impl SchwabSynchronizer {
    /// Sync with storage access for looking up existing accounts.
    pub async fn sync_with_storage<S: Storage + ?Sized>(
        &self,
        connection: &mut Connection,
        storage: &S,
    ) -> Result<SyncResult> {
        self.sync_internal(connection, storage, &SyncOptions::default())
            .await
    }
}

#[async_trait::async_trait]
impl InteractiveAuth for SchwabSynchronizer {
    async fn check_auth(&self) -> Result<AuthStatus> {
        match self.get_session()? {
            None => Ok(AuthStatus::Missing),
            Some(session) => {
                // Check if session has a token
                if session.token.is_none() {
                    return Ok(AuthStatus::Missing);
                }

                // Check age - sessions older than 24 hours are likely expired
                if let Some(captured_at) = session.captured_at {
                    let age_secs = Utc::now().timestamp() - captured_at;
                    if age_secs > 24 * 60 * 60 {
                        return Ok(AuthStatus::Expired {
                            reason: format!("Session is {} hours old", age_secs / 3600),
                        });
                    }
                }

                // Try a simple API call to verify
                let client = SchwabClient::new(session)?;
                match client.get_accounts().await {
                    Ok(_) => Ok(AuthStatus::Valid),
                    Err(e) => Ok(AuthStatus::Expired {
                        reason: e.to_string(),
                    }),
                }
            }
        }
    }

    async fn login(&mut self) -> Result<()> {
        // Check for Chrome
        let chrome_path = find_chrome().context(
            "Chrome/Chromium not found. Please install Chrome or Chromium to use the login command.",
        )?;

        // Configure browser with anti-detection flags
        let config = BrowserConfig::builder()
            .chrome_executable(chrome_path)
            .with_head() // Show the browser window
            .viewport(None) // Use default viewport
            .arg("--disable-blink-features=AutomationControlled")
            .arg("--disable-infobars")
            .arg("--no-first-run")
            .arg("--no-default-browser-check")
            .build()
            .map_err(|e| anyhow::anyhow!("Failed to configure browser: {e}"))?;

        let (browser, mut handler) = Browser::launch(config)
            .await
            .context("Failed to launch browser")?;

        // Spawn the handler task
        let handler_task = tokio::spawn(async move { while (handler.next().await).is_some() {} });

        // Create a new page
        let page = browser.new_page("about:blank").await?;

        install_schwab_token_capture(&page).await?;

        // Navigate to Schwab login
        println!("Navigating to Schwab login page...");
        page.goto(SCHWAB_LOGIN_URL).await?;

        if Self::should_autofill_login() {
            match self.get_login_credentials().await {
                Ok(Some((username, password))) => {
                    eprintln!(
                        "Schwab: attempting autofill (set KEEPBOOK_SCHWAB_AUTOFILL=0 to disable)..."
                    );
                    if let Err(err) = autofill_login_form(&page, &username, &password).await {
                        eprintln!(
                            "Schwab: autofill failed (continuing with manual login): {err:#}"
                        );
                    }
                }
                Ok(None) => {}
                Err(err) => {
                    eprintln!(
                        "Schwab: could not load credentials (continuing with manual login): {err:#}"
                    );
                }
            }
        }

        println!("\n========================================");
        println!("Complete the login process in the browser.");
        println!("Include any 2FA/security verification.");
        println!("Once logged in, navigate around the site");
        println!("(e.g., click on Accounts) to trigger API calls.");
        println!("========================================\n");

        // Wait for token capture
        println!("Waiting for session capture...");
        let timeout = Duration::from_secs(300); // 5 minute timeout
        let start = std::time::Instant::now();
        let mut last_post_login_drive = std::time::Instant::now() - Duration::from_secs(30);

        let (token, api_base) = loop {
            tokio::time::sleep(Duration::from_millis(500)).await;

            if let Some(auth) = extract_schwab_auth_capture(&page).await? {
                println!("\nCaptured bearer token!");
                break auth;
            }

            if last_post_login_drive.elapsed() >= Duration::from_secs(5) {
                last_post_login_drive = std::time::Instant::now();
                if let Err(err) = drive_schwab_post_login(&page).await {
                    eprintln!("Schwab: post-login browser drive failed (continuing): {err:#}");
                }
            }

            if start.elapsed() > timeout {
                anyhow::bail!("Timeout waiting for login. Please try again.");
            }
        };

        // Get all cookies
        println!("Capturing cookies...");
        let cookies = page.get_cookies().await?;

        let mut cookie_map = HashMap::new();
        for cookie in cookies {
            cookie_map.insert(cookie.name.clone(), cookie.value.clone());
        }

        println!("Captured {} cookies", cookie_map.len());

        // Build session data
        let session = SessionData {
            token: Some(token.clone()),
            cookies: cookie_map,
            cookie_jar: Vec::new(),
            captured_at: Some(Utc::now().timestamp()),
            data: api_base
                .map(|base| [("api_base".to_string(), base)].into())
                .unwrap_or_default(),
        };

        // Save to cache
        self.session_cache.set(&self.session_key(), &session)?;

        println!("\nSession saved successfully!");
        println!("Token: {}...", &token[..50.min(token.len())]);
        println!("Cookies: {} captured", session.cookies.len());

        // Clean up
        drop(browser);
        handler_task.abort();

        Ok(())
    }
}

const SCHWAB_TOKEN_CAPTURE_SCRIPT: &str = r#"(function() {
  if (window.__keepbookSchwabCaptureInstalled) return;
  window.__keepbookSchwabCaptureInstalled = true;
  window.__keepbookSchwabAuthCaptures = window.__keepbookSchwabAuthCaptures || [];

  function saveAuth(value, url) {
    const text = String(value || '');
    const match = text.match(/Bearer\s+([A-Za-z0-9._~+/=-]+)/i);
    if (!match) return;
    const token = match[1];
    const capture = { token, url: String(url || location.href || ''), at: Date.now() };
    window.__keepbookSchwabAuthCaptures.push(capture);
    if (window.__keepbookSchwabAuthCaptures.length > 20) {
      window.__keepbookSchwabAuthCaptures.shift();
    }
  }

  function inspectHeaders(headers, url) {
    if (!headers) return;
    try {
      if (typeof Headers !== 'undefined' && headers instanceof Headers) {
        for (const [key, value] of headers.entries()) {
          if (String(key).toLowerCase() === 'authorization') saveAuth(value, url);
        }
        return;
      }
    } catch (_) {}
    if (Array.isArray(headers)) {
      for (const pair of headers) {
        if (pair && String(pair[0]).toLowerCase() === 'authorization') saveAuth(pair[1], url);
      }
      return;
    }
    if (typeof headers === 'object') {
      for (const key of Object.keys(headers)) {
        if (String(key).toLowerCase() === 'authorization') saveAuth(headers[key], url);
      }
    }
  }

  try {
    const originalFetch = window.fetch;
    if (typeof originalFetch === 'function') {
      window.fetch = function(input, init) {
        try {
          const url = typeof input === 'string' ? input : (input && input.url);
          if (input && input.headers) inspectHeaders(input.headers, url);
          if (init && init.headers) inspectHeaders(init.headers, url);
        } catch (_) {}
        return originalFetch.apply(this, arguments);
      };
    }
  } catch (_) {}

  try {
    const originalOpen = XMLHttpRequest.prototype.open;
    const originalSetRequestHeader = XMLHttpRequest.prototype.setRequestHeader;
    XMLHttpRequest.prototype.open = function(method, url) {
      try { this.__keepbookSchwabUrl = String(url || ''); } catch (_) {}
      return originalOpen.apply(this, arguments);
    };
    XMLHttpRequest.prototype.setRequestHeader = function(name, value) {
      try {
        if (String(name).toLowerCase() === 'authorization') saveAuth(value, this.__keepbookSchwabUrl);
      } catch (_) {}
      return originalSetRequestHeader.apply(this, arguments);
    };
  } catch (_) {}
})()"#;

async fn install_schwab_token_capture(page: &chromiumoxide::Page) -> Result<()> {
    page.evaluate_on_new_document(SCHWAB_TOKEN_CAPTURE_SCRIPT)
        .await?;
    match page.evaluate(SCHWAB_TOKEN_CAPTURE_SCRIPT).await {
        Ok(_) => Ok(()),
        Err(err) => {
            let message = err.to_string();
            if message.contains("Cannot find context with specified id")
                || message.contains("Execution context was destroyed")
            {
                Ok(())
            } else {
                Err(err.into())
            }
        }
    }
}

async fn extract_schwab_auth_capture(
    page: &chromiumoxide::Page,
) -> Result<Option<(String, Option<String>)>> {
    let expr = r#"(function() {
  const captures = Array.isArray(window.__keepbookSchwabAuthCaptures)
    ? window.__keepbookSchwabAuthCaptures
    : [];
  const latest = captures[captures.length - 1];
  if (!latest || !latest.token) return null;
  let apiBase = null;
  try {
    const url = new URL(latest.url, location.href);
    if (/schwab\.com$/i.test(url.hostname)) {
      apiBase = url.origin + '/api/is.ClientSummaryExpWeb/V1/api';
    }
  } catch (_) {}
  return { token: latest.token, apiBase };
})()"#;

    let frames = page.frames().await.unwrap_or_default();
    for frame_id in frames {
        let Some(context_id) = page.frame_execution_context(frame_id).await? else {
            continue;
        };
        let mut params = EvaluateParams::from(expr);
        params.context_id = Some(context_id);
        params.return_by_value = Some(true);
        let value = match page.evaluate(params).await {
            Ok(result) => result.into_value::<Option<serde_json::Value>>()?,
            Err(err) => {
                let message = err.to_string();
                if message.contains("Cannot find context with specified id")
                    || message.contains("Execution context was destroyed")
                {
                    continue;
                }
                return Err(err.into());
            }
        };
        let Some(value) = value else {
            continue;
        };
        let Some(token) = value
            .get("token")
            .and_then(|v| v.as_str())
            .map(str::to_string)
        else {
            continue;
        };
        let api_base = value
            .get("apiBase")
            .and_then(|v| v.as_str())
            .map(str::to_string);
        return Ok(Some((token, api_base)));
    }

    Ok(None)
}

async fn drive_schwab_post_login(page: &chromiumoxide::Page) -> Result<()> {
    let js = r#"(async function() {
  const url = String(location.href || '');
  const text = String(document.body && document.body.innerText || '').toLowerCase();
  if (/security code|enter security code|confirm your identity|let's be sure it's you/.test(text)) {
    return { action: 'waiting-for-mfa' };
  }
  if (/login|signon|password|user id|username/.test(text) && /client\.schwab\.com\/login|signon/i.test(url)) {
    return { action: 'waiting-for-login' };
  }

  const accountHref = Array.from(document.querySelectorAll('a,button,[role="button"]'))
    .map((el) => ({
      el,
      text: String(el.innerText || el.textContent || el.getAttribute('aria-label') || '').toLowerCase(),
      href: String(el.href || el.getAttribute('href') || '')
    }))
    .find((item) => /accounts|positions|portfolio|summary/.test(item.text) || /accounts|positions|portfolio|summary/.test(item.href));
  if (accountHref) {
    try {
      accountHref.el.click();
      return { action: 'clicked-account-nav' };
    } catch (_) {}
  }

  if (!/client\.schwab\.com/i.test(url) || /sws-gateway/i.test(url)) {
    location.href = 'https://client.schwab.com/clientapps/accounts/summary/';
    return { action: 'navigate-summary' };
  }

  const apiUrls = [
    'https://ausgateway.schwab.com/api/is.ClientSummaryExpWeb/V1/api/Account?includeCustomGroups=true',
    'https://ausgateway.schwab.com/api/is.ClientSummaryExpWeb/V1/api/AggregatedPositions'
  ];
  for (const apiUrl of apiUrls) {
    try {
      await fetch(apiUrl, {
        credentials: 'include',
        headers: {
          'accept': 'application/json',
          'schwab-client-channel': 'IO',
          'schwab-client-correlid': (crypto && crypto.randomUUID) ? crypto.randomUUID() : String(Date.now()),
          'schwab-env': 'PROD',
          'schwab-resource-version': '1'
        }
      });
      return { action: 'fetch-api', apiUrl };
    } catch (_) {}
  }
  return { action: 'none' };
})()"#;

    match page.evaluate(js).await {
        Ok(_) => Ok(()),
        Err(err) => {
            let message = err.to_string();
            if message.contains("Cannot find context with specified id")
                || message.contains("Execution context was destroyed")
            {
                Ok(())
            } else {
                Err(err.into())
            }
        }
    }
}

async fn autofill_login_form(
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
  function isVisible(el) {{
    if (!el || !el.getBoundingClientRect) return false;
    const r = el.getBoundingClientRect();
    return r.width > 20 && r.height > 12;
  }}
  function bySelectors(root, selectors) {{
    for (const sel of selectors) {{
      const el = root.querySelector(sel);
      if (el && isVisible(el)) return el;
    }}
    return null;
  }}
  function collectDocs(rootDoc) {{
    const out = [rootDoc];
    const seen = new Set([rootDoc]);
    const queue = [rootDoc];
    let hops = 0;
    while (queue.length && hops < 30) {{
      hops += 1;
      const doc = queue.shift();
      const frames = Array.from(doc.querySelectorAll('iframe,frame'));
      for (const fr of frames) {{
        let child = null;
        try {{
          child = fr.contentDocument || (fr.contentWindow && fr.contentWindow.document) || null;
        }} catch (_) {{
          child = null;
        }}
        if (child && !seen.has(child)) {{
          seen.add(child);
          out.push(child);
          queue.push(child);
        }}
      }}
    }}
    return out;
  }}

  const passSelectors = [
    'input[type="password"]',
    'input[name="password"]',
    'input[name*="pass" i]',
    'input[id*="password" i]',
    'input[autocomplete="current-password"]',
  ];
  const userSelectors = [
    'input[name="LoginId"]',
    'input[autocomplete="username"]',
    'input[id*="login" i]',
    'input[name*="user" i]',
    'input[name*="email" i]',
    'input[id*="user" i]',
    'input[type="email"]',
    'input[type="text"]',
  ];
  const submitSelectors = [
    'button[type="submit"]',
    'input[type="submit"]',
    'button[id*="sign" i]',
    'button[id*="submit" i]',
    'button[id*="login" i]',
    'button[name*="login" i]',
    'button[name*="submit" i]',
    'button[aria-label*="sign in" i]',
    'button[aria-label*="log in" i]',
    'a[role="button"][id*="login" i]',
  ];
  const docs = collectDocs(document);
  const frameNames = Array.from(document.querySelectorAll('iframe,frame'))
    .map(f => (f.getAttribute('name') || f.id || '').toString())
    .filter(Boolean)
    .slice(0, 12);

  let pass = null;
  let form = null;
  let user = null;
  let submit = null;
  let chosenDoc = null;

  for (const doc of docs) {{
    pass = bySelectors(doc, passSelectors);
    if (!pass) continue;
    form = pass.form || pass.closest('form') || doc;
    user = bySelectors(form, userSelectors) || bySelectors(doc, userSelectors);
    submit =
      (form.querySelector && bySelectors(form, submitSelectors)) || bySelectors(doc, submitSelectors);
    if (pass && user && submit) {{
      chosenDoc = doc;
      break;
    }}
  }}

  if (!pass) return {{ ok: false, error: "password input not found", frameNames }};
  if (!user) return {{ ok: false, error: "username input not found", frameNames }};
  if (!submit) return {{ ok: false, error: "submit control not found", frameNames }};

  try {{ user.focus(); }} catch (_) {{}}
  user.value = String(creds.username || "");
  fire(user, "input");
  fire(user, "change");
  fire(user, "blur");

  try {{ pass.focus(); }} catch (_) {{}}
  pass.value = String(creds.password || "");
  fire(pass, "input");
  fire(pass, "change");
  fire(pass, "blur");

  let submittedBy = null;
  const actualForm = submit.form || form;
  if (actualForm && typeof actualForm.requestSubmit === 'function') {{
    try {{ actualForm.requestSubmit(submit); submittedBy = 'requestSubmit'; }} catch (_) {{}}
  }}
  if (!submittedBy && actualForm && typeof actualForm.submit === 'function') {{
    try {{ actualForm.submit(); submittedBy = 'form.submit'; }} catch (_) {{}}
  }}
  if (!submittedBy) {{
    try {{ submit.focus(); }} catch (_) {{}}
    try {{ submit.click(); submittedBy = 'submit.click'; }} catch (_) {{}}
  }}
  if (!submittedBy) {{
    try {{
      const win = (chosenDoc && chosenDoc.defaultView) || window;
      pass.focus();
      pass.dispatchEvent(new win.KeyboardEvent('keydown', {{ key: 'Enter', code: 'Enter', keyCode: 13, which: 13, bubbles: true, cancelable: true }}));
      pass.dispatchEvent(new win.KeyboardEvent('keyup', {{ key: 'Enter', code: 'Enter', keyCode: 13, which: 13, bubbles: true, cancelable: true }}));
      submittedBy = 'enter';
    }} catch (_) {{}}
  }}

  if (!submittedBy) return {{ ok: false, error: "submit failed", frameNames }};
  return {{ ok: true, submittedBy, frameNames }};
}})({creds})"#
    );

    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    let mut attempted_iframe_navigation = false;
    loop {
        let v: serde_json::Value = page.evaluate(js.clone()).await?.into_value()?;
        let ok = v.get("ok").and_then(|x| x.as_bool()).unwrap_or(false);
        if ok {
            if let Some(by) = v.get("submittedBy").and_then(|x| x.as_str()) {
                eprintln!("Schwab: submitted login ({by})");
            }
            return Ok(());
        }

        let err = v
            .get("error")
            .and_then(|x| x.as_str())
            .unwrap_or("unknown error");

        if err == "password input not found" && !attempted_iframe_navigation {
            attempted_iframe_navigation = true;
            if let Some(src) = extract_login_iframe_src(page).await? {
                eprintln!("Schwab: trying iframe-url fallback for autofill...");
                if let Err(nav_err) = page.goto(src).await {
                    eprintln!("Schwab: iframe-url fallback navigation failed: {nav_err:#}");
                } else {
                    tokio::time::sleep(Duration::from_millis(500)).await;
                    continue;
                }
            }
        }

        let retryable = matches!(
            err,
            "password input not found" | "username input not found" | "submit control not found"
        );
        if retryable && std::time::Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(250)).await;
            continue;
        }

        let frame_names = v
            .get("frameNames")
            .and_then(|x| x.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .take(12)
                    .collect::<Vec<_>>()
                    .join(",")
            })
            .unwrap_or_default();

        anyhow::bail!("Schwab autofill JS failed: {err} (frames={frame_names})");
    }
}

async fn extract_login_iframe_src(page: &chromiumoxide::Page) -> Result<Option<String>> {
    let js = r#"(function() {
  const selectors = [
    'iframe[name="lmsIframe"]',
    'iframe#lmsIframe',
    'iframe[id*="lms" i]',
    'iframe[name*="lms" i]',
    'iframe[name*="login" i]',
  ];
  for (const sel of selectors) {
    const iframe = document.querySelector(sel);
    if (!iframe) continue;
    const src = (iframe.getAttribute('src') || iframe.src || '').trim();
    if (src) return src;
  }
  return null;
})()"#;

    let src: Option<String> = page.evaluate(js).await?.into_value()?;
    Ok(src.filter(|s| !s.trim().is_empty()))
}

/// Find Chrome/Chromium executable.
fn find_chrome() -> Option<String> {
    // First try using `which` to find chrome in PATH
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

    // Fall back to known paths
    let candidates = [
        "/usr/bin/google-chrome",
        "/usr/bin/google-chrome-stable",
        "/usr/bin/chromium",
        "/usr/bin/chromium-browser",
        "/snap/bin/chromium",
        // NixOS
        "/run/current-system/sw/bin/google-chrome",
        "/run/current-system/sw/bin/chromium",
        // macOS
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
