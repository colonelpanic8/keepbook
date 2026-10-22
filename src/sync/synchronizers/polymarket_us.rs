//! Polymarket US synchronizer.
//!
//! Uses the Polymarket US retail API with Ed25519-signed requests. Credentials
//! are loaded via the CredentialStore abstraction.

use std::collections::{BTreeMap, HashMap};
use std::str::FromStr;

use anyhow::{Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use chrono::Utc;
use ed25519_dalek::{Signer, SigningKey};
use reqwest::Client;
use rust_decimal::Decimal;
use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;

use crate::credentials::CredentialStore;
use crate::models::{
    Account, Asset, AssetBalance, Connection, ConnectionStatus, Id, LastSync, SyncStatus,
};
use crate::storage::Storage;
use crate::sync::{AccountBalances, AccountListing, SyncResult, SyncedAssetBalance, Synchronizer};

const API_BASE: &str = "https://api.polymarket.us";
const GATEWAY_BASE: &str = "https://gateway.polymarket.us";

/// Polymarket US synchronizer.
pub struct PolymarketUsSynchronizer {
    key_id: String,
    secret_key: SecretString,
    client: Client,
    api_base: String,
    gateway_base: String,
}

impl PolymarketUsSynchronizer {
    /// Create a new Polymarket US synchronizer with explicit credentials.
    pub fn new(key_id: String, secret_key: SecretString) -> Self {
        Self {
            key_id,
            secret_key,
            client: Client::new(),
            api_base: API_BASE.to_string(),
            gateway_base: GATEWAY_BASE.to_string(),
        }
    }

    /// Point both the authenticated API and the public gateway at one base URL (useful for tests).
    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        let base_url = base_url.into();
        self.api_base = base_url.clone();
        self.gateway_base = base_url;
        self
    }

    /// Create a synchronizer by loading credentials from storage.
    pub async fn from_credentials(store: &dyn CredentialStore) -> Result<Self> {
        let key_id = store
            .get("key-id")
            .await?
            .context("Missing key-id in credentials")?;

        let secret_key = store
            .get("secret-key")
            .await?
            .context("Missing secret-key in credentials")?;

        Ok(Self::new(
            key_id.expose_secret().trim().to_string(),
            secret_key,
        ))
    }

    /// Create a new synchronizer from connection credentials.
    pub async fn from_connection<S: Storage + ?Sized>(
        connection: &Connection,
        storage: &S,
    ) -> Result<Self> {
        let credential_store = storage
            .get_credential_store(connection.id())?
            .context("No credentials configured for this connection")?;

        Self::from_credentials(credential_store.as_ref()).await
    }

    fn signing_key(&self) -> Result<SigningKey> {
        let bytes = STANDARD
            .decode(self.secret_key.expose_secret().trim())
            .context("Polymarket US secret-key is not valid base64")?;
        // Keys are issued as a 64-byte keypair; the first 32 bytes are the Ed25519 seed.
        let seed: [u8; 32] = bytes
            .get(..32)
            .and_then(|seed| seed.try_into().ok())
            .context("Polymarket US secret-key must decode to at least 32 bytes")?;
        Ok(SigningKey::from_bytes(&seed))
    }

    /// Signs `timestamp + method + path`; the query string is not part of the signed message.
    fn auth_headers(&self, method: &str, path: &str) -> Result<[(&'static str, String); 3]> {
        let timestamp = Utc::now().timestamp_millis().to_string();
        let message = format!("{timestamp}{method}{path}");
        let signature = self.signing_key()?.sign(message.as_bytes());
        Ok([
            ("X-PM-Access-Key", self.key_id.clone()),
            ("X-PM-Timestamp", timestamp),
            ("X-PM-Signature", STANDARD.encode(signature.to_bytes())),
        ])
    }

    async fn get<T: for<'de> Deserialize<'de>>(
        &self,
        path: &str,
        query: &[(&str, &str)],
        authenticated: bool,
    ) -> Result<T> {
        let base = if authenticated {
            &self.api_base
        } else {
            &self.gateway_base
        };
        let url = format!("{}{path}", base.trim_end_matches('/'));
        let mut request = self.client.get(&url).query(query);
        if authenticated {
            for (name, value) in self.auth_headers("GET", path)? {
                request = request.header(name, value);
            }
        }

        let response = request.send().await.context("HTTP request failed")?;
        let status = response.status();
        let body = response
            .text()
            .await
            .context("Failed to read response body")?;

        if !status.is_success() {
            anyhow::bail!("Polymarket US request {path} failed ({status}): {body}");
        }

        serde_json::from_str(&body)
            .with_context(|| format!("Failed to parse Polymarket US response from {path}"))
    }

    async fn get_balances(&self) -> Result<Vec<PolymarketBalance>> {
        #[derive(Deserialize)]
        struct Response {
            #[serde(default)]
            balances: Vec<PolymarketBalance>,
        }

        let response: Response = self.get("/v1/account/balances", &[], true).await?;
        Ok(response.balances)
    }

    async fn get_positions(&self) -> Result<Vec<(String, PolymarketPosition)>> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Response {
            #[serde(default)]
            positions: BTreeMap<String, PolymarketPosition>,
            #[serde(default)]
            next_cursor: Option<String>,
            #[serde(default)]
            eof: bool,
        }

        let mut positions = Vec::new();
        let mut cursor: Option<String> = None;

        loop {
            let mut query = vec![("limit", "100")];
            if let Some(cursor) = cursor.as_deref() {
                query.push(("cursor", cursor));
            }

            let response: Response = self.get("/v1/portfolio/positions", &query, true).await?;
            positions.extend(response.positions);

            if response.eof {
                break;
            }
            cursor = response.next_cursor.filter(|c| !c.is_empty());
            if cursor.is_none() {
                break;
            }
        }

        Ok(positions)
    }

    async fn get_mark_price(&self, slug: &str) -> Result<PolymarketAmount> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct MarketData {
            current_px: Option<PolymarketAmount>,
            last_trade_px: Option<PolymarketAmount>,
            settlement_px: Option<PolymarketAmount>,
        }
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Response {
            market_data: MarketData,
        }

        let path = format!("/v1/markets/{}/bbo", encode_path_segment(slug));
        let response: Response = self.get(&path, &[], false).await?;
        let data = response.market_data;
        data.current_px
            .or(data.last_trade_px)
            .or(data.settlement_px)
            .with_context(|| format!("No price available for Polymarket US market {slug}"))
    }

    async fn sync_internal<S: Storage + ?Sized>(
        &self,
        connection: &mut Connection,
        storage: &S,
    ) -> Result<SyncResult> {
        let balances = self.get_balances().await?;
        let positions = self.get_positions().await?;

        let mut asset_balances: Vec<AssetBalance> = balances
            .iter()
            .map(|balance| {
                AssetBalance::new(
                    Asset::currency(&balance.currency),
                    balance.current_balance.normalize().to_string(),
                )
            })
            .collect();

        // Settled positions have already been paid out into the cash balance.
        for (slug, position) in positions.iter().filter(|(_, p)| !p.expired) {
            let quantity = parse_decimal(&position.net_position_decimal)
                .with_context(|| format!("Invalid net position for {slug}"))?;
            if quantity.is_zero() {
                continue;
            }

            let price = self.get_mark_price(slug).await?;
            // Shorts carry a negative quantity, which nets their payout liability against the
            // sale proceeds already included in the cash balance.
            let value = quantity * parse_decimal(&price.value)?;
            let mut asset_balance = AssetBalance::new(
                Asset::manual_value(position.display_name(slug), &price.currency),
                value.normalize().to_string(),
            );
            if let Some(cost) = &position.cost {
                asset_balance = asset_balance.with_cost_basis(cost.value.trim());
            }
            asset_balances.push(asset_balance);
        }

        let existing_created_at: HashMap<Id, _> = storage
            .list_accounts()
            .await?
            .into_iter()
            .filter(|a| a.connection_id == *connection.id())
            .map(|a| (a.id, a.created_at))
            .collect();

        let account_id = Id::from_external(&format!("polymarket_us:{}", connection.id()));
        let account = Account {
            id: account_id.clone(),
            name: "Polymarket US".to_string(),
            connection_id: connection.id().clone(),
            tags: vec!["polymarket_us".to_string()],
            created_at: existing_created_at
                .get(&account_id)
                .copied()
                .unwrap_or_else(Utc::now),
            active: true,
            synchronizer_data: serde_json::json!({
                "market_slugs": positions.iter().map(|(slug, _)| slug).collect::<Vec<_>>(),
            }),
        };

        let account_balances = AccountBalances::snapshot_or_unavailable(
            asset_balances
                .into_iter()
                .map(SyncedAssetBalance::new)
                .collect(),
            "Polymarket US returned no balances",
        );

        connection.state.account_ids = vec![account_id.clone()];
        connection.state.last_sync = Some(LastSync {
            at: Utc::now(),
            status: SyncStatus::Success,
            error: None,
        });
        connection.state.status = ConnectionStatus::Active;

        Ok(SyncResult {
            connection: connection.clone(),
            accounts: vec![account],
            account_listing: AccountListing::Complete,
            balances: vec![(account_id.clone(), account_balances)],
            transactions: vec![(account_id, Vec::new())],
        })
    }
}

fn parse_decimal(value: &str) -> Result<Decimal> {
    Decimal::from_str(value.trim()).with_context(|| format!("Invalid decimal: {value:?}"))
}

/// Accepts a JSON number or string without round-tripping through `f64`.
fn deserialize_json_decimal<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<Decimal, D::Error> {
    let raw = match serde_json::Value::deserialize(deserializer)? {
        serde_json::Value::Number(number) => number.to_string(),
        serde_json::Value::String(string) => string,
        other => {
            return Err(serde::de::Error::custom(format!(
                "expected decimal, got {other}"
            )))
        }
    };
    let raw = raw.trim();
    Decimal::from_str(raw)
        .or_else(|_| Decimal::from_scientific(raw))
        .map_err(serde::de::Error::custom)
}

fn encode_path_segment(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for b in value.as_bytes() {
        match *b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(*b as char)
            }
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PolymarketBalance {
    #[serde(deserialize_with = "deserialize_json_decimal")]
    current_balance: Decimal,
    currency: String,
}

#[derive(Debug, Deserialize)]
struct PolymarketAmount {
    value: String,
    currency: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PolymarketMarketMetadata {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    outcome: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PolymarketPosition {
    net_position_decimal: String,
    #[serde(default)]
    cost: Option<PolymarketAmount>,
    #[serde(default)]
    expired: bool,
    #[serde(default)]
    market_metadata: PolymarketMarketMetadata,
}

impl PolymarketPosition {
    fn display_name(&self, slug: &str) -> String {
        let non_empty = |value: &Option<String>| {
            value
                .as_deref()
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(str::to_string)
        };
        let title = non_empty(&self.market_metadata.title).unwrap_or_else(|| slug.to_string());
        match non_empty(&self.market_metadata.outcome) {
            Some(outcome) => format!("{title} ({outcome})"),
            None => title,
        }
    }
}

#[async_trait::async_trait]
impl Synchronizer for PolymarketUsSynchronizer {
    fn name(&self) -> &str {
        "polymarket_us"
    }

    async fn sync(&self, connection: &mut Connection, storage: &dyn Storage) -> Result<SyncResult> {
        self.sync_internal(connection, storage).await
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/sync/synchronizers/polymarket_us_tests.rs"]
mod polymarket_us_tests;
