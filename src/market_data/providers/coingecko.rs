//! CoinGecko crypto price provider implementation.
//!
//! Uses CoinGecko's free API to fetch historical daily prices for cryptocurrencies.
//! The `/coins/{id}/history` endpoint returns price data for a specific date.

use std::collections::HashMap;

use anyhow::{anyhow, Result};
use chrono::{Duration, NaiveDate, Utc};
use serde::Deserialize;

use crate::market_data::{AssetId, CryptoPriceSource, PriceKind, PricePoint};
use crate::models::Asset;

const COINGECKO_API_BASE: &str = "https://api.coingecko.com/api/v3";

/// CoinGecko API response for historical coin data.
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct CoinHistoryResponse {
    id: String,
    symbol: String,
    name: String,
    market_data: Option<MarketData>,
}

#[derive(Debug, Deserialize)]
struct MarketData {
    current_price: HashMap<String, f64>,
}

/// CoinGecko crypto price provider.
///
/// Fetches historical daily close prices from CoinGecko's free API.
/// No API key is required for basic usage, though rate limits apply.
pub struct CoinGeckoPriceSource {
    client: reqwest::Client,
    base_url: String,
    /// Quote currency for prices (e.g., "usd", "eur")
    quote_currency: String,
    /// Custom symbol to CoinGecko ID mappings (overrides defaults)
    custom_mappings: HashMap<String, String>,
}

impl CoinGeckoPriceSource {
    /// Creates a new CoinGecko provider with USD as the default quote currency.
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: COINGECKO_API_BASE.to_string(),
            quote_currency: "usd".to_string(),
            custom_mappings: HashMap::new(),
        }
    }

    /// Creates a new CoinGecko provider with a custom reqwest client.
    pub fn with_client(client: reqwest::Client) -> Self {
        Self {
            client,
            base_url: COINGECKO_API_BASE.to_string(),
            quote_currency: "usd".to_string(),
            custom_mappings: HashMap::new(),
        }
    }

    /// Override the base URL (useful for tests).
    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }

    /// Sets the quote currency for price lookups.
    pub fn with_quote_currency(mut self, currency: impl Into<String>) -> Self {
        self.quote_currency = currency.into().to_lowercase();
        self
    }

    /// Adds custom symbol to CoinGecko ID mappings.
    pub fn with_custom_mappings(mut self, mappings: HashMap<String, String>) -> Self {
        self.custom_mappings = mappings;
        self
    }

    /// Adds a single custom mapping from symbol to CoinGecko ID.
    pub fn with_mapping(
        mut self,
        symbol: impl Into<String>,
        coingecko_id: impl Into<String>,
    ) -> Self {
        self.custom_mappings
            .insert(symbol.into().to_uppercase(), coingecko_id.into());
        self
    }

    /// Maps a crypto symbol to a CoinGecko coin ID.
    ///
    /// First checks custom mappings, then falls back to built-in common mappings.
    /// Returns None if no mapping is found.
    fn symbol_to_coingecko_id(&self, symbol: &str, _network: Option<&str>) -> Option<String> {
        let symbol_upper = symbol.to_uppercase();

        // Check custom mappings first
        if let Some(id) = self.custom_mappings.get(&symbol_upper) {
            return Some(id.clone());
        }

        // Built-in mappings for common cryptocurrencies
        let id = match symbol_upper.as_str() {
            // Major cryptocurrencies
            "BTC" => "bitcoin",
            "ETH" => "ethereum",
            "USDT" => "tether",
            "USDC" => "usd-coin",
            "BNB" => "binancecoin",
            "XRP" => "ripple",
            "ADA" => "cardano",
            "DOGE" => "dogecoin",
            "SOL" => "solana",
            "DOT" => "polkadot",
            "MATIC" | "POL" => "matic-network",
            "LTC" => "litecoin",
            "SHIB" => "shiba-inu",
            "TRX" => "tron",
            "AVAX" => "avalanche-2",
            "DAI" => "dai",
            "LINK" => "chainlink",
            "ATOM" => "cosmos",
            "UNI" => "uniswap",
            "ETC" => "ethereum-classic",
            "XLM" => "stellar",
            "BCH" => "bitcoin-cash",
            "ALGO" => "algorand",
            "FIL" => "filecoin",
            "VET" => "vechain",
            "ICP" => "internet-computer",
            "HBAR" => "hedera-hashgraph",
            "NEAR" => "near",
            "APT" => "aptos",
            "ARB" => "arbitrum",
            "OP" => "optimism",
            "AAVE" => "aave",
            "MKR" => "maker",
            "CRV" => "curve-dao-token",
            "SNX" => "havven",
            "COMP" => "compound-governance-token",
            "GRT" => "the-graph",
            "FTM" => "fantom",
            "SAND" => "the-sandbox",
            "MANA" => "decentraland",
            "AXS" => "axie-infinity",
            "ENJ" => "enjincoin",
            "CHZ" => "chiliz",
            "XMR" => "monero",
            "ZEC" => "zcash",
            "DASH" => "dash",
            "XTZ" => "tezos",
            "EOS" => "eos",
            "THETA" => "theta-token",
            "NEO" => "neo",
            "KLAY" => "klay-token",
            "FLOW" => "flow",
            "EGLD" => "elrond-erd-2",
            "XEC" => "ecash",
            "RUNE" => "thorchain",
            "KSM" => "kusama",
            "ZIL" => "zilliqa",
            "BAT" => "basic-attention-token",
            "ENS" => "ethereum-name-service",
            "LDO" => "lido-dao",
            "RPL" => "rocket-pool",
            "CRO" => "crypto-com-chain",
            "WBTC" => "wrapped-bitcoin",
            "WETH" => "weth",
            "STETH" => "staked-ether",
            _ => return None,
        };

        Some(id.to_string())
    }

    /// Fetches historical price data from CoinGecko.
    async fn fetch_history(
        &self,
        coingecko_id: &str,
        date: NaiveDate,
    ) -> Result<CoinHistoryResponse> {
        // CoinGecko expects date in dd-mm-yyyy format
        let date_str = date.format("%d-%m-%Y").to_string();

        let url = format!(
            "{}/coins/{coingecko_id}/history?date={date_str}&localization=false",
            self.base_url.trim_end_matches('/')
        );

        let response = self
            .client
            .get(&url)
            .header("Accept", "application/json")
            .header(
                "User-Agent",
                "keepbook/0.2.0 (https://github.com/keepbook/keepbook)",
            )
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow!("CoinGecko API error: {status} - {body}"));
        }

        let data: CoinHistoryResponse = response.json().await?;
        Ok(data)
    }
}

impl Default for CoinGeckoPriceSource {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl CryptoPriceSource for CoinGeckoPriceSource {
    async fn fetch_close(
        &self,
        asset: &Asset,
        asset_id: &AssetId,
        date: NaiveDate,
    ) -> Result<Option<PricePoint>> {
        // CoinGecko public API limits historical queries to the last 365 days.
        // Avoid hammering the API for dates we know it can't satisfy.
        let oldest_allowed = Utc::now().date_naive() - Duration::days(365);
        if date < oldest_allowed {
            return Ok(None);
        }

        // Extract symbol and network from the asset
        let (symbol, network) = match asset {
            Asset::Crypto { symbol, network } => (symbol.as_str(), network.as_deref()),
            _ => return Ok(None), // Not a crypto asset
        };

        // Map symbol to CoinGecko ID
        let coingecko_id = match self.symbol_to_coingecko_id(symbol, network) {
            Some(id) => id,
            None => {
                // Try using the symbol as-is (lowercase) as a fallback
                symbol.to_lowercase()
            }
        };

        // Fetch historical data
        let history = self.fetch_history(&coingecko_id, date).await?;

        // Extract price from market data
        let market_data = match history.market_data {
            Some(md) => md,
            None => return Ok(None), // No market data for this date
        };

        let price = match market_data.current_price.get(&self.quote_currency) {
            Some(p) => *p,
            None => return Ok(None), // Price not available in requested currency
        };

        Ok(Some(PricePoint {
            asset_id: asset_id.clone(),
            as_of_date: date,
            timestamp: Utc::now(),
            price: price.to_string(),
            quote_currency: self.quote_currency.to_uppercase(),
            kind: PriceKind::Close,
            source: self.name().to_string(),
        }))
    }

    async fn fetch_quote(&self, asset: &Asset, asset_id: &AssetId) -> Result<Option<PricePoint>> {
        // Extract symbol and network from the asset
        let (symbol, network) = match asset {
            Asset::Crypto { symbol, network } => (symbol.as_str(), network.as_deref()),
            _ => return Ok(None), // Not a crypto asset
        };

        // Map symbol to CoinGecko ID
        let coingecko_id = match self.symbol_to_coingecko_id(symbol, network) {
            Some(id) => id,
            None => symbol.to_lowercase(),
        };

        // Use /simple/price endpoint for current price
        let url = format!(
            "{}/simple/price?ids={}&vs_currencies={}",
            COINGECKO_API_BASE, coingecko_id, self.quote_currency
        );

        let response = self
            .client
            .get(&url)
            .header("Accept", "application/json")
            .header(
                "User-Agent",
                "keepbook/0.2.0 (https://github.com/keepbook/keepbook)",
            )
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow!(
                "CoinGecko simple/price API error: {status} - {body}"
            ));
        }

        let data: std::collections::HashMap<String, std::collections::HashMap<String, f64>> =
            response.json().await?;

        let price = data
            .get(&coingecko_id)
            .and_then(|prices| prices.get(&self.quote_currency))
            .copied();

        let Some(price) = price else {
            return Ok(None);
        };

        let now = Utc::now();

        Ok(Some(PricePoint {
            asset_id: asset_id.clone(),
            as_of_date: now.date_naive(),
            timestamp: now,
            price: price.to_string(),
            quote_currency: self.quote_currency.to_uppercase(),
            kind: PriceKind::Quote,
            source: self.name().to_string(),
        }))
    }

    fn name(&self) -> &str {
        "coingecko"
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/market_data/providers/coingecko_tests.rs"]
mod coingecko_tests;
