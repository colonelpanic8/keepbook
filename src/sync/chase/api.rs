//! Chase API client using raw HTTP requests.
//!
//! This client uses session cookies captured from a browser session to make
//! requests to Chase's internal APIs. It replaces the fragile QFX download
//! approach with direct API access for accounts, balances, and transactions.

use anyhow::{Context, Result};
use reqwest::Client;
use serde::Deserialize;

use crate::credentials::SessionData;

/// Chase API client using cookie-based authentication.
pub struct ChaseClient {
    client: Client,
    session: SessionData,
}

// ---------------------------------------------------------------------------
// API response types
// ---------------------------------------------------------------------------

/// Response from the app data/init endpoint, which contains cached responses
/// for various sub-endpoints.
#[derive(Debug, Deserialize)]
pub struct AppDataResponse {
    pub code: String,
    #[serde(default)]
    pub cache: Vec<CachedResponse>,
}

#[derive(Debug, Deserialize)]
pub struct CachedResponse {
    pub url: String,
    #[serde(default)]
    pub response: serde_json::Value,
}

/// Account info from the activity options endpoint.
#[derive(Debug, Clone, Deserialize)]
pub struct ActivityAccount {
    pub id: i64,
    pub mask: String,
    pub nickname: String,
    #[serde(rename = "categoryType")]
    pub category_type: String,
    #[serde(rename = "accountType")]
    pub account_type: String,
}

/// Card account detail.
#[derive(Debug, Deserialize)]
pub struct CardDetailResponse {
    pub code: String,
    #[serde(rename = "accountId")]
    pub account_id: i64,
    #[serde(default)]
    pub nickname: Option<String>,
    #[serde(default)]
    pub mask: Option<String>,
    #[serde(default)]
    pub detail: Option<CardDetail>,
}

#[derive(Debug, Deserialize)]
pub struct CardDetail {
    #[serde(rename = "detailType")]
    pub detail_type: Option<String>,
    #[serde(rename = "currentBalance")]
    pub current_balance: Option<f64>,
    #[serde(rename = "availableCredit")]
    pub available_credit: Option<f64>,
    #[serde(rename = "creditLimit")]
    pub credit_limit: Option<f64>,
    #[serde(rename = "lastStmtBalance")]
    pub last_stmt_balance: Option<f64>,
    #[serde(rename = "remainingStmtBalance")]
    pub remaining_stmt_balance: Option<f64>,
    #[serde(rename = "nextPaymentDueDate")]
    pub next_payment_due_date: Option<String>,
    #[serde(rename = "nextPaymentAmount")]
    pub next_payment_amount: Option<f64>,
}

/// Mortgage account detail from the v2 detail endpoint.
#[derive(Debug, Deserialize)]
pub struct MortgageDetailResponse {
    pub code: Option<String>,
    #[serde(rename = "accountId")]
    pub account_id: Option<i64>,
    #[serde(default)]
    pub nickname: Option<String>,
    #[serde(default)]
    pub mask: Option<String>,
    #[serde(default)]
    pub detail: Option<MortgageDetail>,
}

#[derive(Debug, Deserialize)]
pub struct MortgageDetail {
    pub balance: Option<f64>,
    #[serde(rename = "nextPaymentDate")]
    pub next_payment_date: Option<String>,
    #[serde(rename = "nextPaymentAmount")]
    pub next_payment_amount: Option<f64>,
    #[serde(rename = "interestRate")]
    pub interest_rate: Option<f64>,
}

/// Transaction list response from the credit card transactions endpoint.
#[derive(Debug, Deserialize)]
pub struct TransactionsResponse {
    #[serde(default, rename = "totalPostedTransactionCount")]
    pub total_posted_transaction_count: Option<i64>,
    #[serde(default, rename = "postedTransactionCount")]
    pub posted_transaction_count: Option<i64>,
    #[serde(default, rename = "pendingAuthorizationCount")]
    pub pending_authorization_count: Option<i64>,
    #[serde(default)]
    pub activities: Vec<ChaseActivity>,
    #[serde(default, rename = "moreRecordsIndicator")]
    pub more_records_indicator: bool,
    #[serde(default, rename = "paginationContextualText")]
    pub pagination_contextual_text: Option<String>,
    #[serde(default, rename = "lastSortFieldValueText")]
    pub last_sort_field_value_text: Option<String>,
    #[serde(default, rename = "asOfDate")]
    pub as_of_date: Option<String>,
}

/// A single transaction/activity from the Chase API.
#[derive(Debug, Clone, Deserialize)]
pub struct ChaseActivity {
    #[serde(default, rename = "transactionStatusCode")]
    pub transaction_status_code: String,
    #[serde(default, rename = "transactionAmount")]
    pub transaction_amount: f64,
    #[serde(default, rename = "transactionDate")]
    pub transaction_date: String,
    #[serde(default, rename = "transactionPostDate")]
    pub transaction_post_date: Option<String>,
    #[serde(default, rename = "sorTransactionIdentifier")]
    pub sor_transaction_identifier: Option<String>,
    #[serde(default, rename = "derivedUniqueTransactionIdentifier")]
    pub derived_unique_transaction_identifier: Option<String>,
    #[serde(default, rename = "transactionReferenceNumber")]
    pub transaction_reference_number: Option<String>,
    #[serde(default, rename = "creditDebitCode")]
    pub credit_debit_code: String,
    #[serde(default, rename = "etuStandardTransactionTypeName")]
    pub etu_standard_transaction_type_name: Option<String>,
    #[serde(default, rename = "etuStandardTransactionTypeGroupName")]
    pub etu_standard_transaction_type_group_name: Option<String>,
    #[serde(default, rename = "etuStandardExpenseCategoryCode")]
    pub etu_standard_expense_category_code: Option<String>,
    #[serde(default, rename = "currencyCode")]
    pub currency_code: Option<String>,
    #[serde(default, rename = "merchantDetails")]
    pub merchant_details: Option<MerchantDetails>,
    #[serde(default, rename = "last4CardNumber")]
    pub last4_card_number: Option<String>,
    #[serde(default, rename = "digitalAccountIdentifier")]
    pub digital_account_identifier: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MerchantDetails {
    #[serde(default, rename = "rawMerchantDetails")]
    pub raw_merchant_details: Option<RawMerchantDetails>,
    #[serde(default, rename = "enrichedMerchants")]
    pub enriched_merchants: Vec<EnrichedMerchant>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RawMerchantDetails {
    #[serde(default, rename = "merchantDbaName")]
    pub merchant_dba_name: Option<String>,
    #[serde(default, rename = "merchantCityName")]
    pub merchant_city_name: Option<String>,
    #[serde(default, rename = "merchantStateCode")]
    pub merchant_state_code: Option<String>,
    #[serde(default, rename = "merchantCategoryCode")]
    pub merchant_category_code: Option<String>,
    #[serde(default, rename = "merchantCategoryName")]
    pub merchant_category_name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EnrichedMerchant {
    #[serde(default, rename = "merchantName")]
    pub merchant_name: Option<String>,
    #[serde(default, rename = "merchantRoleTypeCode")]
    pub merchant_role_type_code: Option<i64>,
}

impl ChaseActivity {
    /// Get the best description for this transaction.
    pub fn description(&self) -> String {
        if let Some(ref details) = self.merchant_details {
            // Prefer enriched merchant names (they're cleaned up)
            let enriched_names: Vec<String> = details
                .enriched_merchants
                .iter()
                .filter_map(|m| m.merchant_name.clone())
                .filter(|n| !n.is_empty())
                .collect();

            if !enriched_names.is_empty() {
                return enriched_names.join(", ");
            }

            // Fall back to raw merchant DBA name
            if let Some(ref raw) = details.raw_merchant_details {
                if let Some(ref dba) = raw.merchant_dba_name {
                    if !dba.is_empty() {
                        return dba.clone();
                    }
                }
            }
        }

        "Chase transaction".to_string()
    }

    /// Get the signed amount (negative for debits).
    pub fn signed_amount(&self) -> f64 {
        if self.credit_debit_code == "C" {
            self.transaction_amount
        } else {
            -self.transaction_amount
        }
    }

    /// Get a stable transaction ID that is consistent across pending→posted transitions.
    pub fn stable_id(&self) -> String {
        // Prefer sorTransactionIdentifier: it is present for both pending and posted
        // transactions and stays the same when a transaction transitions state.
        // derivedUniqueTransactionIdentifier is only assigned after posting, so using
        // it first would cause pending and posted versions to get different IDs.
        if let Some(ref id) = self.sor_transaction_identifier {
            if !id.is_empty() {
                return id.clone();
            }
        }
        if let Some(ref id) = self.derived_unique_transaction_identifier {
            if !id.is_empty() {
                return id.clone();
            }
        }
        // Fall back to reference number
        if let Some(ref id) = self.transaction_reference_number {
            if !id.is_empty() {
                return id.clone();
            }
        }
        // Last resort: hash of date + amount + description
        format!(
            "{}_{}_{}",
            self.transaction_date,
            self.transaction_amount,
            self.description()
        )
    }

    pub fn is_pending(&self) -> bool {
        self.transaction_status_code == "Pending"
    }
}

// ---------------------------------------------------------------------------
// Client implementation
// ---------------------------------------------------------------------------

impl ChaseClient {
    const BASE_URL: &'static str = "https://secure.chase.com";

    /// Create a new Chase client with session data.
    pub fn new(session: SessionData) -> Result<Self> {
        let client = Client::builder()
            .user_agent("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/145.0.0.0 Safari/537.36")
            .gzip(true)
            .deflate(true)
            .brotli(true)
            .build()
            .context("Failed to create HTTP client")?;

        Ok(Self { client, session })
    }

    /// Build a cookie header from the session.
    fn cookie_header(&self) -> String {
        // Prefer cookie_jar if available (has proper domain/path info)
        if !self.session.cookie_jar.is_empty() {
            self.session
                .cookie_jar
                .iter()
                .map(|c| format!("{}={}", c.name, c.value))
                .collect::<Vec<_>>()
                .join("; ")
        } else {
            self.session.cookie_header()
        }
    }

    /// Make an authenticated GET request.
    async fn get<T: for<'de> Deserialize<'de>>(&self, path: &str) -> Result<T> {
        let url = format!("{}{}", Self::BASE_URL, path);

        let response = self
            .client
            .get(&url)
            .header("accept", "application/json, text/plain, */*")
            .header("x-jpmc-csrf-token", "NONE")
            .header("x-jpmc-channel", "id=C30")
            .header("x-jpmc-client-request-id", uuid::Uuid::new_v4().to_string())
            .header("referer", "https://secure.chase.com/web/auth/dashboard")
            .header("cookie", self.cookie_header())
            .send()
            .await
            .context("HTTP GET request failed")?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!(
                "Chase API GET failed ({status}): {}",
                &body[..body.len().min(500)]
            );
        }

        let body = response.text().await.context("Failed to read response")?;
        serde_json::from_str(&body).with_context(|| {
            format!(
                "Failed to parse Chase API response for {path}: {}",
                &body[..body.len().min(200)]
            )
        })
    }

    /// Make an authenticated POST request.
    async fn post<T: for<'de> Deserialize<'de>>(
        &self,
        path: &str,
        body: Option<&str>,
    ) -> Result<T> {
        let url = format!("{}{}", Self::BASE_URL, path);

        let mut req = self
            .client
            .post(&url)
            .header("accept", "application/json, text/plain, */*")
            .header(
                "content-type",
                "application/x-www-form-urlencoded; charset=UTF-8",
            )
            .header("x-jpmc-csrf-token", "NONE")
            .header("x-jpmc-channel", "id=C30")
            .header("x-jpmc-client-request-id", uuid::Uuid::new_v4().to_string())
            .header("referer", "https://secure.chase.com/web/auth/dashboard")
            .header("origin", "https://secure.chase.com")
            .header("cookie", self.cookie_header());

        req = req.body(body.unwrap_or("").to_string());

        let response = req.send().await.context("HTTP POST request failed")?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!(
                "Chase API POST failed ({status}): {}",
                &body[..body.len().min(500)]
            );
        }

        let body = response.text().await.context("Failed to read response")?;
        serde_json::from_str(&body).with_context(|| {
            format!(
                "Failed to parse Chase API response for {path}: {}",
                &body[..body.len().min(200)]
            )
        })
    }

    /// Get the list of accounts from the activity options cache.
    pub async fn get_accounts(&self) -> Result<Vec<ActivityAccount>> {
        let resp: AppDataResponse = self
            .post(
                "/svc/rl/accounts/secure/v1/dashboard/data/list",
                Some("context=GWM_OVD_NEW_PBM"),
            )
            .await?;

        for cached in &resp.cache {
            if cached.url.contains("activity/options/list") {
                if let Some(accounts) = cached.response.get("accounts") {
                    let accounts: Vec<ActivityAccount> =
                        serde_json::from_value(accounts.clone())
                            .context("Failed to parse accounts from cache")?;
                    return Ok(accounts);
                }
            }
        }

        anyhow::bail!("Could not find accounts in dashboard data response")
    }

    /// Get card detail (balances) for a specific account.
    pub async fn get_card_detail(&self, account_id: i64) -> Result<CardDetailResponse> {
        self.post(
            "/svc/rr/accounts/secure/v2/account/detail/card/list",
            Some(&format!("accountId={account_id}")),
        )
        .await
    }

    /// Get mortgage detail info.
    pub async fn get_mortgage_detail(&self, account_id: i64) -> Result<MortgageDetailResponse> {
        self.post(
            "/svc/rr/accounts/secure/v2/account/detail/mortgage/list",
            Some(&format!("accountId={account_id}")),
        )
        .await
    }

    /// Get all accounts and their detail data from the dashboard cache.
    /// This returns mortgage detail responses found in the dashboard cache,
    /// avoiding extra API calls.
    pub async fn get_dashboard_mortgage_details(&self) -> Result<Vec<MortgageDetailResponse>> {
        let resp: AppDataResponse = self
            .post(
                "/svc/rl/accounts/secure/v1/dashboard/data/list",
                Some("context=GWM_OVD_NEW_PBM"),
            )
            .await?;

        let mut mortgages = Vec::new();
        for cached in &resp.cache {
            if cached.url.contains("account/detail/mortgage/list") {
                if let Ok(detail) =
                    serde_json::from_value::<MortgageDetailResponse>(cached.response.clone())
                {
                    mortgages.push(detail);
                }
            }
        }
        Ok(mortgages)
    }

    /// Get transactions for a credit card account.
    ///
    /// Returns up to `record_count` transactions. Use `pagination_key` for
    /// subsequent pages (pass `paginationContextualText` from previous response).
    pub async fn get_card_transactions(
        &self,
        account_id: i64,
        record_count: u32,
        pagination_key: Option<&str>,
    ) -> Result<TransactionsResponse> {
        let mut path = format!(
            "/svc/rr/accounts/secure/gateway/credit-card/transactions/inquiry-maintenance/etu-transactions/v4/accounts/transactions?digital-account-identifier={account_id}&provide-available-statement-indicator=true&record-count={record_count}&sort-order-code=D&sort-key-code=T"
        );

        if let Some(key) = pagination_key {
            path.push_str(&format!("&next-page-key={}", urlencoding::encode(key)));
        }

        self.get(&path).await
    }

    /// Get all transactions for a card account, handling pagination.
    pub async fn get_all_card_transactions(&self, account_id: i64) -> Result<Vec<ChaseActivity>> {
        let mut all_activities = Vec::new();
        let mut pagination_key: Option<String> = None;
        let page_size = 100;

        loop {
            let resp = self
                .get_card_transactions(account_id, page_size, pagination_key.as_deref())
                .await?;

            all_activities.extend(resp.activities);

            if !resp.more_records_indicator {
                break;
            }

            match resp.pagination_contextual_text {
                Some(ref key) if !key.is_empty() => {
                    pagination_key = Some(key.clone());
                }
                _ => break,
            }

            // Safety limit
            if all_activities.len() > 5000 {
                eprintln!(
                    "Chase: stopping pagination at {} transactions (safety limit)",
                    all_activities.len()
                );
                break;
            }
        }

        Ok(all_activities)
    }

    /// Test that the session is still valid by making a simple API call.
    pub async fn test_auth(&self) -> Result<()> {
        let resp: AppDataResponse = self
            .post(
                "/svc/rl/accounts/secure/v1/dashboard/data/list",
                Some("context=GWM_OVD_NEW_PBM"),
            )
            .await?;

        let code = resp.code.as_str();

        if code != "SUCCESS" {
            anyhow::bail!("Chase auth test failed: code={code}");
        }

        Ok(())
    }
}
