//! Kalshi prediction market prices.
//!
//! Uses Kalshi's public market data endpoints: `/markets` for current quotes and
//! `/markets/candlesticks` for daily history. Kalshi lists separate YES and NO contracts
//! for each market, so assets carry the side as their outcome.

use std::collections::HashMap;
use std::str::FromStr;

use anyhow::{Context, Result};
use chrono::{DateTime, Duration, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::Deserialize;

use super::price_point;
use crate::market_data::{AssetId, PredictionMarketPriceSource, PriceKind, PricePoint};
use crate::models::Asset;
pub use crate::models::ContractSide;

pub const KALSHI_EXCHANGE: &str = "kalshi";
pub const KALSHI_API_BASE: &str = "https://api.elections.kalshi.com";
pub const KALSHI_API_PREFIX: &str = "/trade-api/v2";
const MARKET_BATCH_SIZE: usize = 50;
const DAILY_MINUTES: &str = "1440";
/// Kalshi's daily candlesticks are aligned to US Eastern days.
const KALSHI_TZ: chrono_tz::Tz = chrono_tz::America::New_York;

fn eastern_midnight(date: NaiveDate) -> Result<DateTime<chrono_tz::Tz>> {
    date.and_time(chrono::NaiveTime::MIN)
        .and_local_timezone(KALSHI_TZ)
        .earliest()
        .with_context(|| format!("No Eastern midnight on {date}"))
}

/// The asset for one side of a Kalshi market.
pub fn kalshi_asset(ticker: &str, side: ContractSide, name: Option<String>) -> Asset {
    Asset::kalshi(ticker, side, name)
}

fn parse_decimal(value: &str) -> Result<Decimal> {
    Decimal::from_str(value.trim()).with_context(|| format!("Invalid decimal: {value:?}"))
}

fn parse_optional_decimal(value: &Option<String>) -> Result<Option<Decimal>> {
    value
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(parse_decimal)
        .transpose()
}

/// Midpoint of a two-sided quote; `None` when either side is missing.
fn quote_midpoint(bid: Decimal, ask: Decimal, notional: Decimal) -> Option<Decimal> {
    (bid > Decimal::ZERO && ask < notional && bid <= ask).then(|| (bid + ask) / Decimal::TWO)
}

#[derive(Debug, Deserialize)]
pub struct KalshiMarket {
    pub ticker: String,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    yes_sub_title: Option<String>,
    #[serde(default)]
    no_sub_title: Option<String>,
    #[serde(default)]
    yes_bid_dollars: Option<String>,
    #[serde(default)]
    yes_ask_dollars: Option<String>,
    #[serde(default)]
    no_bid_dollars: Option<String>,
    #[serde(default)]
    no_ask_dollars: Option<String>,
    #[serde(default)]
    last_price_dollars: Option<String>,
    #[serde(default)]
    notional_value_dollars: Option<String>,
    #[serde(default)]
    settlement_value_dollars: Option<String>,
}

impl KalshiMarket {
    /// Per-contract value for `side`: the settlement value once determined, otherwise the
    /// bid/ask midpoint when both sides are quoted, otherwise the last trade price.
    pub fn mark_price(&self, side: ContractSide) -> Result<Decimal> {
        let notional =
            parse_optional_decimal(&self.notional_value_dollars)?.unwrap_or(Decimal::ONE);
        // Settlement and last-trade prices are quoted for YES.
        let from_yes = |yes: Decimal| match side {
            ContractSide::Yes => yes,
            ContractSide::No => notional - yes,
        };

        if let Some(settlement) = parse_optional_decimal(&self.settlement_value_dollars)? {
            return Ok(from_yes(settlement));
        }

        let (bid, ask) = match side {
            ContractSide::Yes => (&self.yes_bid_dollars, &self.yes_ask_dollars),
            ContractSide::No => (&self.no_bid_dollars, &self.no_ask_dollars),
        };
        if let (Some(bid), Some(ask)) = (parse_optional_decimal(bid)?, parse_optional_decimal(ask)?)
        {
            if let Some(mid) = quote_midpoint(bid, ask, notional) {
                return Ok(mid);
            }
        }

        parse_optional_decimal(&self.last_price_dollars)?
            .map(from_yes)
            .with_context(|| format!("No price available for Kalshi market {}", self.ticker))
    }

    pub fn display_name(&self, side: ContractSide) -> String {
        let non_empty = |value: &Option<String>| {
            value
                .as_deref()
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(str::to_string)
        };
        let title = non_empty(&self.title).unwrap_or_else(|| self.ticker.clone());
        let yes_sub_title = non_empty(&self.yes_sub_title);
        let sub_title = match side {
            ContractSide::Yes => yes_sub_title,
            // Kalshi often repeats the YES subtitle here, which would mislabel the NO side.
            ContractSide::No => {
                non_empty(&self.no_sub_title).filter(|no| Some(no) != yes_sub_title.as_ref())
            }
        };
        let label = side.label();
        match sub_title {
            Some(sub_title) if sub_title != title => format!("{title} ({label}: {sub_title})"),
            _ => format!("{title} ({label})"),
        }
    }
}

#[derive(Debug, Deserialize)]
struct Quote {
    close_dollars: String,
}

#[derive(Debug, Deserialize)]
struct TradePrice {
    #[serde(default)]
    close_dollars: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Candlestick {
    end_period_ts: i64,
    yes_bid: Quote,
    yes_ask: Quote,
    price: TradePrice,
}

impl Candlestick {
    /// YES value at the end of the period, preferring the closing quote midpoint over the last
    /// trade. Candlesticks carry no notional, so this assumes the standard $1 contract.
    fn yes_close(&self) -> Result<Option<Decimal>> {
        let bid = parse_decimal(&self.yes_bid.close_dollars)?;
        let ask = parse_decimal(&self.yes_ask.close_dollars)?;
        if let Some(mid) = quote_midpoint(bid, ask, Decimal::ONE) {
            return Ok(Some(mid));
        }
        parse_optional_decimal(&self.price.close_dollars)
    }
}

/// Kalshi price source using public market data.
pub struct KalshiPriceSource {
    client: reqwest::Client,
    base_url: String,
}

impl KalshiPriceSource {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: KALSHI_API_BASE.to_string(),
        }
    }

    /// Override the API host (useful for tests or the demo environment).
    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }

    async fn get<T: for<'de> Deserialize<'de>>(
        &self,
        endpoint: &str,
        query: &[(&str, &str)],
    ) -> Result<T> {
        let url = format!(
            "{}{KALSHI_API_PREFIX}{endpoint}",
            self.base_url.trim_end_matches('/')
        );
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
            anyhow::bail!("Kalshi request {endpoint} failed ({status}): {body}");
        }

        serde_json::from_str(&body)
            .with_context(|| format!("Failed to parse Kalshi response from {endpoint}"))
    }

    /// Fetch markets by ticker. Tickers Kalshi does not return are absent from the map.
    pub async fn fetch_markets(&self, tickers: &[&str]) -> Result<HashMap<String, KalshiMarket>> {
        #[derive(Deserialize)]
        struct Response {
            #[serde(default)]
            markets: Vec<KalshiMarket>,
        }

        let mut markets = HashMap::new();
        for chunk in tickers.chunks(MARKET_BATCH_SIZE) {
            let tickers = chunk.join(",");
            let limit = chunk.len().to_string();
            let response: Response = self
                .get("/markets", &[("tickers", &tickers), ("limit", &limit)])
                .await?;
            markets.extend(
                response
                    .markets
                    .into_iter()
                    .map(|market| (market.ticker.clone(), market)),
            );
        }
        Ok(markets)
    }

    async fn fetch_candlesticks(
        &self,
        ticker: &str,
        start: NaiveDate,
        end: NaiveDate,
    ) -> Result<Vec<Candlestick>> {
        #[derive(Deserialize)]
        struct MarketCandlesticks {
            market_ticker: String,
            #[serde(default)]
            candlesticks: Vec<Candlestick>,
        }
        #[derive(Deserialize)]
        struct Response {
            #[serde(default)]
            markets: Vec<MarketCandlesticks>,
        }

        let start_ts = eastern_midnight(start)?.timestamp();
        let end_ts = eastern_midnight(end + Duration::days(1))?.timestamp();
        let response: Response = self
            .get(
                "/markets/candlesticks",
                &[
                    ("market_tickers", ticker),
                    ("start_ts", &start_ts.to_string()),
                    ("end_ts", &end_ts.to_string()),
                    ("period_interval", DAILY_MINUTES),
                ],
            )
            .await?;
        Ok(response
            .markets
            .into_iter()
            .filter(|market| market.market_ticker == ticker)
            .flat_map(|market| market.candlesticks)
            .collect())
    }
}

impl Default for KalshiPriceSource {
    fn default() -> Self {
        Self::new()
    }
}

fn kalshi_contract(asset: &Asset) -> Option<(&str, ContractSide)> {
    match asset {
        Asset::Kalshi { ticker, side, .. } => Some((ticker.as_str(), *side)),
        _ => None,
    }
}

#[async_trait::async_trait]
impl PredictionMarketPriceSource for KalshiPriceSource {
    fn supports(&self, asset: &Asset) -> bool {
        matches!(asset, Asset::Kalshi { .. })
    }

    fn name(&self) -> &str {
        KALSHI_EXCHANGE
    }

    async fn fetch_quote(&self, asset: &Asset, asset_id: &AssetId) -> Result<Option<PricePoint>> {
        let Some((ticker, side)) = kalshi_contract(asset) else {
            return Ok(None);
        };
        let markets = self.fetch_markets(&[ticker]).await?;
        let Some(market) = markets.get(ticker) else {
            return Ok(None);
        };
        let now = Utc::now();
        Ok(Some(price_point(
            asset_id,
            now.date_naive(),
            now,
            market.mark_price(side)?,
            "USD",
            PriceKind::Quote,
            KALSHI_EXCHANGE,
        )))
    }

    async fn fetch_closes(
        &self,
        asset: &Asset,
        asset_id: &AssetId,
        start: NaiveDate,
        end: NaiveDate,
    ) -> Result<Vec<PricePoint>> {
        let Some((ticker, side)) = kalshi_contract(asset) else {
            return Ok(Vec::new());
        };

        let mut prices = Vec::new();
        for candle in self.fetch_candlesticks(ticker, start, end).await? {
            let Some(yes) = candle.yes_close()? else {
                continue;
            };
            let timestamp = DateTime::from_timestamp(candle.end_period_ts, 0)
                .context("Invalid Kalshi candlestick timestamp")?;
            // `end_period_ts` is the inclusive end of the period: the Eastern midnight after the day.
            let date = (timestamp - Duration::seconds(1))
                .with_timezone(&KALSHI_TZ)
                .date_naive();
            if date < start || date > end {
                continue;
            }
            let price = match side {
                ContractSide::Yes => yes,
                ContractSide::No => Decimal::ONE - yes,
            };
            prices.push(price_point(
                asset_id,
                date,
                timestamp,
                price,
                "USD",
                PriceKind::Close,
                KALSHI_EXCHANGE,
            ));
        }
        Ok(prices)
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/market_data/providers/kalshi_tests.rs"]
mod kalshi_tests;
