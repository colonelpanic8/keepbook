//! EODHD (End of Day Historical Data) equity price provider.
//!
//! EODHD provides daily end-of-day price data for equities across many exchanges.
//! Their API uses symbols in the format `TICKER.EXCHANGE`, e.g., `AAPL.US` or `VOD.LSE`.
//!
//! Free tier has very limited request volume - cache aggressively.

use anyhow::{anyhow, Result};
use chrono::{NaiveDate, Utc};
use reqwest::Client;
use serde::Deserialize;

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

/// Provider for fetching equity prices from EODHD.
pub struct EodhdProvider {
    api_key: String,
    client: Client,
}

impl EodhdProvider {
    /// Create a new EODHD provider with the given API key.
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            client: Client::new(),
        }
    }

    /// Create a new EODHD provider with a custom HTTP client.
    pub fn with_client(api_key: impl Into<String>, client: Client) -> Self {
        Self {
            api_key: api_key.into(),
            client,
        }
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
impl EquityPriceSource for EodhdProvider {
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

    fn name(&self) -> &str {
        "eodhd"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_EODHD_RESPONSE: &str = r#"[
        {
            "date": "2024-01-15",
            "open": 185.05,
            "high": 186.22,
            "low": 184.82,
            "close": 186.01,
            "adjusted_close": 185.45,
            "volume": 52894000
        }
    ]"#;

    const SAMPLE_EODHD_RESPONSE_EMPTY: &str = "[]";

    const SAMPLE_EODHD_RESPONSE_NO_CLOSE: &str = r#"[
        {
            "date": "2024-01-15",
            "open": 185.05,
            "high": 186.22,
            "low": 184.82,
            "close": null,
            "adjusted_close": null,
            "volume": 52894000
        }
    ]"#;

    #[test]
    fn test_parse_eodhd_response() {
        let data: Vec<EodhdEodResponse> = serde_json::from_str(SAMPLE_EODHD_RESPONSE).unwrap();
        assert_eq!(data.len(), 1);

        let entry = &data[0];
        assert_eq!(entry.date, "2024-01-15");
        assert_eq!(entry.close, Some(186.01));
        assert_eq!(entry.volume, Some(52894000));
    }

    #[test]
    fn test_parse_empty_response() {
        let data: Vec<EodhdEodResponse> = serde_json::from_str(SAMPLE_EODHD_RESPONSE_EMPTY).unwrap();
        assert!(data.is_empty());
    }

    #[test]
    fn test_parse_response_no_close() {
        let data: Vec<EodhdEodResponse> =
            serde_json::from_str(SAMPLE_EODHD_RESPONSE_NO_CLOSE).unwrap();
        assert_eq!(data.len(), 1);

        let asset = Asset::equity("AAPL");
        let asset_id = AssetId::from_asset(&asset);

        let result = EodhdProvider::parse_response(&data[0], &asset_id, None).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_response_to_price_point() {
        let data: Vec<EodhdEodResponse> = serde_json::from_str(SAMPLE_EODHD_RESPONSE).unwrap();

        let asset = Asset::equity("AAPL");
        let asset_id = AssetId::from_asset(&asset);

        let price_point = EodhdProvider::parse_response(&data[0], &asset_id, None)
            .unwrap()
            .unwrap();

        assert_eq!(price_point.price, "186.01");
        assert_eq!(price_point.quote_currency, "USD");
        assert_eq!(price_point.kind, PriceKind::Close);
        assert_eq!(price_point.source, "eodhd");
        assert_eq!(
            price_point.as_of_date,
            NaiveDate::from_ymd_opt(2024, 1, 15).unwrap()
        );
    }

    #[test]
    fn test_parse_response_with_exchange() {
        let data: Vec<EodhdEodResponse> = serde_json::from_str(SAMPLE_EODHD_RESPONSE).unwrap();

        let asset = Asset::Equity {
            ticker: "VOD".to_string(),
            exchange: Some("LSE".to_string()),
        };
        let asset_id = AssetId::from_asset(&asset);

        let price_point = EodhdProvider::parse_response(&data[0], &asset_id, Some("LSE"))
            .unwrap()
            .unwrap();

        assert_eq!(price_point.quote_currency, "GBP");
    }

    #[test]
    fn test_build_symbol_us() {
        assert_eq!(EodhdProvider::build_symbol("AAPL", None), "AAPL.US");
        assert_eq!(EodhdProvider::build_symbol("AAPL", Some("NYSE")), "AAPL.US");
        assert_eq!(
            EodhdProvider::build_symbol("AAPL", Some("NASDAQ")),
            "AAPL.US"
        );
        assert_eq!(EodhdProvider::build_symbol("AAPL", Some("XNYS")), "AAPL.US");
        assert_eq!(EodhdProvider::build_symbol("AAPL", Some("XNAS")), "AAPL.US");
    }

    #[test]
    fn test_build_symbol_uk() {
        assert_eq!(EodhdProvider::build_symbol("VOD", Some("LSE")), "VOD.LSE");
        assert_eq!(EodhdProvider::build_symbol("VOD", Some("XLON")), "VOD.LSE");
    }

    #[test]
    fn test_build_symbol_germany() {
        assert_eq!(EodhdProvider::build_symbol("SAP", Some("XETRA")), "SAP.XETRA");
        assert_eq!(EodhdProvider::build_symbol("SAP", Some("XETR")), "SAP.XETRA");
        assert_eq!(
            EodhdProvider::build_symbol("SAP", Some("FRANKFURT")),
            "SAP.F"
        );
    }

    #[test]
    fn test_build_symbol_case_insensitive() {
        assert_eq!(EodhdProvider::build_symbol("aapl", None), "AAPL.US");
        assert_eq!(EodhdProvider::build_symbol("Aapl", Some("nyse")), "AAPL.US");
    }

    #[test]
    fn test_quote_currency_mapping() {
        assert_eq!(EodhdProvider::quote_currency_for_exchange(None), "USD");
        assert_eq!(EodhdProvider::quote_currency_for_exchange(Some("NYSE")), "USD");
        assert_eq!(EodhdProvider::quote_currency_for_exchange(Some("LSE")), "GBP");
        assert_eq!(EodhdProvider::quote_currency_for_exchange(Some("XETRA")), "EUR");
        assert_eq!(EodhdProvider::quote_currency_for_exchange(Some("TSE")), "JPY");
        assert_eq!(EodhdProvider::quote_currency_for_exchange(Some("HKEX")), "HKD");
        assert_eq!(EodhdProvider::quote_currency_for_exchange(Some("ASX")), "AUD");
        assert_eq!(EodhdProvider::quote_currency_for_exchange(Some("TSX")), "CAD");
        assert_eq!(EodhdProvider::quote_currency_for_exchange(Some("SIX")), "CHF");
        assert_eq!(EodhdProvider::quote_currency_for_exchange(Some("SGX")), "SGD");
        assert_eq!(EodhdProvider::quote_currency_for_exchange(Some("BSE")), "INR");
    }

    #[test]
    fn test_exchange_mapping() {
        // US exchanges
        assert_eq!(EodhdProvider::map_exchange(Some("NYSE")), "US");
        assert_eq!(EodhdProvider::map_exchange(Some("NASDAQ")), "US");
        assert_eq!(EodhdProvider::map_exchange(Some("XNYS")), "US");
        assert_eq!(EodhdProvider::map_exchange(Some("XNAS")), "US");

        // International
        assert_eq!(EodhdProvider::map_exchange(Some("LSE")), "LSE");
        assert_eq!(EodhdProvider::map_exchange(Some("XLON")), "LSE");
        assert_eq!(EodhdProvider::map_exchange(Some("XETRA")), "XETRA");
        assert_eq!(EodhdProvider::map_exchange(Some("TSE")), "TSE");

        // Default
        assert_eq!(EodhdProvider::map_exchange(None), "US");
        assert_eq!(EodhdProvider::map_exchange(Some("UNKNOWN")), "US");
    }

    #[test]
    fn test_provider_name() {
        let provider = EodhdProvider::new("test_key");
        assert_eq!(provider.name(), "eodhd");
    }

    // Multi-day response parsing test
    const SAMPLE_EODHD_MULTI_DAY: &str = r#"[
        {
            "date": "2024-01-15",
            "open": 185.05,
            "high": 186.22,
            "low": 184.82,
            "close": 186.01,
            "adjusted_close": 185.45,
            "volume": 52894000
        },
        {
            "date": "2024-01-16",
            "open": 186.50,
            "high": 187.00,
            "low": 185.20,
            "close": 185.75,
            "adjusted_close": 185.19,
            "volume": 48750000
        }
    ]"#;

    #[test]
    fn test_parse_multi_day_response() {
        let data: Vec<EodhdEodResponse> = serde_json::from_str(SAMPLE_EODHD_MULTI_DAY).unwrap();
        assert_eq!(data.len(), 2);

        assert_eq!(data[0].date, "2024-01-15");
        assert_eq!(data[0].close, Some(186.01));

        assert_eq!(data[1].date, "2024-01-16");
        assert_eq!(data[1].close, Some(185.75));
    }
}
