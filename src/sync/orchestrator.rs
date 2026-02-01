//! Orchestrates sync operations with automatic price fetching.

use std::collections::HashSet;
use std::sync::Arc;

use anyhow::Result;
use chrono::NaiveDate;

use crate::market_data::MarketDataService;
use crate::models::Asset;
use crate::storage::Storage;

/// Coordinates sync + price fetching operations.
pub struct SyncOrchestrator<S: Storage> {
    storage: Arc<S>,
    market_data: MarketDataService,
    reporting_currency: String,
}

/// Result of a price refresh operation.
#[derive(Debug, Default)]
pub struct PriceRefreshResult {
    pub fetched: usize,
    pub skipped: usize,
    pub failed: Vec<(Asset, String)>,
}

impl<S: Storage> SyncOrchestrator<S> {
    pub fn new(storage: Arc<S>, market_data: MarketDataService, reporting_currency: String) -> Self {
        Self {
            storage,
            market_data,
            reporting_currency,
        }
    }

    pub fn reporting_currency(&self) -> &str {
        &self.reporting_currency
    }
}

impl<S: Storage + Send + Sync> SyncOrchestrator<S> {
    /// Ensure prices exist for the given assets on the given date.
    /// Returns counts of fetched, skipped, and failed.
    pub async fn ensure_prices(
        &self,
        assets: &HashSet<Asset>,
        date: NaiveDate,
        _force: bool,  // TODO: implement force refresh
    ) -> Result<PriceRefreshResult> {
        let mut result = PriceRefreshResult::default();
        let mut needed_fx_pairs: HashSet<(String, String)> = HashSet::new();

        for asset in assets {
            match asset {
                Asset::Currency { iso_code } => {
                    // Currencies just need FX rate to reporting currency
                    if iso_code.to_uppercase() != self.reporting_currency.to_uppercase() {
                        needed_fx_pairs.insert((
                            iso_code.to_uppercase(),
                            self.reporting_currency.to_uppercase(),
                        ));
                    }
                }
                Asset::Equity { .. } | Asset::Crypto { .. } => {
                    // Try to get/fetch price
                    match self.market_data.price_close(asset, date).await {
                        Ok(price) => {
                            result.fetched += 1;
                            // Check if we need FX conversion
                            if price.quote_currency.to_uppercase() != self.reporting_currency.to_uppercase() {
                                needed_fx_pairs.insert((
                                    price.quote_currency.to_uppercase(),
                                    self.reporting_currency.to_uppercase(),
                                ));
                            }
                        }
                        Err(e) => {
                            result.failed.push((asset.clone(), e.to_string()));
                        }
                    }
                }
            }
        }

        // Fetch needed FX rates
        for (base, quote) in needed_fx_pairs {
            match self.market_data.fx_close(&base, &quote, date).await {
                Ok(_) => {
                    result.fetched += 1;
                }
                Err(_) => {
                    // FX rate failures are less critical, don't add to failed
                }
            }
        }

        Ok(result)
    }
}
