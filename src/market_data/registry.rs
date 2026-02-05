//! Price source registry.
//!
//! Loads and manages price sources from the data directory.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};

use super::providers::{
    AlphaVantagePriceSource, CoinCapPriceSource, CoinGeckoPriceSource,
    CryptoComparePriceSource, EodhdPriceSource, FrankfurterRateSource, MarketstackPriceSource,
    TwelveDataPriceSource,
};
use super::providers::coincap::CoinCapConfig;
use super::providers::cryptocompare::CryptoCompareConfig;
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
                        self.loaded.push(LoadedPriceSource { name, config });
                    }
                }
                Err(e) => {
                    eprintln!("Warning: failed to load {}: {}", source_toml.display(), e);
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
                        format!("Price source {} ({:?}) requires credentials",
                            loaded.name, loaded.config.source_type)
                    })?;
                    let store = credentials.build();
                    let source = EodhdPriceSource::from_credentials(store.as_ref()).await?;
                    Arc::new(source) as Arc<dyn EquityPriceSource>
                }
                PriceSourceType::TwelveData => {
                    let credentials = loaded.config.credentials.as_ref().with_context(|| {
                        format!("Price source {} ({:?}) requires credentials",
                            loaded.name, loaded.config.source_type)
                    })?;
                    let store = credentials.build();
                    let source = TwelveDataPriceSource::from_credentials(store.as_ref()).await?;
                    Arc::new(source) as Arc<dyn EquityPriceSource>
                }
                PriceSourceType::AlphaVantage => {
                    let credentials = loaded.config.credentials.as_ref().with_context(|| {
                        format!("Price source {} ({:?}) requires credentials",
                            loaded.name, loaded.config.source_type)
                    })?;
                    let store = credentials.build();
                    let source = AlphaVantagePriceSource::from_credentials(store.as_ref()).await?;
                    Arc::new(source) as Arc<dyn EquityPriceSource>
                }
                PriceSourceType::Marketstack => {
                    let credentials = loaded.config.credentials.as_ref().with_context(|| {
                        format!("Price source {} ({:?}) requires credentials",
                            loaded.name, loaded.config.source_type)
                    })?;
                    let store = credentials.build();
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
                        let store = credentials.build();
                        CryptoComparePriceSource::from_credentials(store.as_ref()).await?
                    } else {
                        CryptoComparePriceSource::new()
                    };

                    if let Some(config) = &loaded.config.config {
                        let parsed: CryptoCompareConfig = config.clone().try_into().with_context(
                            || {
                                format!(
                                    "Failed to parse config for CryptoCompare source {}",
                                    loaded.name
                                )
                            },
                        )?;
                        provider = provider.with_config(parsed);
                    }

                    Arc::new(provider) as Arc<dyn CryptoPriceSource>
                }
                PriceSourceType::Coincap => {
                    let mut provider = if let Some(credentials) = &loaded.config.credentials {
                        let store = credentials.build();
                        CoinCapPriceSource::from_credentials(store.as_ref()).await?
                    } else {
                        CoinCapPriceSource::new()
                    };

                    if let Some(config) = &loaded.config.config {
                        let parsed: CoinCapConfig = config.clone().try_into().with_context(|| {
                            format!(
                                "Failed to parse config for CoinCap source {}",
                                loaded.name
                            )
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
mod tests {
    use super::*;
    use crate::market_data::source_config::{LoadedPriceSource, PriceSourceConfig, PriceSourceType};
    use std::fs;
    use std::io::Write;
    use tempfile::TempDir;

    #[tokio::test]
    async fn build_equity_sources_missing_credentials_returns_error_not_panic() -> Result<()> {
        // PriceSourceConfig::load() validates this, but we still want the registry to be robust
        // against malformed state (hand-edits, older versions, etc.).
        let dir = TempDir::new()?;
        let mut registry = PriceSourceRegistry::new(dir.path());
        registry.loaded = vec![LoadedPriceSource {
            name: "bad-eodhd".to_string(),
            config: PriceSourceConfig {
                source_type: PriceSourceType::Eodhd,
                enabled: true,
                priority: 1,
                credentials: None,
                config: None,
            },
        }];

        match registry.build_equity_sources().await {
            Ok(_) => anyhow::bail!("expected error for missing credentials"),
            Err(err) => {
                assert!(err.to_string().contains("requires credentials"));
            }
        }

        Ok(())
    }

    #[test]
    fn test_load_empty_directory() -> Result<()> {
        let dir = TempDir::new()?;
        let mut registry = PriceSourceRegistry::new(dir.path());
        registry.load()?;
        assert!(registry.sources().is_empty());
        Ok(())
    }

    #[test]
    fn test_load_sources() -> Result<()> {
        let dir = TempDir::new()?;
        let sources_dir = dir.path().join("price_sources");

        // Create coingecko source (no credentials needed)
        let cg_dir = sources_dir.join("coingecko");
        fs::create_dir_all(&cg_dir)?;
        let mut file = fs::File::create(cg_dir.join("source.toml"))?;
        writeln!(file, r#"type = "coingecko""#)?;
        writeln!(file, r#"priority = 20"#)?;

        // Create frankfurter source
        let ff_dir = sources_dir.join("frankfurter");
        fs::create_dir_all(&ff_dir)?;
        let mut file = fs::File::create(ff_dir.join("source.toml"))?;
        writeln!(file, r#"type = "frankfurter""#)?;
        writeln!(file, r#"priority = 10"#)?;

        let mut registry = PriceSourceRegistry::new(dir.path());
        registry.load()?;

        assert_eq!(registry.sources().len(), 2);
        // Should be sorted by priority
        assert_eq!(registry.sources()[0].config.priority, 10);
        assert_eq!(registry.sources()[1].config.priority, 20);

        Ok(())
    }

    #[test]
    fn test_disabled_source_not_loaded() -> Result<()> {
        let dir = TempDir::new()?;
        let sources_dir = dir.path().join("price_sources");

        let cg_dir = sources_dir.join("coingecko");
        fs::create_dir_all(&cg_dir)?;
        let mut file = fs::File::create(cg_dir.join("source.toml"))?;
        writeln!(file, r#"type = "coingecko""#)?;
        writeln!(file, r#"enabled = false"#)?;

        let mut registry = PriceSourceRegistry::new(dir.path());
        registry.load()?;

        assert!(registry.sources().is_empty());

        Ok(())
    }
}
