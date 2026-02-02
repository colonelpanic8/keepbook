//! Schwab synchronizer with browser-based authentication.
//!
//! This synchronizer uses Chrome DevTools Protocol for automated session capture
//! and Schwab's internal APIs for data fetching.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use chrono::Utc;
use chromiumoxide::browser::{Browser, BrowserConfig};
use chromiumoxide::cdp::browser_protocol::fetch::{
    self, EventRequestPaused, RequestPattern, RequestStage,
};
use futures::StreamExt;
use tokio::sync::Mutex;

use crate::credentials::{SessionCache, SessionData};
use crate::market_data::{AssetId, PriceKind, PricePoint};
use crate::models::{
    Account, Asset, Balance, Connection, ConnectionStatus, Id, LastSync, SyncStatus,
};
use crate::storage::Storage;
use crate::sync::schwab::{SchwabClient, Position};
use crate::sync::{AuthStatus, InteractiveAuth, SyncResult, SyncedBalance, Synchronizer};

const SCHWAB_LOGIN_URL: &str = "https://client.schwab.com/Login/SignOn/CustomerCenterLogin.aspx";
const SCHWAB_API_DOMAIN: &str = "ausgateway.schwab.com";

/// Schwab synchronizer with browser-based authentication.
pub struct SchwabSynchronizer {
    connection_id: Id,
    session_cache: SessionCache,
}

impl SchwabSynchronizer {
    /// Create a new Schwab synchronizer for a connection.
    pub async fn from_connection<S: Storage>(connection: &Connection, _storage: &S) -> Result<Self> {
        let session_cache = SessionCache::new()?;

        Ok(Self {
            connection_id: connection.id().clone(),
            session_cache,
        })
    }

    fn session_key(&self) -> String {
        self.connection_id.to_string()
    }

    fn get_session(&self) -> Result<Option<SessionData>> {
        self.session_cache.get(&self.session_key())
    }

    async fn sync_internal<S: Storage>(
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
        let mut balances: Vec<(Id, Vec<SyncedBalance>)> = Vec::new();

        for schwab_account in accounts_resp.accounts {
            // Use Schwab's account_id to generate a stable, filesystem-safe ID
            let account_id = Id::from_external(&schwab_account.account_id);

            // Preserve created_at from existing account if it exists
            let created_at = existing_by_id
                .get(&schwab_account.account_id)
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

            let mut account_balances = vec![];

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
                    let balance = Balance::new(asset.clone(), position.quantity.to_string());

                    let price_point = PricePoint {
                        asset_id: AssetId::from_asset(&asset),
                        as_of_date: Utc::now().date_naive(),
                        timestamp: Utc::now(),
                        price: position.price.to_string(),
                        quote_currency: "USD".to_string(),
                        kind: PriceKind::Close,
                        source: "schwab".to_string(),
                    };
                    account_balances.push(SyncedBalance::new(balance).with_price(price_point));
                }

                // Add actual cash balance from account balances (not from CASH position)
                if let Some(bal) = &schwab_account.balances {
                    if let Some(cash) = bal.cash {
                        if cash > 0.0 {
                            account_balances.push(SyncedBalance::new(Balance::new(
                                Asset::currency("USD"),
                                cash.to_string(),
                            )));
                        }
                    }
                }
            } else if let Some(bal) = &schwab_account.balances {
                // Non-brokerage accounts (bank/checking): store total balance as USD
                account_balances.push(SyncedBalance::new(Balance::new(
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

    async fn sync(&self, _connection: &mut Connection) -> Result<SyncResult> {
        anyhow::bail!(
            "SchwabSynchronizer::sync requires storage access. Use sync_with_storage instead."
        )
    }
}

impl SchwabSynchronizer {
    /// Sync with storage access for looking up existing accounts.
    pub async fn sync_with_storage<S: Storage>(
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
        let handler_task = tokio::spawn(async move {
            while let Some(_) = handler.next().await {}
        });

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

    if let Ok(output) = std::process::Command::new("which")
        .arg("chromium")
        .output()
    {
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
