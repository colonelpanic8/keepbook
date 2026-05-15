//! Price source registry.
//!
//! Loads and manages price sources from the data directory.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use tracing::warn;

use super::providers::coincap::CoinCapConfig;
use super::providers::cryptocompare::CryptoCompareConfig;
use super::providers::{
    AlphaVantagePriceSource, CoinCapPriceSource, CoinGeckoPriceSource, CryptoComparePriceSource,
    EodhdPriceSource, FrankfurterRateSource, MarketstackPriceSource, TwelveDataPriceSource,
};
use super::source_config::{LoadedPriceSource, PriceSourceConfig, PriceSourceType};
use super::sources::{CryptoPriceSource, EquityPriceSource, FxRateSource};

/// Registry of configured price sources.
///
/// Loads sources from `{data_dir}/price_sources/*/source.toml` and builds
/// the appropriate implementations.
pub struct PriceSourceRegistry {
    sources_dir: PathBuf,
    loaded: Vec<LoadedPriceSource>,
}

impl PriceSourceRegistry {
    /// Create a new registry pointing to the given data directory.
    pub fn new(data_dir: &Path) -> Self {
        Self {
            sources_dir: data_dir.join("price_sources"),
            loaded: Vec::new(),
        }
    }

    /// Load all source configurations from the price_sources directory.
    pub fn load(&mut self) -> Result<()> {
        self.loaded.clear();

        if !self.sources_dir.exists() {
            return Ok(());
        }

        let entries = std::fs::read_dir(&self.sources_dir)
            .with_context(|| format!("Failed to read {}", self.sources_dir.display()))?;

        for entry in entries {
            let entry = entry?;
            let path = entry.path();

            if !path.is_dir() {
                continue;
            }

            let source_toml = path.join("source.toml");
            if !source_toml.exists() {
                continue;
            }

            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .map(|s| s.to_string())
                .unwrap_or_default();

            match PriceSourceConfig::load(&source_toml) {
                Ok(config) => {
                    if config.enabled {
                        self.loaded.push(LoadedPriceSource {
                            name,
                            base_dir: path,
                            config,
                        });
                    }
                }
                Err(e) => {
                    warn!(
                        source = %source_toml.display(),
                        error = %e,
                        "failed to load price source config; skipping"
                    );
                }
            }
        }

        // Sort by priority (lower = higher priority)
        self.loaded.sort_by_key(|s| s.config.priority);

        Ok(())
    }

    /// Get all loaded source configurations.
    pub fn sources(&self) -> &[LoadedPriceSource] {
        &self.loaded
    }

    /// Build equity price sources from loaded configurations.
    pub async fn build_equity_sources(&self) -> Result<Vec<Arc<dyn EquityPriceSource>>> {
        let mut sources: Vec<Arc<dyn EquityPriceSource>> = Vec::new();

        for loaded in &self.loaded {
            let source = match loaded.config.source_type {
                PriceSourceType::Eodhd => {
                    let credentials = loaded.config.credentials.as_ref().with_context(|| {
                        format!(
                            "Price source {} ({:?}) requires credentials",
                            loaded.name, loaded.config.source_type
                        )
                    })?;
                    let store = credentials.build_with_base_dir(Some(&loaded.base_dir));
                    let source = EodhdPriceSource::from_credentials(store.as_ref()).await?;
                    Arc::new(source) as Arc<dyn EquityPriceSource>
                }
                PriceSourceType::TwelveData => {
                    let credentials = loaded.config.credentials.as_ref().with_context(|| {
                        format!(
                            "Price source {} ({:?}) requires credentials",
                            loaded.name, loaded.config.source_type
                        )
                    })?;
                    let store = credentials.build_with_base_dir(Some(&loaded.base_dir));
                    let source = TwelveDataPriceSource::from_credentials(store.as_ref()).await?;
                    Arc::new(source) as Arc<dyn EquityPriceSource>
                }
                PriceSourceType::AlphaVantage => {
                    let credentials = loaded.config.credentials.as_ref().with_context(|| {
                        format!(
                            "Price source {} ({:?}) requires credentials",
                            loaded.name, loaded.config.source_type
                        )
                    })?;
                    let store = credentials.build_with_base_dir(Some(&loaded.base_dir));
                    let source = AlphaVantagePriceSource::from_credentials(store.as_ref()).await?;
                    Arc::new(source) as Arc<dyn EquityPriceSource>
                }
                PriceSourceType::Marketstack => {
                    let credentials = loaded.config.credentials.as_ref().with_context(|| {
                        format!(
                            "Price source {} ({:?}) requires credentials",
                            loaded.name, loaded.config.source_type
                        )
                    })?;
                    let store = credentials.build_with_base_dir(Some(&loaded.base_dir));
                    let source = MarketstackPriceSource::from_credentials(store.as_ref()).await?;
                    Arc::new(source) as Arc<dyn EquityPriceSource>
                }
                // Skip non-equity sources
                PriceSourceType::Coingecko
                | PriceSourceType::Cryptocompare
                | PriceSourceType::Coincap
                | PriceSourceType::Frankfurter => continue,
            };
            sources.push(source);
        }

        Ok(sources)
    }

    /// Build crypto price sources from loaded configurations.
    pub async fn build_crypto_sources(&self) -> Result<Vec<Arc<dyn CryptoPriceSource>>> {
        let mut sources: Vec<Arc<dyn CryptoPriceSource>> = Vec::new();

        for loaded in &self.loaded {
            let source = match loaded.config.source_type {
                PriceSourceType::Coingecko => {
                    Arc::new(CoinGeckoPriceSource::new()) as Arc<dyn CryptoPriceSource>
                }
                PriceSourceType::Cryptocompare => {
                    let mut provider = if let Some(credentials) = &loaded.config.credentials {
                        let store = credentials.build_with_base_dir(Some(&loaded.base_dir));
                        CryptoComparePriceSource::from_credentials(store.as_ref()).await?
                    } else {
                        CryptoComparePriceSource::new()
                    };

                    if let Some(config) = &loaded.config.config {
                        let parsed: CryptoCompareConfig =
                            config.clone().try_into().with_context(|| {
                                format!(
                                    "Failed to parse config for CryptoCompare source {}",
                                    loaded.name
                                )
                            })?;
                        provider = provider.with_config(parsed);
                    }

                    Arc::new(provider) as Arc<dyn CryptoPriceSource>
                }
                PriceSourceType::Coincap => {
                    let mut provider = if let Some(credentials) = &loaded.config.credentials {
                        let store = credentials.build_with_base_dir(Some(&loaded.base_dir));
                        CoinCapPriceSource::from_credentials(store.as_ref()).await?
                    } else {
                        CoinCapPriceSource::new()
                    };

                    if let Some(config) = &loaded.config.config {
                        let parsed: CoinCapConfig =
                            config.clone().try_into().with_context(|| {
                                format!("Failed to parse config for CoinCap source {}", loaded.name)
                            })?;
                        provider = provider.with_config(parsed);
                    }

                    Arc::new(provider) as Arc<dyn CryptoPriceSource>
                }
                // Skip non-crypto sources
                _ => continue,
            };
            sources.push(source);
        }

        Ok(sources)
    }

    /// Build FX rate sources from loaded configurations.
    pub async fn build_fx_sources(&self) -> Result<Vec<Arc<dyn FxRateSource>>> {
        let mut sources: Vec<Arc<dyn FxRateSource>> = Vec::new();

        for loaded in &self.loaded {
            let source = match loaded.config.source_type {
                PriceSourceType::Frankfurter => {
                    Arc::new(FrankfurterRateSource::new()) as Arc<dyn FxRateSource>
                }
                // Skip non-FX sources
                _ => continue,
            };
            sources.push(source);
        }

        Ok(sources)
    }

    /// Get the path to the price_sources directory.
    pub fn sources_dir(&self) -> &Path {
        &self.sources_dir
    }
}

#[cfg(test)]
#[path = "../../tests/unit/market_data/registry_tests.rs"]
mod registry_tests;
