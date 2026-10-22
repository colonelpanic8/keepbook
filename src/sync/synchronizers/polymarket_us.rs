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
use crate::market_data::providers::polymarket_us::{
    polymarket_us_asset, side_price, PolymarketAmount, POLYMARKET_US_EXCHANGE,
};
use crate::market_data::providers::PolymarketUsPriceSource;
use crate::market_data::{AssetId, PriceKind, PricePoint};
use crate::models::{
    Account, Asset, AssetBalance, Connection, ConnectionStatus, ContractSide, Id, LastSync,
    SyncStatus,
};
use crate::storage::Storage;
use crate::sync::{AccountBalances, AccountListing, SyncResult, SyncedAssetBalance, Synchronizer};

const API_BASE: &str = "https://api.polymarket.us";

/// Polymarket US synchronizer.
pub struct PolymarketUsSynchronizer {
    key_id: String,
    secret_key: SecretString,
    client: Client,
    api_base: String,
    prices: PolymarketUsPriceSource,
}

impl PolymarketUsSynchronizer {
    /// Create a new Polymarket US synchronizer with explicit credentials.
    pub fn new(key_id: String, secret_key: SecretString) -> Self {
        Self {
            key_id,
            secret_key,
            client: Client::new(),
            api_base: API_BASE.to_string(),
            prices: PolymarketUsPriceSource::new(),
        }
    }

    /// Point both the authenticated API and the public gateway at one base URL (useful for tests).
    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        let base_url = base_url.into();
        self.prices = PolymarketUsPriceSource::new().with_base_url(&base_url);
        self.api_base = base_url;
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
    ) -> Result<T> {
        let url = format!("{}{path}", self.api_base.trim_end_matches('/'));
        let mut request = self.client.get(&url).query(query);
        for (name, value) in self.auth_headers("GET", path)? {
            request = request.header(name, value);
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

        let response: Response = self.get("/v1/account/balances", &[]).await?;
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

            let response: Response = self.get("/v1/portfolio/positions", &query).await?;
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

    async fn sync_internal<S: Storage + ?Sized>(
        &self,
        connection: &mut Connection,
        storage: &S,
    ) -> Result<SyncResult> {
        let balances = self.get_balances().await?;
        let positions = self.get_positions().await?;

        let now = Utc::now();

        let mut contract_balances = Vec::new();
        // $1 per short contract is locked as margin inside the fiat balance.
        let mut short_margin: HashMap<String, Decimal> = HashMap::new();

        // Settled positions have already been paid out into the cash balance.
        for (slug, position) in positions.iter().filter(|(_, p)| !p.expired) {
            let quantity = parse_decimal(&position.net_position_decimal)
                .with_context(|| format!("Invalid net position for {slug}"))?;
            if quantity.is_zero() {
                continue;
            }

            let mark = self.prices.fetch_mark_price(slug).await?;
            let currency = mark.currency.trim().to_uppercase();
            // The exchange records a NO position as a short of the YES instrument. Holding it as
            // NO contracts with the margin taken out of cash leaves the total unchanged:
            // cash - qty + qty * (1 - price) = cash - qty * price.
            let side = if quantity.is_sign_negative() {
                *short_margin.entry(currency.clone()).or_default() += quantity.abs();
                ContractSide::No
            } else {
                ContractSide::Yes
            };
            let asset = polymarket_us_asset(slug, side, Some(position.display_name(slug, side)));
            let price = PricePoint {
                asset_id: AssetId::from_asset(&asset),
                as_of_date: now.date_naive(),
                timestamp: now,
                price: side_price(side, parse_decimal(&mark.value)?)
                    .normalize()
                    .to_string(),
                quote_currency: currency,
                kind: PriceKind::Quote,
                source: POLYMARKET_US_EXCHANGE.to_string(),
            };
            let mut asset_balance =
                AssetBalance::new(asset, quantity.abs().normalize().to_string());
            if let Some(cost) = &position.cost {
                asset_balance = asset_balance.with_cost_basis(cost.value.trim());
            }
            contract_balances.push(SyncedAssetBalance::new(asset_balance).with_price(price));
        }

        let mut asset_balances: Vec<SyncedAssetBalance> = balances
            .iter()
            .map(|balance| {
                let currency = balance.currency.trim().to_uppercase();
                let margin = short_margin.remove(&currency).unwrap_or_default();
                SyncedAssetBalance::new(AssetBalance::new(
                    Asset::currency(&balance.currency),
                    (balance.current_balance - margin).normalize().to_string(),
                ))
            })
            .collect();
        for (currency, margin) in short_margin {
            asset_balances.push(SyncedAssetBalance::new(AssetBalance::new(
                Asset::currency(currency),
                (-margin).normalize().to_string(),
            )));
        }
        asset_balances.extend(contract_balances);

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
            asset_balances,
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

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PolymarketBalance {
    #[serde(deserialize_with = "deserialize_json_decimal")]
    current_balance: Decimal,
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
    fn display_name(&self, slug: &str, side: ContractSide) -> String {
        let non_empty = |value: &Option<String>| {
            value
                .as_deref()
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(str::to_string)
        };
        let title = non_empty(&self.market_metadata.title).unwrap_or_else(|| slug.to_string());
        let label = side.label();
        match non_empty(&self.market_metadata.outcome) {
            Some(outcome)
                if !outcome.eq_ignore_ascii_case("yes") && !outcome.eq_ignore_ascii_case("no") =>
            {
                format!("{title} ({label}: {outcome})")
            }
            _ => format!("{title} ({label})"),
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
