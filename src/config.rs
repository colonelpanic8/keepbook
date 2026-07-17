use std::collections::{HashMap, HashSet};
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
    /// When true, desktop UI starts hidden and is opened from the tray icon.
    pub start_minimized: bool,

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
            start_minimized: false,
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

/// User-owned tag hierarchy configuration.
///
/// `parents` is a recursive child -> parents graph. For example:
///
/// ```toml
/// [tags.parents]
/// "Groceries" = ["Food"]
/// "Supermarket" = ["Groceries"]
/// ```
///
/// A transaction tagged `Supermarket` is therefore also under the top-level
/// `Food` tag for default spending breakdowns.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct TagsConfig {
    /// Provider/user label aliases mapped to canonical keepbook tag names.
    pub aliases: HashMap<String, String>,

    /// Child tag -> parent tag list.
    pub parents: HashMap<String, Vec<String>>,
}

impl TagsConfig {
    pub fn canonical_label(&self, raw: &str) -> Option<String> {
        self.configured_canonical_label(raw, &mut HashSet::new())
            .or_else(|| normalized_user_label(raw))
    }

    pub fn canonical_provider_label(&self, raw: &str) -> Option<String> {
        self.configured_canonical_label(raw, &mut HashSet::new())
            .or_else(|| normalized_display_label(raw))
    }

    pub fn root_tags_for(&self, raw: &str) -> Vec<String> {
        let Some(label) = self.canonical_label(raw) else {
            return Vec::new();
        };
        let roots = self.root_tags_for_canonical(&label, &mut HashSet::new());
        if roots.is_empty() {
            vec![label]
        } else {
            roots
        }
    }

    pub fn parent_root_tags_for(&self, raw: &str) -> Vec<String> {
        let Some(label) = self.canonical_label(raw) else {
            return Vec::new();
        };
        if self.direct_parents_for_canonical(&label).is_empty() {
            return Vec::new();
        }
        self.root_tags_for_canonical(&label, &mut HashSet::new())
    }

    pub fn canonicalize_labels(&self, labels: Vec<String>) -> Vec<String> {
        labels.into_iter().fold(Vec::new(), |mut acc, label| {
            if let Some(canonical) = self.canonical_label(&label) {
                push_unique_label(&mut acc, canonical);
            }
            acc
        })
    }

    fn configured_canonical_label(
        &self,
        raw: &str,
        seen_aliases: &mut HashSet<String>,
    ) -> Option<String> {
        let lookup = label_lookup_key(raw)?;
        for (alias, target) in &self.aliases {
            if label_lookup_key(alias).as_deref() == Some(lookup.as_str()) {
                if !seen_aliases.insert(lookup) {
                    return normalized_user_label(raw);
                }
                return self
                    .configured_canonical_label(target, seen_aliases)
                    .or_else(|| normalized_user_label(target));
            }
        }

        for (child, parents) in &self.parents {
            if label_lookup_key(child).as_deref() == Some(lookup.as_str()) {
                return normalized_user_label(child);
            }
            for parent in parents {
                if label_lookup_key(parent).as_deref() == Some(lookup.as_str()) {
                    return normalized_user_label(parent);
                }
            }
        }

        None
    }

    fn direct_parents_for_canonical(&self, canonical: &str) -> Vec<String> {
        let Some(lookup) = label_lookup_key(canonical) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for (child, parents) in &self.parents {
            if label_lookup_key(child).as_deref() != Some(lookup.as_str()) {
                continue;
            }
            for parent in parents {
                if let Some(parent) = self.canonical_label(parent) {
                    push_unique_label(&mut out, parent);
                }
            }
        }
        out
    }

    fn root_tags_for_canonical(&self, canonical: &str, seen: &mut HashSet<String>) -> Vec<String> {
        let Some(lookup) = label_lookup_key(canonical) else {
            return Vec::new();
        };
        if !seen.insert(lookup) {
            return Vec::new();
        }

        let parents = self.direct_parents_for_canonical(canonical);
        if parents.is_empty() {
            return vec![canonical.to_string()];
        }

        let mut roots = Vec::new();
        for parent in parents {
            let mut branch_seen = seen.clone();
            let parent_roots = self.root_tags_for_canonical(&parent, &mut branch_seen);
            if parent_roots.is_empty() {
                push_unique_label(&mut roots, parent);
            } else {
                for root in parent_roots {
                    push_unique_label(&mut roots, root);
                }
            }
        }
        roots
    }
}

fn push_unique_label(values: &mut Vec<String>, value: String) {
    if !values
        .iter()
        .any(|existing| label_lookup_key(existing) == label_lookup_key(&value))
    {
        values.push(value);
    }
}

fn label_lookup_key(raw: &str) -> Option<String> {
    let normalized = raw.trim().replace(['_', '-'], " ").replace('&', " and ");
    let words = normalized
        .split_whitespace()
        .flat_map(|word| word.chars())
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(|ch| ch.to_lowercase())
        .collect::<String>();
    if words.is_empty() {
        None
    } else {
        Some(words)
    }
}

fn normalized_display_label(raw: &str) -> Option<String> {
    let normalized = raw.trim().replace(['_', '-'], " ");
    if normalized.is_empty() {
        return None;
    }

    let words = normalized
        .split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => format!("{}{}", first.to_uppercase(), chars.as_str().to_lowercase()),
                None => String::new(),
            }
        })
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>();

    if words.is_empty() {
        None
    } else {
        Some(words.join(" "))
    }
}

fn normalized_user_label(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
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

fn default_openai_model() -> String {
    "gpt-5.5".to_string()
}

/// AI assistant configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct AiConfig {
    /// OpenAI Responses API settings.
    pub openai: AiOpenAiConfig,
}

/// OpenAI Responses API settings for local assistant features.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AiOpenAiConfig {
    /// Model used for transaction rule suggestions.
    #[serde(default = "default_openai_model")]
    pub model: String,

    /// Environment variable that contains the API key. Defaults to OPENAI_API_KEY.
    pub api_key_env: Option<String>,

    /// Optional credential-store config. Supports the same backend table shape as connection
    /// credentials and should expose an `api_key` field.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credentials: Option<toml::Value>,
}

impl Default for AiOpenAiConfig {
    fn default() -> Self {
        Self {
            model: default_openai_model(),
            api_key_env: None,
            credentials: None,
        }
    }
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

    /// SSH private key path used by libgit2 git operations.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ssh_key_path: Option<PathBuf>,
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
            ssh_key_path: None,
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

    /// Tag hierarchy and aliases.
    #[serde(default)]
    pub tags: TagsConfig,

    /// Portfolio reporting settings.
    #[serde(default)]
    pub portfolio: PortfolioConfig,

    /// Global ignore rules.
    #[serde(default)]
    pub ignore: IgnoreConfig,

    /// AI assistant settings.
    #[serde(default)]
    pub ai: AiConfig,

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
            tags: TagsConfig::default(),
            portfolio: PortfolioConfig::default(),
            ignore: IgnoreConfig::default(),
            ai: AiConfig::default(),
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

    /// Tag hierarchy and aliases.
    pub tags: TagsConfig,

    /// Portfolio reporting settings.
    pub portfolio: PortfolioConfig,

    /// Global ignore rules.
    pub ignore: IgnoreConfig,

    /// AI assistant settings.
    pub ai: AiConfig,

    /// Git-related settings.
    pub git: GitConfig,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct DeviceConfig {
    git: DeviceGitConfig,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct DeviceGitConfig {
    ssh_key_path: Option<PathBuf>,
}

/// Returns the device-local config path used for settings that must not be
/// stored in a repository's `keepbook.toml`.
pub fn device_config_path(_repo_config_path: &Path) -> Option<PathBuf> {
    #[cfg(target_os = "android")]
    {
        return app_private_device_config_path(_repo_config_path);
    }

    #[cfg(not(target_os = "android"))]
    {
        dirs::config_dir().map(|dir| dir.join("keepbook").join("device.toml"))
    }
}

#[cfg(any(target_os = "android", test))]
fn app_private_device_config_path(repo_config_path: &Path) -> Option<PathBuf> {
    let repo_dir = repo_config_path.parent()?;
    Some(
        repo_dir
            .parent()
            .unwrap_or(repo_dir)
            .join("keepbook-device.toml"),
    )
}

/// Loads the device-local SSH identity path, resolving relative paths from the
/// device config rather than from the repository config.
pub fn load_device_ssh_key_path(repo_config_path: &Path) -> Result<Option<PathBuf>> {
    let Some(device_config_path) = device_config_path(repo_config_path) else {
        return Ok(None);
    };
    load_device_ssh_key_path_from(&device_config_path)
}

/// Loads an SSH identity from an explicit device config path.
pub fn load_device_ssh_key_path_from(device_config_path: &Path) -> Result<Option<PathBuf>> {
    if !device_config_path.exists() {
        return Ok(None);
    }

    let content = std::fs::read_to_string(device_config_path).with_context(|| {
        format!(
            "Failed to read device config file: {}",
            device_config_path.display()
        )
    })?;
    let config: DeviceConfig = toml::from_str(&content).with_context(|| {
        format!(
            "Failed to parse device config file: {}",
            device_config_path.display()
        )
    })?;
    let Some(path) = config.git.ssh_key_path else {
        return Ok(None);
    };
    let device_config_dir = device_config_path
        .parent()
        .context("Device config file has no parent directory")?;

    Ok(Some(resolve_config_path(device_config_dir, path)))
}

/// Returns the default config file path.
///
/// Resolution order:
/// 1. `./keepbook.toml` if it exists in current directory
/// 2. The platform data directory's `keepbook/keepbook.toml`
///    (for example, `~/.local/share/keepbook/keepbook.toml` on Linux or
///    `~/Library/Application Support/keepbook/keepbook.toml` on macOS)
pub fn default_config_path() -> PathBuf {
    let local_config = PathBuf::from("keepbook.toml");
    if local_config.exists() {
        return absolute_from_current_dir(local_config);
    }

    // Platform data directory fallback. On Linux this follows XDG; on macOS it
    // uses the standard Application Support directory.
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

fn resolve_config_path(config_dir: &Path, path: PathBuf) -> PathBuf {
    let path = expand_tilde_path(&path);
    if path.is_absolute() {
        normalize_path_components(path)
    } else {
        normalize_path_components(config_dir.join(path))
    }
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

        let mut config = Config::load(&config_path)?;
        let data_dir = config.resolve_data_dir(config_dir);
        config.git.ssh_key_path = load_device_ssh_key_path(&config_path)?;

        Ok(Self {
            data_dir,
            reporting_currency: config.reporting_currency,
            display: config.display,
            refresh: config.refresh,
            history: config.history,
            tray: config.tray,
            spending: config.spending,
            tags: config.tags,
            portfolio: config.portfolio,
            ignore: config.ignore,
            ai: config.ai,
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

            let git = GitConfig {
                ssh_key_path: load_device_ssh_key_path(&config_path)?,
                ..GitConfig::default()
            };

            Ok(Self {
                data_dir: config_dir.to_path_buf(),
                reporting_currency: default_reporting_currency(),
                display: DisplayConfig::default(),
                refresh: RefreshConfig::default(),
                history: HistoryConfig::default(),
                tray: TrayConfig::default(),
                spending: SpendingConfig::default(),
                tags: TagsConfig::default(),
                portfolio: PortfolioConfig::default(),
                ignore: IgnoreConfig::default(),
                ai: AiConfig::default(),
                git,
            })
        }
    }
}

#[cfg(test)]
#[path = "../tests/unit/config_tests.rs"]
mod config_tests;
