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
    base_url: String,
}

impl FrankfurterRateSource {
    /// Creates a new Frankfurter provider with a default HTTP client.
    pub fn new() -> Self {
        Self {
            client: Client::new(),
            base_url: FRANKFURTER_BASE_URL.to_string(),
        }
    }

    /// Creates a new Frankfurter provider with a custom HTTP client.
    pub fn with_client(client: Client) -> Self {
        Self {
            client,
            base_url: FRANKFURTER_BASE_URL.to_string(),
        }
    }

    /// Override the base URL (useful for tests).
    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }

    /// Fetches rates from Frankfurter API with EUR as base.
    async fn fetch_eur_rates(
        &self,
        currencies: &[&str],
        date: NaiveDate,
    ) -> Result<HashMap<String, f64>> {
        let symbols = currencies.join(",");
        let base = self.base_url.trim_end_matches('/');
        let url = format!("{base}/{date}?from=EUR&to={symbols}");

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
#[path = "../../../tests/unit/market_data/providers/frankfurter_tests.rs"]
mod frankfurter_tests;
