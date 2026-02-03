//! Schwab synchronizer using raw HTTP requests.
//!
//! This synchronizer uses session data (bearer token + cookies) captured from
//! a browser session to make API requests to Schwab's internal APIs.
//!
//! # Session Capture
//!
//! Run this in the browser console after logging into Schwab:
//!
//! ```javascript
//! // After logging into client.schwab.com, run in console:
//! (async () => {
//!   // Make a request to capture the auth token
//!   const origFetch = window.fetch;
//!   let token = null;
//!   window.fetch = async (...args) => {
//!     const res = await origFetch(...args);
//!     return res;
//!   };
//!
//!   // Get cookies
//!   const cookies = {};
//!   document.cookie.split(';').forEach(c => {
//!     const [k, v] = c.trim().split('=');
//!     if (k) cookies[k] = v;
//!   });
//!
//!   // Make a request and intercept the token
//!   const resp = await fetch('/api/auth/authorize/scope/api');
//!
//!   console.log(JSON.stringify({
//!     cookies: cookies,
//!     // Token would need to be extracted from Angular's auth service
//!   }, null, 2));
//! })();
//! ```

use std::collections::HashMap;

use anyhow::{Context, Result};
use reqwest::Client;
use serde::Deserialize;

use crate::credentials::SessionData;

/// Schwab API client using raw HTTP requests.
pub struct SchwabClient {
    client: Client,
    session: SessionData,
}

/// Account summary from Schwab API.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct AccountsResponse {
    pub accounts: Vec<SchwabAccount>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct SchwabAccount {
    pub account_id: String,
    pub account_number_display: String,
    pub account_number_display_full: String,
    pub default_name: String,
    pub nick_name: String,
    pub account_type: String,
    pub is_brokerage: bool,
    pub is_bank: bool,
    pub balances: Option<AccountBalances>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct AccountBalances {
    pub balance: f64,
    pub day_change: f64,
    pub day_change_pct: f64,
    pub cash: Option<f64>,
    pub market_value: Option<f64>,
}

/// Positions response from Schwab API.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct PositionsResponse {
    pub security_groupings: Vec<SecurityGrouping>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct SecurityGrouping {
    pub group_name: String,
    pub positions: Vec<Position>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Position {
    pub default_symbol: String,
    pub description: String,
    pub quantity: f64,
    pub price: f64,
    pub market_value: f64,
    pub cost: f64,
    pub profit_loss: f64,
    pub profit_loss_percent: f64,
    pub day_change: f64,
    pub percent_day_change: f64,
}

impl SchwabClient {
    const API_BASE: &'static str =
        "https://ausgateway.schwab.com/api/is.ClientSummaryExpWeb/V1/api";

    /// Create a new Schwab client with session data.
    pub fn new(session: SessionData) -> Result<Self> {
        let client = Client::builder()
            .user_agent("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36")
            .build()
            .context("Failed to create HTTP client")?;

        Ok(Self { client, session })
    }

    /// Make an authenticated request to the Schwab API.
    async fn request<T: for<'de> Deserialize<'de>>(&self, path: &str) -> Result<T> {
        let url = format!("{}{}", Self::API_BASE, path);

        let token = self
            .session
            .token
            .as_ref()
            .context("No bearer token in session")?;

        let mut req = self
            .client
            .get(&url)
            .header("authorization", format!("Bearer {token}"))
            .header("schwab-client-channel", "IO")
            .header("schwab-client-correlid", uuid::Uuid::new_v4().to_string())
            .header("schwab-env", "PROD")
            .header("schwab-resource-version", "1")
            .header("origin", "https://client.schwab.com")
            .header("referer", "https://client.schwab.com/")
            .header("accept", "application/json");

        // Add cookies if present
        if !self.session.cookies.is_empty() {
            req = req.header("cookie", self.session.cookie_header());
        }

        let response = req.send().await.context("HTTP request failed")?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("API request failed ({status}): {body}");
        }

        let body = response.text().await.context("Failed to read response")?;
        serde_json::from_str(&body).context("Failed to parse JSON response")
    }

    /// Get account information.
    pub async fn get_accounts(&self) -> Result<AccountsResponse> {
        self.request("/Account?includeCustomGroups=true").await
    }

    /// Get all positions.
    pub async fn get_positions(&self) -> Result<PositionsResponse> {
        self.request("/AggregatedPositions").await
    }
}

/// Helper to parse session data exported from browser.
///
/// Expected JSON format:
/// ```json
/// {
///   "token": "Bearer I0.xxx...",
///   "cookies": {
///     "bm_sz": "...",
///     "_abck": "...",
///     ...
///   }
/// }
/// ```
pub fn parse_exported_session(json: &str) -> Result<SessionData> {
    #[derive(Deserialize)]
    struct ExportedSession {
        token: String,
        #[serde(default)]
        cookies: HashMap<String, String>,
    }

    let exported: ExportedSession =
        serde_json::from_str(json).context("Failed to parse exported session JSON")?;

    Ok(SessionData {
        token: Some(exported.token),
        cookies: exported.cookies,
        captured_at: Some(chrono::Utc::now().timestamp()),
        data: HashMap::new(),
    })
}
