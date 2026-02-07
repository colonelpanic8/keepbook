//! CoinCap crypto price provider.
//!
//! Uses CoinCap's public API for historical daily prices.
//! Docs: https://docs.coincap.io/

use std::collections::HashMap;

use anyhow::{anyhow, Context, Result};
use chrono::{Duration, NaiveDate, TimeZone, Utc};
use reqwest::Client;
use secrecy::ExposeSecret;
use serde::Deserialize;
use tokio::sync::Mutex;

use crate::credentials::CredentialStore;
use crate::market_data::{AssetId, CryptoPriceSource, PriceKind, PricePoint};
use crate::models::Asset;

const COINCAP_API_BASE: &str = "https://api.coincap.io/v2";

#[derive(Debug, Default, Deserialize)]
pub struct CoinCapConfig {
    /// Optional symbol -> CoinCap asset ID overrides.
    #[serde(default)]
    pub symbol_map: HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
struct AssetSearchResponse {
    data: Vec<CoinCapAsset>,
}

#[derive(Debug, Deserialize)]
struct CoinCapAsset {
    id: String,
    symbol: String,
}

#[derive(Debug, Deserialize)]
struct HistoryResponse {
    data: Vec<HistoryPoint>,
}

#[derive(Debug, Deserialize)]
struct HistoryPoint {
    #[serde(rename = "priceUsd")]
    price_usd: serde_json::Value,
    time: i64,
}

/// CoinCap crypto price provider.
pub struct CoinCapPriceSource {
    client: Client,
    api_key: Option<String>,
    custom_mappings: HashMap<String, String>,
    asset_id_cache: Mutex<HashMap<String, String>>,
}

impl CoinCapPriceSource {
    /// Create a new CoinCap provider without an API key.
    pub fn new() -> Self {
        Self {
            client: Client::new(),
            api_key: None,
            custom_mappings: HashMap::new(),
            asset_id_cache: Mutex::new(HashMap::new()),
        }
    }

    /// Create a new CoinCap provider with an API key.
    pub fn with_api_key(mut self, api_key: impl Into<String>) -> Self {
        self.api_key = Some(api_key.into());
        self
    }

    /// Apply a CoinCap config (symbol map).
    pub fn with_config(mut self, config: CoinCapConfig) -> Self {
        if !config.symbol_map.is_empty() {
            self.custom_mappings = config
                .symbol_map
                .into_iter()
                .map(|(k, v)| (k.to_uppercase(), v))
                .collect();
        }
        self
    }

    /// Create from credentials (api_key or password).
    pub async fn from_credentials(store: &dyn CredentialStore) -> Result<Self> {
        let api_key = store
            .get("api_key")
            .await?
            .or(store.get("password").await?)
            .ok_or_else(|| anyhow!("missing api_key in credential store"))?;

        Ok(Self::new().with_api_key(api_key.expose_secret()))
    }

    async fn send_request(&self, url: &str) -> Result<reqwest::Response> {
        let mut request = self.client.get(url).header("Accept", "application/json");

        if let Some(key) = &self.api_key {
            request = request.bearer_auth(key);
        }

        let response = request.send().await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow!("CoinCap API error: {status} - {body}"));
        }

        Ok(response)
    }

    async fn resolve_asset_id(&self, symbol: &str) -> Result<Option<String>> {
        let symbol_upper = symbol.to_uppercase();

        if let Some(id) = self.custom_mappings.get(&symbol_upper) {
            return Ok(Some(id.clone()));
        }

        if let Some(id) = self.asset_id_cache.lock().await.get(&symbol_upper) {
            return Ok(Some(id.clone()));
        }

        let url = format!("{COINCAP_API_BASE}/assets?search={symbol_upper}");
        let response = self.send_request(&url).await?;
        let data: AssetSearchResponse = response
            .json()
            .await
            .context("Failed to parse CoinCap asset search response")?;

        let mut matches = data
            .data
            .into_iter()
            .filter(|asset| asset.symbol.eq_ignore_ascii_case(&symbol_upper))
            .collect::<Vec<_>>();

        if matches.is_empty() {
            return Ok(None);
        }

        if matches.len() > 1 {
            let ids: Vec<String> = matches.iter().map(|a| a.id.clone()).collect();
            return Err(anyhow!(
                "Multiple CoinCap assets match symbol {symbol_upper}: {ids:?}. Add a symbol_map override."
            ));
        }

        let asset_id = matches.pop().unwrap().id;
        self.asset_id_cache
            .lock()
            .await
            .insert(symbol_upper, asset_id.clone());

        Ok(Some(asset_id))
    }

    async fn fetch_history(&self, asset_id: &str, date: NaiveDate) -> Result<Option<HistoryPoint>> {
        let start = Utc
            .from_utc_datetime(&date.and_hms_opt(0, 0, 0).unwrap())
            .timestamp_millis();
        let end = Utc
            .from_utc_datetime(&(date + Duration::days(1)).and_hms_opt(0, 0, 0).unwrap())
            .timestamp_millis();

        let url = format!(
            "{COINCAP_API_BASE}/assets/{asset_id}/history?interval=d1&start={start}&end={end}"
        );
        let response = self.send_request(&url).await?;
        let data: HistoryResponse = response
            .json()
            .await
            .context("Failed to parse CoinCap history response")?;

        Ok(data.data.into_iter().last())
    }
}

impl Default for CoinCapPriceSource {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl CryptoPriceSource for CoinCapPriceSource {
    async fn fetch_close(
        &self,
        asset: &Asset,
        asset_id: &AssetId,
        date: NaiveDate,
    ) -> Result<Option<PricePoint>> {
        let symbol = match asset {
            Asset::Crypto { symbol, .. } => symbol,
            _ => return Ok(None),
        };

        let Some(coincap_id) = self.resolve_asset_id(symbol).await? else {
            return Ok(None);
        };

        let Some(point) = self.fetch_history(&coincap_id, date).await? else {
            return Ok(None);
        };

        let price = match point.price_usd {
            serde_json::Value::String(s) => s,
            serde_json::Value::Number(n) => n.to_string(),
            _ => return Ok(None),
        };

        let as_of_date = Utc
            .timestamp_millis_opt(point.time)
            .single()
            .unwrap_or_else(|| Utc::now())
            .date_naive();

        Ok(Some(PricePoint {
            asset_id: asset_id.clone(),
            as_of_date,
            timestamp: Utc::now(),
            price,
            quote_currency: "USD".to_string(),
            kind: PriceKind::Close,
            source: self.name().to_string(),
        }))
    }

    fn name(&self) -> &str {
        "coincap"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_history_response() {
        let json = r#"{
            "data": [
                {
                    "priceUsd": "42685.1234",
                    "time": 1704067200000
                }
            ],
            "timestamp": 1704153600000
        }"#;

        let response: HistoryResponse = serde_json::from_str(json).expect("parse history");
        assert_eq!(response.data.len(), 1);
        assert_eq!(response.data[0].price_usd.as_str().unwrap(), "42685.1234");
    }

    #[test]
    fn parse_asset_search_response() {
        let json = r#"{
            "data": [
                { "id": "bitcoin", "symbol": "BTC" },
                { "id": "bitcash", "symbol": "BTC" }
            ],
            "timestamp": 1704153600000
        }"#;

        let response: AssetSearchResponse = serde_json::from_str(json).expect("parse assets");
        assert_eq!(response.data.len(), 2);
        assert_eq!(response.data[0].id, "bitcoin");
    }
}
