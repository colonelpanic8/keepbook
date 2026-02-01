//! Schwab synchronizer - Proof of Concept
//!
//! This example demonstrates syncing from Schwab using their internal APIs.
//! Uses Chrome DevTools Protocol for automated session capture.
//!
//! Run with:
//!   cargo run --example schwab -- login   # Opens Chrome, captures session
//!   cargo run --example schwab -- export  # Export session to stdout
//!   cargo run --example schwab -- import  # Import session from stdin
//!   cargo run --example schwab -- sync    # Sync using stored session

use std::io::{self, Read};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use chrono::Utc;
use chromiumoxide::browser::{Browser, BrowserConfig};
use chromiumoxide::cdp::browser_protocol::fetch::{
    self, EventRequestPaused, RequestPattern, RequestStage,
};
use futures::StreamExt;
use keepbook::credentials::{SessionCache, SessionData};
use keepbook::market_data::{AssetId, PriceKind, PricePoint};
use keepbook::models::{
    Account, Asset, Balance, Connection, ConnectionConfig, ConnectionStatus, Id, LastSync, SyncStatus,
};
use keepbook::storage::{JsonFileStorage, Storage};
use keepbook::sync::schwab::{SchwabClient, Position};
use keepbook::sync::{SyncResult, SyncedBalance};
use tokio::sync::Mutex;

const CONNECTION_ID: &str = "schwab";
const SCHWAB_LOGIN_URL: &str = "https://client.schwab.com/Login/SignOn/CustomerCenterLogin.aspx";
const SCHWAB_API_DOMAIN: &str = "ausgateway.schwab.com";

/// Session data captured from browser, ready for export.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct ExportableSession {
    /// When this session was captured
    captured_at: chrono::DateTime<Utc>,
    /// Bearer token (without "Bearer " prefix)
    token: String,
    /// All cookies from the session
    cookies: std::collections::HashMap<String, String>,
}

impl From<ExportableSession> for SessionData {
    fn from(exp: ExportableSession) -> Self {
        SessionData {
            token: Some(exp.token),
            cookies: exp.cookies,
            captured_at: Some(exp.captured_at.timestamp()),
            data: std::collections::HashMap::new(),
        }
    }
}

impl TryFrom<&SessionData> for ExportableSession {
    type Error = anyhow::Error;

    fn try_from(session: &SessionData) -> Result<Self> {
        let token = session
            .token
            .clone()
            .context("Session has no token")?;
        let captured_at = session
            .captured_at
            .and_then(|ts| chrono::DateTime::from_timestamp(ts, 0))
            .unwrap_or_else(Utc::now);

        Ok(ExportableSession {
            captured_at,
            token,
            cookies: session.cookies.clone(),
        })
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

/// Login command: Opens Chrome, guides user through login, captures session.
async fn login() -> Result<()> {
    // Check for Chrome
    let chrome_path = find_chrome().context(
        "Chrome/Chromium not found. Please install Chrome or Chromium to use the login command.",
    )?;
    println!("Using browser: {}", chrome_path);

    // Configure browser with anti-detection flags
    let config = BrowserConfig::builder()
        .chrome_executable(chrome_path)
        .with_head() // Show the browser window
        .viewport(None) // Use default viewport
        // Anti-automation detection flags
        .arg("--disable-blink-features=AutomationControlled")
        .arg("--disable-infobars")
        .arg("--no-first-run")
        .arg("--no-default-browser-check")
        .build()
        .map_err(|e| anyhow::anyhow!("Failed to configure browser: {}", e))?;

    println!("Launching browser...");
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
        url_pattern: Some(format!("*{}*", SCHWAB_API_DOMAIN)),
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
                // Try both lowercase and capitalized versions
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

    let mut cookie_map = std::collections::HashMap::new();
    for cookie in cookies {
        cookie_map.insert(cookie.name.clone(), cookie.value.clone());
    }

    println!("Captured {} cookies", cookie_map.len());

    // Build session data
    let session = SessionData {
        token: Some(token.clone()),
        cookies: cookie_map,
        captured_at: Some(Utc::now().timestamp()),
        data: std::collections::HashMap::new(),
    };

    // Save to cache
    let cache = SessionCache::new()?;
    cache.set(CONNECTION_ID, &session)?;

    println!("\nSession saved successfully!");
    println!(
        "Token: {}...",
        &token[..50.min(token.len())]
    );
    println!("Cookies: {} captured", session.cookies.len());

    // Clean up
    intercept_task.abort();
    drop(browser);
    handler_task.abort();

    println!("\nRun 'cargo run --example schwab -- sync' to fetch positions.");
    println!("Run 'cargo run --example schwab -- export' to export session for remote use.");

    Ok(())
}

/// Export command: Dump session to stdout as JSON.
fn export_session() -> Result<()> {
    let cache = SessionCache::new()?;
    let session = cache
        .get(CONNECTION_ID)?
        .context("No session found. Run 'cargo run --example schwab -- login' first.")?;

    let exportable = ExportableSession::try_from(&session)?;
    let json = serde_json::to_string_pretty(&exportable)?;

    // Write to stdout
    println!("{}", json);

    // Also print info to stderr so it doesn't pollute the JSON
    eprintln!("\n# Session exported. Pipe to a file or remote host:");
    eprintln!("#   cargo run --example schwab -- export > session.json");
    eprintln!("#   cargo run --example schwab -- export | ssh remote 'schwab import'");

    Ok(())
}

/// Import command: Read session from stdin.
fn import_session() -> Result<()> {
    eprintln!("Reading session JSON from stdin...");
    eprintln!("(Paste JSON, then press Ctrl+D)\n");

    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;

    // Parse as ExportableSession
    let exportable: ExportableSession = serde_json::from_str(&input)
        .context("Failed to parse session JSON. Expected format: {\"captured_at\": ..., \"token\": ..., \"cookies\": {...}}")?;

    // Convert to SessionData
    let session: SessionData = exportable.into();

    // Save to cache
    let cache = SessionCache::new()?;
    cache.set(CONNECTION_ID, &session)?;

    eprintln!("\nSession imported successfully!");
    eprintln!(
        "Token: {}...",
        &session.token.as_ref().unwrap()[..50.min(session.token.as_ref().unwrap().len())]
    );
    eprintln!("Cookies: {} imported", session.cookies.len());
    eprintln!("\nRun 'cargo run --example schwab -- sync' to fetch positions.");

    Ok(())
}

async fn sync(storage: &JsonFileStorage) -> Result<()> {
    // Load session
    let cache = SessionCache::new()?;
    let session = cache
        .get(CONNECTION_ID)?
        .context("No session found. Run 'cargo run --example schwab -- login' first.")?;

    println!(
        "Using cached session from {:?}",
        session
            .captured_at
            .map(|t| chrono::DateTime::from_timestamp(t, 0))
    );

    // Get or create connection
    let connections = storage.list_connections().await?;
    let mut connection = connections
        .into_iter()
        .find(|c| c.synchronizer() == "schwab")
        .unwrap_or_else(|| {
            Connection::new(ConnectionConfig {
                name: "Charles Schwab".to_string(),
                synchronizer: "schwab".to_string(),
                credentials: None,
            })
        });

    println!("Connection: {} ({})\n", connection.name(), connection.id());

    // Create client and fetch data
    let client = SchwabClient::new(session)?;

    println!("Fetching accounts...");
    let accounts_resp = client.get_accounts().await?;
    println!("Found {} accounts\n", accounts_resp.accounts.len());

    println!("Fetching positions...");
    let positions_resp = client.get_positions().await?;

    // Collect all positions into a flat list
    let all_positions: Vec<Position> = positions_resp
        .security_groupings
        .into_iter()
        .flat_map(|g| g.positions)
        .collect();

    println!("Found {} positions\n", all_positions.len());

    // Build sync result
    let mut accounts = Vec::new();
    let mut balances: Vec<(Id, Vec<SyncedBalance>)> = Vec::new();

    for schwab_account in accounts_resp.accounts {
        let account_id = Id::new();

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
            created_at: Utc::now(),
            active: true,
            synchronizer_data: serde_json::json!({
                "schwab_account_id": schwab_account.account_id,
                "account_number": schwab_account.account_number_display_full,
            }),
        };

        // Create balance for total account value
        let mut account_balances = vec![];
        if let Some(bal) = &schwab_account.balances {
            account_balances.push(SyncedBalance::new(Balance::new(
                Asset::currency("USD"),
                bal.balance.to_string(),
            )));
        }

        // Add position balances for brokerage accounts
        if schwab_account.is_brokerage {
            for position in &all_positions {
                let asset = if position.default_symbol == "CASH" {
                    Asset::currency("USD")
                } else {
                    Asset::equity(&position.default_symbol)
                };
                let balance = Balance::new(asset.clone(), position.quantity.to_string());

                // Create price point for non-cash positions
                let synced_balance = if position.default_symbol != "CASH" {
                    let price_point = PricePoint {
                        asset_id: AssetId::from_asset(&asset),
                        as_of_date: Utc::now().date_naive(),
                        timestamp: Utc::now(),
                        price: position.price.to_string(),
                        quote_currency: "USD".to_string(),
                        kind: PriceKind::Close,
                        source: "schwab".to_string(),
                    };
                    SyncedBalance::new(balance).with_price(price_point)
                } else {
                    SyncedBalance::new(balance)
                };
                account_balances.push(synced_balance);
            }
        }

        println!(
            "  {} ({}) - ${:.2}",
            account.name,
            schwab_account.account_number_display,
            schwab_account
                .balances
                .as_ref()
                .map(|b| b.balance)
                .unwrap_or(0.0)
        );

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

    let result = SyncResult {
        connection,
        accounts,
        balances,
        transactions: Vec::new(),
    };

    result.save(storage).await?;

    println!("\nSync complete!");
    println!("Saved {} accounts", result.accounts.len());

    Ok(())
}

fn print_usage() {
    println!("Usage:");
    println!("  cargo run --example schwab -- login   # Opens Chrome, captures session");
    println!("  cargo run --example schwab -- export  # Export session JSON to stdout");
    println!("  cargo run --example schwab -- import  # Import session JSON from stdin");
    println!("  cargo run --example schwab -- sync    # Sync using stored session");
}

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let command = args.get(1).map(|s| s.as_str()).unwrap_or("help");

    // Only print header for interactive commands (not export which outputs JSON)
    if command != "export" {
        println!("Keepbook - Schwab Sync");
        println!("======================\n");
    }

    let storage = JsonFileStorage::new("data");

    match command {
        "login" => login().await,
        "export" => export_session(),
        "import" => import_session(),
        "sync" => sync(&storage).await,
        "help" | "--help" | "-h" => {
            print_usage();
            Ok(())
        }
        other => {
            println!("Unknown command: {}\n", other);
            print_usage();
            Ok(())
        }
    }
}
