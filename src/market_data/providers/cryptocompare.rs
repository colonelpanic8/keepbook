//! CryptoCompare crypto price provider.
//!
//! Uses CryptoCompare's histoday endpoint for historical daily prices.
//! Docs: https://min-api.cryptocompare.com/

use std::collections::HashMap;

use anyhow::{anyhow, Context, Result};
use chrono::{Duration, NaiveDate, TimeZone, Utc};
use reqwest::Client;
use secrecy::ExposeSecret;
use serde::Deserialize;

use crate::credentials::CredentialStore;
use crate::market_data::{AssetId, CryptoPriceSource, PriceKind, PricePoint};
use crate::models::Asset;

const CRYPTOCOMPARE_API_BASE: &str = "https://min-api.cryptocompare.com";

#[derive(Debug, Default, Deserialize)]
pub struct CryptoCompareConfig {
    /// Optional symbol -> CryptoCompare symbol overrides.
    #[serde(default)]
    pub symbol_map: HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
struct HistoryResponse {
    #[serde(rename = "Response")]
    response: String,
    #[serde(rename = "Message")]
    message: Option<String>,
    #[serde(rename = "Data")]
    data: Option<HistoryContainer>,
}

#[derive(Debug, Deserialize)]
struct HistoryContainer {
    #[serde(rename = "Data")]
    data: Vec<HistoryPoint>,
}

#[derive(Debug, Deserialize)]
struct HistoryPoint {
    #[allow(dead_code)]
    time: i64,
    close: Option<f64>,
}

/// CryptoCompare crypto price provider.
pub struct CryptoComparePriceSource {
    client: Client,
    api_key: Option<String>,
    custom_mappings: HashMap<String, String>,
}

impl CryptoComparePriceSource {
    /// Create a new CryptoCompare provider without an API key.
    pub fn new() -> Self {
        Self {
            client: Client::new(),
            api_key: None,
            custom_mappings: HashMap::new(),
        }
    }

    /// Create a new CryptoCompare provider with an API key.
    pub fn with_api_key(mut self, api_key: impl Into<String>) -> Self {
        self.api_key = Some(api_key.into());
        self
    }

    /// Apply a CryptoCompare config (symbol map).
    pub fn with_config(mut self, config: CryptoCompareConfig) -> Self {
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

    fn map_symbol(&self, symbol: &str) -> String {
        let symbol_upper = symbol.to_uppercase();
        self.custom_mappings
            .get(&symbol_upper)
            .cloned()
            .unwrap_or(symbol_upper)
    }

    async fn fetch_histoday(&self, symbol: &str, date: NaiveDate) -> Result<Option<f64>> {
        let to_ts = Utc
            .from_utc_datetime(&(date + Duration::days(1)).and_hms_opt(0, 0, 0).unwrap())
            .timestamp();

        let url = format!(
            "{CRYPTOCOMPARE_API_BASE}/data/v2/histoday?fsym={symbol}&tsym=USD&limit=1&toTs={to_ts}"
        );

        let mut request = self.client.get(&url).header("Accept", "application/json");
        if let Some(key) = &self.api_key {
            request = request.header("authorization", format!("Apikey {key}"));
        }

        let response = request.send().await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow!("CryptoCompare API error: {status} - {body}"));
        }

        let data: HistoryResponse = response
            .json()
            .await
            .context("Failed to parse CryptoCompare response")?;

        if data.response.to_lowercase() != "success" {
            let message = data.message.unwrap_or_else(|| "unknown error".to_string());
            return Err(anyhow!("CryptoCompare API error: {message}"));
        }

        let points = data.data.map(|d| d.data).unwrap_or_default();
        let Some(point) = points.into_iter().last() else {
            return Ok(None);
        };

        Ok(point.close)
    }
}

impl Default for CryptoComparePriceSource {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl CryptoPriceSource for CryptoComparePriceSource {
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

        let mapped_symbol = self.map_symbol(symbol);
        let price = self.fetch_histoday(&mapped_symbol, date).await?;
        let Some(price) = price else {
            return Ok(None);
        };
        if price <= 0.0 {
            return Ok(None);
        }

        Ok(Some(PricePoint {
            asset_id: asset_id.clone(),
            as_of_date: date,
            timestamp: Utc::now(),
            price: price.to_string(),
            quote_currency: "USD".to_string(),
            kind: PriceKind::Close,
            source: self.name().to_string(),
        }))
    }

    fn name(&self) -> &str {
        "cryptocompare"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_histoday_response() {
        let json = r#"{
            "Response": "Success",
            "Data": {
                "Data": [
                    { "time": 1704067200, "close": 42850.12 },
                    { "time": 1704153600, "close": 43500.34 }
                ]
            }
        }"#;

        let response: HistoryResponse = serde_json::from_str(json).expect("parse");
        assert_eq!(response.response, "Success");
        let points = response.data.unwrap().data;
        assert_eq!(points.len(), 2);
        assert_eq!(points[1].close, Some(43500.34));
    }
}
