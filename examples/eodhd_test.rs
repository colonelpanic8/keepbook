//! Test script for the EODHD provider with live API.
//!
//! Run with: cargo run --example eodhd_test
//!
//! Requires the API key to be stored in pass at `keepbook/eodhd-api-key`.

use std::process::Command;

use chrono::NaiveDate;
use keepbook::market_data::providers::eodhd::EodhdProvider;
use keepbook::market_data::{AssetId, EquityPriceSource};
use keepbook::models::Asset;

fn get_api_key_from_pass() -> anyhow::Result<String> {
    let output = Command::new("pass")
        .arg("show")
        .arg("keepbook/eodhd-api-key")
        .output()?;

    if !output.status.success() {
        anyhow::bail!(
            "Failed to get API key from pass: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    Ok(String::from_utf8(output.stdout)?.trim().to_string())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("EODHD Provider Live Test");
    println!("========================\n");

    // Get API key from pass
    let api_key = get_api_key_from_pass()?;
    println!("API key retrieved from pass: {}...{}", &api_key[..8], &api_key[api_key.len()-4..]);

    // Create provider
    let provider = EodhdProvider::new(&api_key);
    println!("Provider name: {}\n", provider.name());

    // Test with AAPL on a recent trading day (Thursday Jan 30, 2026)
    // Note: Free tier only allows data from within the last year
    let asset = Asset::equity("AAPL");
    let asset_id = AssetId::from_asset(&asset);
    let test_date = NaiveDate::from_ymd_opt(2026, 1, 30).unwrap();

    println!("Fetching close price for:");
    println!("  Asset: {:?}", asset);
    println!("  Asset ID: {}", asset_id);
    println!("  Date: {}\n", test_date);

    match provider.fetch_close(&asset, &asset_id, test_date).await {
        Ok(Some(price_point)) => {
            println!("SUCCESS! Got PricePoint:");
            println!("  Price: {} {}", price_point.price, price_point.quote_currency);
            println!("  As of date: {}", price_point.as_of_date);
            println!("  Kind: {:?}", price_point.kind);
            println!("  Source: {}", price_point.source);
            println!("  Timestamp: {}", price_point.timestamp);
        }
        Ok(None) => {
            println!("No data returned for the requested date.");
            println!("This might happen if the market was closed.");
        }
        Err(e) => {
            println!("ERROR: {}", e);
            return Err(e);
        }
    }

    // Test with a UK stock (VOD on LSE) to verify exchange mapping
    println!("\n--- Testing UK Stock (VOD.LSE) ---\n");
    let uk_asset = Asset::Equity {
        ticker: "VOD".to_string(),
        exchange: Some("LSE".to_string()),
    };
    let uk_asset_id = AssetId::from_asset(&uk_asset);

    println!("Fetching close price for:");
    println!("  Asset: {:?}", uk_asset);
    println!("  Date: {}\n", test_date);

    match provider.fetch_close(&uk_asset, &uk_asset_id, test_date).await {
        Ok(Some(price_point)) => {
            println!("SUCCESS! Got PricePoint:");
            println!("  Price: {} {}", price_point.price, price_point.quote_currency);
            println!("  As of date: {}", price_point.as_of_date);
            println!("  Kind: {:?}", price_point.kind);
            println!("  Source: {}", price_point.source);
        }
        Ok(None) => {
            println!("No data returned for VOD on the requested date.");
        }
        Err(e) => {
            println!("ERROR fetching VOD: {}", e);
        }
    }

    println!("\n========================");
    println!("Live test completed!");

    Ok(())
}
