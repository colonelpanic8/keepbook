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
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionHistoryTimeFrame {
    Last6Months,
    All,
}

impl TransactionHistoryTimeFrame {
    fn as_api_value(self) -> &'static str {
        match self {
            Self::Last6Months => "Last6Months",
            Self::All => "All",
        }
    }
}

const TRANSACTION_HISTORY_MAX_PAGES: usize = 20;
const TRANSACTION_HISTORY_INIT_PATH: &str =
    "/api/is.TransactionHistoryWeb/TransactionHistoryInterface/TransactionHistory/init";
const TRANSACTION_HISTORY_PATH: &str =
    "/api/is.TransactionHistoryWeb/TransactionHistoryInterface/TransactionHistory/brokerage/transactions";
const TRANSACTION_TYPES: &[&str] = &[
    "Adjustments",
    "AtmActivity",
    "BillPay",
    "CorporateActions",
    "Checks",
    "Deposits",
    "DividendsAndCapitalGains",
    "ElectronicTransfers",
    "Fees",
    "Interest",
    "Misc",
    "SecurityTransfers",
    "Taxes",
    "Trades",
    "VisaDebitCard",
    "Withdrawals",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TransactionHistoryBookmarkKey {
    primary_sort_code: Option<String>,
    primary_sort_value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TransactionHistoryBookmark {
    from_key: TransactionHistoryBookmarkKey,
    from_execution_date: String,
    from_publ_time_stamp: String,
    from_secondary_sort_code: String,
    from_secondary_sort_value: String,
    from_tertiary_sort_value: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct BrokerageTransactionsRequest {
    account_nickname: String,
    export_type: &'static str,
    include_options_in_search: bool,
    is_sps_linked_uk_account: bool,
    selected_account_id: String,
    selected_transaction_types: Vec<&'static str>,
    sort_column: &'static str,
    sort_direction: &'static str,
    symbol: String,
    time_frame: &'static str,
    bookmark: Option<TransactionHistoryBookmark>,
    should_paginate: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BrokerageTransactionsResponse {
    #[serde(default)]
    pub brokerage_transactions: Vec<BrokerageTransactionRow>,
    pub bookmark: Option<TransactionHistoryBookmark>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrokerageTransactionRow {
    pub transaction_date: String,
    #[serde(default)]
    pub action: Option<String>,
    #[serde(default)]
    pub symbol: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub share_quantity: Option<String>,
    #[serde(default)]
    pub execution_price: Option<String>,
    #[serde(default)]
    pub fees_and_commission: Option<String>,
    #[serde(default)]
    pub amount: Option<String>,
    #[serde(default)]
    pub source_code: Option<String>,
    #[serde(default)]
    pub effective_date: Option<String>,
    #[serde(default)]
    pub deposit_sequence_id: Option<String>,
    #[serde(default)]
    pub check_date: Option<String>,
    #[serde(default)]
    pub item_issue_id: Option<String>,
    #[serde(default)]
    pub schwab_order_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TransactionHistoryInitResponse {
    #[serde(default)]
    account_selector_data: Option<TransactionHistoryAccountSelectorData>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TransactionHistoryAccountSelectorData {
    #[serde(default)]
    brokerage_account_list: Option<TransactionHistoryBrokerageAccountList>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TransactionHistoryBrokerageAccountList {
    #[serde(default)]
    brokerage_accounts: Vec<TransactionHistoryBrokerageAccount>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransactionHistoryBrokerageAccount {
    pub id: String,
    #[serde(default)]
    pub nick_name: String,
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

    fn api_origin(&self) -> String {
        let base = self.api_base.trim_end_matches('/');
        if let Some((origin, _)) = base.split_once("/api/") {
            origin.to_string()
        } else {
            base.to_string()
        }
    }

    fn resolve_url(&self, path: &str) -> String {
        if path.starts_with("http://") || path.starts_with("https://") {
            return path.to_string();
        }
        if path.starts_with("/api/") {
            return format!("{}{}", self.api_origin(), path);
        }
        format!("{}{}", self.api_base.trim_end_matches('/'), path)
    }

    fn base_request(&self, method: reqwest::Method, path: &str) -> Result<reqwest::RequestBuilder> {
        let token = self
            .session
            .token
            .as_ref()
            .context("No bearer token in session")?;

        let url = self.resolve_url(path);
        let mut req = self
            .client
            .request(method, &url)
            .header("authorization", format!("Bearer {token}"))
            .header("schwab-client-channel", "IO")
            .header("schwab-client-correlid", uuid::Uuid::new_v4().to_string())
            .header("schwab-env", "PROD")
            .header("schwab-resource-version", "1")
            .header("origin", "https://client.schwab.com")
            .header("referer", "https://client.schwab.com/")
            .header("accept", "application/json");

        if !self.session.cookies.is_empty() {
            req = req.header("cookie", self.session.cookie_header());
        }

        Ok(req)
    }

    async fn send_json<T: DeserializeOwned>(&self, req: reqwest::RequestBuilder) -> Result<T> {
        let response = req.send().await.context("HTTP request failed")?;
        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("API request failed ({status}): {body}");
        }
        let body = response.text().await.context("Failed to read response")?;
        serde_json::from_str(&body).context("Failed to parse JSON response")
    }

    /// Make an authenticated GET request to the Schwab API.
    async fn request<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let req = self.base_request(reqwest::Method::GET, path)?;
        self.send_json(req).await
    }

    /// Make an authenticated POST request with a JSON body.
    async fn request_with_body<T: DeserializeOwned, B: Serialize>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T> {
        let req = self
            .base_request(reqwest::Method::POST, path)?
            .header("content-type", "application/json")
            .json(body);
        self.send_json(req).await
    }

    /// Get account information.
    pub async fn get_accounts(&self) -> Result<AccountsResponse> {
        self.request("/Account?includeCustomGroups=true").await
    }

    /// Get all positions.
    pub async fn get_positions(&self) -> Result<PositionsResponse> {
        self.request("/AggregatedPositions").await
    }

    /// Get brokerage account metadata used by transaction-history APIs.
    pub async fn get_transaction_history_brokerage_accounts(
        &self,
    ) -> Result<Vec<TransactionHistoryBrokerageAccount>> {
        let init: TransactionHistoryInitResponse =
            self.request(TRANSACTION_HISTORY_INIT_PATH).await?;
        Ok(init
            .account_selector_data
            .and_then(|d| d.brokerage_account_list)
            .map(|b| b.brokerage_accounts)
            .unwrap_or_default())
    }

    /// Get all brokerage transactions for an account using pagination.
    pub async fn get_brokerage_transactions(
        &self,
        account_id: &str,
        account_nickname: &str,
        time_frame: TransactionHistoryTimeFrame,
    ) -> Result<Vec<BrokerageTransactionRow>> {
        let mut req = BrokerageTransactionsRequest {
            account_nickname: account_nickname.to_string(),
            export_type: "Csv",
            include_options_in_search: false,
            is_sps_linked_uk_account: false,
            selected_account_id: account_id.to_string(),
            selected_transaction_types: TRANSACTION_TYPES.to_vec(),
            sort_column: "Date",
            sort_direction: "Descending",
            symbol: String::new(),
            time_frame: time_frame.as_api_value(),
            bookmark: None,
            should_paginate: true,
        };

        let mut rows: Vec<BrokerageTransactionRow> = Vec::new();
        for _ in 0..TRANSACTION_HISTORY_MAX_PAGES {
            let page: BrokerageTransactionsResponse = self
                .request_with_body(TRANSACTION_HISTORY_PATH, &req)
                .await?;
            rows.extend(page.brokerage_transactions);

            if let Some(bookmark) = page.bookmark {
                req.bookmark = Some(bookmark);
            } else {
                return Ok(rows);
            }
        }

        anyhow::bail!(
            "Schwab transaction pagination exceeded {TRANSACTION_HISTORY_MAX_PAGES} pages"
        )
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

fn nonempty_opt(v: &Option<String>) -> Option<String> {
    v.as_ref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Parse Schwab brokerage transaction-history API rows into keepbook transactions.
pub fn parse_brokerage_transactions_rows(
    account_id: &Id,
    rows: &[BrokerageTransactionRow],
) -> Result<SchwabTransactionsImportResult> {
    let mut skipped = 0usize;
    let mut txns: Vec<Transaction> = Vec::new();
    let mut seen_counts: std::collections::HashMap<String, u32> = std::collections::HashMap::new();

    for row in rows {
        let date_raw = row.transaction_date.trim().to_string();
        let dates = extract_mmddyyyy_dates(&date_raw);
        let Some(primary_date) = dates.first().copied() else {
            skipped += 1;
            continue;
        };
        let as_of_date = dates.get(1).copied();

        let action = nonempty_opt(&row.action);
        let symbol = nonempty_opt(&row.symbol);
        let description = nonempty_opt(&row.description);
        let quantity = nonempty_opt(&row.share_quantity);
        let price = nonempty_opt(&row.execution_price);
        let fees_comm = nonempty_opt(&row.fees_and_commission);
        let amount_raw = nonempty_opt(&row.amount);

        let Some(amount_norm) = amount_raw.as_deref().and_then(normalize_amount) else {
            skipped += 1;
            continue;
        };

        let source_code = nonempty_opt(&row.source_code);
        let effective_date = nonempty_opt(&row.effective_date);
        let deposit_sequence_id = nonempty_opt(&row.deposit_sequence_id);
        let check_date = nonempty_opt(&row.check_date);
        let item_issue_id = nonempty_opt(&row.item_issue_id);
        let schwab_order_id = nonempty_opt(&row.schwab_order_id);

        let mut parts: Vec<String> = Vec::new();
        if let Some(a) = action
            .as_deref()
            .map(normalize_ws)
            .filter(|s| !s.is_empty())
        {
            parts.push(a);
        }
        if let Some(s) = symbol
            .as_deref()
            .map(normalize_ws)
            .filter(|s| !s.is_empty())
        {
            parts.push(s);
        }
        if let Some(d) = description
            .as_deref()
            .map(normalize_ws)
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
            "date={date_iso}|asof={}|action={}|symbol={}|desc={}|qty={}|price={}|fees={}|amount={}|source_code={}|effective_date={}|deposit_sequence_id={}|check_date={}|item_issue_id={}|schwab_order_id={}",
            as_of_iso.clone().unwrap_or_default(),
            action.as_deref().map(normalize_ws).unwrap_or_default(),
            symbol.as_deref().map(normalize_ws).unwrap_or_default(),
            description.as_deref().map(normalize_ws).unwrap_or_default(),
            quantity.as_deref().map(normalize_ws).unwrap_or_default(),
            price.as_deref().map(normalize_ws).unwrap_or_default(),
            fees_comm.as_deref().map(normalize_ws).unwrap_or_default(),
            amount_norm,
            source_code.as_deref().map(normalize_ws).unwrap_or_default(),
            effective_date.as_deref().map(normalize_ws).unwrap_or_default(),
            deposit_sequence_id
                .as_deref()
                .map(normalize_ws)
                .unwrap_or_default(),
            check_date.as_deref().map(normalize_ws).unwrap_or_default(),
            item_issue_id.as_deref().map(normalize_ws).unwrap_or_default(),
            schwab_order_id.as_deref().map(normalize_ws).unwrap_or_default(),
        );
        let count = seen_counts.entry(fingerprint.clone()).or_insert(0);
        *count += 1;
        let occurrence = *count;

        let tx_id = Id::from_external(&format!(
            "schwab:history:{account}:{fingerprint}:{occurrence}",
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
            "source": "schwab_transaction_history_api",
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
            "source_code": source_code,
            "effective_date": effective_date,
            "deposit_sequence_id": deposit_sequence_id,
            "check_date": check_date,
            "item_issue_id": item_issue_id,
            "schwab_order_id": schwab_order_id,
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
    use crate::credentials::SessionData;
    use crate::models::Id;
    use serde_json::json;
    use wiremock::matchers::{body_partial_json, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

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

    #[test]
    fn parse_brokerage_transactions_rows_parses_and_generates_deterministic_ids() {
        let rows = vec![
            BrokerageTransactionRow {
                transaction_date: "02/10/2026".to_string(),
                action: Some("Buy".to_string()),
                symbol: Some("ADBE".to_string()),
                description: Some("ADOBE INC".to_string()),
                share_quantity: Some("75".to_string()),
                execution_price: Some("$265.1199".to_string()),
                fees_and_commission: Some(String::new()),
                amount: Some("-$19,883.99".to_string()),
                source_code: Some(String::new()),
                effective_date: Some("02/10/2026".to_string()),
                deposit_sequence_id: Some("0".to_string()),
                check_date: Some("02/11/2026".to_string()),
                item_issue_id: Some("1670511108".to_string()),
                schwab_order_id: Some("719598531600".to_string()),
            },
            BrokerageTransactionRow {
                transaction_date: "01/13/2026 as of 12/31/2025".to_string(),
                action: Some("Cash In Lieu".to_string()),
                symbol: Some("FG".to_string()),
                description: Some("F&G ANNUITIES & LIFE INC".to_string()),
                share_quantity: Some(String::new()),
                execution_price: Some(String::new()),
                fees_and_commission: Some(String::new()),
                amount: Some("$9.05".to_string()),
                source_code: Some("CIL".to_string()),
                effective_date: Some("12/31/2025".to_string()),
                deposit_sequence_id: Some("1".to_string()),
                check_date: Some("01/13/2026".to_string()),
                item_issue_id: Some("84212712".to_string()),
                schwab_order_id: Some("0".to_string()),
            },
        ];

        let account_id = Id::from_string("acct-1");
        let first = parse_brokerage_transactions_rows(&account_id, &rows).expect("parse");
        assert_eq!(first.skipped, 0);
        assert_eq!(first.transactions.len(), 2);

        let second = parse_brokerage_transactions_rows(&account_id, &rows).expect("parse");
        assert_eq!(first.transactions[0].id, second.transactions[0].id);
        assert_eq!(first.transactions[1].id, second.transactions[1].id);
    }

    #[tokio::test]
    async fn get_brokerage_transactions_paginates_with_bookmark() -> Result<()> {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path(
                "/api/is.TransactionHistoryWeb/TransactionHistoryInterface/TransactionHistory/brokerage/transactions",
            ))
            .and(body_partial_json(json!({
                "timeFrame": "All",
                "bookmark": null
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "bookmark": {
                    "fromKey": { "primarySortCode": null, "primarySortValue": "" },
                    "fromExecutionDate": "2022-12-21T00:00:00",
                    "fromPublTimeStamp": "2022-12-21 13:27:00.423163",
                    "fromSecondarySortCode": "4",
                    "fromSecondarySortValue": "FG",
                    "fromTertiarySortValue": "0.00000"
                },
                "brokerageTransactions": [
                    {
                        "transactionDate": "02/10/2026",
                        "action": "Buy",
                        "symbol": "ADBE",
                        "description": "ADOBE INC",
                        "shareQuantity": "75",
                        "executionPrice": "$265.1199",
                        "feesAndCommission": "",
                        "amount": "-$19,883.99",
                        "sourceCode": "",
                        "effectiveDate": "02/10/2026",
                        "depositSequenceId": "0",
                        "checkDate": "02/11/2026",
                        "itemIssueId": "1670511108",
                        "schwabOrderId": "719598531600"
                    }
                ]
            })))
            .expect(1)
            .mount(&server)
            .await;

        Mock::given(method("POST"))
            .and(path(
                "/api/is.TransactionHistoryWeb/TransactionHistoryInterface/TransactionHistory/brokerage/transactions",
            ))
            .and(body_partial_json(json!({
                "bookmark": {
                    "fromExecutionDate": "2022-12-21T00:00:00"
                }
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "bookmark": null,
                "brokerageTransactions": [
                    {
                        "transactionDate": "01/13/2026 as of 12/31/2025",
                        "action": "Cash In Lieu",
                        "symbol": "FG",
                        "description": "F&G ANNUITIES & LIFE INC",
                        "shareQuantity": "",
                        "executionPrice": "",
                        "feesAndCommission": "",
                        "amount": "$9.05",
                        "sourceCode": "CIL",
                        "effectiveDate": "12/31/2025",
                        "depositSequenceId": "1",
                        "checkDate": "01/13/2026",
                        "itemIssueId": "84212712",
                        "schwabOrderId": "0"
                    }
                ]
            })))
            .expect(1)
            .mount(&server)
            .await;

        let mut session = SessionData::new().with_token("test-token");
        session.data.insert("api_base".to_string(), server.uri());

        let client = SchwabClient::new(session)?;
        let rows = client
            .get_brokerage_transactions("81636739", "Individual", TransactionHistoryTimeFrame::All)
            .await?;

        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].action.as_deref(), Some("Buy"));
        assert_eq!(rows[1].action.as_deref(), Some("Cash In Lieu"));
        Ok(())
    }
}
