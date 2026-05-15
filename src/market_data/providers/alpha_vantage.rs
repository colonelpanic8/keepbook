//! Alpha Vantage equity price provider.
//!
//! Uses the TIME_SERIES_DAILY endpoint to fetch historical daily close prices.
//! Note: Free tier is limited to 25 requests/day.

use anyhow::{anyhow, Result};
use chrono::{Duration, NaiveDate, Utc};
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

    async fn fetch_time_series(
        &self,
        ticker: &str,
        exchange: Option<&str>,
        start: NaiveDate,
    ) -> Result<TimeSeriesResponse> {
        let symbol = self.format_symbol(ticker, exchange);
        let outputsize = if start < (Utc::now().date_naive() - Duration::days(120)) {
            "full"
        } else {
            "compact"
        };

        let response = self
            .client
            .get(BASE_URL)
            .query(&[
                ("function", "TIME_SERIES_DAILY"),
                ("symbol", &symbol),
                ("outputsize", outputsize),
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

        if let Ok(error) = serde_json::from_str::<ErrorResponse>(&text) {
            if error.error_message.is_some() || error.note.is_some() {
                if let Some(msg) = error.error_message {
                    return Err(anyhow!("Alpha Vantage API error: {msg}"));
                }
                if let Some(note) = error.note {
                    return Err(anyhow!("Alpha Vantage rate limit: {note}"));
                }
            }
            if error.information.is_some() {
                return Err(anyhow!(
                    "Alpha Vantage returned no time series for {symbol}"
                ));
            }
        }

        serde_json::from_str(&text).map_err(Into::into)
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

        let time_series = match self.fetch_time_series(ticker, exchange, date).await {
            Ok(ts) => ts,
            Err(e) if e.to_string().contains("no time series") => return Ok(None),
            Err(e) => return Err(e),
        };

        Ok(self.parse_response(&time_series, asset_id, date))
    }

    async fn fetch_closes(
        &self,
        asset: &Asset,
        asset_id: &AssetId,
        start: NaiveDate,
        end: NaiveDate,
    ) -> Result<Vec<PricePoint>> {
        let (ticker, exchange) = match asset {
            Asset::Equity { ticker, exchange } => (ticker, exchange.as_deref()),
            _ => return Ok(Vec::new()),
        };

        let time_series = match self.fetch_time_series(ticker, exchange, start).await {
            Ok(ts) => ts,
            Err(e) if e.to_string().contains("no time series") => return Ok(Vec::new()),
            Err(e) => return Err(e),
        };

        let mut prices = Vec::new();
        let now = Utc::now();
        for (date_str, daily_data) in time_series.time_series {
            let Ok(as_of_date) = NaiveDate::parse_from_str(&date_str, "%Y-%m-%d") else {
                continue;
            };
            if as_of_date < start || as_of_date > end {
                continue;
            }
            prices.push(PricePoint {
                asset_id: asset_id.clone(),
                as_of_date,
                timestamp: now,
                price: daily_data.close,
                quote_currency: "USD".to_string(),
                kind: PriceKind::Close,
                source: self.name().to_string(),
            });
        }

        prices.sort_by_key(|p| p.as_of_date);
        Ok(prices)
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
#[path = "../../../tests/unit/market_data/providers/alpha_vantage_tests.rs"]
mod alpha_vantage_tests;
