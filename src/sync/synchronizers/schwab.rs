//! Schwab synchronizer with browser-based authentication.
//!
//! This synchronizer uses a headless browser helper for automated session capture
//! and Schwab's internal APIs for data fetching.

use std::collections::HashMap;
use std::process::Stdio;
use std::time::Duration;

use anyhow::{Context, Result};
use chrono::Utc;
use secrecy::ExposeSecret;
use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

use crate::credentials::{CredentialStore, SessionCache, SessionData, StoredCookie};
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
const SCHWAB_HEADLESS_AUTH_HELPER: &str = include_str!("../../../scripts/schwab-headless-auth.mjs");

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
        if !Self::should_autofill_login() {
            anyhow::bail!(
                "Schwab headless login requires credential autofill; unset KEEPBOOK_SCHWAB_AUTOFILL=0 to use this experiment"
            );
        }

        let (username, password) = self.get_login_credentials().await?.context(
            "Schwab headless login requires KEEPBOOK_SCHWAB_USERNAME/KEEPBOOK_SCHWAB_PASSWORD or credential-store username/password entries",
        )?;

        println!("Starting headless Schwab login...");
        println!(
            "MFA can be completed headlessly when KEEPBOOK_SCHWAB_SMS_CODE_COMMAND or KEEPBOOK_SMS_CODE_COMMAND is configured."
        );

        let session = run_schwab_headless_auth(&username, &password).await?;
        let token = session.token.clone().unwrap_or_default();

        // Save to cache
        self.session_cache.set(&self.session_key(), &session)?;

        println!("\nSession saved successfully!");
        println!("Token: {}...", &token[..50.min(token.len())]);
        println!("Cookies: {} captured", session.cookies.len());

        Ok(())
    }
}

#[derive(Debug, Serialize)]
struct SchwabHeadlessAuthRequest<'a> {
    username: &'a str,
    password: &'a str,
    #[serde(rename = "loginUrl")]
    login_url: &'a str,
    #[serde(rename = "timeoutMs")]
    timeout_ms: u64,
}

#[derive(Debug, Deserialize)]
struct SchwabHeadlessAuthOutput {
    token: String,
    #[serde(default)]
    api_base: Option<String>,
    #[serde(default)]
    cookies: HashMap<String, String>,
    #[serde(default)]
    cookie_jar: Vec<StoredCookie>,
}

async fn run_schwab_headless_auth(username: &str, password: &str) -> Result<SessionData> {
    let timeout_ms = std::env::var("KEEPBOOK_SCHWAB_AUTH_TIMEOUT_MS")
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(300_000);
    let node = std::env::var("KEEPBOOK_NODE").unwrap_or_else(|_| "node".to_string());

    let tempdir = tempfile::tempdir().context("Failed to create Schwab auth helper tempdir")?;
    let script_path = tempdir.path().join("schwab-headless-auth.mjs");
    std::fs::write(&script_path, SCHWAB_HEADLESS_AUTH_HELPER)
        .with_context(|| format!("Failed to write Schwab auth helper to {script_path:?}"))?;

    let request = SchwabHeadlessAuthRequest {
        username,
        password,
        login_url: SCHWAB_LOGIN_URL,
        timeout_ms,
    };
    let request_json =
        serde_json::to_vec(&request).context("Failed to serialize Schwab auth helper input")?;

    let mut child = Command::new(&node)
        .arg(&script_path)
        .env("KEEPBOOK_PLAYWRIGHT_ROOT", env!("CARGO_MANIFEST_DIR"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("Failed to start Node.js for Schwab headless auth ({node})"))?;

    let mut stdin = child
        .stdin
        .take()
        .context("Failed to open stdin for Schwab headless auth helper")?;
    stdin
        .write_all(&request_json)
        .await
        .context("Failed to write Schwab auth helper input")?;
    drop(stdin);

    let wait_timeout = Duration::from_millis(timeout_ms.saturating_add(30_000));
    let output = tokio::time::timeout(wait_timeout, child.wait_with_output())
        .await
        .context("Timed out waiting for Schwab headless auth helper")?
        .context("Failed to wait for Schwab headless auth helper")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Schwab headless auth helper failed: {}", stderr.trim());
    }

    let auth: SchwabHeadlessAuthOutput =
        serde_json::from_slice(&output.stdout).with_context(|| {
            format!(
                "Failed to parse Schwab headless auth helper output: {}",
                String::from_utf8_lossy(&output.stdout).trim()
            )
        })?;

    let mut data = HashMap::new();
    if let Some(api_base) = auth.api_base.filter(|v| !v.trim().is_empty()) {
        data.insert("api_base".to_string(), api_base);
    }

    Ok(SessionData {
        token: Some(auth.token),
        cookies: auth.cookies,
        cookie_jar: auth.cookie_jar,
        captured_at: Some(Utc::now().timestamp()),
        data,
    })
}
