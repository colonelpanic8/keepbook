//! Orchestrates sync operations with automatic price fetching.

use std::collections::HashSet;
use std::sync::Arc;

use anyhow::Result;
use chrono::NaiveDate;

use crate::clock::{Clock, SystemClock};
use crate::market_data::MarketDataService;
use crate::models::{Asset, Connection, Id};
use crate::storage::Storage;

use super::{SyncOptions, SyncResult, Synchronizer};

/// Coordinates sync + price fetching operations.
pub struct SyncOrchestrator {
    storage: Arc<dyn Storage>,
    market_data: MarketDataService,
    reporting_currency: String,
    clock: Arc<dyn Clock>,
}

/// Result of a sync operation that also stores and refreshes prices.
#[derive(Debug)]
pub struct SyncWithPricesResult {
    pub result: SyncResult,
    pub stored_prices: usize,
    pub refresh: PriceRefreshResult,
}

/// Result of a price refresh operation.
#[derive(Debug, Default)]
pub struct PriceRefreshResult {
    pub fetched: usize,
    pub skipped: usize,
    pub failed: Vec<(Asset, String)>,
}

impl SyncOrchestrator {
    pub fn new(
        storage: Arc<dyn Storage>,
        market_data: MarketDataService,
        reporting_currency: String,
    ) -> Self {
        Self {
            storage,
            market_data,
            reporting_currency,
            clock: Arc::new(SystemClock),
        }
    }

    pub fn with_clock(mut self, clock: Arc<dyn Clock>) -> Self {
        self.clock = clock;
        self
    }

    pub fn reporting_currency(&self) -> &str {
        &self.reporting_currency
    }
}

impl SyncOrchestrator {
    /// Ensure prices exist for the given assets on the given date.
    /// Returns counts of fetched, skipped, and failed.
    pub async fn ensure_prices(
        &self,
        assets: &HashSet<Asset>,
        date: NaiveDate,
        force: bool,
    ) -> Result<PriceRefreshResult> {
        let mut result = PriceRefreshResult::default();
        let mut needed_fx_pairs: HashSet<(String, String)> = HashSet::new();

        for asset in assets {
            let asset = asset.normalized();
            match &asset {
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
                    if force {
                        match self.market_data.price_close_force(&asset, date).await {
                            Ok((price, fetched)) => {
                                if fetched {
                                    result.fetched += 1;
                                } else {
                                    result.skipped += 1;
                                }
                                if price.quote_currency.to_uppercase()
                                    != self.reporting_currency.to_uppercase()
                                {
                                    needed_fx_pairs.insert((
                                        price.quote_currency.to_uppercase(),
                                        self.reporting_currency.to_uppercase(),
                                    ));
                                }
                            }
                            Err(e) => result.failed.push((asset.clone(), e.to_string())),
                        }
                    } else {
                        // Count cache hits as skipped.
                        if let Some(price) = self.market_data.price_from_store(&asset, date).await?
                        {
                            result.skipped += 1;
                            if price.quote_currency.to_uppercase()
                                != self.reporting_currency.to_uppercase()
                            {
                                needed_fx_pairs.insert((
                                    price.quote_currency.to_uppercase(),
                                    self.reporting_currency.to_uppercase(),
                                ));
                            }
                            continue;
                        }

                        // Otherwise, fetch and store.
                        match self.market_data.price_close(&asset, date).await {
                            Ok(price) => {
                                result.fetched += 1;
                                if price.quote_currency.to_uppercase()
                                    != self.reporting_currency.to_uppercase()
                                {
                                    needed_fx_pairs.insert((
                                        price.quote_currency.to_uppercase(),
                                        self.reporting_currency.to_uppercase(),
                                    ));
                                }
                            }
                            Err(e) => result.failed.push((asset.clone(), e.to_string())),
                        }
                    }
                }
            }
        }

        // Fetch needed FX rates
        for (base, quote) in needed_fx_pairs {
            if force {
                match self.market_data.fx_close_force(&base, &quote, date).await {
                    Ok((_rate, fetched)) => {
                        if fetched {
                            result.fetched += 1;
                        } else {
                            result.skipped += 1;
                        }
                    }
                    Err(_) => {
                        // FX rate failures are less critical, don't add to failed.
                    }
                }
            } else {
                if self
                    .market_data
                    .fx_from_store(&base, &quote, date)
                    .await?
                    .is_some()
                {
                    result.skipped += 1;
                    continue;
                }

                match self.market_data.fx_close(&base, &quote, date).await {
                    Ok(_) => result.fetched += 1,
                    Err(_) => {
                        // FX rate failures are less critical, don't add to failed.
                    }
                }
            }
        }

        Ok(result)
    }

    /// Ensure prices exist for valuation purposes:
    /// - for today: prefer live quotes (respecting quote staleness unless `force`)
    /// - for past dates: close prices
    /// - for currencies: FX to reporting currency
    pub async fn ensure_valuation_prices(
        &self,
        assets: &HashSet<Asset>,
        date: NaiveDate,
        force: bool,
    ) -> Result<PriceRefreshResult> {
        let mut result = PriceRefreshResult::default();
        let mut needed_fx_pairs: HashSet<(String, String)> = HashSet::new();
        let is_today = date == self.clock.today();

        for asset in assets {
            let asset = asset.normalized();
            match &asset {
                Asset::Currency { iso_code } => {
                    if iso_code.to_uppercase() != self.reporting_currency.to_uppercase() {
                        needed_fx_pairs.insert((
                            iso_code.to_uppercase(),
                            self.reporting_currency.to_uppercase(),
                        ));
                    }
                }
                Asset::Equity { .. } | Asset::Crypto { .. } => {
                    if is_today {
                        let resp = if force {
                            self.market_data.price_latest_force(&asset, date).await
                        } else {
                            self.market_data
                                .price_latest_with_status(&asset, date)
                                .await
                        };
                        match resp {
                            Ok((price, fetched)) => {
                                if fetched {
                                    result.fetched += 1;
                                } else {
                                    result.skipped += 1;
                                }
                                if price.quote_currency.to_uppercase()
                                    != self.reporting_currency.to_uppercase()
                                {
                                    needed_fx_pairs.insert((
                                        price.quote_currency.to_uppercase(),
                                        self.reporting_currency.to_uppercase(),
                                    ));
                                }
                            }
                            Err(e) => result.failed.push((asset.clone(), e.to_string())),
                        }
                    } else {
                        // Past date: close-only semantics.
                        if force {
                            match self.market_data.price_close_force(&asset, date).await {
                                Ok((price, fetched)) => {
                                    if fetched {
                                        result.fetched += 1;
                                    } else {
                                        result.skipped += 1;
                                    }
                                    if price.quote_currency.to_uppercase()
                                        != self.reporting_currency.to_uppercase()
                                    {
                                        needed_fx_pairs.insert((
                                            price.quote_currency.to_uppercase(),
                                            self.reporting_currency.to_uppercase(),
                                        ));
                                    }
                                }
                                Err(e) => result.failed.push((asset.clone(), e.to_string())),
                            }
                        } else {
                            if let Some(price) =
                                self.market_data.price_from_store(&asset, date).await?
                            {
                                result.skipped += 1;
                                if price.quote_currency.to_uppercase()
                                    != self.reporting_currency.to_uppercase()
                                {
                                    needed_fx_pairs.insert((
                                        price.quote_currency.to_uppercase(),
                                        self.reporting_currency.to_uppercase(),
                                    ));
                                }
                                continue;
                            }

                            match self.market_data.price_close(&asset, date).await {
                                Ok(price) => {
                                    result.fetched += 1;
                                    if price.quote_currency.to_uppercase()
                                        != self.reporting_currency.to_uppercase()
                                    {
                                        needed_fx_pairs.insert((
                                            price.quote_currency.to_uppercase(),
                                            self.reporting_currency.to_uppercase(),
                                        ));
                                    }
                                }
                                Err(e) => result.failed.push((asset.clone(), e.to_string())),
                            }
                        }
                    }
                }
            }
        }

        // Fetch needed FX rates (close).
        for (base, quote) in needed_fx_pairs {
            if force {
                match self.market_data.fx_close_force(&base, &quote, date).await {
                    Ok((_rate, fetched)) => {
                        if fetched {
                            result.fetched += 1;
                        } else {
                            result.skipped += 1;
                        }
                    }
                    Err(_) => {
                        // FX failures are less critical; ignore.
                    }
                }
            } else {
                if self
                    .market_data
                    .fx_from_store(&base, &quote, date)
                    .await?
                    .is_some()
                {
                    result.skipped += 1;
                    continue;
                }

                match self.market_data.fx_close(&base, &quote, date).await {
                    Ok(_) => result.fetched += 1,
                    Err(_) => {
                        // FX failures are less critical; ignore.
                    }
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
        let snapshots = self.storage.get_latest_balances().await?;
        let assets: HashSet<Asset> = snapshots
            .into_iter()
            .flat_map(|(_, snapshot)| snapshot.balances.into_iter().map(|ab| ab.asset))
            .collect();
        self.ensure_prices(&assets, date, force).await
    }

    /// Refresh valuation-relevant prices for all assets across all accounts.
    /// For today, this prefers quotes and respects quote staleness unless forced.
    pub async fn refresh_all_valuation_prices(
        &self,
        date: NaiveDate,
        force: bool,
    ) -> Result<PriceRefreshResult> {
        let snapshots = self.storage.get_latest_balances().await?;
        let assets: HashSet<Asset> = snapshots
            .into_iter()
            .flat_map(|(_, snapshot)| snapshot.balances.into_iter().map(|ab| ab.asset))
            .collect();
        self.ensure_valuation_prices(&assets, date, force).await
    }

    /// Refresh prices for assets in a specific connection's accounts.
    pub async fn refresh_connection_prices(
        &self,
        connection_id: &Id,
        date: NaiveDate,
        force: bool,
    ) -> Result<PriceRefreshResult> {
        let snapshots = self
            .storage
            .get_latest_balances_for_connection(connection_id)
            .await?;
        let assets: HashSet<Asset> = snapshots
            .into_iter()
            .flat_map(|(_, snapshot)| snapshot.balances.into_iter().map(|ab| ab.asset))
            .collect();
        self.ensure_prices(&assets, date, force).await
    }

    /// Refresh valuation-relevant prices for assets in a specific connection's accounts.
    pub async fn refresh_connection_valuation_prices(
        &self,
        connection_id: &Id,
        date: NaiveDate,
        force: bool,
    ) -> Result<PriceRefreshResult> {
        let snapshots = self
            .storage
            .get_latest_balances_for_connection(connection_id)
            .await?;
        let assets: HashSet<Asset> = snapshots
            .into_iter()
            .flat_map(|(_, snapshot)| snapshot.balances.into_iter().map(|ab| ab.asset))
            .collect();
        self.ensure_valuation_prices(&assets, date, force).await
    }

    /// Refresh prices for assets in a specific account.
    pub async fn refresh_account_prices(
        &self,
        account_id: &Id,
        date: NaiveDate,
        force: bool,
    ) -> Result<PriceRefreshResult> {
        let snapshot = self.storage.get_latest_balance_snapshot(account_id).await?;
        let assets: HashSet<Asset> = snapshot
            .map(|s| s.balances.into_iter().map(|ab| ab.asset).collect())
            .unwrap_or_default();
        self.ensure_prices(&assets, date, force).await
    }

    /// Refresh valuation-relevant prices for assets in a specific account.
    pub async fn refresh_account_valuation_prices(
        &self,
        account_id: &Id,
        date: NaiveDate,
        force: bool,
    ) -> Result<PriceRefreshResult> {
        let snapshot = self.storage.get_latest_balance_snapshot(account_id).await?;
        let assets: HashSet<Asset> = snapshot
            .map(|s| s.balances.into_iter().map(|ab| ab.asset).collect())
            .unwrap_or_default();
        self.ensure_valuation_prices(&assets, date, force).await
    }

    /// Run sync and fetch any missing prices.
    pub async fn sync_with_prices(
        &self,
        synchronizer: &dyn Synchronizer,
        connection: &mut Connection,
        force_refresh: bool,
        options: &SyncOptions,
    ) -> Result<SyncWithPricesResult> {
        // 1. Run the sync
        let result = synchronizer
            .sync_with_options(connection, self.storage.as_ref(), options)
            .await?;

        // 2. Save sync results (this stores balances)
        result
            .save_with_clock(self.storage.as_ref(), self.clock.as_ref())
            .await?;

        // 3. Store any prices the synchronizer provided
        let mut stored_prices = 0;
        for (_, synced_balances) in &result.balances {
            for sb in synced_balances {
                if let Some(price) = &sb.price {
                    self.market_data.store_price(price).await?;
                    stored_prices += 1;
                }
            }
        }

        // 4. Collect assets that need prices
        let assets: HashSet<Asset> = result
            .balances
            .iter()
            .flat_map(|(_, sbs)| sbs.iter().map(|sb| sb.asset_balance.asset.clone()))
            .collect();

        // 5. Fetch missing prices
        let date = self.clock.today();
        let refresh = self.ensure_prices(&assets, date, force_refresh).await?;

        Ok(SyncWithPricesResult {
            result,
            stored_prices,
            refresh,
        })
    }
}
