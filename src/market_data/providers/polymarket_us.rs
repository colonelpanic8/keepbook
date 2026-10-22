//! Polymarket US prediction market prices.
//!
//! Uses the public gateway: `/v1/markets/{slug}/bbo` for current quotes and
//! `/v1/price-history` for history. Polymarket US lists one instrument per market slug and
//! quotes it as YES; a NO contract is worth the $1 payout minus that price.

use std::collections::BTreeMap;
use std::str::FromStr;

use anyhow::{Context, Result};
use chrono::{DateTime, Duration, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::Deserialize;

use super::price_point;
use crate::market_data::{AssetId, PredictionMarketPriceSource, PriceKind, PricePoint};
use crate::models::{Asset, ContractSide};

pub const POLYMARKET_US_EXCHANGE: &str = "polymarket_us";
pub const POLYMARKET_US_GATEWAY_BASE: &str = "https://gateway.polymarket.us";
const PRICE_DECIMALS: u32 = 6;

/// The asset for one side of a Polymarket US market.
pub fn polymarket_us_asset(slug: &str, side: ContractSide, name: Option<String>) -> Asset {
    Asset::polymarket_us(slug, side, name)
}

/// Converts a YES price into the price of `side`.
pub fn side_price(side: ContractSide, yes: Decimal) -> Decimal {
    match side {
        ContractSide::Yes => yes,
        ContractSide::No => Decimal::ONE - yes,
    }
}

#[derive(Debug, Deserialize)]
pub struct PolymarketAmount {
    pub value: String,
    pub currency: String,
}

/// Accepts a JSON number or string without round-tripping through `f64` arithmetic.
fn json_decimal(value: &serde_json::Value) -> Option<Decimal> {
    let raw = match value {
        serde_json::Value::Number(number) => number.to_string(),
        serde_json::Value::String(string) => string.clone(),
        _ => return None,
    };
    let raw = raw.trim();
    Decimal::from_str(raw)
        .or_else(|_| Decimal::from_scientific(raw))
        .ok()
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PriceHistoryPoint {
    timestamp: i64,
    #[serde(default)]
    long_price: serde_json::Value,
    #[serde(default)]
    short_price: serde_json::Value,
}

impl PriceHistoryPoint {
    /// `longPrice` is derived from the best ask and `shortPrice` from one minus the best bid,
    /// so their midpoint is the bid/ask midpoint.
    fn mid_price(&self) -> Option<Decimal> {
        let long = json_decimal(&self.long_price)?;
        let price = match json_decimal(&self.short_price) {
            Some(short) => (long + Decimal::ONE - short) / Decimal::TWO,
            None => long,
        };
        Some(price.round_dp(PRICE_DECIMALS))
    }
}

/// Polymarket US price source using the public gateway.
pub struct PolymarketUsPriceSource {
    client: reqwest::Client,
    base_url: String,
}

impl PolymarketUsPriceSource {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: POLYMARKET_US_GATEWAY_BASE.to_string(),
        }
    }

    /// Override the gateway host (useful for tests).
    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }

    async fn get<T: for<'de> Deserialize<'de>>(
        &self,
        path: &str,
        query: &[(&str, &str)],
    ) -> Result<T> {
        let url = format!("{}{path}", self.base_url.trim_end_matches('/'));
        let response = self
            .client
            .get(&url)
            .query(query)
            .send()
            .await
            .context("HTTP request failed")?;
        let status = response.status();
        let body = response
            .text()
            .await
            .context("Failed to read response body")?;

        if !status.is_success() {
            anyhow::bail!("Polymarket US request {path} failed ({status}): {body}");
        }

        serde_json::from_str(&body)
            .with_context(|| format!("Failed to parse Polymarket US response from {path}"))
    }

    /// Per-contract value of a market: the current price, otherwise the last trade, otherwise
    /// the settlement price. Open markets also report a settlement price, so it comes last.
    pub async fn fetch_mark_price(&self, slug: &str) -> Result<PolymarketAmount> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct MarketData {
            current_px: Option<PolymarketAmount>,
            last_trade_px: Option<PolymarketAmount>,
            settlement_px: Option<PolymarketAmount>,
        }
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Response {
            market_data: MarketData,
        }

        let path = format!("/v1/markets/{}/bbo", encode_path_segment(slug));
        let response: Response = self.get(&path, &[]).await?;
        let data = response.market_data;
        data.current_px
            .or(data.last_trade_px)
            .or(data.settlement_px)
            .with_context(|| format!("No price available for Polymarket US market {slug}"))
    }

    async fn fetch_price_history(&self, slug: &str) -> Result<Vec<PriceHistoryPoint>> {
        #[derive(Deserialize)]
        struct Response {
            #[serde(default)]
            history: Vec<PriceHistoryPoint>,
        }

        let response: Response = self
            .get(
                "/v1/price-history",
                &[
                    ("symbol", slug),
                    ("fixedInterval", "INTERVAL_ALL"),
                    ("fidelity", "180"),
                ],
            )
            .await?;
        Ok(response.history)
    }
}

impl Default for PolymarketUsPriceSource {
    fn default() -> Self {
        Self::new()
    }
}

fn encode_path_segment(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for b in value.as_bytes() {
        match *b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(*b as char)
            }
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

fn polymarket_contract(asset: &Asset) -> Option<(&str, ContractSide)> {
    match asset {
        Asset::PolymarketUs { slug, side, .. } => Some((slug.as_str(), *side)),
        _ => None,
    }
}

#[async_trait::async_trait]
impl PredictionMarketPriceSource for PolymarketUsPriceSource {
    fn supports(&self, asset: &Asset) -> bool {
        matches!(asset, Asset::PolymarketUs { .. })
    }

    fn name(&self) -> &str {
        POLYMARKET_US_EXCHANGE
    }

    async fn fetch_quote(&self, asset: &Asset, asset_id: &AssetId) -> Result<Option<PricePoint>> {
        let Some((slug, side)) = polymarket_contract(asset) else {
            return Ok(None);
        };
        let price = self.fetch_mark_price(slug).await?;
        let yes = Decimal::from_str(price.value.trim())
            .with_context(|| format!("Invalid Polymarket US price {:?}", price.value))?;
        let now = Utc::now();
        Ok(Some(price_point(
            asset_id,
            now.date_naive(),
            now,
            side_price(side, yes),
            &price.currency,
            PriceKind::Quote,
            POLYMARKET_US_EXCHANGE,
        )))
    }

    async fn fetch_closes(
        &self,
        asset: &Asset,
        asset_id: &AssetId,
        start: NaiveDate,
        end: NaiveDate,
    ) -> Result<Vec<PricePoint>> {
        let Some((slug, side)) = polymarket_contract(asset) else {
            return Ok(Vec::new());
        };

        // The latest observation of each day stands in for its close.
        let mut closes: BTreeMap<NaiveDate, (DateTime<Utc>, Decimal)> = BTreeMap::new();
        for point in self.fetch_price_history(slug).await? {
            let (Some(timestamp), Some(price)) = (
                DateTime::from_timestamp(point.timestamp, 0),
                point.mid_price(),
            ) else {
                continue;
            };
            // An observation at midnight closes the previous day.
            let date = (timestamp - Duration::seconds(1)).date_naive();
            if date < start || date > end {
                continue;
            }
            let price = side_price(side, price);
            let entry = closes.entry(date).or_insert((timestamp, price));
            if timestamp >= entry.0 {
                *entry = (timestamp, price);
            }
        }

        Ok(closes
            .into_iter()
            .map(|(date, (timestamp, price))| {
                price_point(
                    asset_id,
                    date,
                    timestamp,
                    price,
                    "USD",
                    PriceKind::Close,
                    POLYMARKET_US_EXCHANGE,
                )
            })
            .collect())
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/market_data/providers/polymarket_us_tests.rs"]
mod polymarket_us_tests;
