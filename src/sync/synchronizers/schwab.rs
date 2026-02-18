//! Schwab synchronizer with browser-based authentication.
//!
//! This synchronizer uses Chrome DevTools Protocol for automated session capture
//! and Schwab's internal APIs for data fetching.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use chromiumoxide::browser::{Browser, BrowserConfig};
use chromiumoxide::cdp::browser_protocol::fetch::{
    self, EventRequestPaused, RequestPattern, RequestStage,
};
use chrono::Utc;
use futures::StreamExt;
use secrecy::ExposeSecret;
use tokio::sync::Mutex;

use crate::credentials::{CredentialStore, SessionCache, SessionData};
use crate::market_data::{AssetId, PriceKind, PricePoint};
use crate::models::{
    Account, Asset, AssetBalance, Connection, ConnectionStatus, Id, LastSync, SyncStatus,
};
use crate::storage::Storage;
use crate::sync::schwab::{Position, SchwabClient};
use crate::sync::{AuthStatus, InteractiveAuth, SyncResult, SyncedAssetBalance, Synchronizer};

const SCHWAB_LOGIN_URL: &str = "https://client.schwab.com/Login/SignOn/CustomerCenterLogin.aspx";
const SCHWAB_API_DOMAIN: &str = "ausgateway.schwab.com";

/// Schwab synchronizer with browser-based authentication.
pub struct SchwabSynchronizer {
    connection_id: Id,
    session_cache: SessionCache,
    credential_store: Option<Box<dyn CredentialStore>>,
}

impl SchwabSynchronizer {
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

        // Collect all positions into a flat list
        let all_positions: Vec<Position> = positions_resp
            .security_groupings
            .into_iter()
            .flat_map(|g| g.positions)
            .collect();

        // Build sync result
        let mut accounts = Vec::new();
        let mut balances: Vec<(Id, Vec<SyncedAssetBalance>)> = Vec::new();

        for schwab_account in accounts_resp.accounts {
            // Use Schwab's account_id to generate a stable, filesystem-safe ID
            let account_id = Id::from_external(&schwab_account.account_id);

            // Preserve created_at from existing account if it exists
            let created_at = existing_by_id
                .get(&account_id.to_string())
                .map(|a| a.created_at)
                .unwrap_or_else(Utc::now);

            let account = Account {
                id: account_id.clone(),
                name: if schwab_account.nick_name.is_empty() {
                    schwab_account.default_name.clone()
                } else {
                    schwab_account.nick_name.clone()
                },
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
                        AssetBalance::new(asset.clone(), position.quantity.to_string());

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
            transactions: Vec::new(),
        })
    }
}

#[async_trait::async_trait]
impl Synchronizer for SchwabSynchronizer {
    fn name(&self) -> &str {
        "schwab"
    }

    async fn sync(&self, connection: &mut Connection, storage: &dyn Storage) -> Result<SyncResult> {
        self.sync_internal(connection, storage).await
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
        self.sync_internal(connection, storage).await
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

        // Set up request interception to capture the bearer token
        let captured_token: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
        let token_clone = captured_token.clone();

        // Enable fetch domain for request interception
        let patterns = vec![RequestPattern {
            url_pattern: Some(format!("*{SCHWAB_API_DOMAIN}*")),
            resource_type: None,
            request_stage: Some(RequestStage::Request),
        }];

        page.execute(fetch::EnableParams {
            patterns: Some(patterns),
            handle_auth_requests: None,
        })
        .await?;

        // Listen for paused requests
        let mut request_events = page.event_listener::<EventRequestPaused>().await?;

        let page_clone = page.clone();
        let intercept_task = tokio::spawn(async move {
            while let Some(event) = request_events.next().await {
                // Check for Authorization header in the request
                let headers = event.request.headers.inner();
                if let Some(headers_obj) = headers.as_object() {
                    let auth_value = headers_obj
                        .get("authorization")
                        .or_else(|| headers_obj.get("Authorization"));

                    if let Some(auth) = auth_value.and_then(|v| v.as_str()) {
                        if auth.starts_with("Bearer ") {
                            let token = auth.strip_prefix("Bearer ").unwrap().to_string();
                            println!("\nCaptured bearer token!");
                            let mut guard = token_clone.lock().await;
                            *guard = Some(token);
                        }
                    }
                }

                // Continue the request
                let _ = page_clone
                    .execute(fetch::ContinueRequestParams {
                        request_id: event.request_id.clone(),
                        url: None,
                        method: None,
                        post_data: None,
                        headers: None,
                        intercept_response: None,
                    })
                    .await;
            }
        });

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

        loop {
            tokio::time::sleep(Duration::from_millis(500)).await;

            let guard = captured_token.lock().await;
            if guard.is_some() {
                break;
            }
            drop(guard);

            if start.elapsed() > timeout {
                anyhow::bail!("Timeout waiting for login. Please try again.");
            }
        }

        // Get the token
        let token = captured_token.lock().await.clone().unwrap();

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
            data: HashMap::new(),
        };

        // Save to cache
        self.session_cache.set(&self.session_key(), &session)?;

        println!("\nSession saved successfully!");
        println!("Token: {}...", &token[..50.min(token.len())]);
        println!("Cookies: {} captured", session.cookies.len());

        // Clean up
        intercept_task.abort();
        drop(browser);
        handler_task.abort();

        Ok(())
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
}})({})"#,
        creds
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
