# Staleness/Refresh Configuration Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Implement hierarchical staleness configuration with auto-refresh for portfolio snapshots.

**Architecture:** Duration parsing utilities → Config structs → Staleness module → Model updates → Service integration → CLI updates.

**Tech Stack:** Rust, serde (with custom deserializer), tracing, chrono

---

### Task 1: Duration Parsing Module

**Files:**
- Create: `src/duration.rs`
- Modify: `src/lib.rs`
- Modify: `src/main.rs` (remove duplicate)

**Step 1: Create the duration module with parsing and serde support**

Create `src/duration.rs`:

```rust
//! Duration parsing utilities for human-readable durations like "14d", "24h".

use std::time::Duration;

use anyhow::{Context, Result};
use serde::{de, Deserialize, Deserializer};

/// Parse a duration string like "14d", "24h", "30m", "60s".
pub fn parse_duration(s: &str) -> Result<Duration> {
    let s = s.trim().to_lowercase();
    let (num, unit) = if s.ends_with('d') {
        (s.trim_end_matches('d'), "d")
    } else if s.ends_with('h') {
        (s.trim_end_matches('h'), "h")
    } else if s.ends_with('m') {
        (s.trim_end_matches('m'), "m")
    } else if s.ends_with('s') {
        (s.trim_end_matches('s'), "s")
    } else {
        anyhow::bail!("Duration must end with d, h, m, or s");
    };

    let num: u64 = num.parse().with_context(|| "Invalid number in duration")?;

    Ok(match unit {
        "d" => Duration::from_secs(num * 24 * 60 * 60),
        "h" => Duration::from_secs(num * 60 * 60),
        "m" => Duration::from_secs(num * 60),
        "s" => Duration::from_secs(num),
        _ => unreachable!(),
    })
}

/// Format a duration as a human-readable string.
pub fn format_duration(d: Duration) -> String {
    let secs = d.as_secs();
    if secs >= 24 * 60 * 60 && secs % (24 * 60 * 60) == 0 {
        format!("{}d", secs / (24 * 60 * 60))
    } else if secs >= 60 * 60 && secs % (60 * 60) == 0 {
        format!("{}h", secs / (60 * 60))
    } else if secs >= 60 && secs % 60 == 0 {
        format!("{}m", secs / 60)
    } else {
        format!("{}s", secs)
    }
}

/// Serde deserializer for duration strings.
pub fn deserialize_duration<'de, D>(deserializer: D) -> Result<Duration, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    parse_duration(&s).map_err(de::Error::custom)
}

/// Serde deserializer for optional duration strings.
pub fn deserialize_duration_opt<'de, D>(deserializer: D) -> Result<Option<Duration>, D::Error>
where
    D: Deserializer<'de>,
{
    let opt: Option<String> = Option::deserialize(deserializer)?;
    match opt {
        Some(s) => parse_duration(&s).map(Some).map_err(de::Error::custom),
        None => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_days() {
        assert_eq!(parse_duration("14d").unwrap(), Duration::from_secs(14 * 24 * 60 * 60));
        assert_eq!(parse_duration("1d").unwrap(), Duration::from_secs(24 * 60 * 60));
    }

    #[test]
    fn test_parse_hours() {
        assert_eq!(parse_duration("24h").unwrap(), Duration::from_secs(24 * 60 * 60));
        assert_eq!(parse_duration("1h").unwrap(), Duration::from_secs(60 * 60));
    }

    #[test]
    fn test_parse_minutes() {
        assert_eq!(parse_duration("30m").unwrap(), Duration::from_secs(30 * 60));
    }

    #[test]
    fn test_parse_seconds() {
        assert_eq!(parse_duration("60s").unwrap(), Duration::from_secs(60));
    }

    #[test]
    fn test_parse_case_insensitive() {
        assert_eq!(parse_duration("14D").unwrap(), Duration::from_secs(14 * 24 * 60 * 60));
        assert_eq!(parse_duration("24H").unwrap(), Duration::from_secs(24 * 60 * 60));
    }

    #[test]
    fn test_parse_with_whitespace() {
        assert_eq!(parse_duration("  14d  ").unwrap(), Duration::from_secs(14 * 24 * 60 * 60));
    }

    #[test]
    fn test_parse_invalid() {
        assert!(parse_duration("14").is_err());
        assert!(parse_duration("d").is_err());
        assert!(parse_duration("14x").is_err());
    }

    #[test]
    fn test_format_duration() {
        assert_eq!(format_duration(Duration::from_secs(14 * 24 * 60 * 60)), "14d");
        assert_eq!(format_duration(Duration::from_secs(24 * 60 * 60)), "1d");
        assert_eq!(format_duration(Duration::from_secs(2 * 60 * 60)), "2h");
        assert_eq!(format_duration(Duration::from_secs(30 * 60)), "30m");
        assert_eq!(format_duration(Duration::from_secs(45)), "45s");
    }
}
```

**Step 2: Export from lib.rs**

Add to `src/lib.rs`:

```rust
pub mod duration;
```

**Step 3: Update main.rs to use shared function**

In `src/main.rs`, replace the local `parse_duration` function with an import:

Remove the `parse_duration` function (around line 945-968) and add import:

```rust
use keepbook::duration::parse_duration;
```

**Step 4: Run tests**

Run: `cargo test duration`
Expected: All tests pass

**Step 5: Commit**

```bash
git add src/duration.rs src/lib.rs src/main.rs
git commit -m "feat: add duration parsing module with serde support"
```

---

### Task 2: Refresh Config in Global Config

**Files:**
- Modify: `src/config.rs`

**Step 1: Add RefreshConfig struct and update Config**

Add to `src/config.rs`:

```rust
use crate::duration::{deserialize_duration, parse_duration};

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
    #[serde(default = "default_balance_staleness", deserialize_with = "deserialize_duration")]
    pub balance_staleness: std::time::Duration,

    /// How old price data can be before it's considered stale.
    #[serde(default = "default_price_staleness", deserialize_with = "deserialize_duration")]
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
```

Update the `Config` struct to include refresh config:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub data_dir: Option<PathBuf>,
    #[serde(default = "default_reporting_currency")]
    pub reporting_currency: String,
    #[serde(default)]
    pub refresh: RefreshConfig,
}
```

Update `Default for Config`:

```rust
impl Default for Config {
    fn default() -> Self {
        Self {
            data_dir: None,
            reporting_currency: default_reporting_currency(),
            refresh: RefreshConfig::default(),
        }
    }
}
```

Update `ResolvedConfig`:

```rust
#[derive(Debug, Clone)]
pub struct ResolvedConfig {
    pub data_dir: PathBuf,
    pub reporting_currency: String,
    pub refresh: RefreshConfig,
}
```

Update `ResolvedConfig::load` and `load_or_default` to include refresh config.

**Step 2: Add test for refresh config parsing**

```rust
#[test]
fn test_load_refresh_config() -> Result<()> {
    let dir = TempDir::new()?;
    let config_path = dir.path().join("keepbook.toml");

    let mut file = std::fs::File::create(&config_path)?;
    writeln!(file, "[refresh]")?;
    writeln!(file, "balance_staleness = \"7d\"")?;
    writeln!(file, "price_staleness = \"1h\"")?;

    let config = Config::load(&config_path)?;
    assert_eq!(config.refresh.balance_staleness, std::time::Duration::from_secs(7 * 24 * 60 * 60));
    assert_eq!(config.refresh.price_staleness, std::time::Duration::from_secs(60 * 60));

    Ok(())
}

#[test]
fn test_default_refresh_config() {
    let config = Config::default();
    assert_eq!(config.refresh.balance_staleness, std::time::Duration::from_secs(14 * 24 * 60 * 60));
    assert_eq!(config.refresh.price_staleness, std::time::Duration::from_secs(24 * 60 * 60));
}
```

**Step 3: Run tests**

Run: `cargo test config`
Expected: All tests pass

**Step 4: Commit**

```bash
git add src/config.rs
git commit -m "feat: add RefreshConfig to global config"
```

---

### Task 3: Add balance_staleness to ConnectionConfig

**Files:**
- Modify: `src/models/connection.rs`

**Step 1: Add optional balance_staleness field**

```rust
use crate::duration::deserialize_duration_opt;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionConfig {
    pub name: String,
    pub synchronizer: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credentials: Option<CredentialConfig>,
    /// Override balance staleness for this connection.
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "deserialize_duration_opt")]
    pub balance_staleness: Option<std::time::Duration>,
}
```

**Step 2: Run tests**

Run: `cargo test`
Expected: All tests pass (no breaking changes)

**Step 3: Commit**

```bash
git add src/models/connection.rs
git commit -m "feat: add optional balance_staleness to ConnectionConfig"
```

---

### Task 4: Add AccountConfig with balance_staleness

**Files:**
- Modify: `src/models/account.rs`
- Modify: `src/storage/json_file.rs`

**Step 1: Add AccountConfig struct**

Add to `src/models/account.rs`:

```rust
use crate::duration::deserialize_duration_opt;

/// Optional account configuration (stored in account_config.toml).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AccountConfig {
    /// Override balance staleness for this account.
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "deserialize_duration_opt")]
    pub balance_staleness: Option<std::time::Duration>,
}
```

**Step 2: Add method to load account config in storage**

Add to `JsonFileStorage`:

```rust
fn account_config_file(&self, id: &Id) -> PathBuf {
    self.account_dir(id).join("account_config.toml")
}

/// Load optional account config.
pub fn get_account_config(&self, id: &Id) -> Result<Option<AccountConfig>> {
    let path = self.account_config_file(id);
    if !path.exists() {
        return Ok(None);
    }
    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("Failed to read {}", path.display()))?;
    let config: AccountConfig = toml::from_str(&content)
        .with_context(|| format!("Failed to parse {}", path.display()))?;
    Ok(Some(config))
}
```

**Step 3: Run tests**

Run: `cargo test`
Expected: All tests pass

**Step 4: Commit**

```bash
git add src/models/account.rs src/storage/json_file.rs
git commit -m "feat: add AccountConfig with optional balance_staleness"
```

---

### Task 5: Create Staleness Module

**Files:**
- Create: `src/staleness.rs`
- Modify: `src/lib.rs`

**Step 1: Create staleness module**

Create `src/staleness.rs`:

```rust
//! Staleness detection and resolution for balances and prices.

use std::time::Duration;

use chrono::{DateTime, Utc};
use tracing::info;

use crate::config::RefreshConfig;
use crate::market_data::PricePoint;
use crate::models::{Account, Connection};
use crate::models::account::AccountConfig;

/// Result of a staleness check.
#[derive(Debug, Clone)]
pub struct StalenessCheck {
    pub is_stale: bool,
    pub age: Option<Duration>,
    pub threshold: Duration,
}

impl StalenessCheck {
    pub fn stale(age: Duration, threshold: Duration) -> Self {
        Self {
            is_stale: true,
            age: Some(age),
            threshold,
        }
    }

    pub fn fresh(age: Duration, threshold: Duration) -> Self {
        Self {
            is_stale: false,
            age: Some(age),
            threshold,
        }
    }

    pub fn missing(threshold: Duration) -> Self {
        Self {
            is_stale: true,
            age: None,
            threshold,
        }
    }
}

/// Resolve the effective balance staleness threshold for an account.
/// Resolution order: account config → connection config → global config → default.
pub fn resolve_balance_staleness(
    account_config: Option<&AccountConfig>,
    connection: &Connection,
    global_config: &RefreshConfig,
) -> Duration {
    // Account override takes precedence
    if let Some(config) = account_config {
        if let Some(staleness) = config.balance_staleness {
            return staleness;
        }
    }

    // Connection override
    if let Some(staleness) = connection.config.balance_staleness {
        return staleness;
    }

    // Global config
    global_config.balance_staleness
}

/// Check if a connection's balances are stale.
pub fn check_balance_staleness(connection: &Connection, threshold: Duration) -> StalenessCheck {
    let now = Utc::now();

    match &connection.state.last_sync {
        Some(last_sync) => {
            let age = (now - last_sync.at).to_std().unwrap_or(Duration::MAX);
            if age > threshold {
                StalenessCheck::stale(age, threshold)
            } else {
                StalenessCheck::fresh(age, threshold)
            }
        }
        None => StalenessCheck::missing(threshold),
    }
}

/// Check if a price is stale.
pub fn check_price_staleness(
    price: Option<&PricePoint>,
    threshold: Duration,
) -> StalenessCheck {
    let now = Utc::now();

    match price {
        Some(p) => {
            let age = (now - p.timestamp).to_std().unwrap_or(Duration::MAX);
            if age > threshold {
                StalenessCheck::stale(age, threshold)
            } else {
                StalenessCheck::fresh(age, threshold)
            }
        }
        None => StalenessCheck::missing(threshold),
    }
}

/// Log staleness check results.
pub fn log_balance_staleness(connection_name: &str, check: &StalenessCheck) {
    let status = if check.is_stale { "stale" } else { "fresh" };
    let age_str = check
        .age
        .map(|d| crate::duration::format_duration(d))
        .unwrap_or_else(|| "never".to_string());
    let threshold_str = crate::duration::format_duration(check.threshold);

    info!(
        connection = connection_name,
        age = %age_str,
        threshold = %threshold_str,
        status = status,
        "balance staleness check"
    );
}

/// Log price staleness check results.
pub fn log_price_staleness(asset_id: &str, check: &StalenessCheck) {
    let status = if check.is_stale { "stale" } else { "fresh" };
    let age_str = check
        .age
        .map(|d| crate::duration::format_duration(d))
        .unwrap_or_else(|| "never".to_string());
    let threshold_str = crate::duration::format_duration(check.threshold);

    info!(
        asset = asset_id,
        age = %age_str,
        threshold = %threshold_str,
        status = status,
        "price staleness check"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{ConnectionConfig, ConnectionState, LastSync, SyncStatus};

    fn make_connection(last_sync_age_hours: Option<i64>) -> Connection {
        let mut state = ConnectionState::new();
        if let Some(hours) = last_sync_age_hours {
            state.last_sync = Some(LastSync {
                at: Utc::now() - chrono::Duration::hours(hours),
                status: SyncStatus::Success,
                error: None,
            });
        }
        Connection {
            config: ConnectionConfig {
                name: "Test".to_string(),
                synchronizer: "manual".to_string(),
                credentials: None,
                balance_staleness: None,
            },
            state,
        }
    }

    #[test]
    fn test_balance_stale_when_old() {
        let connection = make_connection(Some(48)); // 48 hours old
        let threshold = Duration::from_secs(24 * 60 * 60); // 24 hours
        let check = check_balance_staleness(&connection, threshold);
        assert!(check.is_stale);
    }

    #[test]
    fn test_balance_fresh_when_recent() {
        let connection = make_connection(Some(12)); // 12 hours old
        let threshold = Duration::from_secs(24 * 60 * 60); // 24 hours
        let check = check_balance_staleness(&connection, threshold);
        assert!(!check.is_stale);
    }

    #[test]
    fn test_balance_stale_when_never_synced() {
        let connection = make_connection(None);
        let threshold = Duration::from_secs(24 * 60 * 60);
        let check = check_balance_staleness(&connection, threshold);
        assert!(check.is_stale);
        assert!(check.age.is_none());
    }

    #[test]
    fn test_resolve_account_override() {
        let account_config = AccountConfig {
            balance_staleness: Some(Duration::from_secs(7 * 24 * 60 * 60)),
        };
        let connection = make_connection(None);
        let global = RefreshConfig::default();

        let result = resolve_balance_staleness(Some(&account_config), &connection, &global);
        assert_eq!(result, Duration::from_secs(7 * 24 * 60 * 60));
    }

    #[test]
    fn test_resolve_connection_override() {
        let mut connection = make_connection(None);
        connection.config.balance_staleness = Some(Duration::from_secs(3 * 24 * 60 * 60));
        let global = RefreshConfig::default();

        let result = resolve_balance_staleness(None, &connection, &global);
        assert_eq!(result, Duration::from_secs(3 * 24 * 60 * 60));
    }

    #[test]
    fn test_resolve_global_default() {
        let connection = make_connection(None);
        let global = RefreshConfig::default();

        let result = resolve_balance_staleness(None, &connection, &global);
        assert_eq!(result, Duration::from_secs(14 * 24 * 60 * 60));
    }
}
```

**Step 2: Export from lib.rs**

Add to `src/lib.rs`:

```rust
pub mod staleness;
```

**Step 3: Run tests**

Run: `cargo test staleness`
Expected: All tests pass

**Step 4: Commit**

```bash
git add src/staleness.rs src/lib.rs
git commit -m "feat: add staleness detection module"
```

---

### Task 6: Update CLI Flags

**Files:**
- Modify: `src/main.rs`

**Step 1: Update PortfolioCommand enum**

Replace the current flags with the new ones:

```rust
#[derive(Subcommand)]
enum PortfolioCommand {
    /// Calculate portfolio snapshot with valuations
    Snapshot {
        /// Base currency for valuations (default: from config)
        #[arg(long)]
        currency: Option<String>,

        /// Calculate as of this date (YYYY-MM-DD, default: today)
        #[arg(long)]
        date: Option<String>,

        /// Output grouping: asset, account, or both
        #[arg(long, default_value = "both")]
        group_by: String,

        /// Include per-account breakdown when grouping by asset
        #[arg(long)]
        detail: bool,

        /// Auto-refresh stale data (default behavior, explicit flag for scripts)
        #[arg(long, conflicts_with_all = ["offline", "dry_run", "force_refresh"])]
        auto: bool,

        /// Use cached data only, no network requests
        #[arg(long, conflicts_with_all = ["auto", "dry_run", "force_refresh"])]
        offline: bool,

        /// Show what would be refreshed without actually refreshing
        #[arg(long, conflicts_with_all = ["auto", "offline", "force_refresh"])]
        dry_run: bool,

        /// Force refresh all data regardless of staleness
        #[arg(long, conflicts_with_all = ["auto", "offline", "dry_run"])]
        force_refresh: bool,
    },
}
```

**Step 2: Update SyncCommand with --if-stale**

```rust
#[derive(Subcommand)]
enum SyncCommand {
    /// Sync a specific connection by ID or name
    Connection {
        /// Connection ID or name
        id_or_name: String,
        /// Only sync if data is stale
        #[arg(long)]
        if_stale: bool,
    },
    /// Sync all connections
    All {
        /// Only sync connections with stale data
        #[arg(long)]
        if_stale: bool,
    },
}
```

**Step 3: Commit**

```bash
git add src/main.rs
git commit -m "feat: update CLI flags for staleness-aware refresh"
```

---

### Task 7: Implement Portfolio Snapshot with Staleness

**Files:**
- Modify: `src/main.rs`

**Step 1: Update the portfolio snapshot handler**

Update the match arm for `PortfolioCommand::Snapshot` to use the new flags and staleness checking:

```rust
PortfolioCommand::Snapshot {
    currency,
    date,
    group_by,
    detail,
    auto,
    offline,
    dry_run,
    force_refresh,
} => {
    use keepbook::portfolio::{Grouping, PortfolioQuery, PortfolioService};
    use keepbook::staleness::{
        check_balance_staleness, check_price_staleness, log_balance_staleness,
        log_price_staleness, resolve_balance_staleness,
    };

    // Parse date
    let as_of_date = match date {
        Some(d) => chrono::NaiveDate::parse_from_str(&d, "%Y-%m-%d")
            .with_context(|| format!("Invalid date format: {d}"))?,
        None => chrono::Utc::now().date_naive(),
    };

    // Parse grouping
    let grouping = match group_by.as_str() {
        "asset" => Grouping::Asset,
        "account" => Grouping::Account,
        "both" => Grouping::Both,
        _ => anyhow::bail!("Invalid grouping: {group_by}. Use: asset, account, both"),
    };

    // Determine refresh mode
    // Default is auto-refresh (if none of the flags are set)
    let do_balance_refresh = !offline && !dry_run;
    let do_price_refresh = !offline && !dry_run;
    let check_staleness = !force_refresh; // force_refresh ignores staleness

    // Build query
    let query = PortfolioQuery {
        as_of_date,
        currency: currency.unwrap_or_else(|| config.reporting_currency.clone()),
        grouping,
        include_detail: detail,
    };

    // Setup market data store
    let store = Arc::new(keepbook::market_data::JsonlMarketDataStore::new(
        &config.data_dir,
    ));

    // Check which connections need syncing
    let connections = storage.list_connections().await?;
    let mut connections_to_sync = Vec::new();

    for connection in &connections {
        let account_config = None; // Could load per-account config here
        let threshold = resolve_balance_staleness(
            account_config,
            connection,
            &config.refresh,
        );
        let check = check_balance_staleness(connection, threshold);

        if dry_run {
            log_balance_staleness(&connection.config.name, &check);
        }

        if do_balance_refresh && (!check_staleness || check.is_stale) {
            connections_to_sync.push(connection.clone());
        }
    }

    // Sync stale connections (if not dry_run or offline)
    if !dry_run && !offline && !connections_to_sync.is_empty() {
        for connection in &connections_to_sync {
            // Sync connection (reuse existing sync logic)
            let _ = sync_connection(&storage, &connection.id().to_string(), &config).await;
        }
    }

    // Configure price providers if refresh is enabled
    let market_data = if do_price_refresh && !dry_run {
        use keepbook::market_data::{CryptoPriceRouter, EquityPriceRouter, FxRateRouter};

        let mut registry = PriceSourceRegistry::new(&config.data_dir);
        registry.load()?;

        let equity_sources = registry.build_equity_sources().await?;
        let crypto_sources = registry.build_crypto_sources().await?;
        let fx_sources = registry.build_fx_sources().await?;

        let mut service = keepbook::market_data::MarketDataService::new(store, None);

        if !equity_sources.is_empty() {
            let equity_router = EquityPriceRouter::new(equity_sources);
            service = service.with_equity_router(Arc::new(equity_router));
        }

        if !crypto_sources.is_empty() {
            let crypto_router = CryptoPriceRouter::new(crypto_sources);
            service = service.with_crypto_router(Arc::new(crypto_router));
        }

        if !fx_sources.is_empty() {
            let fx_router = FxRateRouter::new(fx_sources);
            service = service.with_fx_router(Arc::new(fx_router));
        }

        Arc::new(service)
    } else {
        Arc::new(keepbook::market_data::MarketDataService::new(store, None))
    };

    let storage_arc: Arc<dyn keepbook::storage::Storage> = Arc::new(storage);
    let service = PortfolioService::new(storage_arc, market_data);

    // Calculate and output
    let snapshot = service
        .calculate_with_staleness(&query, &config.refresh, dry_run, force_refresh)
        .await?;
    println!("{}", serde_json::to_string_pretty(&snapshot)?);
}
```

**Step 2: Commit**

```bash
git add src/main.rs
git commit -m "feat: implement staleness-aware portfolio snapshot"
```

---

### Task 8: Update PortfolioService for Staleness

**Files:**
- Modify: `src/portfolio/service.rs`
- Modify: `src/portfolio/models.rs`

**Step 1: Update RefreshPolicy in models.rs**

Replace the existing `RefreshPolicy`:

```rust
#[derive(Debug, Clone)]
pub struct RefreshPolicy {
    pub balance_staleness: std::time::Duration,
    pub price_staleness: std::time::Duration,
}

impl Default for RefreshPolicy {
    fn default() -> Self {
        Self {
            balance_staleness: std::time::Duration::from_secs(14 * 24 * 60 * 60),
            price_staleness: std::time::Duration::from_secs(24 * 60 * 60),
        }
    }
}
```

Remove `RefreshMode` enum (no longer needed).

**Step 2: Add staleness-aware calculate method to service**

Add to `PortfolioService`:

```rust
use crate::config::RefreshConfig;
use crate::staleness::{check_price_staleness, log_price_staleness};

/// Calculate portfolio with staleness checking.
pub async fn calculate_with_staleness(
    &self,
    query: &PortfolioQuery,
    refresh_config: &RefreshConfig,
    dry_run: bool,
    force_refresh: bool,
) -> Result<PortfolioSnapshot> {
    // For now, delegate to calculate()
    // Price staleness checking happens in MarketDataService
    self.calculate(query, refresh_config, dry_run, force_refresh).await
}
```

Update the `calculate` method signature and implementation to accept the new parameters and check price staleness before fetching.

**Step 3: Run tests**

Run: `cargo test portfolio`
Expected: All tests pass

**Step 4: Commit**

```bash
git add src/portfolio/service.rs src/portfolio/models.rs
git commit -m "feat: update PortfolioService for staleness-aware refresh"
```

---

### Task 9: Update Sync Command with --if-stale

**Files:**
- Modify: `src/main.rs`

**Step 1: Update sync_connection to check staleness**

Update the `SyncCommand::Connection` handler:

```rust
SyncCommand::Connection { id_or_name, if_stale } => {
    let connection = find_connection(&storage, &id_or_name)
        .await?
        .context(format!("Connection not found: {}", id_or_name))?;

    if if_stale {
        use keepbook::staleness::{check_balance_staleness, resolve_balance_staleness};
        let threshold = resolve_balance_staleness(None, &connection, &config.refresh);
        let check = check_balance_staleness(&connection, threshold);
        if !check.is_stale {
            return Ok(serde_json::json!({
                "success": true,
                "skipped": true,
                "reason": "not stale",
                "connection": connection.config.name
            }));
        }
    }

    let result = sync_connection(&storage, &id_or_name, &config).await?;
    println!("{}", serde_json::to_string_pretty(&result)?);
}
```

**Step 2: Update sync_all similarly**

**Step 3: Commit**

```bash
git add src/main.rs
git commit -m "feat: add --if-stale flag to sync command"
```

---

### Task 10: Integration Testing

**Files:**
- Create: `tests/staleness_integration.rs`

**Step 1: Write integration test**

```rust
//! Integration tests for staleness configuration.

use std::time::Duration;

#[test]
fn test_parse_config_with_refresh() {
    let toml = r#"
data_dir = "data"

[refresh]
balance_staleness = "7d"
price_staleness = "1h"
"#;

    let config: keepbook::config::Config = toml::from_str(toml).unwrap();
    assert_eq!(config.refresh.balance_staleness, Duration::from_secs(7 * 24 * 60 * 60));
    assert_eq!(config.refresh.price_staleness, Duration::from_secs(60 * 60));
}

#[test]
fn test_connection_config_with_staleness() {
    let toml = r#"
name = "Test"
synchronizer = "manual"
balance_staleness = "3d"
"#;

    let config: keepbook::models::ConnectionConfig = toml::from_str(toml).unwrap();
    assert_eq!(config.balance_staleness, Some(Duration::from_secs(3 * 24 * 60 * 60)));
}
```

**Step 2: Run all tests**

Run: `cargo test`
Expected: All tests pass

**Step 3: Commit**

```bash
git add tests/staleness_integration.rs
git commit -m "test: add staleness configuration integration tests"
```

---

## Summary

After completing all tasks, you will have:

1. **Duration module** - Parsing and serde support for human-readable durations
2. **RefreshConfig** - Global staleness settings in keepbook.toml
3. **ConnectionConfig.balance_staleness** - Per-connection override
4. **AccountConfig.balance_staleness** - Per-account override
5. **Staleness module** - Resolution and checking logic
6. **Updated CLI** - `--auto`, `--offline`, `--dry-run`, `--force-refresh` flags
7. **Staleness-aware portfolio** - Auto-refreshes only stale data by default
8. **Staleness-aware sync** - `--if-stale` flag for conditional sync
