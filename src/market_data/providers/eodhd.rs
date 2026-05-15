//! EODHD (End of Day Historical Data) equity price provider.
//!
//! EODHD provides daily end-of-day price data for equities across many exchanges.
//! Their API uses symbols in the format `TICKER.EXCHANGE`, e.g., `AAPL.US` or `VOD.LSE`.
//!
//! Free tier has very limited request volume - cache aggressively.

use anyhow::{anyhow, Result};
use chrono::{NaiveDate, Utc};
use reqwest::Client;
use secrecy::ExposeSecret;
use serde::Deserialize;

use crate::credentials::CredentialStore;
use crate::market_data::{AssetId, EquityPriceSource, PriceKind, PricePoint};
use crate::models::Asset;

const EODHD_BASE_URL: &str = "https://eodhd.com/api/eod";

/// EODHD API response for a single day's EOD data.
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct EodhdEodResponse {
    date: String,
    open: Option<f64>,
    high: Option<f64>,
    low: Option<f64>,
    close: Option<f64>,
    adjusted_close: Option<f64>,
    volume: Option<u64>,
}

/// Price source for fetching equity prices from EODHD.
pub struct EodhdPriceSource {
    api_key: String,
    client: Client,
}

impl EodhdPriceSource {
    /// Create a new EODHD price source with the given API key.
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            client: Client::new(),
        }
    }

    /// Create a new EODHD price source with a custom HTTP client.
    pub fn with_client(api_key: impl Into<String>, client: Client) -> Self {
        Self {
            api_key: api_key.into(),
            client,
        }
    }

    /// Create a new EODHD price source from a credential store.
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

    /// Convert our exchange code to EODHD's exchange suffix.
    ///
    /// EODHD uses specific exchange codes that may differ from standard MIC codes.
    /// See: https://eodhd.com/financial-apis/list-supported-exchanges/
    fn map_exchange(exchange: Option<&str>) -> &'static str {
        match exchange.map(|s| s.to_uppercase()).as_deref() {
            // US exchanges
            Some("XNYS") | Some("NYSE") => "US",
            Some("XNAS") | Some("NASDAQ") => "US",
            Some("XASE") | Some("AMEX") => "US",
            Some("ARCX") | Some("ARCA") | Some("NYSE ARCA") => "US",
            Some("BATS") | Some("BATS GLOBAL MARKETS") => "US",
            Some("US") => "US",

            // UK
            Some("XLON") | Some("LSE") | Some("LONDON") => "LSE",

            // Germany
            Some("XETR") | Some("XETRA") => "XETRA",
            Some("XFRA") | Some("FRA") | Some("FRANKFURT") => "F",

            // France
            Some("XPAR") | Some("PARIS") | Some("EURONEXT PARIS") => "PA",

            // Netherlands
            Some("XAMS") | Some("AMSTERDAM") | Some("EURONEXT AMSTERDAM") => "AS",

            // Switzerland
            Some("XSWX") | Some("SIX") | Some("SWISS") => "SW",

            // Japan
            Some("XTKS") | Some("TSE") | Some("TOKYO") => "TSE",

            // Hong Kong
            Some("XHKG") | Some("HKEX") | Some("HONG KONG") => "HK",

            // Australia
            Some("XASX") | Some("ASX") | Some("AUSTRALIA") => "AU",

            // Canada
            Some("XTSE") | Some("TSX") | Some("TORONTO") => "TO",
            Some("XTSX") | Some("TSXV") | Some("TSX VENTURE") => "V",

            // Singapore
            Some("XSES") | Some("SGX") | Some("SINGAPORE") => "SG",

            // India
            Some("XBOM") | Some("BSE") | Some("BOMBAY") => "BSE",
            Some("XNSE") | Some("NSE") | Some("NATIONAL STOCK EXCHANGE") => "NSE",

            // Default to US for unspecified or unknown exchanges
            None | Some(_) => "US",
        }
    }

    /// Build the EODHD symbol from ticker and exchange.
    fn build_symbol(ticker: &str, exchange: Option<&str>) -> String {
        let eodhd_exchange = Self::map_exchange(exchange);
        format!("{}.{}", ticker.to_uppercase(), eodhd_exchange)
    }

    /// Determine the quote currency based on the exchange.
    fn quote_currency_for_exchange(exchange: Option<&str>) -> &'static str {
        match exchange.map(|s| s.to_uppercase()).as_deref() {
            // UK
            Some("XLON") | Some("LSE") | Some("LONDON") => "GBP",

            // Eurozone
            Some("XETR") | Some("XETRA") | Some("XFRA") | Some("FRA") | Some("FRANKFURT") => "EUR",
            Some("XPAR") | Some("PARIS") | Some("EURONEXT PARIS") => "EUR",
            Some("XAMS") | Some("AMSTERDAM") | Some("EURONEXT AMSTERDAM") => "EUR",

            // Switzerland
            Some("XSWX") | Some("SIX") | Some("SWISS") => "CHF",

            // Japan
            Some("XTKS") | Some("TSE") | Some("TOKYO") => "JPY",

            // Hong Kong
            Some("XHKG") | Some("HKEX") | Some("HONG KONG") => "HKD",

            // Australia
            Some("XASX") | Some("ASX") | Some("AUSTRALIA") => "AUD",

            // Canada
            Some("XTSE") | Some("TSX") | Some("TORONTO") => "CAD",
            Some("XTSX") | Some("TSXV") | Some("TSX VENTURE") => "CAD",

            // Singapore
            Some("XSES") | Some("SGX") | Some("SINGAPORE") => "SGD",

            // India
            Some("XBOM") | Some("BSE") | Some("BOMBAY") => "INR",
            Some("XNSE") | Some("NSE") | Some("NATIONAL STOCK EXCHANGE") => "INR",

            // Default to USD for US and unknown exchanges
            _ => "USD",
        }
    }

    /// Parse the API response into a PricePoint.
    fn parse_response(
        response: &EodhdEodResponse,
        asset_id: &AssetId,
        exchange: Option<&str>,
    ) -> Result<Option<PricePoint>> {
        let close_price = match response.close {
            Some(price) => price,
            None => return Ok(None),
        };

        let as_of_date = NaiveDate::parse_from_str(&response.date, "%Y-%m-%d")
            .map_err(|e| anyhow!("Failed to parse date '{}': {}", response.date, e))?;

        Ok(Some(PricePoint {
            asset_id: asset_id.clone(),
            as_of_date,
            timestamp: Utc::now(),
            price: close_price.to_string(),
            quote_currency: Self::quote_currency_for_exchange(exchange).to_string(),
            kind: PriceKind::Close,
            source: "eodhd".to_string(),
        }))
    }
}

#[async_trait::async_trait]
impl EquityPriceSource for EodhdPriceSource {
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
        let date_str = date.format("%Y-%m-%d").to_string();

        // EODHD EOD endpoint: /api/eod/{SYMBOL}?api_token={KEY}&from={DATE}&to={DATE}&fmt=json
        let url = format!(
            "{}/{}?api_token={}&from={}&to={}&fmt=json",
            EODHD_BASE_URL, symbol, self.api_key, date_str, date_str
        );

        let response = self.client.get(&url).send().await?;

        if !response.status().is_success() {
            // EODHD returns 404 for unknown symbols or dates with no data
            if response.status().as_u16() == 404 {
                return Ok(None);
            }
            return Err(anyhow!(
                "EODHD API returned status {}: {}",
                response.status(),
                response.text().await.unwrap_or_default()
            ));
        }

        let data: Vec<EodhdEodResponse> = response.json().await?;

        // Find the entry for the requested date
        for entry in &data {
            if entry.date == date_str {
                return Self::parse_response(entry, asset_id, exchange);
            }
        }

        // No data for the requested date
        Ok(None)
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
        let url = format!(
            "{}/{}?api_token={}&from={}&to={}&fmt=json",
            EODHD_BASE_URL,
            symbol,
            self.api_key,
            start.format("%Y-%m-%d"),
            end.format("%Y-%m-%d")
        );

        let response = self.client.get(&url).send().await?;
        if !response.status().is_success() {
            if response.status().as_u16() == 404 {
                return Ok(Vec::new());
            }
            return Err(anyhow!(
                "EODHD API returned status {}: {}",
                response.status(),
                response.text().await.unwrap_or_default()
            ));
        }

        let data: Vec<EodhdEodResponse> = response.json().await?;
        let mut prices = Vec::new();
        for entry in &data {
            if let Some(price) = Self::parse_response(entry, asset_id, exchange)? {
                if price.as_of_date >= start && price.as_of_date <= end {
                    prices.push(price);
                }
            }
        }

        prices.sort_by_key(|p| p.as_of_date);
        Ok(prices)
    }

    fn name(&self) -> &str {
        "eodhd"
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/market_data/providers/eodhd_tests.rs"]
mod eodhd_tests;
