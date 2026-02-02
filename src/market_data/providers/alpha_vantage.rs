//! Alpha Vantage equity price provider.
//!
//! Uses the TIME_SERIES_DAILY endpoint to fetch historical daily close prices.
//! Note: Free tier is limited to 25 requests/day.

use anyhow::{anyhow, Result};
use chrono::{NaiveDate, Utc};
use reqwest::Client;
use secrecy::ExposeSecret;
use serde::Deserialize;
use std::collections::HashMap;

use crate::credentials::CredentialStore;
use crate::market_data::{AssetId, EquityPriceSource, PriceKind, PricePoint};
use crate::models::Asset;

const BASE_URL: &str = "https://www.alphavantage.co/query";

/// Alpha Vantage provider for equity prices.
///
/// Fetches daily time series data using the TIME_SERIES_DAILY endpoint.
/// Free tier is limited to 25 requests per day.
pub struct AlphaVantagePriceSource {
    api_key: String,
    client: Client,
}

impl AlphaVantagePriceSource {
    /// Create a new Alpha Vantage price source with the given API key.
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            client: Client::new(),
        }
    }

    /// Create a new Alpha Vantage price source with a custom reqwest client.
    pub fn with_client(api_key: impl Into<String>, client: Client) -> Self {
        Self {
            api_key: api_key.into(),
            client,
        }
    }

    /// Create a new Alpha Vantage price source from a credential store.
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

    /// Format the symbol for Alpha Vantage API.
    ///
    /// Alpha Vantage typically uses plain ticker symbols for US equities.
    /// For international exchanges, it may require exchange suffix (e.g., "BMW.DEX" for German stocks).
    fn format_symbol(&self, ticker: &str, exchange: Option<&str>) -> String {
        match exchange {
            Some(ex) => {
                // Map common exchange codes to Alpha Vantage suffixes
                let suffix = match ex.to_uppercase().as_str() {
                    // US exchanges - no suffix needed
                    "NYSE" | "XNYS" | "NASDAQ" | "XNAS" | "AMEX" | "ARCX" => {
                        return ticker.to_uppercase()
                    }
                    // German exchanges
                    "XETR" | "XFRA" | "FRA" => ".DEX",
                    // London Stock Exchange
                    "XLON" | "LSE" => ".LON",
                    // Toronto Stock Exchange
                    "XTSE" | "TSX" => ".TRT",
                    // Tokyo Stock Exchange
                    "XTKS" | "TSE" => ".TYO",
                    // Australian Securities Exchange
                    "XASX" | "ASX" => ".AX",
                    // Paris Stock Exchange
                    "XPAR" => ".PAR",
                    // Default: try the exchange code as suffix
                    _ => return format!("{}.{}", ticker.to_uppercase(), ex.to_uppercase()),
                };
                format!("{}{}", ticker.to_uppercase(), suffix)
            }
            None => ticker.to_uppercase(),
        }
    }

    /// Parse the API response into a PricePoint for the requested date.
    fn parse_response(
        &self,
        response: &TimeSeriesResponse,
        asset_id: &AssetId,
        date: NaiveDate,
    ) -> Option<PricePoint> {
        let date_str = date.format("%Y-%m-%d").to_string();
        let daily_data = response.time_series.get(&date_str)?;

        Some(PricePoint {
            asset_id: asset_id.clone(),
            as_of_date: date,
            timestamp: Utc::now(),
            price: daily_data.close.clone(),
            quote_currency: "USD".to_string(), // Alpha Vantage returns prices in the asset's trading currency
            kind: PriceKind::Close,
            source: self.name().to_string(),
        })
    }
}

#[async_trait::async_trait]
impl EquityPriceSource for AlphaVantagePriceSource {
    async fn fetch_close(
        &self,
        asset: &Asset,
        asset_id: &AssetId,
        date: NaiveDate,
    ) -> Result<Option<PricePoint>> {
        let (ticker, exchange) = match asset {
            Asset::Equity { ticker, exchange } => (ticker, exchange.as_deref()),
            _ => return Ok(None),
        };

        let symbol = self.format_symbol(ticker, exchange);

        let response = self
            .client
            .get(BASE_URL)
            .query(&[
                ("function", "TIME_SERIES_DAILY"),
                ("symbol", &symbol),
                ("outputsize", "compact"), // compact = last 100 data points
                ("apikey", &self.api_key),
            ])
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(anyhow!(
                "Alpha Vantage API request failed with status: {}",
                response.status()
            ));
        }

        let text = response.text().await?;

        // Check for API error responses
        if let Ok(error) = serde_json::from_str::<ErrorResponse>(&text) {
            if error.error_message.is_some() || error.note.is_some() {
                // Rate limit or invalid API key - these are errors
                if let Some(msg) = error.error_message {
                    return Err(anyhow!("Alpha Vantage API error: {msg}"));
                }
                if let Some(note) = error.note {
                    // Rate limit note
                    return Err(anyhow!("Alpha Vantage rate limit: {note}"));
                }
            }
            if error.information.is_some() {
                // Demo/info message - likely invalid symbol, return None
                return Ok(None);
            }
        }

        // Parse successful response
        let time_series: TimeSeriesResponse = match serde_json::from_str(&text) {
            Ok(ts) => ts,
            Err(_) => {
                // Could not parse as time series - symbol not found or other issue
                return Ok(None);
            }
        };

        Ok(self.parse_response(&time_series, asset_id, date))
    }

    fn name(&self) -> &str {
        "alpha_vantage"
    }
}

/// Response structure for TIME_SERIES_DAILY endpoint.
#[derive(Debug, Deserialize)]
struct TimeSeriesResponse {
    #[serde(rename = "Meta Data")]
    #[allow(dead_code)]
    meta_data: MetaData,

    #[serde(rename = "Time Series (Daily)")]
    time_series: HashMap<String, DailyData>,
}

#[derive(Debug, Deserialize)]
struct MetaData {
    #[serde(rename = "1. Information")]
    #[allow(dead_code)]
    information: String,

    #[serde(rename = "2. Symbol")]
    #[allow(dead_code)]
    symbol: String,

    #[serde(rename = "3. Last Refreshed")]
    #[allow(dead_code)]
    last_refreshed: String,

    #[serde(rename = "4. Output Size")]
    #[allow(dead_code)]
    output_size: String,

    #[serde(rename = "5. Time Zone")]
    #[allow(dead_code)]
    time_zone: String,
}

#[derive(Debug, Deserialize)]
struct DailyData {
    #[serde(rename = "1. open")]
    #[allow(dead_code)]
    open: String,

    #[serde(rename = "2. high")]
    #[allow(dead_code)]
    high: String,

    #[serde(rename = "3. low")]
    #[allow(dead_code)]
    low: String,

    #[serde(rename = "4. close")]
    close: String,

    #[serde(rename = "5. volume")]
    #[allow(dead_code)]
    volume: String,
}

/// Error response from Alpha Vantage API.
#[derive(Debug, Deserialize)]
struct ErrorResponse {
    #[serde(rename = "Error Message")]
    error_message: Option<String>,

    #[serde(rename = "Note")]
    note: Option<String>,

    #[serde(rename = "Information")]
    information: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_RESPONSE: &str = r#"{
        "Meta Data": {
            "1. Information": "Daily Prices (open, high, low, close) and Volumes",
            "2. Symbol": "AAPL",
            "3. Last Refreshed": "2024-01-15",
            "4. Output Size": "Compact",
            "5. Time Zone": "US/Eastern"
        },
        "Time Series (Daily)": {
            "2024-01-15": {
                "1. open": "186.0600",
                "2. high": "187.4700",
                "3. low": "183.6200",
                "4. close": "185.9200",
                "5. volume": "65076672"
            },
            "2024-01-12": {
                "1. open": "186.0900",
                "2. high": "186.7400",
                "3. low": "185.1900",
                "4. close": "185.5900",
                "5. volume": "40477783"
            }
        }
    }"#;

    const ERROR_RESPONSE_RATE_LIMIT: &str = r#"{
        "Note": "Thank you for using Alpha Vantage! Our standard API rate limit is 25 requests per day."
    }"#;

    const ERROR_RESPONSE_INVALID_KEY: &str = r#"{
        "Error Message": "Invalid API call. Please retry or visit the documentation."
    }"#;

    const INFO_RESPONSE: &str = r#"{
        "Information": "Please consider upgrading to our premium service for more API calls."
    }"#;

    #[test]
    fn test_parse_time_series_response() {
        let response: TimeSeriesResponse = serde_json::from_str(SAMPLE_RESPONSE).unwrap();

        assert_eq!(response.meta_data.symbol, "AAPL");
        assert_eq!(response.time_series.len(), 2);

        let jan_15 = response.time_series.get("2024-01-15").unwrap();
        assert_eq!(jan_15.close, "185.9200");
        assert_eq!(jan_15.open, "186.0600");
        assert_eq!(jan_15.high, "187.4700");
        assert_eq!(jan_15.low, "183.6200");
        assert_eq!(jan_15.volume, "65076672");
    }

    #[test]
    fn test_parse_price_point() {
        let provider = AlphaVantagePriceSource::new("test_key");
        let response: TimeSeriesResponse = serde_json::from_str(SAMPLE_RESPONSE).unwrap();
        let asset_id = AssetId::from("test_asset_id");
        let date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();

        let price_point = provider.parse_response(&response, &asset_id, date).unwrap();

        assert_eq!(price_point.price, "185.9200");
        assert_eq!(price_point.quote_currency, "USD");
        assert_eq!(price_point.kind, PriceKind::Close);
        assert_eq!(price_point.source, "alpha_vantage");
        assert_eq!(price_point.as_of_date, date);
    }

    #[test]
    fn test_parse_price_point_missing_date() {
        let provider = AlphaVantagePriceSource::new("test_key");
        let response: TimeSeriesResponse = serde_json::from_str(SAMPLE_RESPONSE).unwrap();
        let asset_id = AssetId::from("test_asset_id");
        let date = NaiveDate::from_ymd_opt(2024, 1, 14).unwrap(); // Not in response

        let price_point = provider.parse_response(&response, &asset_id, date);

        assert!(price_point.is_none());
    }

    #[test]
    fn test_parse_error_response_rate_limit() {
        let error: ErrorResponse = serde_json::from_str(ERROR_RESPONSE_RATE_LIMIT).unwrap();
        assert!(error.note.is_some());
        assert!(error.note.unwrap().contains("25 requests per day"));
    }

    #[test]
    fn test_parse_error_response_invalid_key() {
        let error: ErrorResponse = serde_json::from_str(ERROR_RESPONSE_INVALID_KEY).unwrap();
        assert!(error.error_message.is_some());
        assert!(error.error_message.unwrap().contains("Invalid API call"));
    }

    #[test]
    fn test_parse_info_response() {
        let error: ErrorResponse = serde_json::from_str(INFO_RESPONSE).unwrap();
        assert!(error.information.is_some());
    }

    #[test]
    fn test_format_symbol_us_no_exchange() {
        let provider = AlphaVantagePriceSource::new("test_key");
        assert_eq!(provider.format_symbol("aapl", None), "AAPL");
        assert_eq!(provider.format_symbol("MSFT", None), "MSFT");
    }

    #[test]
    fn test_format_symbol_us_exchanges() {
        let provider = AlphaVantagePriceSource::new("test_key");
        assert_eq!(provider.format_symbol("aapl", Some("NYSE")), "AAPL");
        assert_eq!(provider.format_symbol("msft", Some("NASDAQ")), "MSFT");
        assert_eq!(provider.format_symbol("goog", Some("XNAS")), "GOOG");
        assert_eq!(provider.format_symbol("jpm", Some("XNYS")), "JPM");
    }

    #[test]
    fn test_format_symbol_international_exchanges() {
        let provider = AlphaVantagePriceSource::new("test_key");
        assert_eq!(provider.format_symbol("BMW", Some("XETR")), "BMW.DEX");
        assert_eq!(provider.format_symbol("BARC", Some("XLON")), "BARC.LON");
        assert_eq!(provider.format_symbol("RY", Some("XTSE")), "RY.TRT");
        assert_eq!(provider.format_symbol("7203", Some("XTKS")), "7203.TYO");
        assert_eq!(provider.format_symbol("BHP", Some("XASX")), "BHP.AX");
        assert_eq!(provider.format_symbol("OR", Some("XPAR")), "OR.PAR");
    }

    #[test]
    fn test_format_symbol_unknown_exchange() {
        let provider = AlphaVantagePriceSource::new("test_key");
        assert_eq!(provider.format_symbol("ticker", Some("UNKNOWN")), "TICKER.UNKNOWN");
    }

    #[test]
    fn test_provider_name() {
        let provider = AlphaVantagePriceSource::new("test_key");
        assert_eq!(provider.name(), "alpha_vantage");
    }
}
