use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::duration::deserialize_duration;

/// Default reporting currency.
fn default_reporting_currency() -> String {
    "USD".to_string()
}

/// Default balance staleness (14 days).
fn default_balance_staleness() -> std::time::Duration {
    std::time::Duration::from_secs(14 * 24 * 60 * 60)
}

/// Default price staleness (24 hours).
fn default_price_staleness() -> std::time::Duration {
    std::time::Duration::from_secs(24 * 60 * 60)
}

/// Refresh/staleness configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RefreshConfig {
    /// How old balance data can be before it's considered stale.
    #[serde(
        default = "default_balance_staleness",
        deserialize_with = "deserialize_duration"
    )]
    pub balance_staleness: std::time::Duration,

    /// How old price data can be before it's considered stale.
    #[serde(
        default = "default_price_staleness",
        deserialize_with = "deserialize_duration"
    )]
    pub price_staleness: std::time::Duration,
}

impl Default for RefreshConfig {
    fn default() -> Self {
        Self {
            balance_staleness: default_balance_staleness(),
            price_staleness: default_price_staleness(),
        }
    }
}

/// Application configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Path to data directory. If relative, resolved from config file location.
    /// If not specified, defaults to the config file's directory.
    pub data_dir: Option<PathBuf>,

    /// Currency for reporting all values (e.g., "USD")
    #[serde(default = "default_reporting_currency")]
    pub reporting_currency: String,

    /// Refresh/staleness settings.
    #[serde(default)]
    pub refresh: RefreshConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            data_dir: None,
            reporting_currency: default_reporting_currency(),
            refresh: RefreshConfig::default(),
        }
    }
}

impl Config {
    /// Load config from a TOML file.
    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read config file: {}", path.display()))?;

        let config: Config = toml::from_str(&content)
            .with_context(|| format!("Failed to parse config file: {}", path.display()))?;

        Ok(config)
    }

    /// Load config from a file, or return default config if file doesn't exist.
    pub fn load_or_default(path: &Path) -> Result<Self> {
        if path.exists() {
            Self::load(path)
        } else {
            Ok(Self::default())
        }
    }

    /// Resolve the data directory path.
    ///
    /// If `data_dir` is set and relative, it's resolved relative to `config_dir`.
    /// If `data_dir` is not set, returns `config_dir`.
    pub fn resolve_data_dir(&self, config_dir: &Path) -> PathBuf {
        match &self.data_dir {
            Some(data_dir) if data_dir.is_absolute() => data_dir.clone(),
            Some(data_dir) => config_dir.join(data_dir),
            None => config_dir.to_path_buf(),
        }
    }
}

/// Loaded configuration with resolved paths.
#[derive(Debug, Clone)]
pub struct ResolvedConfig {
    /// The resolved data directory path.
    pub data_dir: PathBuf,

    /// Currency for reporting all values (e.g., "USD")
    pub reporting_currency: String,

    /// Refresh/staleness settings.
    pub refresh: RefreshConfig,
}

/// Returns the default config file path.
///
/// Resolution order:
/// 1. `./keepbook.toml` if it exists in current directory
/// 2. `~/.local/share/keepbook/keepbook.toml` (XDG data directory)
pub fn default_config_path() -> PathBuf {
    let local_config = PathBuf::from("keepbook.toml");
    if local_config.exists() {
        return local_config;
    }

    // XDG data directory fallback
    if let Some(data_dir) = dirs::data_dir() {
        return data_dir.join("keepbook").join("keepbook.toml");
    }

    // Final fallback to local
    local_config
}

impl ResolvedConfig {
    /// Load and resolve config from a file path.
    ///
    /// The data directory is resolved relative to the config file's parent directory.
    pub fn load(config_path: &Path) -> Result<Self> {
        let config_path = config_path
            .canonicalize()
            .with_context(|| format!("Config file not found: {}", config_path.display()))?;

        let config_dir = config_path
            .parent()
            .context("Config file has no parent directory")?;

        let config = Config::load(&config_path)?;
        let data_dir = config.resolve_data_dir(config_dir);

        Ok(Self {
            data_dir,
            reporting_currency: config.reporting_currency,
            refresh: config.refresh,
        })
    }

    /// Load config, creating a default if the file doesn't exist.
    ///
    /// If the config file doesn't exist, uses the config file's intended
    /// parent directory as the data directory.
    pub fn load_or_default(config_path: &Path) -> Result<Self> {
        if config_path.exists() {
            Self::load(config_path)
        } else {
            // Resolve the config path relative to current directory
            let config_path = if config_path.is_relative() {
                std::env::current_dir()
                    .context("Failed to get current directory")?
                    .join(config_path)
            } else {
                config_path.to_path_buf()
            };

            // Use the intended config directory as data dir
            let config_dir = config_path
                .parent()
                .context("Config path has no parent directory")?;

            Ok(Self {
                data_dir: config_dir.to_path_buf(),
                reporting_currency: default_reporting_currency(),
                refresh: RefreshConfig::default(),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn test_default_data_dir_is_config_dir() {
        let config = Config::default();
        let config_dir = Path::new("/home/user/finances");
        assert_eq!(
            config.resolve_data_dir(config_dir),
            PathBuf::from("/home/user/finances")
        );
    }

    #[test]
    fn test_relative_data_dir() {
        let config = Config {
            data_dir: Some(PathBuf::from("data")),
            ..Default::default()
        };
        let config_dir = Path::new("/home/user/finances");
        assert_eq!(
            config.resolve_data_dir(config_dir),
            PathBuf::from("/home/user/finances/data")
        );
    }

    #[test]
    fn test_absolute_data_dir() {
        let config = Config {
            data_dir: Some(PathBuf::from("/var/keepbook/data")),
            ..Default::default()
        };
        let config_dir = Path::new("/home/user/finances");
        assert_eq!(
            config.resolve_data_dir(config_dir),
            PathBuf::from("/var/keepbook/data")
        );
    }

    #[test]
    fn test_load_config() -> Result<()> {
        let dir = TempDir::new()?;
        let config_path = dir.path().join("keepbook.toml");

        let mut file = std::fs::File::create(&config_path)?;
        writeln!(file, "data_dir = \"./my-data\"")?;

        let config = Config::load(&config_path)?;
        assert_eq!(config.data_dir, Some(PathBuf::from("./my-data")));

        Ok(())
    }

    #[test]
    fn test_load_empty_config() -> Result<()> {
        let dir = TempDir::new()?;
        let config_path = dir.path().join("keepbook.toml");

        std::fs::File::create(&config_path)?;

        let config = Config::load(&config_path)?;
        assert_eq!(config.data_dir, None);

        Ok(())
    }

    #[test]
    fn test_load_refresh_config() -> Result<()> {
        let dir = TempDir::new()?;
        let config_path = dir.path().join("keepbook.toml");

        let mut file = std::fs::File::create(&config_path)?;
        writeln!(file, "[refresh]")?;
        writeln!(file, "balance_staleness = \"7d\"")?;
        writeln!(file, "price_staleness = \"1h\"")?;

        let config = Config::load(&config_path)?;
        assert_eq!(
            config.refresh.balance_staleness,
            std::time::Duration::from_secs(7 * 24 * 60 * 60)
        );
        assert_eq!(
            config.refresh.price_staleness,
            std::time::Duration::from_secs(60 * 60)
        );

        Ok(())
    }

    #[test]
    fn test_default_refresh_config() {
        let config = Config::default();
        assert_eq!(
            config.refresh.balance_staleness,
            std::time::Duration::from_secs(14 * 24 * 60 * 60)
        );
        assert_eq!(
            config.refresh.price_staleness,
            std::time::Duration::from_secs(24 * 60 * 60)
        );
    }
}
