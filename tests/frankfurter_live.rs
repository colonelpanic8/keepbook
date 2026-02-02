//! Live integration test for Frankfurter FX provider
//! Run with: cargo test --test frankfurter_live -- --ignored

use chrono::NaiveDate;
use keepbook::market_data::providers::frankfurter::FrankfurterRateSource;
use keepbook::market_data::FxRateSource;

#[tokio::test]
#[ignore] // Run manually with --ignored flag
async fn test_frankfurter_live_usd_eur() {
    let provider = FrankfurterRateSource::new();

    // Use a recent weekday date
    let date = NaiveDate::from_ymd_opt(2025, 1, 15).unwrap();

    let result = provider.fetch_close("USD", "EUR", date).await;

    match result {
        Ok(Some(fx_point)) => {
            println!("USD/EUR rate on {}: {}", date, fx_point.rate);
            assert!(!fx_point.rate.is_empty());
            // USD/EUR should be somewhere around 0.9-1.0
            let rate: f64 = fx_point.rate.parse().unwrap();
            assert!(rate > 0.5 && rate < 2.0, "Rate {rate} seems unreasonable");
        }
        Ok(None) => panic!("No FX rate returned"),
        Err(e) => panic!("Error fetching FX rate: {e}"),
    }
}

#[tokio::test]
#[ignore]
async fn test_frankfurter_live_eur_gbp() {
    let provider = FrankfurterRateSource::new();
    let date = NaiveDate::from_ymd_opt(2025, 1, 15).unwrap();

    let result = provider.fetch_close("EUR", "GBP", date).await;

    match result {
        Ok(Some(fx_point)) => {
            println!("EUR/GBP rate on {}: {}", date, fx_point.rate);
            assert!(!fx_point.rate.is_empty());
        }
        Ok(None) => panic!("No FX rate returned"),
        Err(e) => panic!("Error fetching FX rate: {e}"),
    }
}

#[tokio::test]
#[ignore]
async fn test_frankfurter_live_cross_rate_usd_gbp() {
    let provider = FrankfurterRateSource::new();
    let date = NaiveDate::from_ymd_opt(2025, 1, 15).unwrap();

    // This tests the cross-rate computation (neither currency is EUR)
    let result = provider.fetch_close("USD", "GBP", date).await;

    match result {
        Ok(Some(fx_point)) => {
            println!("USD/GBP cross-rate on {}: {}", date, fx_point.rate);
            assert!(!fx_point.rate.is_empty());
            // USD/GBP should be around 0.7-0.9
            let rate: f64 = fx_point.rate.parse().unwrap();
            assert!(rate > 0.5 && rate < 1.5, "Rate {rate} seems unreasonable");
        }
        Ok(None) => panic!("No FX rate returned"),
        Err(e) => panic!("Error fetching FX rate: {e}"),
    }
}
