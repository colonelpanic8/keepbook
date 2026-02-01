# Sync + Price Integration Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Integrate price fetching with account synchronization so synchronizers can provide prices alongside balances, and missing prices are automatically fetched from external sources.

**Architecture:** New `SyncOrchestrator` coordinates sync + price fetching. `SyncResult` uses `SyncedBalance` wrapper pairing balances with optional prices. `AssetId` changes to human-readable path format. FX rates are fetched for currency conversion to a global `reporting_currency`.

**Tech Stack:** Rust, async-trait, chrono, serde

---

### Task 1: Remove Asset::Other Variant

**Files:**
- Modify: `src/models/asset.rs`
- Modify: `src/market_data/asset_id.rs` (remove Other handling)

**Step 1: Remove Other variant from Asset enum**

In `src/models/asset.rs`, remove the `Other` variant:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Asset {
    Currency {
        iso_code: String,
    },
    Equity {
        ticker: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        exchange: Option<String>,
    },
    Crypto {
        symbol: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        network: Option<String>,
    },
}
```

**Step 2: Remove Other handling from asset_id.rs**

In `src/market_data/asset_id.rs`, remove the `Asset::Other` match arm from `canonical_asset_json` function (lines 92-107).

**Step 3: Run tests to verify nothing breaks**

Run: `cargo test`
Expected: All tests pass

**Step 4: Commit**

```bash
git add src/models/asset.rs src/market_data/asset_id.rs
git commit -m "refactor: remove Asset::Other variant

New asset types should extend the enum rather than use a catch-all."
```

---

### Task 2: Change AssetId to Human-Readable Path Format

**Files:**
- Modify: `src/market_data/asset_id.rs`

**Step 1: Write test for new AssetId format**

Add to the existing tests in `src/market_data/asset_id.rs`:

```rust
#[test]
fn asset_id_is_human_readable_currency() {
    let asset = Asset::currency("USD");
    let id = AssetId::from_asset(&asset);
    assert_eq!(id.as_str(), "currency/USD");
}

#[test]
fn asset_id_is_human_readable_equity() {
    let asset = Asset::equity("AAPL");
    let id = AssetId::from_asset(&asset);
    assert_eq!(id.as_str(), "equity/AAPL");
}

#[test]
fn asset_id_is_human_readable_equity_with_exchange() {
    let asset = Asset::Equity {
        ticker: "AAPL".to_string(),
        exchange: Some("NYSE".to_string()),
    };
    let id = AssetId::from_asset(&asset);
    assert_eq!(id.as_str(), "equity/AAPL/NYSE");
}

#[test]
fn asset_id_is_human_readable_crypto() {
    let asset = Asset::crypto("BTC");
    let id = AssetId::from_asset(&asset);
    assert_eq!(id.as_str(), "crypto/BTC");
}

#[test]
fn asset_id_is_human_readable_crypto_with_network() {
    let asset = Asset::Crypto {
        symbol: "ETH".to_string(),
        network: Some("arbitrum".to_string()),
    };
    let id = AssetId::from_asset(&asset);
    assert_eq!(id.as_str(), "crypto/ETH/arbitrum");
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test asset_id_is_human_readable`
Expected: FAIL - assertions don't match hash format

**Step 3: Rewrite AssetId::from_asset**

Replace the entire `from_asset` implementation and remove the helper functions:

```rust
impl AssetId {
    pub fn from_asset(asset: &Asset) -> Self {
        let id = match asset {
            Asset::Currency { iso_code } => {
                format!("currency/{}", iso_code.trim().to_uppercase())
            }
            Asset::Equity { ticker, exchange: None } => {
                format!("equity/{}", ticker.trim().to_uppercase())
            }
            Asset::Equity { ticker, exchange: Some(ex) } => {
                format!("equity/{}/{}", ticker.trim().to_uppercase(), ex.trim().to_uppercase())
            }
            Asset::Crypto { symbol, network: None } => {
                format!("crypto/{}", symbol.trim().to_uppercase())
            }
            Asset::Crypto { symbol, network: Some(net) } => {
                format!("crypto/{}/{}", symbol.trim().to_uppercase(), net.trim().to_lowercase())
            }
        };
        Self(id)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}
```

Remove these functions (no longer needed):
- `canonical_asset_json`
- `normalize_upper`

Remove these imports from the top of the file:
- `base64::Engine`
- `sha2::{Digest, Sha256}`

**Step 4: Update existing tests**

The existing tests in the file need updating. Replace:

```rust
#[test]
fn asset_id_is_deterministic() {
    let asset = Asset::equity("AAPL");
    let first = AssetId::from_asset(&asset);
    let second = AssetId::from_asset(&asset);
    assert_eq!(first, second);
}

#[test]
fn asset_id_differs_for_distinct_assets() {
    let aapl = Asset::equity("AAPL");
    let msft = Asset::equity("MSFT");
    let aapl_id = AssetId::from_asset(&aapl);
    let msft_id = AssetId::from_asset(&msft);
    assert_ne!(aapl_id, msft_id);
}

#[test]
fn canonicalization_normalizes_case() {
    let asset = Asset::Currency {
        iso_code: "usd".to_string(),
    };
    let id_lower = AssetId::from_asset(&asset);
    let asset_upper = Asset::Currency {
        iso_code: "USD".to_string(),
    };
    let id_upper = AssetId::from_asset(&asset_upper);
    assert_eq!(id_lower, id_upper);
    assert_eq!(id_lower.as_str(), "currency/USD");
}
```

**Step 5: Run tests to verify they pass**

Run: `cargo test asset_id`
Expected: All tests pass

**Step 6: Commit**

```bash
git add src/market_data/asset_id.rs
git commit -m "refactor: change AssetId to human-readable path format

Format: {type}/{primary}[/{qualifier}]
Examples: equity/AAPL, crypto/BTC, currency/USD

This makes IDs debuggable and directly usable as storage paths."
```

---

### Task 3: Add reporting_currency to Config

**Files:**
- Modify: `src/config.rs`

**Step 1: Read current config structure**

First examine `src/config.rs` to understand the current structure.

**Step 2: Add reporting_currency field**

Add to the main config struct:

```rust
/// Currency for reporting all values (e.g., "USD")
#[serde(default = "default_reporting_currency")]
pub reporting_currency: String,
```

Add the default function:

```rust
fn default_reporting_currency() -> String {
    "USD".to_string()
}
```

**Step 3: Run tests to verify compilation**

Run: `cargo build`
Expected: Compiles successfully

**Step 4: Commit**

```bash
git add src/config.rs
git commit -m "feat: add reporting_currency to config

Defaults to USD. Used for converting all asset values to a single currency."
```

---

### Task 4: Add SyncedBalance Wrapper

**Files:**
- Modify: `src/sync/mod.rs`

**Step 1: Add SyncedBalance struct**

Add after the existing imports:

```rust
use crate::market_data::PricePoint;

/// A balance paired with optional price data from the synchronizer.
#[derive(Debug, Clone)]
pub struct SyncedBalance {
    pub balance: Balance,
    pub price: Option<PricePoint>,
}

impl SyncedBalance {
    pub fn new(balance: Balance) -> Self {
        Self { balance, price: None }
    }

    pub fn with_price(mut self, price: PricePoint) -> Self {
        self.price = Some(price);
        self
    }
}
```

**Step 2: Update SyncResult to use SyncedBalance**

Change the `balances` field type:

```rust
pub struct SyncResult {
    pub connection: Connection,
    pub accounts: Vec<Account>,
    pub balances: Vec<(Id, Vec<SyncedBalance>)>,
    pub transactions: Vec<(Id, Vec<Transaction>)>,
}
```

**Step 3: Update SyncResult::save to handle SyncedBalance**

Update the save method to extract balances from SyncedBalance:

```rust
impl SyncResult {
    /// Save this sync result to storage.
    pub async fn save(&self, storage: &impl Storage) -> Result<()> {
        storage.save_connection(&self.connection).await?;

        for account in &self.accounts {
            storage.save_account(account).await?;
        }

        for (account_id, synced_balances) in &self.balances {
            let balances: Vec<Balance> = synced_balances
                .iter()
                .map(|sb| sb.balance.clone())
                .collect();
            if !balances.is_empty() {
                storage.append_balances(account_id, &balances).await?;
            }
        }

        for (account_id, txns) in &self.transactions {
            if !txns.is_empty() {
                storage.append_transactions(account_id, txns).await?;
            }
        }

        Ok(())
    }
}
```

**Step 4: Export SyncedBalance**

Update the module exports if needed.

**Step 5: Run cargo build to check for compilation errors**

Run: `cargo build`
Expected: Compilation errors in examples/coinbase.rs, examples/plaid.rs, examples/schwab.rs

**Step 6: Commit work in progress**

```bash
git add src/sync/mod.rs
git commit -m "feat: add SyncedBalance wrapper for balance + optional price

WIP: Examples need updating to use new type."
```

---

### Task 5: Update Coinbase Example for SyncedBalance

**Files:**
- Modify: `examples/coinbase.rs`

**Step 1: Update balance creation to use SyncedBalance**

Find where balances are created and wrap them:

```rust
use keepbook::sync::SyncedBalance;

// Change from:
// balances.push((account_id.clone(), vec![balance]));

// To:
balances.push((account_id.clone(), vec![SyncedBalance::new(balance)]));
```

**Step 2: Run cargo build to verify**

Run: `cargo build --example coinbase`
Expected: Compiles successfully

**Step 3: Commit**

```bash
git add examples/coinbase.rs
git commit -m "fix: update coinbase example for SyncedBalance"
```

---

### Task 6: Update Plaid Example for SyncedBalance

**Files:**
- Modify: `examples/plaid.rs`

**Step 1: Update balance creation to use SyncedBalance**

Same pattern as coinbase:

```rust
use keepbook::sync::SyncedBalance;

// Wrap Balance in SyncedBalance::new()
```

**Step 2: Run cargo build to verify**

Run: `cargo build --example plaid`
Expected: Compiles successfully

**Step 3: Commit**

```bash
git add examples/plaid.rs
git commit -m "fix: update plaid example for SyncedBalance"
```

---

### Task 7: Update Schwab Example for SyncedBalance with Prices

**Files:**
- Modify: `examples/schwab.rs`

**Step 1: Import required types**

Add imports:

```rust
use keepbook::market_data::{AssetId, PriceKind, PricePoint};
use keepbook::sync::SyncedBalance;
```

**Step 2: Update balance creation to include price from Position**

When creating balances from positions, extract the price:

```rust
// For each position, create SyncedBalance with price
let asset = Asset::equity(&position.default_symbol);
let balance = Balance::new(asset.clone(), position.quantity.to_string());

let price_point = PricePoint {
    asset_id: AssetId::from_asset(&asset),
    as_of_date: chrono::Utc::now().date_naive(),
    timestamp: chrono::Utc::now(),
    price: position.price.to_string(),
    quote_currency: "USD".to_string(),
    kind: PriceKind::Close,
    source: "schwab".to_string(),
};

let synced_balance = SyncedBalance::new(balance).with_price(price_point);
```

**Step 3: Run cargo build to verify**

Run: `cargo build --example schwab`
Expected: Compiles successfully

**Step 4: Commit**

```bash
git add examples/schwab.rs
git commit -m "feat: schwab example extracts prices into SyncedBalance

Prices from Schwab positions are now captured alongside balances."
```

---

### Task 8: Add get_latest_balances to Storage Trait

**Files:**
- Modify: `src/storage/mod.rs`

**Step 1: Add method signatures to Storage trait**

Add these methods to the `Storage` trait:

```rust
/// Get the most recent balance for each (account, asset) pair across all accounts.
async fn get_latest_balances(&self) -> Result<Vec<(Id, Balance)>>;

/// Get the most recent balance for each asset in accounts belonging to a connection.
async fn get_latest_balances_for_connection(&self, connection_id: &Id) -> Result<Vec<(Id, Balance)>>;

/// Get the most recent balance for each asset in a specific account.
async fn get_latest_balances_for_account(&self, account_id: &Id) -> Result<Vec<Balance>>;
```

**Step 2: Run cargo build to see what needs implementing**

Run: `cargo build`
Expected: Compilation errors - JsonFileStorage needs to implement new methods

**Step 3: Commit trait changes**

```bash
git add src/storage/mod.rs
git commit -m "feat: add get_latest_balances methods to Storage trait

WIP: JsonFileStorage implementation needed."
```

---

### Task 9: Implement get_latest_balances in JsonFileStorage

**Files:**
- Modify: `src/storage/json_file.rs`

**Step 1: Implement get_latest_balances_for_account**

```rust
async fn get_latest_balances_for_account(&self, account_id: &Id) -> Result<Vec<Balance>> {
    let balances = self.get_balances(account_id).await?;

    // Group by asset, keep most recent
    let mut latest: std::collections::HashMap<Asset, Balance> = std::collections::HashMap::new();
    for balance in balances {
        latest
            .entry(balance.asset.clone())
            .and_modify(|existing| {
                if balance.timestamp > existing.timestamp {
                    *existing = balance.clone();
                }
            })
            .or_insert(balance);
    }

    Ok(latest.into_values().collect())
}
```

**Step 2: Implement get_latest_balances_for_connection**

```rust
async fn get_latest_balances_for_connection(&self, connection_id: &Id) -> Result<Vec<(Id, Balance)>> {
    let connection = self.get_connection(connection_id).await?
        .ok_or_else(|| anyhow::anyhow!("Connection not found"))?;

    let mut results = Vec::new();
    for account_id in &connection.state.account_ids {
        let balances = self.get_latest_balances_for_account(account_id).await?;
        for balance in balances {
            results.push((account_id.clone(), balance));
        }
    }

    Ok(results)
}
```

**Step 3: Implement get_latest_balances (all)**

```rust
async fn get_latest_balances(&self) -> Result<Vec<(Id, Balance)>> {
    let connections = self.list_connections().await?;

    let mut results = Vec::new();
    for connection in connections {
        let connection_balances = self.get_latest_balances_for_connection(connection.id()).await?;
        results.extend(connection_balances);
    }

    Ok(results)
}
```

**Step 4: Run cargo build to verify**

Run: `cargo build`
Expected: Compiles successfully

**Step 5: Commit**

```bash
git add src/storage/json_file.rs
git commit -m "feat: implement get_latest_balances for JsonFileStorage"
```

---

### Task 10: Create SyncOrchestrator Module Structure

**Files:**
- Create: `src/sync/orchestrator.rs`
- Modify: `src/sync/mod.rs`

**Step 1: Create orchestrator.rs with basic structure**

```rust
//! Orchestrates sync operations with automatic price fetching.

use std::collections::HashSet;
use std::sync::Arc;

use anyhow::Result;
use chrono::NaiveDate;

use crate::market_data::MarketDataService;
use crate::models::{Asset, Id};
use crate::storage::Storage;

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
```

**Step 2: Add module to sync/mod.rs**

Add to `src/sync/mod.rs`:

```rust
mod orchestrator;
pub use orchestrator::{SyncOrchestrator, PriceRefreshResult};
```

**Step 3: Run cargo build to verify**

Run: `cargo build`
Expected: Compiles successfully

**Step 4: Commit**

```bash
git add src/sync/orchestrator.rs src/sync/mod.rs
git commit -m "feat: add SyncOrchestrator module structure"
```

---

### Task 11: Add ensure_prices Method to SyncOrchestrator

**Files:**
- Modify: `src/sync/orchestrator.rs`

**Step 1: Add ensure_prices internal method**

```rust
use crate::market_data::{AssetId, PricePoint};
use crate::models::Balance;

impl<S: Storage> SyncOrchestrator<S> {
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
                    let asset_id = AssetId::from_asset(asset);

                    // Check if we already have a price
                    if !force {
                        if let Some(existing) = self.market_data.get_price(&asset_id, date).await? {
                            result.skipped += 1;
                            // Check if we need FX conversion
                            if existing.quote_currency.to_uppercase() != self.reporting_currency.to_uppercase() {
                                needed_fx_pairs.insert((
                                    existing.quote_currency.to_uppercase(),
                                    self.reporting_currency.to_uppercase(),
                                ));
                            }
                            continue;
                        }
                    }

                    // Fetch price from market data service
                    match self.market_data.fetch_price(asset, &asset_id, date).await {
                        Ok(Some(price)) => {
                            // Check if we need FX conversion
                            if price.quote_currency.to_uppercase() != self.reporting_currency.to_uppercase() {
                                needed_fx_pairs.insert((
                                    price.quote_currency.to_uppercase(),
                                    self.reporting_currency.to_uppercase(),
                                ));
                            }
                            self.market_data.store_price(&price).await?;
                            result.fetched += 1;
                        }
                        Ok(None) => {
                            result.failed.push((asset.clone(), "No price available".to_string()));
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
            if !force {
                if self.market_data.get_fx_rate(&base, &quote, date).await?.is_some() {
                    result.skipped += 1;
                    continue;
                }
            }

            match self.market_data.fetch_fx_rate(&base, &quote, date).await {
                Ok(Some(rate)) => {
                    self.market_data.store_fx_rate(&rate).await?;
                    result.fetched += 1;
                }
                Ok(None) => {
                    // FX rate failures are less critical, just log
                }
                Err(_) => {
                    // FX rate failures are less critical
                }
            }
        }

        Ok(result)
    }
}
```

**Step 2: Run cargo build to check**

Run: `cargo build`
Expected: May have errors if MarketDataService doesn't have all methods - note what's missing

**Step 3: Commit what we have**

```bash
git add src/sync/orchestrator.rs
git commit -m "feat: add ensure_prices method to SyncOrchestrator

Fetches missing prices and FX rates, respects force flag."
```

---

### Task 12: Add Price Refresh Methods to SyncOrchestrator

**Files:**
- Modify: `src/sync/orchestrator.rs`

**Step 1: Add refresh methods**

```rust
impl<S: Storage + Send + Sync> SyncOrchestrator<S> {
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
}
```

**Step 2: Run cargo build**

Run: `cargo build`
Expected: Compiles (or note missing pieces)

**Step 3: Commit**

```bash
git add src/sync/orchestrator.rs
git commit -m "feat: add price refresh methods to SyncOrchestrator

- refresh_all_prices: all accounts
- refresh_connection_prices: single connection
- refresh_account_prices: single account"
```

---

### Task 13: Add sync_with_prices Method

**Files:**
- Modify: `src/sync/orchestrator.rs`

**Step 1: Add the sync_with_prices method**

```rust
use super::{SyncResult, SyncedBalance, Synchronizer};

impl<S: Storage + Send + Sync> SyncOrchestrator<S> {
    /// Run sync and fetch any missing prices.
    pub async fn sync_with_prices(
        &self,
        synchronizer: &dyn Synchronizer,
        connection: &mut crate::models::Connection,
        force_refresh: bool,
    ) -> Result<SyncResult> {
        // 1. Run the sync
        let result = synchronizer.sync(connection).await?;

        // 2. Save sync results (this stores balances)
        result.save(self.storage.as_ref()).await?;

        // 3. Store any prices the synchronizer provided
        for (_, synced_balances) in &result.balances {
            for sb in synced_balances {
                if let Some(price) = &sb.price {
                    self.market_data.store_price(price).await?;
                }
            }
        }

        // 4. Collect assets that need prices
        let assets: HashSet<Asset> = result.balances
            .iter()
            .flat_map(|(_, sbs)| sbs.iter().map(|sb| sb.balance.asset.clone()))
            .collect();

        // 5. Fetch missing prices
        let date = chrono::Utc::now().date_naive();
        self.ensure_prices(&assets, date, force_refresh).await?;

        Ok(result)
    }
}
```

**Step 2: Run cargo build**

Run: `cargo build`
Expected: Compiles

**Step 3: Commit**

```bash
git add src/sync/orchestrator.rs
git commit -m "feat: add sync_with_prices to SyncOrchestrator

Runs sync, stores inline prices, then fetches any missing prices."
```

---

### Task 14: Add Missing MarketDataService Methods (if needed)

**Files:**
- Modify: `src/market_data/service.rs`

**Step 1: Check what methods are missing**

Run `cargo build` and note any missing methods on `MarketDataService`. Common ones needed:
- `get_price(asset_id, date)` - check store for existing price
- `get_fx_rate(base, quote, date)` - check store for existing rate
- `store_price(price)` - save to store
- `store_fx_rate(rate)` - save to store
- `fetch_price(asset, asset_id, date)` - fetch from sources
- `fetch_fx_rate(base, quote, date)` - fetch from sources

**Step 2: Implement missing methods**

This will depend on the current state of `MarketDataService`. Add methods as needed following the existing patterns in the file.

**Step 3: Run cargo build**

Run: `cargo build`
Expected: Compiles

**Step 4: Commit**

```bash
git add src/market_data/service.rs
git commit -m "feat: add missing methods to MarketDataService for orchestrator"
```

---

### Task 15: Export New Types from lib.rs

**Files:**
- Modify: `src/lib.rs`

**Step 1: Ensure new types are exported**

Check `src/lib.rs` and ensure these are publicly accessible:
- `sync::SyncOrchestrator`
- `sync::SyncedBalance`
- `sync::PriceRefreshResult`

**Step 2: Run cargo build**

Run: `cargo build`
Expected: Compiles

**Step 3: Commit**

```bash
git add src/lib.rs
git commit -m "chore: export new sync types from lib.rs"
```

---

### Task 16: Final Integration Test

**Files:**
- All

**Step 1: Run full test suite**

Run: `cargo test`
Expected: All tests pass

**Step 2: Run all examples to verify they compile**

Run: `cargo build --examples`
Expected: All examples compile

**Step 3: Final commit if any changes needed**

```bash
git status
# If any uncommitted changes, commit them
```

---

### Task 17: Merge to Main Branch

**Step 1: Review all changes**

```bash
git log --oneline master..HEAD
```

**Step 2: Merge or create PR**

If working locally:
```bash
git checkout master
git merge feature/sync-price-integration
```

Or create PR if using GitHub workflow.
