//! Live integration test for CoinGecko provider
//! Run with: cargo test --test coingecko_live -- --ignored

use chrono::Utc;
use keepbook::market_data::providers::coingecko::CoinGeckoPriceSource;
use keepbook::market_data::{AssetId, CryptoPriceSource};
use keepbook::models::Asset;

#[tokio::test]
#[ignore] // Run manually with --ignored flag
async fn test_coingecko_live_btc() {
    let provider = CoinGeckoPriceSource::new();

    let asset = Asset::Crypto {
        symbol: "BTC".to_string(),
        network: None,
    };
    let asset_id = AssetId::from_asset(&asset);

    // Use a date from a few days ago to ensure data exists
    // CoinGecko free API only allows historical data within the past 365 days
    let date = (Utc::now() - chrono::Duration::days(7)).date_naive();

    let result = provider.fetch_close(&asset, &asset_id, date).await;

    match result {
        Ok(Some(price_point)) => {
            println!("BTC price on {}: {} {}", date, price_point.price, price_point.quote_currency);
            assert!(!price_point.price.is_empty());
            assert_eq!(price_point.quote_currency.to_lowercase(), "usd");
        }
        Ok(None) => panic!("No price data returned for BTC"),
        Err(e) => panic!("Error fetching BTC price: {e}"),
    }
}

#[tokio::test]
#[ignore]
async fn test_coingecko_live_eth() {
    let provider = CoinGeckoPriceSource::new();

    let asset = Asset::Crypto {
        symbol: "ETH".to_string(),
        network: None,
    };
    let asset_id = AssetId::from_asset(&asset);
    // Use a date from a few days ago to ensure data exists
    // CoinGecko free API only allows historical data within the past 365 days
    let date = (Utc::now() - chrono::Duration::days(7)).date_naive();

    let result = provider.fetch_close(&asset, &asset_id, date).await;

    match result {
        Ok(Some(price_point)) => {
            println!("ETH price on {}: {} {}", date, price_point.price, price_point.quote_currency);
            assert!(!price_point.price.is_empty());
        }
        Ok(None) => panic!("No price data returned for ETH"),
        Err(e) => panic!("Error fetching ETH price: {e}"),
    }
}
