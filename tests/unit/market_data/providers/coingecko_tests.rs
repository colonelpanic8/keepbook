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
        serde_json::from_str(SAMPLE_NO_MARKET_DATA_RESPONSE).expect("Failed to parse response");

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
    let provider = CoinGeckoPriceSource::new().with_mapping("BTC", "wrapped-bitcoin"); // Override BTC mapping

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
    let provider = CoinGeckoPriceSource::new().with_mapping("MYCOIN", "my-custom-coin-id");

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
        "BTC", "ETH", "USDT", "USDC", "BNB", "XRP", "ADA", "DOGE", "SOL", "DOT", "MATIC", "LTC",
        "SHIB", "AVAX", "DAI", "LINK", "ATOM", "UNI",
    ];

    for symbol in major_cryptos {
        assert!(
            provider.symbol_to_coingecko_id(symbol, None).is_some(),
            "Missing mapping for {symbol}"
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
