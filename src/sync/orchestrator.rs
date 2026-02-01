//! Orchestrates sync operations with automatic price fetching.

use std::sync::Arc;

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
