use super::*;

/// Sample Frankfurter API response for EUR to USD/GBP on 2024-01-15.
const SAMPLE_EUR_RESPONSE: &str = r#"{
    "amount": 1.0,
    "base": "EUR",
    "date": "2024-01-15",
    "rates": {
        "USD": 1.0956,
        "GBP": 0.8623
    }
}"#;

/// Sample response for EUR to USD only.
const SAMPLE_EUR_USD_RESPONSE: &str = r#"{
    "amount": 1.0,
    "base": "EUR",
    "date": "2024-01-15",
    "rates": {
        "USD": 1.0956
    }
}"#;

#[test]
fn test_parse_frankfurter_response() {
    let response: FrankfurterResponse =
        serde_json::from_str(SAMPLE_EUR_RESPONSE).expect("Failed to parse response");

    assert_eq!(response.amount, 1.0);
    assert_eq!(response.base, "EUR");
    assert_eq!(response.date, NaiveDate::from_ymd_opt(2024, 1, 15).unwrap());
    assert_eq!(response.rates.len(), 2);
    assert!((response.rates["USD"] - 1.0956).abs() < 0.0001);
    assert!((response.rates["GBP"] - 0.8623).abs() < 0.0001);
}

#[test]
fn test_parse_single_currency_response() {
    let response: FrankfurterResponse =
        serde_json::from_str(SAMPLE_EUR_USD_RESPONSE).expect("Failed to parse response");

    assert_eq!(response.rates.len(), 1);
    assert!((response.rates["USD"] - 1.0956).abs() < 0.0001);
}

#[test]
fn test_compute_cross_rate() {
    // EUR/USD = 1.0956
    // EUR/GBP = 0.8623
    // USD/GBP = EUR/GBP / EUR/USD = 0.8623 / 1.0956 = 0.7870 (approx)
    let eur_to_usd = 1.0956;
    let eur_to_gbp = 0.8623;
    let usd_to_gbp = FrankfurterRateSource::compute_cross_rate(eur_to_usd, eur_to_gbp);

    assert!((usd_to_gbp - 0.7870).abs() < 0.001);
}

#[test]
fn test_compute_cross_rate_inverse() {
    // If we have EUR/USD = 1.0956 and EUR/GBP = 0.8623
    // GBP/USD = EUR/USD / EUR/GBP = 1.0956 / 0.8623 = 1.2706 (approx)
    let eur_to_gbp = 0.8623;
    let eur_to_usd = 1.0956;
    let gbp_to_usd = FrankfurterRateSource::compute_cross_rate(eur_to_gbp, eur_to_usd);

    assert!((gbp_to_usd - 1.2706).abs() < 0.001);
}

#[test]
fn test_provider_name() {
    let provider = FrankfurterRateSource::new();
    assert_eq!(provider.name(), "frankfurter");
}

#[test]
fn test_provider_default() {
    let provider = FrankfurterRateSource::default();
    assert_eq!(provider.name(), "frankfurter");
}

// Mock-based tests would require a mock HTTP server.
// For now, we test the parsing and cross-rate computation logic.
// Integration tests with the live API should be in a separate test file
// and gated behind a feature flag or ignored by default.

#[tokio::test]
async fn test_same_currency_returns_one() {
    let provider = FrankfurterRateSource::new();
    let date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();

    let result = provider
        .fetch_close("USD", "USD", date)
        .await
        .expect("Should succeed for same currency");

    let rate_point = result.expect("Should return a rate point");
    assert_eq!(rate_point.base, "USD");
    assert_eq!(rate_point.quote, "USD");
    assert_eq!(rate_point.rate, "1");
    assert_eq!(rate_point.source, "frankfurter");
    assert_eq!(rate_point.kind, FxRateKind::Close);
}

#[tokio::test]
async fn test_case_insensitive_currencies() {
    let provider = FrankfurterRateSource::new();
    let date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();

    // Same currency with different cases should still return 1
    let result = provider
        .fetch_close("usd", "USD", date)
        .await
        .expect("Should succeed");

    let rate_point = result.expect("Should return a rate point");
    assert_eq!(rate_point.base, "USD");
    assert_eq!(rate_point.quote, "USD");
    assert_eq!(rate_point.rate, "1");
}
