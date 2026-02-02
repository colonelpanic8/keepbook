//! CoinGecko crypto price provider implementation.
//!
//! Uses CoinGecko's free API to fetch historical daily prices for cryptocurrencies.
//! The `/coins/{id}/history` endpoint returns price data for a specific date.

use std::collections::HashMap;

use anyhow::{anyhow, Result};
use chrono::{NaiveDate, Utc};
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
            quote_currency: "usd".to_string(),
            custom_mappings: HashMap::new(),
        }
    }

    /// Creates a new CoinGecko provider with a custom reqwest client.
    pub fn with_client(client: reqwest::Client) -> Self {
        Self {
            client,
            quote_currency: "usd".to_string(),
            custom_mappings: HashMap::new(),
        }
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
    pub fn with_mapping(mut self, symbol: impl Into<String>, coingecko_id: impl Into<String>) -> Self {
        self.custom_mappings.insert(symbol.into().to_uppercase(), coingecko_id.into());
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
            "{}/coins/{}/history?date={}&localization=false",
            COINGECKO_API_BASE, coingecko_id, date_str
        );

        let response = self
            .client
            .get(&url)
            .header("Accept", "application/json")
            .header("User-Agent", "keepbook/0.1.0 (https://github.com/keepbook/keepbook)")
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow!(
                "CoinGecko API error: {} - {}",
                status,
                body
            ));
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

    async fn fetch_quote(
        &self,
        asset: &Asset,
        asset_id: &AssetId,
    ) -> Result<Option<PricePoint>> {
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
            .header("User-Agent", "keepbook/0.1.0 (https://github.com/keepbook/keepbook)")
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow!(
                "CoinGecko simple/price API error: {} - {}",
                status,
                body
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
mod tests {
    use super::*;

    /// Sample CoinGecko API response for Bitcoin on 2024-01-15
    const SAMPLE_BTC_RESPONSE: &str = r#"{
        "id": "bitcoin",
        "symbol": "btc",
        "name": "Bitcoin",
        "market_data": {
            "current_price": {
                "usd": 42850.12,
                "eur": 39234.56,
                "gbp": 33891.23
            },
            "market_cap": {
                "usd": 840123456789
            },
            "total_volume": {
                "usd": 25678901234
            }
        }
    }"#;

    /// Sample response with no market data (e.g., very old date or delisted coin)
    const SAMPLE_NO_MARKET_DATA_RESPONSE: &str = r#"{
        "id": "bitcoin",
        "symbol": "btc",
        "name": "Bitcoin"
    }"#;

    /// Sample response for Ethereum
    const SAMPLE_ETH_RESPONSE: &str = r#"{
        "id": "ethereum",
        "symbol": "eth",
        "name": "Ethereum",
        "market_data": {
            "current_price": {
                "usd": 2534.89,
                "eur": 2321.45
            }
        }
    }"#;

    #[test]
    fn test_parse_btc_response() {
        let response: CoinHistoryResponse =
            serde_json::from_str(SAMPLE_BTC_RESPONSE).expect("Failed to parse BTC response");

        assert_eq!(response.id, "bitcoin");
        assert_eq!(response.symbol, "btc");
        assert_eq!(response.name, "Bitcoin");

        let market_data = response.market_data.expect("Should have market data");
        let usd_price = market_data
            .current_price
            .get("usd")
            .expect("Should have USD price");
        assert!((usd_price - 42850.12).abs() < 0.01);
    }

    #[test]
    fn test_parse_no_market_data_response() {
        let response: CoinHistoryResponse =
            serde_json::from_str(SAMPLE_NO_MARKET_DATA_RESPONSE)
                .expect("Failed to parse response");

        assert_eq!(response.id, "bitcoin");
        assert!(response.market_data.is_none());
    }

    #[test]
    fn test_parse_eth_response() {
        let response: CoinHistoryResponse =
            serde_json::from_str(SAMPLE_ETH_RESPONSE).expect("Failed to parse ETH response");

        assert_eq!(response.id, "ethereum");
        assert_eq!(response.symbol, "eth");

        let market_data = response.market_data.expect("Should have market data");
        let usd_price = market_data
            .current_price
            .get("usd")
            .expect("Should have USD price");
        assert!((usd_price - 2534.89).abs() < 0.01);

        let eur_price = market_data
            .current_price
            .get("eur")
            .expect("Should have EUR price");
        assert!((eur_price - 2321.45).abs() < 0.01);
    }

    #[test]
    fn test_symbol_to_coingecko_id_common_symbols() {
        let provider = CoinGeckoPriceSource::new();

        assert_eq!(
            provider.symbol_to_coingecko_id("BTC", None),
            Some("bitcoin".to_string())
        );
        assert_eq!(
            provider.symbol_to_coingecko_id("btc", None), // lowercase input
            Some("bitcoin".to_string())
        );
        assert_eq!(
            provider.symbol_to_coingecko_id("ETH", None),
            Some("ethereum".to_string())
        );
        assert_eq!(
            provider.symbol_to_coingecko_id("USDC", None),
            Some("usd-coin".to_string())
        );
        assert_eq!(
            provider.symbol_to_coingecko_id("SOL", None),
            Some("solana".to_string())
        );
        assert_eq!(
            provider.symbol_to_coingecko_id("AVAX", None),
            Some("avalanche-2".to_string())
        );
    }

    #[test]
    fn test_symbol_to_coingecko_id_unknown_symbol() {
        let provider = CoinGeckoPriceSource::new();

        assert_eq!(provider.symbol_to_coingecko_id("UNKNOWN123", None), None);
    }

    #[test]
    fn test_custom_mapping_overrides_default() {
        let provider = CoinGeckoPriceSource::new()
            .with_mapping("BTC", "wrapped-bitcoin"); // Override BTC mapping

        assert_eq!(
            provider.symbol_to_coingecko_id("BTC", None),
            Some("wrapped-bitcoin".to_string())
        );

        // Other mappings still work
        assert_eq!(
            provider.symbol_to_coingecko_id("ETH", None),
            Some("ethereum".to_string())
        );
    }

    #[test]
    fn test_custom_mapping_for_new_symbol() {
        let provider = CoinGeckoPriceSource::new()
            .with_mapping("MYCOIN", "my-custom-coin-id");

        assert_eq!(
            provider.symbol_to_coingecko_id("MYCOIN", None),
            Some("my-custom-coin-id".to_string())
        );
        assert_eq!(
            provider.symbol_to_coingecko_id("mycoin", None), // lowercase
            Some("my-custom-coin-id".to_string())
        );
    }

    #[test]
    fn test_quote_currency_configuration() {
        let provider = CoinGeckoPriceSource::new().with_quote_currency("EUR");
        assert_eq!(provider.quote_currency, "eur");

        let provider = CoinGeckoPriceSource::new().with_quote_currency("gbp");
        assert_eq!(provider.quote_currency, "gbp");
    }

    #[test]
    fn test_provider_name() {
        let provider = CoinGeckoPriceSource::new();
        assert_eq!(provider.name(), "coingecko");
    }

    #[test]
    fn test_default_implementation() {
        let provider = CoinGeckoPriceSource::default();
        assert_eq!(provider.quote_currency, "usd");
        assert!(provider.custom_mappings.is_empty());
    }

    #[test]
    fn test_with_custom_mappings_bulk() {
        let mut mappings = HashMap::new();
        mappings.insert("COIN1".to_string(), "coin-one-id".to_string());
        mappings.insert("COIN2".to_string(), "coin-two-id".to_string());

        let provider = CoinGeckoPriceSource::new().with_custom_mappings(mappings);

        assert_eq!(
            provider.symbol_to_coingecko_id("COIN1", None),
            Some("coin-one-id".to_string())
        );
        assert_eq!(
            provider.symbol_to_coingecko_id("COIN2", None),
            Some("coin-two-id".to_string())
        );
    }

    #[tokio::test]
    async fn test_fetch_close_non_crypto_asset_returns_none() {
        let provider = CoinGeckoPriceSource::new();
        let asset = Asset::equity("AAPL");
        let asset_id = AssetId::from_asset(&asset);
        let date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();

        let result = provider.fetch_close(&asset, &asset_id, date).await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[test]
    fn test_date_format_for_api() {
        // Verify the date format matches CoinGecko's expected format (dd-mm-yyyy)
        let date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();
        let formatted = date.format("%d-%m-%Y").to_string();
        assert_eq!(formatted, "15-01-2024");

        let date = NaiveDate::from_ymd_opt(2023, 12, 31).unwrap();
        let formatted = date.format("%d-%m-%Y").to_string();
        assert_eq!(formatted, "31-12-2023");
    }

    #[test]
    fn test_all_major_crypto_mappings_exist() {
        let provider = CoinGeckoPriceSource::new();

        // Test that common cryptocurrencies are mapped
        let major_cryptos = vec![
            "BTC", "ETH", "USDT", "USDC", "BNB", "XRP", "ADA", "DOGE", "SOL", "DOT",
            "MATIC", "LTC", "SHIB", "AVAX", "DAI", "LINK", "ATOM", "UNI",
        ];

        for symbol in major_cryptos {
            assert!(
                provider.symbol_to_coingecko_id(symbol, None).is_some(),
                "Missing mapping for {}",
                symbol
            );
        }
    }

    #[test]
    fn test_defi_token_mappings() {
        let provider = CoinGeckoPriceSource::new();

        assert_eq!(
            provider.symbol_to_coingecko_id("AAVE", None),
            Some("aave".to_string())
        );
        assert_eq!(
            provider.symbol_to_coingecko_id("MKR", None),
            Some("maker".to_string())
        );
        assert_eq!(
            provider.symbol_to_coingecko_id("CRV", None),
            Some("curve-dao-token".to_string())
        );
        assert_eq!(
            provider.symbol_to_coingecko_id("COMP", None),
            Some("compound-governance-token".to_string())
        );
    }

    #[test]
    fn test_layer2_token_mappings() {
        let provider = CoinGeckoPriceSource::new();

        assert_eq!(
            provider.symbol_to_coingecko_id("ARB", None),
            Some("arbitrum".to_string())
        );
        assert_eq!(
            provider.symbol_to_coingecko_id("OP", None),
            Some("optimism".to_string())
        );
        assert_eq!(
            provider.symbol_to_coingecko_id("MATIC", None),
            Some("matic-network".to_string())
        );
    }

    #[test]
    fn test_wrapped_token_mappings() {
        let provider = CoinGeckoPriceSource::new();

        assert_eq!(
            provider.symbol_to_coingecko_id("WBTC", None),
            Some("wrapped-bitcoin".to_string())
        );
        assert_eq!(
            provider.symbol_to_coingecko_id("WETH", None),
            Some("weth".to_string())
        );
        assert_eq!(
            provider.symbol_to_coingecko_id("STETH", None),
            Some("staked-ether".to_string())
        );
    }
}
