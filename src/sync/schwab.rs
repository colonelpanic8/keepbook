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
use chrono::{Datelike, NaiveDate, TimeZone, Utc};
use reqwest::Client;
use serde::Deserialize;

use crate::credentials::SessionData;
use crate::models::{Asset, Id, Transaction, TransactionStatus};

/// Schwab API client using raw HTTP requests.
pub struct SchwabClient {
    client: Client,
    session: SessionData,
    api_base: String,
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
    const API_BASE_KEY: &'static str = "api_base";

    /// Create a new Schwab client with session data.
    pub fn new(session: SessionData) -> Result<Self> {
        let client = Client::builder()
            .user_agent("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36")
            .build()
            .context("Failed to create HTTP client")?;

        let api_base = session
            .data
            .get(Self::API_BASE_KEY)
            .cloned()
            .unwrap_or_else(|| Self::API_BASE.to_string());

        Ok(Self {
            client,
            session,
            api_base,
        })
    }

    /// Make an authenticated request to the Schwab API.
    async fn request<T: for<'de> Deserialize<'de>>(&self, path: &str) -> Result<T> {
        let base = self.api_base.trim_end_matches('/');
        let url = format!("{base}{path}");

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

    let token = exported
        .token
        .strip_prefix("Bearer ")
        .unwrap_or(&exported.token)
        .to_string();

    Ok(SessionData {
        token: Some(token),
        cookies: exported.cookies,
        cookie_jar: Vec::new(),
        captured_at: Some(chrono::Utc::now().timestamp()),
        data: HashMap::new(),
    })
}

#[derive(Debug)]
pub struct SchwabTransactionsImportResult {
    pub transactions: Vec<Transaction>,
    pub skipped: usize,
}

fn json_value_to_string(v: &serde_json::Value) -> Option<String> {
    match v {
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Number(n) => Some(n.to_string()),
        serde_json::Value::Bool(b) => Some(b.to_string()),
        serde_json::Value::Null => None,
        other => Some(other.to_string()),
    }
}

fn normalize_ws(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn normalize_amount(raw: &str) -> Option<String> {
    let mut s = raw.trim().to_string();
    if s.is_empty() {
        return None;
    }

    // Common Schwab export formats:
    // - "$1,234.56"
    // - "-$1,234.56"
    // - "($1,234.56)" (parentheses = negative)
    let mut negative = false;
    if s.starts_with('(') && s.ends_with(')') && s.len() >= 2 {
        negative = true;
        s = s[1..s.len() - 1].to_string();
    }

    s = s.trim().replace('$', "").replace(',', "");

    // Capture explicit leading sign after stripping formatting.
    if let Some(rest) = s.strip_prefix('-') {
        negative = true;
        s = rest.to_string();
    } else if let Some(rest) = s.strip_prefix('+') {
        s = rest.to_string();
    }

    let s = s.trim();
    if s.is_empty() {
        return None;
    }

    let out = if negative {
        format!("-{s}")
    } else {
        s.to_string()
    };
    Some(out)
}

fn extract_mmddyyyy_dates(raw: &str) -> Vec<NaiveDate> {
    // Schwab export sometimes uses: "05/20/2024 as of 05/17/2024"
    // We scan for any MM/DD/YYYY substrings and parse them.
    let s = raw;
    let mut out = Vec::new();
    if s.len() < 10 {
        return out;
    }
    for i in 0..=s.len() - 10 {
        let sub = &s[i..i + 10];
        let bytes = sub.as_bytes();
        if bytes.len() != 10 {
            continue;
        }
        if bytes[2] != b'/' || bytes[5] != b'/' {
            continue;
        }
        if let Ok(d) = NaiveDate::parse_from_str(sub, "%m/%d/%Y") {
            if out.last().copied() != Some(d) {
                out.push(d);
            }
        }
    }
    out
}

/// Parse Schwab's "transaction history" JSON export into keepbook transactions.
///
/// This is designed to work with the file you can download from Schwab as JSON
/// (history export), which appears as an array of objects with keys like:
/// "Date", "Action", "Symbol", "Description", "Quantity", "Price", "Fees & Comm", "Amount".
///
/// Notes:
/// - We only import rows with a parseable `Amount` field; rows without an amount are skipped.
/// - Export rows typically do not contain a stable transaction id; we generate deterministic IDs
///   from a fingerprint of the row contents, with a per-fingerprint occurrence counter.
pub fn parse_exported_transactions_json(
    account_id: &Id,
    json: &str,
) -> Result<SchwabTransactionsImportResult> {
    let value: serde_json::Value =
        serde_json::from_str(json).context("Failed to parse exported transaction JSON")?;

    let rows = value
        .as_array()
        .context("Expected Schwab export to be a JSON array")?;

    let mut skipped = 0usize;
    let mut txns: Vec<Transaction> = Vec::new();
    let mut seen_counts: std::collections::HashMap<String, u32> = std::collections::HashMap::new();

    for row in rows {
        let obj = match row.as_object() {
            Some(o) => o,
            None => {
                skipped += 1;
                continue;
            }
        };

        let date_raw = obj
            .get("Date")
            .and_then(json_value_to_string)
            .unwrap_or_default();
        let dates = extract_mmddyyyy_dates(&date_raw);
        let Some(primary_date) = dates.first().copied() else {
            skipped += 1;
            continue;
        };
        let as_of_date = dates.get(1).copied();

        let action = obj.get("Action").and_then(json_value_to_string);
        let symbol = obj.get("Symbol").and_then(json_value_to_string);
        let description = obj.get("Description").and_then(json_value_to_string);
        let quantity = obj.get("Quantity").and_then(json_value_to_string);
        let price = obj.get("Price").and_then(json_value_to_string);
        let fees_comm = obj.get("Fees & Comm").and_then(json_value_to_string);
        let amount_raw = obj.get("Amount").and_then(json_value_to_string);

        let Some(amount_norm) = amount_raw.as_deref().and_then(normalize_amount) else {
            skipped += 1;
            continue;
        };

        let mut parts: Vec<String> = Vec::new();
        if let Some(a) = action
            .as_deref()
            .map(|s| normalize_ws(s))
            .filter(|s| !s.is_empty())
        {
            parts.push(a);
        }
        if let Some(s) = symbol
            .as_deref()
            .map(|s| normalize_ws(s))
            .filter(|s| !s.is_empty())
        {
            parts.push(s);
        }
        if let Some(d) = description
            .as_deref()
            .map(|s| normalize_ws(s))
            .filter(|s| !s.is_empty())
        {
            parts.push(d);
        }
        let desc = if parts.is_empty() {
            "Schwab transaction".to_string()
        } else {
            parts.join(" ")
        };

        let date_iso = primary_date.format("%Y-%m-%d").to_string();
        let as_of_iso = as_of_date.map(|d| d.format("%Y-%m-%d").to_string());

        let fingerprint = format!(
            "date={date_iso}|asof={}|action={}|symbol={}|desc={}|qty={}|price={}|fees={}|amount={}",
            as_of_iso.clone().unwrap_or_default(),
            action.as_deref().map(normalize_ws).unwrap_or_default(),
            symbol.as_deref().map(normalize_ws).unwrap_or_default(),
            description.as_deref().map(normalize_ws).unwrap_or_default(),
            quantity.as_deref().map(normalize_ws).unwrap_or_default(),
            price.as_deref().map(normalize_ws).unwrap_or_default(),
            fees_comm.as_deref().map(normalize_ws).unwrap_or_default(),
            amount_norm,
        );
        let count = seen_counts.entry(fingerprint.clone()).or_insert(0);
        *count += 1;
        let occurrence = *count;

        let tx_id = Id::from_external(&format!(
            "schwab:export:{account}:{fingerprint}:{occurrence}",
            account = account_id.as_str()
        ));

        let timestamp = Utc
            .with_ymd_and_hms(
                primary_date.year(),
                primary_date.month(),
                primary_date.day(),
                0,
                0,
                0,
            )
            .single()
            .context("Failed to build transaction timestamp")?;

        let sync_data = serde_json::json!({
            "source": "schwab_export_json",
            "date_raw": date_raw,
            "date": date_iso,
            "as_of_date": as_of_iso,
            "action": action,
            "symbol": symbol,
            "description": description,
            "quantity": quantity,
            "price": price,
            "fees_comm": fees_comm,
            "amount_raw": amount_raw,
            "amount": amount_norm,
            "fingerprint": fingerprint,
            "occurrence": occurrence,
        });

        txns.push(
            Transaction::new(amount_norm, Asset::currency("USD"), desc)
                .with_timestamp(timestamp)
                .with_status(TransactionStatus::Posted)
                .with_id(tx_id)
                .with_synchronizer_data(sync_data),
        );
    }

    Ok(SchwabTransactionsImportResult {
        transactions: txns,
        skipped,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Id;

    #[test]
    fn parse_exported_session_strips_bearer_prefix() {
        let json = r#"{"token":"Bearer test-token","cookies":{}}"#;
        let session = parse_exported_session(json).expect("parse session");
        assert_eq!(session.token.as_deref(), Some("test-token"));
    }

    #[test]
    fn parse_exported_transactions_json_parses_rows_and_generates_deterministic_ids() {
        let json = r#"
[
  {
    "Date": "10/11/2024",
    "Action": "Exchange or Exercise",
    "Symbol": "XPOA",
    "Description": "XPOA CBOE PUT NOV 24 7.5",
    "Quantity": "2",
    "Price": "0",
    "Fees & Comm": "0",
    "Amount": "0"
  },
  {
    "Date": "05/20/2024 as of 05/17/2024",
    "Action": "Dividend",
    "Symbol": "VTI",
    "Description": "VANGUARD TOTAL STOCK MARKET ETF",
    "Amount": "$1.23"
  }
]
"#;

        let account_id = Id::from_string("acct-1");
        let first = parse_exported_transactions_json(&account_id, json).expect("parse");
        assert_eq!(first.skipped, 0);
        assert_eq!(first.transactions.len(), 2);

        let second = parse_exported_transactions_json(&account_id, json).expect("parse");
        assert_eq!(first.transactions[0].id, second.transactions[0].id);
        assert_eq!(first.transactions[1].id, second.transactions[1].id);
        assert_eq!(first.transactions[1].amount, "1.23");
    }
}
