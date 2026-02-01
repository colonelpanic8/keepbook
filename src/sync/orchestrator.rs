//! Orchestrates sync operations with automatic price fetching.

use std::collections::HashSet;
use std::sync::Arc;

use anyhow::Result;
use chrono::NaiveDate;

use crate::market_data::MarketDataService;
use crate::models::{Asset, Connection, Id};
use crate::storage::Storage;

use super::{SyncResult, Synchronizer};

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

    /// Refresh prices for all assets across all accounts.
    pub async fn refresh_all_prices(
        &self,
        date: NaiveDate,
        force: bool,
    ) -> Result<PriceRefreshResult> {
        let balances = self.storage.get_latest_balances().await?;
        let assets: HashSet<Asset> = balances
            .into_iter()
            .map(|(_, b)| b.asset)
            .collect();
        self.ensure_prices(&assets, date, force).await
    }

    /// Refresh prices for assets in a specific connection's accounts.
    pub async fn refresh_connection_prices(
        &self,
        connection_id: &Id,
        date: NaiveDate,
        force: bool,
    ) -> Result<PriceRefreshResult> {
        let balances = self.storage.get_latest_balances_for_connection(connection_id).await?;
        let assets: HashSet<Asset> = balances
            .into_iter()
            .map(|(_, b)| b.asset)
            .collect();
        self.ensure_prices(&assets, date, force).await
    }

    /// Refresh prices for assets in a specific account.
    pub async fn refresh_account_prices(
        &self,
        account_id: &Id,
        date: NaiveDate,
        force: bool,
    ) -> Result<PriceRefreshResult> {
        let balances = self.storage.get_latest_balances_for_account(account_id).await?;
        let assets: HashSet<Asset> = balances
            .into_iter()
            .map(|b| b.asset)
            .collect();
        self.ensure_prices(&assets, date, force).await
    }

    /// Run sync and fetch any missing prices.
    pub async fn sync_with_prices(
        &self,
        synchronizer: &dyn Synchronizer,
        connection: &mut Connection,
        force_refresh: bool,
    ) -> Result<SyncResult> {
        // 1. Run the sync
        let result = synchronizer.sync(connection).await?;

        // 2. Save sync results (this stores balances)
        result.save(self.storage.as_ref()).await?;

        // 3. Collect assets that need prices
        let assets: HashSet<Asset> = result.balances
            .iter()
            .flat_map(|(_, sbs)| sbs.iter().map(|sb| sb.balance.asset.clone()))
            .collect();

        // 4. Fetch missing prices
        let date = chrono::Utc::now().date_naive();
        self.ensure_prices(&assets, date, force_refresh).await?;

        Ok(result)
    }
}
