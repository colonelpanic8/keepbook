//! Yahoo Finance equity price provider.
//!
//! Implements the `EquityPriceSource` trait using Yahoo's public `v8/finance/chart`
//! endpoint. Requires no API key (a browser-like `User-Agent` is sent because Yahoo
//! rate-limits the default reqwest agent).
//!
//! Note on adjustment: Yahoo's `indicators.quote[].close` series is **split-adjusted**
//! (back-adjusted to the current split basis) but not dividend-adjusted — e.g. AAPL on
//! 2020-01-02 returns 75.0875 (the raw 300.35 divided by the later 4:1 split). The most
//! recent close equals the as-traded price. This matches Twelve Data's convention.

use anyhow::{Context, Result};
use chrono::{DateTime, Duration, NaiveDate, Utc};
use reqwest::Client;
use rust_decimal::prelude::FromPrimitive;
use rust_decimal::Decimal;
use serde::Deserialize;

use crate::market_data::{AssetId, EquityPriceSource, PriceKind, PricePoint};
use crate::models::Asset;

const BASE_URL: &str = "https://query1.finance.yahoo.com/v8/finance/chart";
const USER_AGENT: &str =
    "Mozilla/5.0 (X11; Linux x86_64; rv:124.0) Gecko/20100101 Firefox/124.0 keepbook";

/// Yahoo Finance equity price provider (no credentials required).
pub struct YahooPriceSource {
    client: Client,
}

impl Default for YahooPriceSource {
    fn default() -> Self {
        Self::new()
    }
}

impl YahooPriceSource {
    /// Create a new Yahoo price source.
    pub fn new() -> Self {
        let client = Client::builder()
            .user_agent(USER_AGENT)
            .build()
            .unwrap_or_else(|_| Client::new());
        Self { client }
    }

    /// Create with a caller-supplied client (used in tests).
    pub fn with_client(client: Client) -> Self {
        Self { client }
    }

    /// Yahoo uses plain tickers for US listings; an exchange becomes a suffix
    /// (e.g. `ORA.PA`). Our assets normally carry no exchange (US default).
    fn build_symbol(ticker: &str, exchange: Option<&str>) -> String {
        match exchange {
            Some(ex) => format!("{}.{}", ticker.to_uppercase(), ex.to_uppercase()),
            None => ticker.to_uppercase(),
        }
    }

    async fn fetch_chart(
        &self,
        symbol: &str,
        period1: i64,
        period2: i64,
    ) -> Result<Option<ChartResult>> {
        let url = format!("{BASE_URL}/{symbol}?period1={period1}&period2={period2}&interval=1d");
        let response = self
            .client
            .get(&url)
            .send()
            .await
            .context("Failed to send request to Yahoo Finance")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Yahoo Finance API error: status={status}, body={body}");
        }

        let body = response
            .text()
            .await
            .context("Failed to read response body")?;
        parse_chart_body(&body)
    }
}

/// Parse a chart response body into its single result, or `None` when Yahoo reports
/// no data (e.g. an unknown/delisted symbol).
fn parse_chart_body(body: &str) -> Result<Option<ChartResult>> {
    let parsed: ChartResponse =
        serde_json::from_str(body).context("Failed to parse Yahoo Finance response")?;
    Ok(parsed.chart.result.and_then(|mut r| {
        if r.is_empty() {
            None
        } else {
            Some(r.swap_remove(0))
        }
    }))
}

/// Convert a chart result into (date, close) pairs on the current split basis.
fn extract_points(result: &ChartResult) -> Vec<(NaiveDate, String)> {
    let timestamps = match &result.timestamp {
        Some(t) => t,
        None => return Vec::new(),
    };
    let closes = match result.indicators.quote.first() {
        Some(q) => &q.close,
        None => return Vec::new(),
    };
    let offset = result.meta.gmtoffset;
    let mut out = Vec::new();
    for (ts, close) in timestamps.iter().zip(closes.iter()) {
        let Some(close) = close else { continue };
        let Some(price) = decimal_price(*close) else {
            continue;
        };
        // Convert the bar's open timestamp to its local trading date.
        let Some(dt) = DateTime::<Utc>::from_timestamp(ts + offset, 0) else {
            continue;
        };
        out.push((dt.date_naive(), price));
    }
    out
}

/// Format a Yahoo f64 price as a clean decimal string, stripping float noise.
fn decimal_price(value: f64) -> Option<String> {
    Decimal::from_f64(value).map(|d| d.round_dp(4).normalize().to_string())
}

fn day_start_ts(date: NaiveDate) -> i64 {
    date.and_hms_opt(0, 0, 0)
        .map(|dt| dt.and_utc().timestamp())
        .unwrap_or(0)
}

#[async_trait::async_trait]
impl EquityPriceSource for YahooPriceSource {
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
        // Pad the window so non-trading days still resolve to a prior close.
        let period1 = day_start_ts(date - Duration::days(7));
        let period2 = day_start_ts(date + Duration::days(2));
        let Some(result) = self.fetch_chart(&symbol, period1, period2).await? else {
            return Ok(None);
        };

        let currency = result
            .meta
            .currency
            .clone()
            .unwrap_or_else(|| "USD".to_string());
        let point = extract_points(&result)
            .into_iter()
            .filter(|(d, _)| *d <= date)
            .max_by_key(|(d, _)| *d);

        let Some((as_of_date, price)) = point else {
            return Ok(None);
        };

        Ok(Some(PricePoint {
            asset_id: asset_id.clone(),
            as_of_date,
            timestamp: Utc::now(),
            price,
            quote_currency: currency,
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
        let period1 = day_start_ts(start - Duration::days(2));
        let period2 = day_start_ts(end + Duration::days(2));
        let Some(result) = self.fetch_chart(&symbol, period1, period2).await? else {
            return Ok(Vec::new());
        };

        let currency = result
            .meta
            .currency
            .clone()
            .unwrap_or_else(|| "USD".to_string());
        let now = Utc::now();
        let mut prices: Vec<PricePoint> = extract_points(&result)
            .into_iter()
            .filter(|(d, _)| *d >= start && *d <= end)
            .map(|(as_of_date, price)| PricePoint {
                asset_id: asset_id.clone(),
                as_of_date,
                timestamp: now,
                price,
                quote_currency: currency.clone(),
                kind: PriceKind::Close,
                source: self.name().to_string(),
            })
            .collect();
        prices.sort_by_key(|p| p.as_of_date);
        Ok(prices)
    }

    async fn fetch_quote(&self, asset: &Asset, asset_id: &AssetId) -> Result<Option<PricePoint>> {
        let (ticker, exchange) = match asset {
            Asset::Equity { ticker, exchange } => (ticker.as_str(), exchange.as_deref()),
            _ => return Ok(None),
        };
        let symbol = Self::build_symbol(ticker, exchange);
        let now = Utc::now();
        let period1 = day_start_ts(now.date_naive() - Duration::days(5));
        let period2 = day_start_ts(now.date_naive() + Duration::days(2));
        let Some(result) = self.fetch_chart(&symbol, period1, period2).await? else {
            return Ok(None);
        };

        let currency = result
            .meta
            .currency
            .clone()
            .unwrap_or_else(|| "USD".to_string());
        // Prefer the live regular-market price; fall back to the latest close.
        if let Some(price) = result.meta.regular_market_price.and_then(decimal_price) {
            let as_of_date = result
                .meta
                .regular_market_time
                .and_then(|ts| DateTime::<Utc>::from_timestamp(ts + result.meta.gmtoffset, 0))
                .map(|dt| dt.date_naive())
                .unwrap_or_else(|| now.date_naive());
            return Ok(Some(PricePoint {
                asset_id: asset_id.clone(),
                as_of_date,
                timestamp: now,
                price,
                quote_currency: currency,
                kind: PriceKind::Quote,
                source: self.name().to_string(),
            }));
        }

        let latest = extract_points(&result).into_iter().max_by_key(|(d, _)| *d);
        let Some((as_of_date, price)) = latest else {
            return Ok(None);
        };
        Ok(Some(PricePoint {
            asset_id: asset_id.clone(),
            as_of_date,
            timestamp: now,
            price,
            quote_currency: currency,
            kind: PriceKind::Quote,
            source: self.name().to_string(),
        }))
    }

    fn name(&self) -> &str {
        "yahoo"
    }
}

#[derive(Debug, Deserialize)]
struct ChartResponse {
    chart: Chart,
}

#[derive(Debug, Deserialize)]
struct Chart {
    result: Option<Vec<ChartResult>>,
}

#[derive(Debug, Deserialize)]
struct ChartResult {
    meta: Meta,
    timestamp: Option<Vec<i64>>,
    indicators: Indicators,
}

#[derive(Debug, Deserialize)]
struct Meta {
    currency: Option<String>,
    #[serde(default)]
    gmtoffset: i64,
    #[serde(rename = "regularMarketPrice")]
    regular_market_price: Option<f64>,
    #[serde(rename = "regularMarketTime")]
    regular_market_time: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct Indicators {
    quote: Vec<Quote>,
}

#[derive(Debug, Deserialize)]
struct Quote {
    #[serde(default)]
    close: Vec<Option<f64>>,
}

#[cfg(test)]
#[path = "../../../tests/unit/market_data/providers/yahoo_tests.rs"]
mod yahoo_tests;
