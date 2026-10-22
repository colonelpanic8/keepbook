//! Kalshi synchronizer.
//!
//! Uses the Kalshi trade API with requests signed by an Ed25519 or RSA-PSS API key.
//! Credentials are loaded via the CredentialStore abstraction.

use std::collections::HashMap;
use std::str::FromStr;

use anyhow::{Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use chrono::Utc;
use ed25519_dalek::pkcs8::DecodePrivateKey as _;
use ed25519_dalek::Signer as _;
use reqwest::Client;
use rsa::pkcs1::DecodeRsaPrivateKey as _;
use rsa::pss::BlindedSigningKey;
use rsa::sha2::Sha256;
use rsa::signature::{RandomizedSigner as _, SignatureEncoding as _};
use rsa::RsaPrivateKey;
use rust_decimal::Decimal;
use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;

use crate::credentials::CredentialStore;
use crate::models::{
    Account, Asset, AssetBalance, Connection, ConnectionStatus, Id, LastSync, SyncStatus,
};
use crate::storage::Storage;
use crate::sync::{AccountBalances, AccountListing, SyncResult, SyncedAssetBalance, Synchronizer};

const API_BASE: &str = "https://api.elections.kalshi.com";
const API_PREFIX: &str = "/trade-api/v2";
const MARKET_BATCH_SIZE: usize = 50;

enum KalshiSigner {
    Ed25519(ed25519_dalek::SigningKey),
    Rsa(BlindedSigningKey<Sha256>),
}

impl KalshiSigner {
    /// The PEM header does not reliably identify the key type, so try each format.
    fn from_pem(pem: &str) -> Result<Self> {
        let pem = pem.trim();
        if let Ok(key) = ed25519_dalek::SigningKey::from_pkcs8_pem(pem) {
            return Ok(Self::Ed25519(key));
        }
        let rsa_key = RsaPrivateKey::from_pkcs1_pem(pem)
            .or_else(|_| <RsaPrivateKey as rsa::pkcs8::DecodePrivateKey>::from_pkcs8_pem(pem))
            .context("Kalshi private-key is not a PEM-encoded Ed25519 or RSA private key")?;
        Ok(Self::Rsa(BlindedSigningKey::new(rsa_key)))
    }

    fn sign(&self, message: &[u8]) -> Vec<u8> {
        match self {
            Self::Ed25519(key) => key.sign(message).to_bytes().to_vec(),
            Self::Rsa(key) => key.sign_with_rng(&mut rand::thread_rng(), message).to_vec(),
        }
    }
}

/// Kalshi synchronizer.
pub struct KalshiSynchronizer {
    key_id: String,
    signer: KalshiSigner,
    client: Client,
    base_url: String,
}

impl KalshiSynchronizer {
    /// Create a new Kalshi synchronizer from an API key ID and PEM private key.
    pub fn new(key_id: String, private_key: SecretString) -> Result<Self> {
        Ok(Self {
            key_id,
            signer: KalshiSigner::from_pem(private_key.expose_secret())?,
            client: Client::new(),
            base_url: API_BASE.to_string(),
        })
    }

    /// Use a custom API host (useful for tests or the demo environment).
    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }

    /// Create a synchronizer by loading credentials from storage.
    pub async fn from_credentials(store: &dyn CredentialStore) -> Result<Self> {
        let key_id = store
            .get("key-id")
            .await?
            .context("Missing key-id in credentials")?;

        let private_key = store
            .get("private-key")
            .await?
            .context("Missing private-key in credentials")?;

        Self::new(key_id.expose_secret().trim().to_string(), private_key)
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

    async fn get<T: for<'de> Deserialize<'de>>(
        &self,
        endpoint: &str,
        query: &[(&str, &str)],
    ) -> Result<T> {
        // Signs `timestamp + method + path`; the query string is not part of the signed message.
        let path = format!("{API_PREFIX}{endpoint}");
        let timestamp = Utc::now().timestamp_millis().to_string();
        let signature = self.signer.sign(format!("{timestamp}GET{path}").as_bytes());

        let url = format!("{}{path}", self.base_url.trim_end_matches('/'));
        let response = self
            .client
            .get(&url)
            .query(query)
            .header("KALSHI-ACCESS-KEY", &self.key_id)
            .header("KALSHI-ACCESS-TIMESTAMP", timestamp)
            .header("KALSHI-ACCESS-SIGNATURE", STANDARD.encode(signature))
            .send()
            .await
            .context("HTTP request failed")?;
        let status = response.status();
        let body = response
            .text()
            .await
            .context("Failed to read response body")?;

        if !status.is_success() {
            anyhow::bail!("Kalshi request {endpoint} failed ({status}): {body}");
        }

        serde_json::from_str(&body)
            .with_context(|| format!("Failed to parse Kalshi response from {endpoint}"))
    }

    async fn get_balance(&self) -> Result<Decimal> {
        #[derive(Deserialize)]
        struct Response {
            balance_dollars: String,
        }

        let response: Response = self.get("/portfolio/balance", &[]).await?;
        parse_decimal(&response.balance_dollars)
    }

    async fn get_positions(&self) -> Result<Vec<KalshiPosition>> {
        #[derive(Deserialize)]
        struct Response {
            #[serde(default)]
            market_positions: Vec<KalshiPosition>,
            #[serde(default)]
            cursor: Option<String>,
        }

        let mut positions = Vec::new();
        let mut cursor: Option<String> = None;

        loop {
            let mut query = vec![("limit", "1000"), ("count_filter", "position")];
            if let Some(cursor) = cursor.as_deref() {
                query.push(("cursor", cursor));
            }

            let response: Response = self.get("/portfolio/positions", &query).await?;
            positions.extend(response.market_positions);

            cursor = response.cursor.filter(|c| !c.is_empty());
            if cursor.is_none() {
                break;
            }
        }

        Ok(positions)
    }

    async fn get_markets(&self, tickers: &[&str]) -> Result<HashMap<String, KalshiMarket>> {
        #[derive(Deserialize)]
        struct Response {
            #[serde(default)]
            markets: Vec<KalshiMarket>,
        }

        let mut markets = HashMap::new();
        for chunk in tickers.chunks(MARKET_BATCH_SIZE) {
            let tickers = chunk.join(",");
            let limit = chunk.len().to_string();
            let response: Response = self
                .get("/markets", &[("tickers", &tickers), ("limit", &limit)])
                .await?;
            markets.extend(
                response
                    .markets
                    .into_iter()
                    .map(|market| (market.ticker.clone(), market)),
            );
        }
        Ok(markets)
    }

    async fn sync_internal<S: Storage + ?Sized>(
        &self,
        connection: &mut Connection,
        storage: &S,
    ) -> Result<SyncResult> {
        let cash = self.get_balance().await?;
        let positions: Vec<(KalshiPosition, Decimal)> = self
            .get_positions()
            .await?
            .into_iter()
            .map(|position| {
                let quantity = parse_decimal(&position.position_fp)
                    .with_context(|| format!("Invalid position for {}", position.ticker))?;
                Ok((position, quantity))
            })
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .filter(|(_, quantity)| !quantity.is_zero())
            .collect();

        let tickers: Vec<&str> = positions.iter().map(|(p, _)| p.ticker.as_str()).collect();
        let markets = self.get_markets(&tickers).await?;

        let mut asset_balances = vec![AssetBalance::new(
            Asset::currency("USD"),
            cash.normalize().to_string(),
        )];

        for (position, quantity) in &positions {
            let market = markets
                .get(&position.ticker)
                .with_context(|| format!("Kalshi market {} not found", position.ticker))?;
            // Negative positions are NO contracts.
            let side = if quantity.is_sign_negative() {
                Side::No
            } else {
                Side::Yes
            };
            let price = market.mark_price(side)?;
            let value = quantity.abs() * price;
            let cost = parse_decimal(&position.market_exposure_dollars)
                .with_context(|| format!("Invalid market exposure for {}", position.ticker))?;
            asset_balances.push(
                AssetBalance::new(
                    Asset::manual_value(market.display_name(side), "USD"),
                    value.normalize().to_string(),
                )
                .with_cost_basis(cost.normalize().to_string()),
            );
        }

        let existing_created_at: HashMap<Id, _> = storage
            .list_accounts()
            .await?
            .into_iter()
            .filter(|a| a.connection_id == *connection.id())
            .map(|a| (a.id, a.created_at))
            .collect();

        let account_id = Id::from_external(&format!("kalshi:{}", connection.id()));
        let account = Account {
            id: account_id.clone(),
            name: "Kalshi".to_string(),
            connection_id: connection.id().clone(),
            tags: vec!["kalshi".to_string()],
            created_at: existing_created_at
                .get(&account_id)
                .copied()
                .unwrap_or_else(Utc::now),
            active: true,
            synchronizer_data: serde_json::json!({ "market_tickers": tickers }),
        };

        let account_balances = AccountBalances::snapshot_or_unavailable(
            asset_balances
                .into_iter()
                .map(SyncedAssetBalance::new)
                .collect(),
            "Kalshi returned no balances",
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

fn parse_optional_decimal(value: &Option<String>) -> Result<Option<Decimal>> {
    value
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(parse_decimal)
        .transpose()
}

#[derive(Debug, Clone, Copy)]
enum Side {
    Yes,
    No,
}

#[derive(Debug, Deserialize)]
struct KalshiPosition {
    ticker: String,
    position_fp: String,
    market_exposure_dollars: String,
}

#[derive(Debug, Deserialize)]
struct KalshiMarket {
    ticker: String,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    yes_sub_title: Option<String>,
    #[serde(default)]
    no_sub_title: Option<String>,
    #[serde(default)]
    yes_bid_dollars: Option<String>,
    #[serde(default)]
    yes_ask_dollars: Option<String>,
    #[serde(default)]
    no_bid_dollars: Option<String>,
    #[serde(default)]
    no_ask_dollars: Option<String>,
    #[serde(default)]
    last_price_dollars: Option<String>,
    #[serde(default)]
    notional_value_dollars: Option<String>,
    #[serde(default)]
    settlement_value_dollars: Option<String>,
}

impl KalshiMarket {
    /// Per-contract value for `side`: the settlement value once determined, otherwise the
    /// bid/ask midpoint when both sides are quoted, otherwise the last trade price.
    fn mark_price(&self, side: Side) -> Result<Decimal> {
        let notional =
            parse_optional_decimal(&self.notional_value_dollars)?.unwrap_or(Decimal::ONE);
        // Settlement and last-trade prices are quoted for YES.
        let from_yes = |yes: Decimal| match side {
            Side::Yes => yes,
            Side::No => notional - yes,
        };

        if let Some(settlement) = parse_optional_decimal(&self.settlement_value_dollars)? {
            return Ok(from_yes(settlement));
        }

        let (bid, ask) = match side {
            Side::Yes => (&self.yes_bid_dollars, &self.yes_ask_dollars),
            Side::No => (&self.no_bid_dollars, &self.no_ask_dollars),
        };
        if let (Some(bid), Some(ask)) = (parse_optional_decimal(bid)?, parse_optional_decimal(ask)?)
        {
            if bid > Decimal::ZERO && ask < notional && bid <= ask {
                return Ok((bid + ask) / Decimal::TWO);
            }
        }

        parse_optional_decimal(&self.last_price_dollars)?
            .map(from_yes)
            .with_context(|| format!("No price available for Kalshi market {}", self.ticker))
    }

    fn display_name(&self, side: Side) -> String {
        let non_empty = |value: &Option<String>| {
            value
                .as_deref()
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(str::to_string)
        };
        let title = non_empty(&self.title).unwrap_or_else(|| self.ticker.clone());
        let (sub_title, side_label) = match side {
            Side::Yes => (non_empty(&self.yes_sub_title), "Yes"),
            Side::No => (non_empty(&self.no_sub_title), "No"),
        };
        match sub_title {
            Some(sub_title) if sub_title != title => format!("{title} ({side_label}: {sub_title})"),
            _ => format!("{title} ({side_label})"),
        }
    }
}

#[async_trait::async_trait]
impl Synchronizer for KalshiSynchronizer {
    fn name(&self) -> &str {
        "kalshi"
    }

    async fn sync(&self, connection: &mut Connection, storage: &dyn Storage) -> Result<SyncResult> {
        self.sync_internal(connection, storage).await
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/sync/synchronizers/kalshi_tests.rs"]
mod kalshi_tests;
