//! Twelve Data equity price provider.
//!
//! Implements the `EquityPriceSource` trait using Twelve Data's REST API for daily time series.
//! Free tier covers US equities; international symbols may be limited.

use anyhow::{anyhow, Context, Result};
use chrono::{NaiveDate, Utc};
use reqwest::Client;
use secrecy::ExposeSecret;
use serde::Deserialize;

use crate::credentials::CredentialStore;
use crate::market_data::{AssetId, EquityPriceSource, PriceKind, PricePoint};
use crate::models::Asset;

const BASE_URL: &str = "https://api.twelvedata.com";

/// Twelve Data equity price provider.
///
/// Uses the `/time_series` endpoint to fetch daily close prices.
pub struct TwelveDataPriceSource {
    api_key: String,
    client: Client,
}

impl TwelveDataPriceSource {
    /// Creates a new Twelve Data price source with the given API key.
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            client: Client::new(),
        }
    }

    /// Creates a new price source with a custom reqwest client.
    pub fn with_client(api_key: impl Into<String>, client: Client) -> Self {
        Self {
            api_key: api_key.into(),
            client,
        }
    }

    /// Create a new Twelve Data price source from a credential store.
    ///
    /// Expects the store to have an "api_key" field (or "password" for simple pass entries).
    pub async fn from_credentials(store: &dyn CredentialStore) -> Result<Self> {
        let api_key = store
            .get("api_key")
            .await?
            .or(store.get("password").await?)
            .ok_or_else(|| anyhow!("missing api_key in credential store"))?;
        Ok(Self::new(api_key.expose_secret()))
    }

    /// Builds the symbol string for Twelve Data API.
    ///
    /// Twelve Data uses plain ticker symbols for US equities.
    /// For international equities, the exchange can be appended as `TICKER:EXCHANGE`.
    fn build_symbol(ticker: &str, exchange: Option<&str>) -> String {
        match exchange {
            Some(ex) => format!("{}:{}", ticker.to_uppercase(), ex.to_uppercase()),
            None => ticker.to_uppercase(),
        }
    }

    /// Fetches time series data for a symbol on a specific date.
    async fn fetch_time_series(
        &self,
        symbol: &str,
        date: NaiveDate,
    ) -> Result<Option<TimeSeriesResponse>> {
        // Twelve Data's time_series endpoint returns data up to and including end_date.
        // We request a small window around the target date to handle weekends/holidays.
        let start_date = date - chrono::Duration::days(7);

        let url = format!(
            "{}/time_series?symbol={}&interval=1day&start_date={}&end_date={}&apikey={}",
            BASE_URL,
            symbol,
            start_date.format("%Y-%m-%d"),
            date.format("%Y-%m-%d"),
            self.api_key
        );

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .context("Failed to send request to Twelve Data")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!(
                "Twelve Data API error: status={status}, body={body}"
            );
        }

        let body = response
            .text()
            .await
            .context("Failed to read response body")?;

        // Try to parse as error response first
        if let Ok(error) = serde_json::from_str::<ErrorResponse>(&body) {
            if error.status == "error" {
                // Check if it's a "no data" error vs a real error
                if error.code == Some(400) || error.message.contains("No data") {
                    return Ok(None);
                }
                anyhow::bail!("Twelve Data API error: {}", error.message);
            }
        }

        // Parse as successful response
        let data: TimeSeriesResponse =
            serde_json::from_str(&body).context("Failed to parse Twelve Data response")?;

        Ok(Some(data))
    }
}

#[async_trait::async_trait]
impl EquityPriceSource for TwelveDataPriceSource {
    async fn fetch_close(
        &self,
        asset: &Asset,
        asset_id: &AssetId,
        date: NaiveDate,
    ) -> Result<Option<PricePoint>> {
        let (ticker, exchange) = match asset {
            Asset::Equity { ticker, exchange } => (ticker.as_str(), exchange.as_deref()),
            _ => return Ok(None),
        };

        let symbol = Self::build_symbol(ticker, exchange);
        let response = self.fetch_time_series(&symbol, date).await?;

        let Some(data) = response else {
            return Ok(None);
        };

        // Find the exact date or the most recent date before it
        let target_str = date.format("%Y-%m-%d").to_string();

        let value = data
            .values
            .iter()
            .find(|v| v.datetime == target_str)
            .or_else(|| {
                // If exact date not found, find the most recent date <= target
                data.values.iter().find(|v| {
                    NaiveDate::parse_from_str(&v.datetime, "%Y-%m-%d")
                        .map(|d| d <= date)
                        .unwrap_or(false)
                })
            });

        let Some(value) = value else {
            return Ok(None);
        };

        let as_of_date = NaiveDate::parse_from_str(&value.datetime, "%Y-%m-%d")
            .context("Failed to parse date from Twelve Data response")?;

        // Twelve Data returns USD for US equities by default
        // The currency is typically determined by the exchange
        let quote_currency = data
            .meta
            .currency
            .clone()
            .unwrap_or_else(|| "USD".to_string());

        Ok(Some(PricePoint {
            asset_id: asset_id.clone(),
            as_of_date,
            timestamp: Utc::now(),
            price: value.close.clone(),
            quote_currency,
            kind: PriceKind::Close,
            source: self.name().to_string(),
        }))
    }

    async fn fetch_quote(
        &self,
        asset: &Asset,
        asset_id: &AssetId,
    ) -> Result<Option<PricePoint>> {
        let (ticker, exchange) = match asset {
            Asset::Equity { ticker, exchange } => (ticker.as_str(), exchange.as_deref()),
            _ => return Ok(None),
        };

        let symbol = Self::build_symbol(ticker, exchange);

        let url = format!(
            "{}/price?symbol={}&apikey={}",
            BASE_URL, symbol, self.api_key
        );

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .context("Failed to send quote request to Twelve Data")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Twelve Data quote API error: status={status}, body={body}");
        }

        let body = response
            .text()
            .await
            .context("Failed to read quote response body")?;

        // Try to parse as error response first
        if let Ok(error) = serde_json::from_str::<ErrorResponse>(&body) {
            if error.status == "error" {
                if error.code == Some(400) || error.message.contains("No data") {
                    return Ok(None);
                }
                anyhow::bail!("Twelve Data quote API error: {}", error.message);
            }
        }

        // Parse as price response
        let data: PriceResponse =
            serde_json::from_str(&body).context("Failed to parse Twelve Data quote response")?;

        let now = Utc::now();

        Ok(Some(PricePoint {
            asset_id: asset_id.clone(),
            as_of_date: now.date_naive(),
            timestamp: now,
            price: data.price,
            quote_currency: "USD".to_string(), // Quote endpoint doesn't return currency
            kind: PriceKind::Quote,
            source: self.name().to_string(),
        }))
    }

    fn name(&self) -> &str {
        "twelve_data"
    }
}

/// Twelve Data time series response.
#[derive(Debug, Deserialize)]
struct TimeSeriesResponse {
    meta: MetaData,
    values: Vec<TimeSeriesValue>,
}

/// Metadata from Twelve Data response.
#[derive(Debug, Deserialize)]
struct MetaData {
    #[allow(dead_code)]
    symbol: String,
    #[allow(dead_code)]
    interval: String,
    currency: Option<String>,
    #[allow(dead_code)]
    exchange: Option<String>,
    #[allow(dead_code)]
    #[serde(rename = "type")]
    asset_type: Option<String>,
}

/// Single time series data point.
#[derive(Debug, Deserialize)]
struct TimeSeriesValue {
    datetime: String,
    #[allow(dead_code)]
    open: String,
    #[allow(dead_code)]
    high: String,
    #[allow(dead_code)]
    low: String,
    close: String,
    #[allow(dead_code)]
    volume: Option<String>,
}

/// Error response from Twelve Data API.
#[derive(Debug, Deserialize)]
struct ErrorResponse {
    status: String,
    code: Option<i32>,
    message: String,
}

/// Real-time price response from Twelve Data `/price` endpoint.
#[derive(Debug, Deserialize)]
struct PriceResponse {
    price: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_RESPONSE: &str = r#"{
        "meta": {
            "symbol": "AAPL",
            "interval": "1day",
            "currency": "USD",
            "exchange_timezone": "America/New_York",
            "exchange": "NASDAQ",
            "mic_code": "XNAS",
            "type": "Common Stock"
        },
        "values": [
            {
                "datetime": "2024-01-15",
                "open": "186.06",
                "high": "187.00",
                "low": "183.62",
                "close": "185.92",
                "volume": "65076700"
            },
            {
                "datetime": "2024-01-12",
                "open": "186.06",
                "high": "186.74",
                "low": "185.19",
                "close": "185.59",
                "volume": "40477600"
            },
            {
                "datetime": "2024-01-11",
                "open": "186.54",
                "high": "187.05",
                "low": "185.15",
                "close": "185.18",
                "volume": "49128400"
            }
        ],
        "status": "ok"
    }"#;

    const SAMPLE_ERROR_RESPONSE: &str = r#"{
        "status": "error",
        "code": 400,
        "message": "No data is available for this query"
    }"#;

    const SAMPLE_RESPONSE_NO_CURRENCY: &str = r#"{
        "meta": {
            "symbol": "AAPL",
            "interval": "1day",
            "exchange_timezone": "America/New_York",
            "exchange": "NASDAQ",
            "type": "Common Stock"
        },
        "values": [
            {
                "datetime": "2024-01-15",
                "open": "186.06",
                "high": "187.00",
                "low": "183.62",
                "close": "185.92",
                "volume": "65076700"
            }
        ],
        "status": "ok"
    }"#;

    #[test]
    fn test_parse_time_series_response() {
        let response: TimeSeriesResponse = serde_json::from_str(SAMPLE_RESPONSE).unwrap();

        assert_eq!(response.meta.symbol, "AAPL");
        assert_eq!(response.meta.currency, Some("USD".to_string()));
        assert_eq!(response.meta.exchange, Some("NASDAQ".to_string()));
        assert_eq!(response.values.len(), 3);

        let first_value = &response.values[0];
        assert_eq!(first_value.datetime, "2024-01-15");
        assert_eq!(first_value.close, "185.92");
    }

    #[test]
    fn test_parse_error_response() {
        let response: ErrorResponse = serde_json::from_str(SAMPLE_ERROR_RESPONSE).unwrap();

        assert_eq!(response.status, "error");
        assert_eq!(response.code, Some(400));
        assert!(response.message.contains("No data"));
    }

    #[test]
    fn test_parse_response_no_currency() {
        let response: TimeSeriesResponse =
            serde_json::from_str(SAMPLE_RESPONSE_NO_CURRENCY).unwrap();

        assert_eq!(response.meta.symbol, "AAPL");
        assert!(response.meta.currency.is_none());
    }

    #[test]
    fn test_build_symbol_us_equity() {
        let symbol = TwelveDataPriceSource::build_symbol("aapl", None);
        assert_eq!(symbol, "AAPL");
    }

    #[test]
    fn test_build_symbol_with_exchange() {
        let symbol = TwelveDataPriceSource::build_symbol("aapl", Some("nasdaq"));
        assert_eq!(symbol, "AAPL:NASDAQ");
    }

    #[test]
    fn test_build_symbol_international() {
        let symbol = TwelveDataPriceSource::build_symbol("VOD", Some("LSE"));
        assert_eq!(symbol, "VOD:LSE");
    }

    #[test]
    fn test_find_exact_date_in_values() {
        let response: TimeSeriesResponse = serde_json::from_str(SAMPLE_RESPONSE).unwrap();
        let target_date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();
        let target_str = target_date.format("%Y-%m-%d").to_string();

        let value = response.values.iter().find(|v| v.datetime == target_str);

        assert!(value.is_some());
        assert_eq!(value.unwrap().close, "185.92");
    }

    #[test]
    fn test_find_previous_date_in_values() {
        let response: TimeSeriesResponse = serde_json::from_str(SAMPLE_RESPONSE).unwrap();
        // Saturday - market closed, should find Friday's data
        let target_date = NaiveDate::from_ymd_opt(2024, 1, 13).unwrap();
        let target_str = target_date.format("%Y-%m-%d").to_string();

        // First try exact match
        let exact = response.values.iter().find(|v| v.datetime == target_str);
        assert!(exact.is_none());

        // Then find most recent before target
        let previous = response.values.iter().find(|v| {
            NaiveDate::parse_from_str(&v.datetime, "%Y-%m-%d")
                .map(|d| d <= target_date)
                .unwrap_or(false)
        });

        assert!(previous.is_some());
        // Should find 2024-01-12 (Friday)
        assert_eq!(previous.unwrap().datetime, "2024-01-12");
        assert_eq!(previous.unwrap().close, "185.59");
    }

    #[test]
    fn test_provider_name() {
        let provider = TwelveDataPriceSource::new("test_key");
        assert_eq!(provider.name(), "twelve_data");
    }

    #[tokio::test]
    async fn test_non_equity_asset_returns_none() {
        let provider = TwelveDataPriceSource::new("test_key");
        let asset = Asset::crypto("BTC");
        let asset_id = AssetId::from_asset(&asset);
        let date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();

        let result = provider.fetch_close(&asset, &asset_id, date).await;

        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }
}
