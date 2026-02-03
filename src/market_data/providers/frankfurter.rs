//! Frankfurter FX rate provider using ECB daily reference rates.
//!
//! The Frankfurter API provides free access to ECB exchange rates.
//! ECB publishes rates with EUR as the base currency, so cross-rate
//! computation is needed when requesting non-EUR base currencies.

use std::collections::HashMap;

use anyhow::{anyhow, Result};
use chrono::{NaiveDate, Utc};
use reqwest::Client;
use serde::Deserialize;

use crate::market_data::{FxRateKind, FxRatePoint, FxRateSource};

const FRANKFURTER_BASE_URL: &str = "https://api.frankfurter.app";

/// Response from Frankfurter API for a specific date.
#[derive(Debug, Deserialize)]
struct FrankfurterResponse {
    /// The amount (always 1 for our requests).
    #[allow(dead_code)]
    amount: f64,
    /// The base currency.
    #[allow(dead_code)]
    base: String,
    /// The date of the rates.
    #[allow(dead_code)]
    date: NaiveDate,
    /// Map of currency codes to rates.
    rates: HashMap<String, f64>,
}

/// Frankfurter FX rate provider.
///
/// Uses the Frankfurter API which provides ECB (European Central Bank)
/// daily reference exchange rates. No API key is required.
#[derive(Debug, Clone)]
pub struct FrankfurterRateSource {
    client: Client,
}

impl FrankfurterRateSource {
    /// Creates a new Frankfurter provider with a default HTTP client.
    pub fn new() -> Self {
        Self {
            client: Client::new(),
        }
    }

    /// Creates a new Frankfurter provider with a custom HTTP client.
    pub fn with_client(client: Client) -> Self {
        Self { client }
    }

    /// Fetches rates from Frankfurter API with EUR as base.
    async fn fetch_eur_rates(
        &self,
        currencies: &[&str],
        date: NaiveDate,
    ) -> Result<HashMap<String, f64>> {
        let symbols = currencies.join(",");
        let url = format!("{FRANKFURTER_BASE_URL}/{date}?from=EUR&to={symbols}");

        let response = self
            .client
            .get(&url)
            .send()
            .await?
            .error_for_status()?
            .json::<FrankfurterResponse>()
            .await?;

        Ok(response.rates)
    }

    /// Computes the cross-rate for base/quote when base != EUR.
    ///
    /// Given EUR/base and EUR/quote rates, computes base/quote = (EUR/quote) / (EUR/base).
    fn compute_cross_rate(eur_to_base: f64, eur_to_quote: f64) -> f64 {
        eur_to_quote / eur_to_base
    }
}

impl Default for FrankfurterRateSource {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl FxRateSource for FrankfurterRateSource {
    async fn fetch_close(
        &self,
        base: &str,
        quote: &str,
        date: NaiveDate,
    ) -> Result<Option<FxRatePoint>> {
        let base_upper = base.to_uppercase();
        let quote_upper = quote.to_uppercase();

        // Handle same currency case
        if base_upper == quote_upper {
            return Ok(Some(FxRatePoint {
                base: base_upper,
                quote: quote_upper,
                as_of_date: date,
                timestamp: Utc::now(),
                rate: "1".to_string(),
                kind: FxRateKind::Close,
                source: "frankfurter".to_string(),
            }));
        }

        let rate = if base_upper == "EUR" {
            // Direct EUR-based rate
            let rates = self.fetch_eur_rates(&[&quote_upper], date).await?;
            rates
                .get(&quote_upper)
                .copied()
                .ok_or_else(|| anyhow!("Quote currency {quote_upper} not found in response"))?
        } else if quote_upper == "EUR" {
            // Inverse rate: base/EUR = 1 / (EUR/base)
            let rates = self.fetch_eur_rates(&[&base_upper], date).await?;
            let eur_to_base = rates
                .get(&base_upper)
                .copied()
                .ok_or_else(|| anyhow!("Base currency {base_upper} not found in response"))?;
            1.0 / eur_to_base
        } else {
            // Cross-rate: base/quote via EUR
            let rates = self
                .fetch_eur_rates(&[&base_upper, &quote_upper], date)
                .await?;
            let eur_to_base = rates
                .get(&base_upper)
                .copied()
                .ok_or_else(|| anyhow!("Base currency {base_upper} not found in response"))?;
            let eur_to_quote = rates
                .get(&quote_upper)
                .copied()
                .ok_or_else(|| anyhow!("Quote currency {quote_upper} not found in response"))?;
            Self::compute_cross_rate(eur_to_base, eur_to_quote)
        };

        Ok(Some(FxRatePoint {
            base: base_upper,
            quote: quote_upper,
            as_of_date: date,
            timestamp: Utc::now(),
            rate: rate.to_string(),
            kind: FxRateKind::Close,
            source: "frankfurter".to_string(),
        }))
    }

    fn name(&self) -> &str {
        "frankfurter"
    }
}

#[cfg(test)]
mod tests {
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
}
