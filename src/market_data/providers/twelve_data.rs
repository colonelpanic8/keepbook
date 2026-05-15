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

    /// Fetches time series data for a symbol over a date range.
    async fn fetch_time_series(
        &self,
        symbol: &str,
        start_date: NaiveDate,
        end_date: NaiveDate,
    ) -> Result<Option<TimeSeriesResponse>> {
        let url = format!(
            "{}/time_series?symbol={}&interval=1day&start_date={}&end_date={}&apikey={}",
            BASE_URL,
            symbol,
            start_date.format("%Y-%m-%d"),
            end_date.format("%Y-%m-%d"),
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
            anyhow::bail!("Twelve Data API error: status={status}, body={body}");
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
        let response = self
            .fetch_time_series(&symbol, date - chrono::Duration::days(7), date)
            .await?;

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

    async fn fetch_closes(
        &self,
        asset: &Asset,
        asset_id: &AssetId,
        start: NaiveDate,
        end: NaiveDate,
    ) -> Result<Vec<PricePoint>> {
        let (ticker, exchange) = match asset {
            Asset::Equity { ticker, exchange } => (ticker.as_str(), exchange.as_deref()),
            _ => return Ok(Vec::new()),
        };

        let symbol = Self::build_symbol(ticker, exchange);
        let Some(data) = self.fetch_time_series(&symbol, start, end).await? else {
            return Ok(Vec::new());
        };

        let quote_currency = data.meta.currency.unwrap_or_else(|| "USD".to_string());
        let now = Utc::now();
        let mut prices = Vec::new();
        for value in data.values {
            let as_of_date = NaiveDate::parse_from_str(&value.datetime, "%Y-%m-%d")
                .context("Failed to parse date from Twelve Data response")?;
            if as_of_date < start || as_of_date > end {
                continue;
            }
            prices.push(PricePoint {
                asset_id: asset_id.clone(),
                as_of_date,
                timestamp: now,
                price: value.close,
                quote_currency: quote_currency.clone(),
                kind: PriceKind::Close,
                source: self.name().to_string(),
            });
        }

        prices.sort_by_key(|p| p.as_of_date);
        Ok(prices)
    }

    async fn fetch_quote(&self, asset: &Asset, asset_id: &AssetId) -> Result<Option<PricePoint>> {
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
#[path = "../../../tests/unit/market_data/providers/twelve_data_tests.rs"]
mod twelve_data_tests;
