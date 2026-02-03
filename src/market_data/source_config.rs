//! Price source configuration.
//!
//! Defines the structure for `source.toml` files that configure price sources.

use std::path::Path;

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};

use crate::credentials::CredentialConfig;

/// Known price source types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PriceSourceType {
    /// EODHD equity prices
    Eodhd,
    /// Twelve Data equity prices
    TwelveData,
    /// Alpha Vantage equity prices
    AlphaVantage,
    /// Marketstack equity prices
    Marketstack,
    /// CoinGecko crypto prices
    Coingecko,
    /// Frankfurter/ECB FX rates
    Frankfurter,
}

impl PriceSourceType {
    /// Whether this source type requires credentials.
    pub fn requires_credentials(&self) -> bool {
        match self {
            Self::Eodhd | Self::TwelveData | Self::AlphaVantage | Self::Marketstack => true,
            Self::Coingecko | Self::Frankfurter => false,
        }
    }

    /// The asset types this source can provide prices for.
    pub fn supported_assets(&self) -> &'static [AssetCategory] {
        match self {
            Self::Eodhd | Self::TwelveData | Self::AlphaVantage | Self::Marketstack => {
                &[AssetCategory::Equity]
            }
            Self::Coingecko => &[AssetCategory::Crypto],
            Self::Frankfurter => &[AssetCategory::Fx],
        }
    }
}

/// Categories of assets a source can provide.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetCategory {
    Equity,
    Crypto,
    Fx,
}

/// Human-declared price source configuration.
/// Stored in `source.toml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceSourceConfig {
    /// Which price source implementation to use.
    #[serde(rename = "type")]
    pub source_type: PriceSourceType,

    /// Whether this source is enabled.
    #[serde(default = "default_enabled")]
    pub enabled: bool,

    /// Priority for fallback ordering (lower = higher priority).
    #[serde(default = "default_priority")]
    pub priority: u32,

    /// Credential configuration for this source.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credentials: Option<CredentialConfig>,

    /// Source-specific configuration options.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config: Option<toml::Value>,
}

fn default_enabled() -> bool {
    true
}

fn default_priority() -> u32 {
    100
}

impl PriceSourceConfig {
    /// Load configuration from a TOML file.
    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read source config: {}", path.display()))?;

        let config: Self = toml::from_str(&content)
            .with_context(|| format!("Failed to parse source config: {}", path.display()))?;

        // Validate credentials requirement
        if config.source_type.requires_credentials() && config.credentials.is_none() {
            return Err(anyhow!(
                "Source type {:?} requires credentials, but none configured in {}",
                config.source_type,
                path.display()
            ));
        }

        Ok(config)
    }

    /// Load configuration from a file, returning None if file doesn't exist.
    pub fn load_optional(path: &Path) -> Result<Option<Self>> {
        if path.exists() {
            Ok(Some(Self::load(path)?))
        } else {
            Ok(None)
        }
    }
}

/// A loaded price source with its directory name.
#[derive(Debug, Clone)]
pub struct LoadedPriceSource {
    /// Directory name (used as identifier).
    pub name: String,
    /// The source configuration.
    pub config: PriceSourceConfig,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_parse_eodhd_config() -> Result<()> {
        let mut file = NamedTempFile::new()?;
        writeln!(
            file,
            r#"
type = "eodhd"
enabled = true
priority = 10

[credentials]
backend = "pass"
path = "keepbook/eodhd-api-key"
"#
        )?;

        let config = PriceSourceConfig::load(file.path())?;
        assert_eq!(config.source_type, PriceSourceType::Eodhd);
        assert!(config.enabled);
        assert_eq!(config.priority, 10);
        assert!(config.credentials.is_some());

        Ok(())
    }

    #[test]
    fn test_parse_coingecko_config() -> Result<()> {
        let mut file = NamedTempFile::new()?;
        writeln!(
            file,
            r#"
type = "coingecko"
priority = 20
"#
        )?;

        let config = PriceSourceConfig::load(file.path())?;
        assert_eq!(config.source_type, PriceSourceType::Coingecko);
        assert!(config.enabled); // default
        assert_eq!(config.priority, 20);
        assert!(config.credentials.is_none()); // not required

        Ok(())
    }

    #[test]
    fn test_missing_credentials_for_eodhd() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(
            file,
            r#"
type = "eodhd"
"#
        )
        .unwrap();

        let result = PriceSourceConfig::load(file.path());
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("requires credentials"));
    }

    #[test]
    fn test_source_type_requires_credentials() {
        assert!(PriceSourceType::Eodhd.requires_credentials());
        assert!(PriceSourceType::TwelveData.requires_credentials());
        assert!(PriceSourceType::AlphaVantage.requires_credentials());
        assert!(PriceSourceType::Marketstack.requires_credentials());
        assert!(!PriceSourceType::Coingecko.requires_credentials());
        assert!(!PriceSourceType::Frankfurter.requires_credentials());
    }
}
