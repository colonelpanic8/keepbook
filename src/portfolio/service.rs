// src/portfolio/service.rs
use anyhow::Result;
use std::sync::Arc;

use crate::market_data::MarketDataService;
use crate::storage::Storage;

use super::{PortfolioQuery, PortfolioSnapshot, RefreshPolicy};

pub struct PortfolioService {
    storage: Arc<dyn Storage>,
    market_data: Arc<MarketDataService>,
}

impl PortfolioService {
    pub fn new(storage: Arc<dyn Storage>, market_data: Arc<MarketDataService>) -> Self {
        Self { storage, market_data }
    }

    pub async fn calculate(
        &self,
        _query: &PortfolioQuery,
        _refresh: &RefreshPolicy,
    ) -> Result<PortfolioSnapshot> {
        todo!("Implement in Task 2")
    }
}
