use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Deserializer, Serialize};

use crate::duration::deserialize_duration;

/// Default reporting currency.
fn default_reporting_currency() -> String {
    "USD".to_string()
}

/// Display/output formatting configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct DisplayConfig {
    /// If set, values denominated in the output/base currency are rounded to
    /// this many decimal places before being rendered as strings.
    ///
    /// This is purely a presentation setting and does not affect calculations.
    pub currency_decimals: Option<u32>,

    /// When true, render base-currency values with thousands separators.
    ///
    /// This only affects optional `*_display` fields and UI surfaces.
    pub currency_grouping: bool,

    /// Optional currency symbol (e.g. "$", "€") for display rendering.
    ///
    /// This only affects optional `*_display` fields and UI surfaces.
    pub currency_symbol: Option<String>,

    /// When true and `currency_decimals` is set, display values with exactly
    /// that many decimal places (padding with trailing zeros).
    ///
    /// This only affects optional `*_display` fields and UI surfaces.
    pub currency_fixed_decimals: bool,
}

/// Tray UI configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TrayConfig {
    /// Maximum number of recent portfolio history rows shown in the tray menu.
    pub history_points: usize,

    /// Relative-date DSL entries expanded into portfolio history rows in the tray menu.
    pub history_spec: Vec<String>,

    /// Spending lookback windows (days) shown in tray menu.
    pub spending_windows_days: Vec<u32>,

    /// Number of recent transactions shown in tray menu (last 30 days).
    pub transaction_count: usize,
}

impl Default for TrayConfig {
    fn default() -> Self {
        Self {
            history_points: 17,
            history_spec: vec![
                "last 4 days".to_string(),
                "1 week ago".to_string(),
                "2 weeks ago".to_string(),
                "last 12 months".to_string(),
            ],
            spending_windows_days: vec![7, 30, 90, 365],
            transaction_count: 30,
        }
    }
}

/// Spending report configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct SpendingConfig {
    /// Ignore matching account IDs or names.
    ///
    /// These are synthesized into transaction ignore rules used by list/spending/TUI views.
    pub ignore_accounts: Vec<String>,
    /// Ignore matching connection IDs or names.
    ///
    /// These are synthesized into transaction ignore rules used by list/spending/TUI views.
    pub ignore_connections: Vec<String>,
    /// Ignore accounts containing any matching account tag.
    ///
    /// Used by default portfolio spending scope and by list/TUI ignored-transaction filtering.
    pub ignore_tags: Vec<String>,
}

/// Portfolio reporting configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct PortfolioConfig {
    /// Optional virtual account that subtracts an estimate of latent capital
    /// gains tax from portfolio net worth.
    pub latent_capital_gains_tax: LatentCapitalGainsTaxConfig,
}

/// Latent capital gains tax liability configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LatentCapitalGainsTaxConfig {
    /// Include a virtual liability account in portfolio snapshots.
    pub enabled: bool,

    /// Tax rate as a decimal fraction (for example, 0.23 for 23%).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rate: Option<f64>,

    /// Display name for the virtual account.
    pub account_name: String,
}

impl Default for LatentCapitalGainsTaxConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            rate: None,
            account_name: "Latent Capital Gains Tax".to_string(),
        }
    }
}

/// Global ignore rules configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct IgnoreConfig {
    /// Rules for ignoring transactions in app-level transaction views.
    pub transaction_rules: Vec<TransactionIgnoreRule>,
}

/// A single transaction ignore rule.
///
/// All configured fields are matched as regex patterns and must match (AND semantics)
/// for a transaction to be ignored.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct TransactionIgnoreRule {
    pub account_id: Option<String>,
    pub account_name: Option<String>,
    pub connection_id: Option<String>,
    pub connection_name: Option<String>,
    pub synchronizer: Option<String>,
    pub description: Option<String>,
    pub status: Option<String>,
    pub amount: Option<String>,
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

/// Historical valuation configuration.
pub const DEFAULT_HISTORY_PORTFOLIO_GRANULARITY: &str = "daily";
pub const DEFAULT_HISTORY_CHANGE_POINTS_GRANULARITY: &str = "none";
pub const DEFAULT_HISTORY_INCLUDE_PRICES: bool = true;
pub const DEFAULT_HISTORY_GRAPH_RANGE: &str = "1y";
pub const DEFAULT_HISTORY_GRAPH_GRANULARITY: &str = "weekly";

pub fn default_history_portfolio_granularity() -> String {
    DEFAULT_HISTORY_PORTFOLIO_GRANULARITY.to_string()
}

pub fn default_history_change_points_granularity() -> String {
    DEFAULT_HISTORY_CHANGE_POINTS_GRANULARITY.to_string()
}

pub fn default_history_include_prices() -> bool {
    DEFAULT_HISTORY_INCLUDE_PRICES
}

pub fn default_history_graph_range() -> String {
    DEFAULT_HISTORY_GRAPH_RANGE.to_string()
}

pub fn default_history_graph_granularity() -> String {
    DEFAULT_HISTORY_GRAPH_GRANULARITY.to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct HistoryConfig {
    /// When true, historical portfolio valuation may project a later cached
    /// price or FX rate backward when no acceptable earlier reading exists.
    pub allow_future_projection: bool,

    /// Optional bound for older cached history lookups before future
    /// projection is considered. When unset, older lookup remains unbounded.
    pub lookback_days: Option<u32>,

    /// Default granularity for portfolio history when no CLI/API override is supplied.
    #[serde(default = "default_history_portfolio_granularity")]
    pub portfolio_granularity: String,

    /// Default granularity for portfolio change-points when no CLI/API override is supplied.
    #[serde(default = "default_history_change_points_granularity")]
    pub change_points_granularity: String,

    /// Whether history/change-point commands include price changes by default.
    #[serde(default = "default_history_include_prices")]
    pub include_prices: bool,

    /// Default range preset for graphing clients.
    #[serde(default = "default_history_graph_range")]
    pub graph_range: String,

    /// Default sampling preset for graphing clients.
    #[serde(default = "default_history_graph_granularity")]
    pub graph_granularity: String,
}

impl Default for HistoryConfig {
    fn default() -> Self {
        Self {
            allow_future_projection: false,
            lookback_days: None,
            portfolio_granularity: default_history_portfolio_granularity(),
            change_points_granularity: default_history_change_points_granularity(),
            include_prices: default_history_include_prices(),
            graph_range: default_history_graph_range(),
            graph_granularity: default_history_graph_granularity(),
        }
    }
}

/// Git-related configuration.
#[derive(Debug, Clone, Serialize, Default)]
pub struct GitConfig {
    /// Enable automatic commits after data changes.
    pub auto_commit: bool,

    /// Enable automatic pushes after successful auto-commits.
    pub auto_push: bool,

    /// Pull remote changes before commands that edit data.
    pub pull_before_edit: bool,

    /// Push committed changes after sync commands complete.
    pub push_after_sync: bool,

    /// Merge `origin/master` before running commands.
    pub merge_master_before_command: bool,
}

impl<'de> Deserialize<'de> for GitConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize, Default)]
        #[serde(default)]
        struct RawGitConfig {
            auto_commit: bool,
            auto_push: Option<bool>,
            pull_before_edit: bool,
            push_after_sync: bool,
            merge_master_before_command: bool,
        }

        let raw = RawGitConfig::deserialize(deserializer)?;

        Ok(Self {
            auto_commit: raw.auto_commit,
            auto_push: raw.auto_push.unwrap_or(raw.auto_commit),
            pull_before_edit: raw.pull_before_edit,
            push_after_sync: raw.push_after_sync,
            merge_master_before_command: raw.merge_master_before_command,
        })
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

    /// Display/output formatting settings.
    #[serde(default)]
    pub display: DisplayConfig,

    /// Refresh/staleness settings.
    #[serde(default)]
    pub refresh: RefreshConfig,

    /// Historical valuation settings.
    #[serde(default)]
    pub history: HistoryConfig,

    /// Tray UI settings.
    #[serde(default)]
    pub tray: TrayConfig,

    /// Spending report settings.
    #[serde(default)]
    pub spending: SpendingConfig,

    /// Portfolio reporting settings.
    #[serde(default)]
    pub portfolio: PortfolioConfig,

    /// Global ignore rules.
    #[serde(default)]
    pub ignore: IgnoreConfig,

    /// Git-related settings.
    #[serde(default)]
    pub git: GitConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            data_dir: None,
            reporting_currency: default_reporting_currency(),
            display: DisplayConfig::default(),
            refresh: RefreshConfig::default(),
            history: HistoryConfig::default(),
            tray: TrayConfig::default(),
            spending: SpendingConfig::default(),
            portfolio: PortfolioConfig::default(),
            ignore: IgnoreConfig::default(),
            git: GitConfig::default(),
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
        normalize_path_components(match self.data_dir.as_deref().map(expand_tilde_path) {
            Some(data_dir) if data_dir.is_absolute() => data_dir,
            Some(data_dir) => config_dir.join(data_dir),
            None => config_dir.to_path_buf(),
        })
    }
}

/// Loaded configuration with resolved paths.
#[derive(Debug, Clone)]
pub struct ResolvedConfig {
    /// The resolved data directory path.
    pub data_dir: PathBuf,

    /// Currency for reporting all values (e.g., "USD")
    pub reporting_currency: String,

    /// Display/output formatting settings.
    pub display: DisplayConfig,

    /// Refresh/staleness settings.
    pub refresh: RefreshConfig,

    /// Historical valuation settings.
    pub history: HistoryConfig,

    /// Tray UI settings.
    pub tray: TrayConfig,

    /// Spending report settings.
    pub spending: SpendingConfig,

    /// Portfolio reporting settings.
    pub portfolio: PortfolioConfig,

    /// Global ignore rules.
    pub ignore: IgnoreConfig,

    /// Git-related settings.
    pub git: GitConfig,
}

/// Returns the default config file path.
///
/// Resolution order:
/// 1. `./keepbook.toml` if it exists in current directory
/// 2. `~/.local/share/keepbook/keepbook.toml` (XDG data directory)
pub fn default_config_path() -> PathBuf {
    let local_config = PathBuf::from("keepbook.toml");
    if local_config.exists() {
        return absolute_from_current_dir(local_config);
    }

    // XDG data directory fallback
    if let Some(data_dir) = dirs::data_dir() {
        return absolute_from_current_dir(data_dir.join("keepbook").join("keepbook.toml"));
    }

    // Final fallback to local
    absolute_from_current_dir(local_config)
}

fn absolute_from_current_dir(path: PathBuf) -> PathBuf {
    let path = expand_tilde_path(&path);
    if path.is_absolute() {
        return normalize_path_components(path);
    }

    std::env::current_dir()
        .map(|cwd| cwd.join(&path))
        .map(normalize_path_components)
        .unwrap_or_else(|_| normalize_path_components(path))
}

fn expand_tilde_path(path: &Path) -> PathBuf {
    let mut components = path.components();
    let Some(first) = components.next() else {
        return path.to_path_buf();
    };

    if first.as_os_str() != "~" {
        return path.to_path_buf();
    }

    let Some(home_dir) = dirs::home_dir() else {
        return path.to_path_buf();
    };

    components.fold(home_dir, |acc, component| acc.join(component.as_os_str()))
}

fn normalize_path_components(path: PathBuf) -> PathBuf {
    let mut normalized = PathBuf::new();

    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !normalized.pop() {
                    normalized.push(component.as_os_str());
                }
            }
            _ => normalized.push(component.as_os_str()),
        }
    }

    normalized
}

impl ResolvedConfig {
    /// Load and resolve config from a file path.
    ///
    /// The data directory is resolved relative to the config file's parent directory.
    pub fn load(config_path: &Path) -> Result<Self> {
        let config_path = expand_tilde_path(config_path)
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
            display: config.display,
            refresh: config.refresh,
            history: config.history,
            tray: config.tray,
            spending: config.spending,
            portfolio: config.portfolio,
            ignore: config.ignore,
            git: config.git,
        })
    }

    /// Load config, creating a default if the file doesn't exist.
    ///
    /// If the config file doesn't exist, uses the config file's intended
    /// parent directory as the data directory.
    pub fn load_or_default(config_path: &Path) -> Result<Self> {
        let config_path = expand_tilde_path(config_path);

        if config_path.exists() {
            Self::load(&config_path)
        } else {
            // Resolve the config path relative to current directory
            let config_path = if config_path.is_relative() {
                std::env::current_dir()
                    .context("Failed to get current directory")?
                    .join(&config_path)
            } else {
                config_path
            };

            // Use the intended config directory as data dir
            let config_dir = config_path
                .parent()
                .context("Config path has no parent directory")?;

            Ok(Self {
                data_dir: config_dir.to_path_buf(),
                reporting_currency: default_reporting_currency(),
                display: DisplayConfig::default(),
                refresh: RefreshConfig::default(),
                history: HistoryConfig::default(),
                tray: TrayConfig::default(),
                spending: SpendingConfig::default(),
                portfolio: PortfolioConfig::default(),
                ignore: IgnoreConfig::default(),
                git: GitConfig::default(),
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
    fn test_tilde_data_dir_expands_to_home() {
        let Some(home_dir) = dirs::home_dir() else {
            return;
        };

        let config = Config {
            data_dir: Some(PathBuf::from("~/keepbook-data")),
            ..Default::default()
        };
        let config_dir = Path::new("/home/user/finances");
        assert_eq!(
            config.resolve_data_dir(config_dir),
            home_dir.join("keepbook-data")
        );
    }

    #[test]
    fn test_expand_tilde_path() {
        let Some(home_dir) = dirs::home_dir() else {
            return;
        };

        assert_eq!(expand_tilde_path(Path::new("~")), home_dir);
        assert_eq!(
            expand_tilde_path(Path::new("~/keepbook.toml")),
            dirs::home_dir().unwrap().join("keepbook.toml")
        );
        assert_eq!(
            expand_tilde_path(Path::new("~other/keepbook.toml")),
            PathBuf::from("~other/keepbook.toml")
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
    fn test_load_git_config() -> Result<()> {
        let dir = TempDir::new()?;
        let config_path = dir.path().join("keepbook.toml");

        let mut file = std::fs::File::create(&config_path)?;
        writeln!(file, "[git]")?;
        writeln!(file, "auto_commit = true")?;
        writeln!(file, "auto_push = true")?;
        writeln!(file, "pull_before_edit = true")?;
        writeln!(file, "push_after_sync = true")?;
        writeln!(file, "merge_master_before_command = true")?;

        let config = Config::load(&config_path)?;
        assert!(config.git.auto_commit);
        assert!(config.git.auto_push);
        assert!(config.git.pull_before_edit);
        assert!(config.git.push_after_sync);
        assert!(config.git.merge_master_before_command);

        Ok(())
    }

    #[test]
    fn test_load_git_config_defaults_auto_push_to_auto_commit() -> Result<()> {
        let dir = TempDir::new()?;
        let config_path = dir.path().join("keepbook.toml");

        let mut file = std::fs::File::create(&config_path)?;
        writeln!(file, "[git]")?;
        writeln!(file, "auto_commit = true")?;

        let config = Config::load(&config_path)?;
        assert!(config.git.auto_commit);
        assert!(config.git.auto_push);
        assert!(!config.git.pull_before_edit);
        assert!(!config.git.push_after_sync);
        assert!(!config.git.merge_master_before_command);

        Ok(())
    }

    #[test]
    fn test_load_git_config_allows_disabling_auto_push() -> Result<()> {
        let dir = TempDir::new()?;
        let config_path = dir.path().join("keepbook.toml");

        let mut file = std::fs::File::create(&config_path)?;
        writeln!(file, "[git]")?;
        writeln!(file, "auto_commit = true")?;
        writeln!(file, "auto_push = false")?;

        let config = Config::load(&config_path)?;
        assert!(config.git.auto_commit);
        assert!(!config.git.auto_push);

        Ok(())
    }

    #[test]
    fn test_load_display_currency_decimals() -> Result<()> {
        let dir = TempDir::new()?;
        let config_path = dir.path().join("keepbook.toml");

        let mut file = std::fs::File::create(&config_path)?;
        writeln!(file, "[display]")?;
        writeln!(file, "currency_decimals = 2")?;

        let config = Config::load(&config_path)?;
        assert_eq!(config.display.currency_decimals, Some(2));

        Ok(())
    }

    #[test]
    fn test_load_display_currency_formatting_options() -> Result<()> {
        let dir = TempDir::new()?;
        let config_path = dir.path().join("keepbook.toml");

        let mut file = std::fs::File::create(&config_path)?;
        writeln!(file, "[display]")?;
        writeln!(file, "currency_grouping = true")?;
        writeln!(file, "currency_symbol = \"$\"")?;
        writeln!(file, "currency_fixed_decimals = true")?;

        let config = Config::load(&config_path)?;
        assert!(config.display.currency_grouping);
        assert_eq!(config.display.currency_symbol.as_deref(), Some("$"));
        assert!(config.display.currency_fixed_decimals);

        Ok(())
    }

    #[test]
    fn test_load_tray_config() -> Result<()> {
        let dir = TempDir::new()?;
        let config_path = dir.path().join("keepbook.toml");

        let mut file = std::fs::File::create(&config_path)?;
        writeln!(file, "[tray]")?;
        writeln!(file, "history_points = 5")?;
        writeln!(file, "spending_windows_days = [3, 14, 60]")?;

        let config = Config::load(&config_path)?;
        assert_eq!(config.tray.history_points, 5);
        assert_eq!(config.tray.spending_windows_days, vec![3, 14, 60]);

        Ok(())
    }

    #[test]
    fn test_load_history_defaults_config() -> Result<()> {
        let dir = TempDir::new()?;
        let config_path = dir.path().join("keepbook.toml");

        let mut file = std::fs::File::create(&config_path)?;
        writeln!(file, "[history]")?;
        writeln!(file, "portfolio_granularity = \"weekly\"")?;
        writeln!(file, "change_points_granularity = \"daily\"")?;
        writeln!(file, "include_prices = false")?;
        writeln!(file, "graph_range = \"2y\"")?;
        writeln!(file, "graph_granularity = \"monthly\"")?;

        let config = Config::load(&config_path)?;
        assert_eq!(config.history.portfolio_granularity, "weekly");
        assert_eq!(config.history.change_points_granularity, "daily");
        assert!(!config.history.include_prices);
        assert_eq!(config.history.graph_range, "2y");
        assert_eq!(config.history.graph_granularity, "monthly");

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

    #[test]
    fn test_default_git_config() {
        let config = Config::default();
        assert!(!config.git.auto_commit);
        assert!(!config.git.auto_push);
        assert!(!config.git.pull_before_edit);
        assert!(!config.git.push_after_sync);
        assert!(!config.git.merge_master_before_command);
    }

    #[test]
    fn test_default_tray_config() {
        let config = Config::default();
        assert_eq!(config.tray.history_points, 17);
        assert_eq!(
            config.tray.history_spec,
            vec![
                "last 4 days".to_string(),
                "1 week ago".to_string(),
                "2 weeks ago".to_string(),
                "last 12 months".to_string()
            ]
        );
        assert_eq!(config.tray.spending_windows_days, vec![7, 30, 90, 365]);
    }

    #[test]
    fn test_default_history_config() {
        let config = Config::default();
        assert_eq!(config.history.portfolio_granularity, "daily");
        assert_eq!(config.history.change_points_granularity, "none");
        assert!(config.history.include_prices);
        assert_eq!(config.history.graph_range, "1y");
        assert_eq!(config.history.graph_granularity, "weekly");
    }

    #[test]
    fn test_default_spending_config() {
        let config = Config::default();
        assert!(config.spending.ignore_accounts.is_empty());
        assert!(config.spending.ignore_connections.is_empty());
        assert!(config.spending.ignore_tags.is_empty());
    }

    #[test]
    fn test_default_ignore_config() {
        let config = Config::default();
        assert!(config.ignore.transaction_rules.is_empty());
    }

    #[test]
    fn test_absolute_from_current_dir_preserves_absolute_paths() {
        let path = PathBuf::from("/tmp/keepbook.toml");
        assert_eq!(absolute_from_current_dir(path.clone()), path);
    }

    #[test]
    fn test_absolute_from_current_dir_resolves_relative_paths() {
        let path = absolute_from_current_dir(PathBuf::from("keepbook.toml"));
        assert!(path.is_absolute());
        assert!(path.ends_with("keepbook.toml"));
    }

    #[test]
    fn test_absolute_from_current_dir_expands_tilde_paths() {
        let Some(home_dir) = dirs::home_dir() else {
            return;
        };

        let path = absolute_from_current_dir(PathBuf::from("~/keepbook.toml"));
        assert_eq!(path, home_dir.join("keepbook.toml"));
    }

    #[test]
    fn test_load_spending_config() -> Result<()> {
        let dir = TempDir::new()?;
        let config_path = dir.path().join("keepbook.toml");

        let mut file = std::fs::File::create(&config_path)?;
        writeln!(file, "[spending]")?;
        writeln!(file, "ignore_accounts = [\"Individual\", \"acct-1\"]")?;
        writeln!(file, "ignore_connections = [\"Schwab\"]")?;
        writeln!(file, "ignore_tags = [\"brokerage\"]")?;

        let config = Config::load(&config_path)?;
        assert_eq!(config.spending.ignore_accounts.len(), 2);
        assert_eq!(config.spending.ignore_connections, vec!["Schwab"]);
        assert_eq!(config.spending.ignore_tags, vec!["brokerage"]);

        Ok(())
    }

    #[test]
    fn test_load_portfolio_latent_capital_gains_tax_config() -> Result<()> {
        let dir = TempDir::new()?;
        let config_path = dir.path().join("keepbook.toml");

        let mut file = std::fs::File::create(&config_path)?;
        writeln!(file, "[portfolio.latent_capital_gains_tax]")?;
        writeln!(file, "enabled = true")?;
        writeln!(file, "rate = 0.23")?;
        writeln!(file, "account_name = \"Estimated Tax Liability\"")?;

        let config = Config::load(&config_path)?;
        assert!(config.portfolio.latent_capital_gains_tax.enabled);
        assert_eq!(config.portfolio.latent_capital_gains_tax.rate, Some(0.23));
        assert_eq!(
            config.portfolio.latent_capital_gains_tax.account_name,
            "Estimated Tax Liability"
        );

        Ok(())
    }

    #[test]
    fn test_load_ignore_transaction_rules_config() -> Result<()> {
        let dir = TempDir::new()?;
        let config_path = dir.path().join("keepbook.toml");

        let mut file = std::fs::File::create(&config_path)?;
        writeln!(file, "[ignore]")?;
        writeln!(file, "[[ignore.transaction_rules]]")?;
        writeln!(file, "account_name = \"(?i)^Investor Checking$\"")?;
        writeln!(
            file,
            "description = \"(?i)credit\\\\s+crd\\\\s+(?:e?pay|autopay)\""
        )?;
        writeln!(file, "synchronizer = \"(?i)^schwab$\"")?;

        let config = Config::load(&config_path)?;
        assert_eq!(config.ignore.transaction_rules.len(), 1);
        let rule = &config.ignore.transaction_rules[0];
        assert_eq!(
            rule.account_name.as_deref(),
            Some("(?i)^Investor Checking$")
        );
        assert_eq!(
            rule.description.as_deref(),
            Some("(?i)credit\\s+crd\\s+(?:e?pay|autopay)")
        );
        assert_eq!(rule.synchronizer.as_deref(), Some("(?i)^schwab$"));

        Ok(())
    }

    #[test]
    fn test_config_load_or_default_missing_file() -> Result<()> {
        let dir = TempDir::new()?;
        let config_path = dir.path().join("missing.toml");

        let config = Config::load_or_default(&config_path)?;
        assert_eq!(config.data_dir, None);
        assert_eq!(config.reporting_currency, "USD");

        Ok(())
    }

    #[test]
    fn test_resolved_config_load_or_default_missing_file() -> Result<()> {
        let dir = TempDir::new()?;
        let config_path = dir.path().join("keepbook.toml");

        let resolved = ResolvedConfig::load_or_default(&config_path)?;
        assert_eq!(resolved.data_dir, dir.path());
        assert_eq!(resolved.reporting_currency, "USD");

        Ok(())
    }

    #[test]
    fn test_resolved_config_resolves_relative_data_dir() -> Result<()> {
        let dir = TempDir::new()?;
        let config_path = dir.path().join("keepbook.toml");

        let mut file = std::fs::File::create(&config_path)?;
        writeln!(file, "data_dir = \"./data\"")?;

        let resolved = ResolvedConfig::load(&config_path)?;
        let expected_config_dir = config_path
            .canonicalize()?
            .parent()
            .context("Config file has no parent directory")?
            .to_path_buf();
        assert_eq!(resolved.data_dir, expected_config_dir.join("data"));

        Ok(())
    }
}
