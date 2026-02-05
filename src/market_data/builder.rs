use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::clock::{Clock, SystemClock};
use crate::market_data::{
    CryptoPriceRouter, EquityPriceRouter, FxRateRouter, JsonlMarketDataStore, MarketDataService,
    MarketDataStore, PriceSourceRegistry,
};

/// Builds a [`MarketDataService`] from a data directory and optional configured price sources.
///
/// This centralizes registry loading and router construction so app/service code doesn't duplicate it.
pub struct MarketDataServiceBuilder {
    store: Arc<dyn MarketDataStore>,
    data_dir: PathBuf,
    include_equity: bool,
    include_crypto: bool,
    include_fx: bool,
    quote_staleness: Option<std::time::Duration>,
    lookback_days: Option<u32>,
    offline_only: bool,
    clock: Arc<dyn Clock>,
}

impl MarketDataServiceBuilder {
    /// Create a builder using the default JSONL store under `data_dir`.
    pub fn for_data_dir(data_dir: &Path) -> Self {
        let store: Arc<dyn MarketDataStore> = Arc::new(JsonlMarketDataStore::new(data_dir));
        Self::new(store, data_dir.to_path_buf())
    }

    /// Create a builder with a caller-provided store (useful for tests).
    pub fn new(store: Arc<dyn MarketDataStore>, data_dir: PathBuf) -> Self {
        Self {
            store,
            data_dir,
            include_equity: true,
            include_crypto: true,
            include_fx: true,
            quote_staleness: None,
            lookback_days: None,
            offline_only: false,
            clock: Arc::new(SystemClock),
        }
    }

    /// Disable configured network sources and use the store only.
    pub fn offline_only(mut self) -> Self {
        self.offline_only = true;
        self
    }

    /// Enable or disable routers by asset class.
    pub fn with_routers(mut self, equity: bool, crypto: bool, fx: bool) -> Self {
        self.include_equity = equity;
        self.include_crypto = crypto;
        self.include_fx = fx;
        self
    }

    pub fn with_quote_staleness(mut self, staleness: std::time::Duration) -> Self {
        self.quote_staleness = Some(staleness);
        self
    }

    pub fn with_lookback_days(mut self, lookback_days: u32) -> Self {
        self.lookback_days = Some(lookback_days);
        self
    }

    pub fn with_clock(mut self, clock: Arc<dyn Clock>) -> Self {
        self.clock = clock;
        self
    }

    pub async fn build(self) -> MarketDataService {
        let mut service = MarketDataService::new(self.store, None).with_clock(self.clock);

        if let Some(staleness) = self.quote_staleness {
            service = service.with_quote_staleness(staleness);
        }

        if let Some(days) = self.lookback_days {
            service = service.with_lookback_days(days);
        }

        if self.offline_only {
            return service;
        }

        let mut registry = PriceSourceRegistry::new(&self.data_dir);
        if let Err(e) = registry.load() {
            tracing::warn!(error = %e, "failed to load price sources; continuing without network fetch");
            return service;
        }

        if self.include_equity {
            match registry.build_equity_sources().await {
                Ok(sources) => {
                    if !sources.is_empty() {
                        service =
                            service.with_equity_router(Arc::new(EquityPriceRouter::new(sources)));
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        "failed to build equity price sources; continuing without equity fetch"
                    );
                }
            }
        }

        if self.include_crypto {
            match registry.build_crypto_sources().await {
                Ok(sources) => {
                    if !sources.is_empty() {
                        service =
                            service.with_crypto_router(Arc::new(CryptoPriceRouter::new(sources)));
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        "failed to build crypto price sources; continuing without crypto fetch"
                    );
                }
            }
        }

        if self.include_fx {
            match registry.build_fx_sources().await {
                Ok(sources) => {
                    if !sources.is_empty() {
                        service = service.with_fx_router(Arc::new(FxRateRouter::new(sources)));
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        "failed to build fx sources; continuing without fx fetch"
                    );
                }
            }
        }

        service
    }
}
