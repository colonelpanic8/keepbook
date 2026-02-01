//! Marketstack equity price provider implementation.
//!
//! Marketstack provides EOD (end-of-day) stock market data.
//! Note: Free tier has very low request limits, cache aggressively.

use anyhow::{anyhow, Result};
use chrono::{NaiveDate, Utc};
use reqwest::Client;
use secrecy::ExposeSecret;
use serde::Deserialize;

use crate::credentials::CredentialStore;
use crate::market_data::{AssetId, EquityPriceSource, PriceKind, PricePoint};
use crate::models::Asset;

const MARKETSTACK_BASE_URL: &str = "http://api.marketstack.com/v1";

/// Marketstack API response for EOD endpoint.
#[derive(Debug, Deserialize)]
struct EodResponse {
    data: Vec<EodData>,
}

/// Individual EOD data point from Marketstack.
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct EodData {
    close: f64,
    date: String,
    symbol: String,
    exchange: Option<String>,
}

/// Marketstack equity price provider.
///
/// Implements `EquityPriceSource` for fetching daily closing prices
/// from the Marketstack EOD API.
pub struct MarketstackPriceSource {
    api_key: String,
    client: Client,
}

impl MarketstackPriceSource {
    /// Creates a new Marketstack price source with the given API key.
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            client: Client::new(),
        }
    }

    /// Creates a new Marketstack price source with a custom HTTP client.
    pub fn with_client(api_key: impl Into<String>, client: Client) -> Self {
        Self {
            api_key: api_key.into(),
            client,
        }
    }

    /// Create a new Marketstack price source from a credential store.
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

    /// Formats the symbol for Marketstack API.
    ///
    /// Marketstack uses plain ticker symbols for US exchanges.
    /// For international exchanges, it uses format: TICKER.EXCHANGE
    fn format_symbol(ticker: &str, exchange: Option<&str>) -> String {
        match exchange {
            Some(exch) => {
                // Map common exchange codes to Marketstack format
                let ms_exchange = Self::map_exchange_code(exch);
                if ms_exchange.is_empty() || ms_exchange == "US" {
                    // US exchanges don't need suffix
                    ticker.to_uppercase()
                } else {
                    format!("{}.{}", ticker.to_uppercase(), ms_exchange)
                }
            }
            None => ticker.to_uppercase(),
        }
    }

    /// Maps exchange codes to Marketstack's format.
    fn map_exchange_code(exchange: &str) -> &str {
        match exchange.to_uppercase().as_str() {
            // US exchanges - no suffix needed
            "XNAS" | "NASDAQ" | "NAS" => "",
            "XNYS" | "NYSE" | "NYS" => "",
            "XASE" | "AMEX" | "ASE" => "",
            "ARCX" | "ARCA" => "",
            // International exchanges
            "XLON" | "LSE" | "LON" => "XLON",
            "XPAR" | "PAR" => "XPAR",
            "XFRA" | "FRA" => "XFRA",
            "XTSE" | "TSE" | "TSX" => "XTSE",
            "XASX" | "ASX" => "XASX",
            "XHKG" | "HKG" | "HKEX" => "XHKG",
            "XTKS" | "TYO" | "TSE_JP" => "XTKS",
            // Default: use as-is if not recognized
            other => {
                // Return the original if it starts with X (likely MIC code)
                if other.starts_with('X') {
                    return exchange;
                }
                ""
            }
        }
    }

    /// Parses the date string from Marketstack response.
    fn parse_date(date_str: &str) -> Result<NaiveDate> {
        // Marketstack returns dates in ISO 8601 format: "2024-01-15T00:00:00+0000"
        let date_part = date_str.split('T').next().unwrap_or(date_str);
        NaiveDate::parse_from_str(date_part, "%Y-%m-%d")
            .map_err(|e| anyhow!("Failed to parse date '{}': {}", date_str, e))
    }
}

#[async_trait::async_trait]
impl EquityPriceSource for MarketstackPriceSource {
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

        let symbol = Self::format_symbol(ticker, exchange);
        let date_str = date.format("%Y-%m-%d").to_string();

        let url = format!(
            "{}/eod?access_key={}&symbols={}&date_from={}&date_to={}",
            MARKETSTACK_BASE_URL, self.api_key, symbol, date_str, date_str
        );

        let response = self.client.get(&url).send().await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            // Return None for 404 or similar "not found" responses
            if status.as_u16() == 404 {
                return Ok(None);
            }
            return Err(anyhow!(
                "Marketstack API error: {} - {}",
                status,
                body
            ));
        }

        let eod_response: EodResponse = response.json().await?;

        // Find the data point for the requested date
        let data_point = eod_response.data.into_iter().find(|d| {
            Self::parse_date(&d.date)
                .map(|parsed| parsed == date)
                .unwrap_or(false)
        });

        match data_point {
            Some(data) => {
                let parsed_date = Self::parse_date(&data.date)?;
                Ok(Some(PricePoint {
                    asset_id: asset_id.clone(),
                    as_of_date: parsed_date,
                    timestamp: Utc::now(),
                    price: data.close.to_string(),
                    quote_currency: "USD".to_string(), // Marketstack returns USD for US stocks
                    kind: PriceKind::Close,
                    source: self.name().to_string(),
                }))
            }
            None => Ok(None),
        }
    }

    fn name(&self) -> &str {
        "marketstack"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_symbol_no_exchange() {
        assert_eq!(MarketstackPriceSource::format_symbol("AAPL", None), "AAPL");
        assert_eq!(MarketstackPriceSource::format_symbol("aapl", None), "AAPL");
    }

    #[test]
    fn test_format_symbol_us_exchanges() {
        assert_eq!(
            MarketstackPriceSource::format_symbol("AAPL", Some("NASDAQ")),
            "AAPL"
        );
        assert_eq!(
            MarketstackPriceSource::format_symbol("AAPL", Some("XNAS")),
            "AAPL"
        );
        assert_eq!(
            MarketstackPriceSource::format_symbol("IBM", Some("NYSE")),
            "IBM"
        );
        assert_eq!(
            MarketstackPriceSource::format_symbol("IBM", Some("XNYS")),
            "IBM"
        );
    }

    #[test]
    fn test_format_symbol_international_exchanges() {
        assert_eq!(
            MarketstackPriceSource::format_symbol("VOD", Some("XLON")),
            "VOD.XLON"
        );
        assert_eq!(
            MarketstackPriceSource::format_symbol("VOD", Some("LSE")),
            "VOD.XLON"
        );
        assert_eq!(
            MarketstackPriceSource::format_symbol("SAP", Some("XFRA")),
            "SAP.XFRA"
        );
    }

    #[test]
    fn test_parse_date() {
        let date = MarketstackPriceSource::parse_date("2024-01-15T00:00:00+0000").unwrap();
        assert_eq!(date, NaiveDate::from_ymd_opt(2024, 1, 15).unwrap());

        let date = MarketstackPriceSource::parse_date("2024-01-15").unwrap();
        assert_eq!(date, NaiveDate::from_ymd_opt(2024, 1, 15).unwrap());
    }

    #[test]
    fn test_parse_eod_response() {
        let json = r#"{
            "pagination": {
                "limit": 100,
                "offset": 0,
                "count": 1,
                "total": 1
            },
            "data": [
                {
                    "open": 150.25,
                    "high": 152.50,
                    "low": 149.75,
                    "close": 151.30,
                    "volume": 45678900,
                    "adj_high": 152.50,
                    "adj_low": 149.75,
                    "adj_close": 151.30,
                    "adj_open": 150.25,
                    "adj_volume": 45678900,
                    "split_factor": 1.0,
                    "dividend": 0.0,
                    "symbol": "AAPL",
                    "exchange": "XNAS",
                    "date": "2024-01-15T00:00:00+0000"
                }
            ]
        }"#;

        let response: EodResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.data.len(), 1);
        assert_eq!(response.data[0].close, 151.30);
        assert_eq!(response.data[0].symbol, "AAPL");
        assert_eq!(response.data[0].date, "2024-01-15T00:00:00+0000");
    }

    #[test]
    fn test_parse_empty_response() {
        let json = r#"{
            "pagination": {
                "limit": 100,
                "offset": 0,
                "count": 0,
                "total": 0
            },
            "data": []
        }"#;

        let response: EodResponse = serde_json::from_str(json).unwrap();
        assert!(response.data.is_empty());
    }

    #[test]
    fn test_provider_name() {
        let provider = MarketstackPriceSource::new("test_key");
        assert_eq!(provider.name(), "marketstack");
    }

    #[tokio::test]
    async fn test_non_equity_asset_returns_none() {
        let provider = MarketstackPriceSource::new("test_key");
        let asset = Asset::Crypto {
            symbol: "BTC".to_string(),
            network: None,
        };
        let asset_id = AssetId::from_asset(&asset);
        let date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();

        let result = provider.fetch_close(&asset, &asset_id, date).await.unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_exchange_code_mapping() {
        // US exchanges return empty string (no suffix needed)
        assert_eq!(MarketstackPriceSource::map_exchange_code("NASDAQ"), "");
        assert_eq!(MarketstackPriceSource::map_exchange_code("NYSE"), "");
        assert_eq!(MarketstackPriceSource::map_exchange_code("XNAS"), "");
        assert_eq!(MarketstackPriceSource::map_exchange_code("XNYS"), "");

        // International exchanges return MIC codes
        assert_eq!(MarketstackPriceSource::map_exchange_code("LSE"), "XLON");
        assert_eq!(MarketstackPriceSource::map_exchange_code("XLON"), "XLON");
        assert_eq!(MarketstackPriceSource::map_exchange_code("TSX"), "XTSE");
        assert_eq!(MarketstackPriceSource::map_exchange_code("ASX"), "XASX");
    }
}
