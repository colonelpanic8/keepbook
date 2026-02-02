# Portfolio Command Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add a `keepbook portfolio` command that aggregates all assets across accounts, converts to a base currency, and outputs JSON with configurable grouping and refresh policies.

**Architecture:** New `src/portfolio/` module with `PortfolioService` that replaces the existing `NetWorthCalculator`. CLI is a thin wrapper that parses args, calls the service, and serializes output.

**Tech Stack:** Rust, clap (CLI), serde (JSON), rust_decimal (math), chrono (dates), tokio (async)

**Design Document:** `docs/plans/2026-02-02-portfolio-command-design.md`

---

## Task 1: Create Portfolio Module Structure

**Files:**
- Create: `src/portfolio/mod.rs`
- Create: `src/portfolio/models.rs`
- Modify: `src/lib.rs:1-10`

**Step 1: Create models.rs with output structs**

```rust
// src/portfolio/models.rs
use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::models::{Asset, Id};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Grouping {
    Asset,
    Account,
    Both,
}

impl Default for Grouping {
    fn default() -> Self {
        Self::Both
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RefreshMode {
    CachedOnly,
    IfStale,
    Force,
}

impl Default for RefreshMode {
    fn default() -> Self {
        Self::CachedOnly
    }
}

#[derive(Debug, Clone)]
pub struct PortfolioQuery {
    pub as_of_date: NaiveDate,
    pub currency: String,
    pub grouping: Grouping,
    pub include_detail: bool,
}

#[derive(Debug, Clone)]
pub struct RefreshPolicy {
    pub mode: RefreshMode,
    pub stale_threshold: std::time::Duration,
}

impl Default for RefreshPolicy {
    fn default() -> Self {
        Self {
            mode: RefreshMode::CachedOnly,
            stale_threshold: std::time::Duration::from_secs(24 * 60 * 60), // 1 day
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortfolioSnapshot {
    pub as_of_date: NaiveDate,
    pub currency: String,
    pub total_value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub by_asset: Option<Vec<AssetSummary>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub by_account: Option<Vec<AccountSummary>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetSummary {
    pub asset: Asset,
    pub total_amount: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price_date: Option<NaiveDate>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fx_rate: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fx_date: Option<NaiveDate>,
    pub value_in_base: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub holdings: Option<Vec<AccountHolding>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountHolding {
    pub account_id: String,
    pub account_name: String,
    pub amount: String,
    pub balance_date: NaiveDate,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountSummary {
    pub account_id: String,
    pub account_name: String,
    pub connection_name: String,
    pub value_in_base: String,
}
```

**Step 2: Create mod.rs to export**

```rust
// src/portfolio/mod.rs
mod models;
mod service;

pub use models::*;
pub use service::*;
```

**Step 3: Create stub service.rs**

```rust
// src/portfolio/service.rs
use anyhow::Result;
use std::sync::Arc;

use crate::market_data::MarketDataService;
use crate::storage::Storage;

use super::{PortfolioQuery, PortfolioSnapshot, RefreshPolicy};

pub struct PortfolioService {
    storage: Arc<dyn Storage>,
    market_data: Arc<MarketDataService>,
}

impl PortfolioService {
    pub fn new(storage: Arc<dyn Storage>, market_data: Arc<MarketDataService>) -> Self {
        Self { storage, market_data }
    }

    pub async fn calculate(
        &self,
        _query: &PortfolioQuery,
        _refresh: &RefreshPolicy,
    ) -> Result<PortfolioSnapshot> {
        todo!("Implement in Task 2")
    }
}
```

**Step 4: Add module export to lib.rs**

In `src/lib.rs`, add after other module declarations:

```rust
pub mod portfolio;
```

**Step 5: Verify compilation**

Run: `cargo check`
Expected: Compiles with no errors (may have warnings about unused code)

**Step 6: Commit**

```bash
git add src/portfolio/ src/lib.rs
git commit -m "feat(portfolio): add module structure with models

Add PortfolioService stub and output models:
- PortfolioSnapshot, AssetSummary, AccountSummary
- PortfolioQuery, RefreshPolicy, Grouping, RefreshMode

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>"
```

---

## Task 2: Implement PortfolioService Core Logic

**Files:**
- Modify: `src/portfolio/service.rs`
- Test: `src/portfolio/service.rs` (inline tests)

**Step 1: Write failing test for basic calculation**

Add to bottom of `src/portfolio/service.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::market_data::{
        AssetId, FxRateKind, FxRatePoint, MarketDataService, MemoryMarketDataStore,
        PriceKind, PricePoint,
    };
    use crate::models::{Account, Asset, Balance, Connection, Id};
    use crate::storage::MemoryStorage;
    use chrono::{TimeZone, Utc};

    #[tokio::test]
    async fn calculate_single_currency_holding() -> Result<()> {
        // Setup storage with one account holding USD
        let storage = Arc::new(MemoryStorage::new());
        let connection = Connection::new("Test Bank");
        storage.save_connection(&connection).await?;

        let account = Account::new("Checking", connection.id.clone());
        storage.save_account(&account).await?;

        let balance = Balance::new(Asset::currency("USD"), "1000.00")
            .with_timestamp(Utc.with_ymd_and_hms(2026, 2, 1, 12, 0, 0).unwrap());
        storage.save_balance(&account.id, &balance).await?;

        // Setup market data (no prices needed for USD->USD)
        let store = Arc::new(MemoryMarketDataStore::new());
        let market_data = Arc::new(MarketDataService::new(store, None));

        // Calculate
        let service = PortfolioService::new(storage, market_data);
        let query = PortfolioQuery {
            as_of_date: chrono::NaiveDate::from_ymd_opt(2026, 2, 2).unwrap(),
            currency: "USD".to_string(),
            grouping: Grouping::Both,
            include_detail: false,
        };
        let result = service.calculate(&query, &RefreshPolicy::default()).await?;

        assert_eq!(result.total_value, "1000.00");
        assert_eq!(result.currency, "USD");
        Ok(())
    }
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p keepbook --lib portfolio::service::tests::calculate_single_currency_holding`
Expected: FAIL (todo! panic or MemoryStorage not found)

**Step 3: Create MemoryStorage for testing**

If `MemoryStorage` doesn't exist, add to `src/storage/mod.rs`:

```rust
#[cfg(test)]
mod memory;
#[cfg(test)]
pub use memory::MemoryStorage;
```

Create `src/storage/memory.rs`:

```rust
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::RwLock;

use super::Storage;
use crate::models::{Account, Balance, Connection, Id, Transaction};
use anyhow::Result;

pub struct MemoryStorage {
    connections: RwLock<HashMap<Id, Connection>>,
    accounts: RwLock<HashMap<Id, Account>>,
    balances: RwLock<HashMap<Id, Vec<Balance>>>,
    transactions: RwLock<HashMap<Id, Vec<Transaction>>>,
}

impl MemoryStorage {
    pub fn new() -> Self {
        Self {
            connections: RwLock::new(HashMap::new()),
            accounts: RwLock::new(HashMap::new()),
            balances: RwLock::new(HashMap::new()),
            transactions: RwLock::new(HashMap::new()),
        }
    }

    pub async fn save_connection(&self, connection: &Connection) -> Result<()> {
        self.connections
            .write()
            .unwrap()
            .insert(connection.id.clone(), connection.clone());
        Ok(())
    }

    pub async fn save_account(&self, account: &Account) -> Result<()> {
        self.accounts
            .write()
            .unwrap()
            .insert(account.id.clone(), account.clone());
        Ok(())
    }

    pub async fn save_balance(&self, account_id: &Id, balance: &Balance) -> Result<()> {
        self.balances
            .write()
            .unwrap()
            .entry(account_id.clone())
            .or_default()
            .push(balance.clone());
        Ok(())
    }
}

#[async_trait]
impl Storage for MemoryStorage {
    async fn list_connections(&self) -> Result<Vec<Connection>> {
        Ok(self.connections.read().unwrap().values().cloned().collect())
    }

    async fn get_connection(&self, id: &Id) -> Result<Option<Connection>> {
        Ok(self.connections.read().unwrap().get(id).cloned())
    }

    async fn save_connection(&self, connection: &Connection) -> Result<()> {
        self.connections
            .write()
            .unwrap()
            .insert(connection.id.clone(), connection.clone());
        Ok(())
    }

    async fn remove_connection(&self, id: &Id) -> Result<()> {
        self.connections.write().unwrap().remove(id);
        Ok(())
    }

    async fn list_accounts(&self) -> Result<Vec<Account>> {
        Ok(self.accounts.read().unwrap().values().cloned().collect())
    }

    async fn get_account(&self, id: &Id) -> Result<Option<Account>> {
        Ok(self.accounts.read().unwrap().get(id).cloned())
    }

    async fn save_account(&self, account: &Account) -> Result<()> {
        self.accounts
            .write()
            .unwrap()
            .insert(account.id.clone(), account.clone());
        Ok(())
    }

    async fn remove_account(&self, id: &Id) -> Result<()> {
        self.accounts.write().unwrap().remove(id);
        self.balances.write().unwrap().remove(id);
        self.transactions.write().unwrap().remove(id);
        Ok(())
    }

    async fn get_balances(&self, account_id: &Id) -> Result<Vec<Balance>> {
        Ok(self
            .balances
            .read()
            .unwrap()
            .get(account_id)
            .cloned()
            .unwrap_or_default())
    }

    async fn save_balance(&self, account_id: &Id, balance: &Balance) -> Result<()> {
        self.balances
            .write()
            .unwrap()
            .entry(account_id.clone())
            .or_default()
            .push(balance.clone());
        Ok(())
    }

    async fn get_latest_balances(&self) -> Result<Vec<(Id, Balance)>> {
        let balances = self.balances.read().unwrap();
        let mut result = Vec::new();
        for (account_id, account_balances) in balances.iter() {
            // Group by asset and get latest for each
            let mut by_asset: HashMap<String, &Balance> = HashMap::new();
            for balance in account_balances {
                let key = serde_json::to_string(&balance.asset).unwrap_or_default();
                match by_asset.get(&key) {
                    Some(existing) if existing.timestamp >= balance.timestamp => {}
                    _ => {
                        by_asset.insert(key, balance);
                    }
                }
            }
            for balance in by_asset.into_values() {
                result.push((account_id.clone(), balance.clone()));
            }
        }
        Ok(result)
    }

    async fn get_latest_balances_for_connection(&self, connection_id: &Id) -> Result<Vec<(Id, Balance)>> {
        let accounts = self.accounts.read().unwrap();
        let account_ids: Vec<Id> = accounts
            .values()
            .filter(|a| &a.connection_id == connection_id)
            .map(|a| a.id.clone())
            .collect();

        let all = self.get_latest_balances().await?;
        Ok(all
            .into_iter()
            .filter(|(id, _)| account_ids.contains(id))
            .collect())
    }

    async fn get_latest_balances_for_account(&self, account_id: &Id) -> Result<Vec<Balance>> {
        let all = self.get_latest_balances().await?;
        Ok(all
            .into_iter()
            .filter(|(id, _)| id == account_id)
            .map(|(_, b)| b)
            .collect())
    }

    async fn get_transactions(&self, account_id: &Id) -> Result<Vec<Transaction>> {
        Ok(self
            .transactions
            .read()
            .unwrap()
            .get(account_id)
            .cloned()
            .unwrap_or_default())
    }

    async fn save_transaction(&self, account_id: &Id, transaction: &Transaction) -> Result<()> {
        self.transactions
            .write()
            .unwrap()
            .entry(account_id.clone())
            .or_default()
            .push(transaction.clone());
        Ok(())
    }
}
```

**Step 4: Implement calculate method**

Replace the `calculate` method in `src/portfolio/service.rs`:

```rust
use anyhow::{Context, Result};
use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;

use crate::market_data::{AssetId, MarketDataService};
use crate::models::{Asset, Id};
use crate::storage::Storage;

use super::{
    AccountHolding, AccountSummary, AssetSummary, Grouping, PortfolioQuery,
    PortfolioSnapshot, RefreshPolicy,
};

pub struct PortfolioService {
    storage: Arc<dyn Storage>,
    market_data: Arc<MarketDataService>,
}

impl PortfolioService {
    pub fn new(storage: Arc<dyn Storage>, market_data: Arc<MarketDataService>) -> Self {
        Self { storage, market_data }
    }

    pub async fn calculate(
        &self,
        query: &PortfolioQuery,
        _refresh: &RefreshPolicy,
    ) -> Result<PortfolioSnapshot> {
        // 1. Get all accounts
        let accounts = self.storage.list_accounts().await?;
        let accounts_by_id: HashMap<Id, _> = accounts
            .iter()
            .map(|a| (a.id.clone(), a))
            .collect();

        // 2. Get all connections for account names
        let connections = self.storage.list_connections().await?;
        let connections_by_id: HashMap<Id, _> = connections
            .iter()
            .map(|c| (c.id.clone(), c))
            .collect();

        // 3. Get latest balances, filtered by as_of_date
        let all_balances = self.storage.get_latest_balances().await?;
        let as_of_datetime = query.as_of_date
            .and_hms_opt(23, 59, 59)
            .unwrap()
            .and_utc();

        let filtered_balances: Vec<_> = all_balances
            .into_iter()
            .filter(|(_, b)| b.timestamp <= as_of_datetime)
            .collect();

        // 4. Aggregate by asset
        struct HoldingInfo {
            account_id: Id,
            amount: Decimal,
            balance_date: NaiveDate,
        }

        let mut by_asset: HashMap<String, (Asset, Vec<HoldingInfo>)> = HashMap::new();

        for (account_id, balance) in &filtered_balances {
            let key = serde_json::to_string(&balance.asset)?;
            let amount = Decimal::from_str(&balance.amount)
                .with_context(|| format!("Invalid amount: {}", balance.amount))?;

            by_asset
                .entry(key)
                .or_insert_with(|| (balance.asset.clone(), Vec::new()))
                .1
                .push(HoldingInfo {
                    account_id: account_id.clone(),
                    amount,
                    balance_date: balance.timestamp.date_naive(),
                });
        }

        // 5. Calculate values for each asset
        let mut asset_summaries = Vec::new();
        let mut total_value = Decimal::ZERO;

        for (_, (asset, holdings)) in by_asset {
            let total_amount: Decimal = holdings.iter().map(|h| h.amount).sum();

            let (value, price, price_date, fx_rate, fx_date) =
                self.value_asset(&asset, total_amount, query.as_of_date, &query.currency).await?;

            total_value += value;

            let detail_holdings = if query.include_detail {
                Some(
                    holdings
                        .iter()
                        .map(|h| {
                            let account = accounts_by_id.get(&h.account_id);
                            AccountHolding {
                                account_id: h.account_id.to_string(),
                                account_name: account
                                    .map(|a| a.name.clone())
                                    .unwrap_or_else(|| "Unknown".to_string()),
                                amount: h.amount.normalize().to_string(),
                                balance_date: h.balance_date,
                            }
                        })
                        .collect(),
                )
            } else {
                None
            };

            asset_summaries.push(AssetSummary {
                asset,
                total_amount: total_amount.normalize().to_string(),
                price: price.map(|p| p.normalize().to_string()),
                price_date,
                fx_rate: fx_rate.map(|r| r.normalize().to_string()),
                fx_date,
                value_in_base: value.normalize().to_string(),
                holdings: detail_holdings,
            });
        }

        // 6. Aggregate by account
        let mut account_values: HashMap<Id, Decimal> = HashMap::new();

        for (account_id, balance) in &filtered_balances {
            let amount = Decimal::from_str(&balance.amount)?;
            let (value, _, _, _, _) =
                self.value_asset(&balance.asset, amount, query.as_of_date, &query.currency).await?;

            *account_values.entry(account_id.clone()).or_default() += value;
        }

        let account_summaries: Vec<_> = account_values
            .into_iter()
            .map(|(account_id, value)| {
                let account = accounts_by_id.get(&account_id);
                let connection = account
                    .and_then(|a| connections_by_id.get(&a.connection_id));

                AccountSummary {
                    account_id: account_id.to_string(),
                    account_name: account
                        .map(|a| a.name.clone())
                        .unwrap_or_else(|| "Unknown".to_string()),
                    connection_name: connection
                        .map(|c| c.name.clone())
                        .unwrap_or_else(|| "Unknown".to_string()),
                    value_in_base: value.normalize().to_string(),
                }
            })
            .collect();

        // 7. Build snapshot based on grouping
        let (by_asset_output, by_account_output) = match query.grouping {
            Grouping::Asset => (Some(asset_summaries), None),
            Grouping::Account => (None, Some(account_summaries)),
            Grouping::Both => (Some(asset_summaries), Some(account_summaries)),
        };

        Ok(PortfolioSnapshot {
            as_of_date: query.as_of_date,
            currency: query.currency.clone(),
            total_value: total_value.normalize().to_string(),
            by_asset: by_asset_output,
            by_account: by_account_output,
        })
    }

    async fn value_asset(
        &self,
        asset: &Asset,
        amount: Decimal,
        as_of: NaiveDate,
        target_currency: &str,
    ) -> Result<(Decimal, Option<Decimal>, Option<NaiveDate>, Option<Decimal>, Option<NaiveDate>)> {
        match asset {
            Asset::Currency { iso_code } => {
                if iso_code.to_uppercase() == target_currency.to_uppercase() {
                    // Same currency, no conversion needed
                    Ok((amount, None, None, None, None))
                } else {
                    // Need FX conversion
                    let fx = self.market_data
                        .fx_close(iso_code, target_currency, as_of)
                        .await
                        .ok();

                    match fx {
                        Some(fx_point) => {
                            let rate = Decimal::from_str(&fx_point.rate)?;
                            let value = amount * rate;
                            Ok((value, None, None, Some(rate), Some(fx_point.as_of_date)))
                        }
                        None => {
                            // No FX rate available, use 1:1 as fallback
                            Ok((amount, None, None, None, None))
                        }
                    }
                }
            }
            Asset::Equity { .. } | Asset::Crypto { .. } => {
                // Get price
                let price_result = self.market_data
                    .price_close(asset, as_of)
                    .await
                    .ok();

                match price_result {
                    Some(price_point) => {
                        let price = Decimal::from_str(&price_point.price)?;
                        let mut value = amount * price;
                        let price_date = Some(price_point.as_of_date);

                        // Check if we need FX conversion
                        let (fx_rate, fx_date) =
                            if price_point.quote_currency.to_uppercase() != target_currency.to_uppercase() {
                                let fx = self.market_data
                                    .fx_close(&price_point.quote_currency, target_currency, as_of)
                                    .await
                                    .ok();

                                match fx {
                                    Some(fx_point) => {
                                        let rate = Decimal::from_str(&fx_point.rate)?;
                                        value *= rate;
                                        (Some(rate), Some(fx_point.as_of_date))
                                    }
                                    None => (None, None),
                                }
                            } else {
                                (None, None)
                            };

                        Ok((value, Some(price), price_date, fx_rate, fx_date))
                    }
                    None => {
                        // No price available, value is 0
                        Ok((Decimal::ZERO, None, None, None, None))
                    }
                }
            }
        }
    }
}
```

**Step 5: Run test to verify it passes**

Run: `cargo test -p keepbook --lib portfolio::service::tests::calculate_single_currency_holding`
Expected: PASS

**Step 6: Add more tests**

Add to the tests module:

```rust
    #[tokio::test]
    async fn calculate_with_equity_and_fx() -> Result<()> {
        let storage = Arc::new(MemoryStorage::new());
        let connection = Connection::new("Broker");
        storage.save_connection(&connection).await?;

        let account = Account::new("Brokerage", connection.id.clone());
        storage.save_account(&account).await?;

        // 10 shares of AAPL
        let balance = Balance::new(Asset::equity("AAPL"), "10")
            .with_timestamp(Utc.with_ymd_and_hms(2026, 2, 1, 12, 0, 0).unwrap());
        storage.save_balance(&account.id, &balance).await?;

        // Setup market data: AAPL = $200, EUR/USD = 1.10
        let store = Arc::new(MemoryMarketDataStore::new());
        let aapl_id = AssetId::from_asset(&Asset::equity("AAPL"));

        store.put_prices(&[PricePoint {
            asset_id: aapl_id,
            as_of_date: chrono::NaiveDate::from_ymd_opt(2026, 2, 1).unwrap(),
            timestamp: Utc.with_ymd_and_hms(2026, 2, 1, 16, 0, 0).unwrap(),
            price: "200".to_string(),
            quote_currency: "USD".to_string(),
            kind: PriceKind::Close,
            source: "test".to_string(),
        }]).await?;

        store.put_fx_rates(&[FxRatePoint {
            base: "USD".to_string(),
            quote: "EUR".to_string(),
            as_of_date: chrono::NaiveDate::from_ymd_opt(2026, 2, 1).unwrap(),
            timestamp: Utc.with_ymd_and_hms(2026, 2, 1, 16, 0, 0).unwrap(),
            rate: "0.91".to_string(), // 1 USD = 0.91 EUR
            kind: FxRateKind::Close,
            source: "test".to_string(),
        }]).await?;

        let market_data = Arc::new(MarketDataService::new(store, None));
        let service = PortfolioService::new(storage, market_data);

        let query = PortfolioQuery {
            as_of_date: chrono::NaiveDate::from_ymd_opt(2026, 2, 2).unwrap(),
            currency: "EUR".to_string(),
            grouping: Grouping::Asset,
            include_detail: false,
        };

        let result = service.calculate(&query, &RefreshPolicy::default()).await?;

        // 10 * 200 * 0.91 = 1820
        assert_eq!(result.total_value, "1820");
        assert_eq!(result.currency, "EUR");
        assert!(result.by_asset.is_some());
        assert!(result.by_account.is_none());

        Ok(())
    }

    #[tokio::test]
    async fn calculate_with_detail() -> Result<()> {
        let storage = Arc::new(MemoryStorage::new());
        let connection = Connection::new("Bank");
        storage.save_connection(&connection).await?;

        let account1 = Account::new("Checking", connection.id.clone());
        let account2 = Account::new("Savings", connection.id.clone());
        storage.save_account(&account1).await?;
        storage.save_account(&account2).await?;

        // USD in both accounts
        let b1 = Balance::new(Asset::currency("USD"), "1000")
            .with_timestamp(Utc.with_ymd_and_hms(2026, 2, 1, 12, 0, 0).unwrap());
        let b2 = Balance::new(Asset::currency("USD"), "5000")
            .with_timestamp(Utc.with_ymd_and_hms(2026, 2, 1, 12, 0, 0).unwrap());
        storage.save_balance(&account1.id, &b1).await?;
        storage.save_balance(&account2.id, &b2).await?;

        let store = Arc::new(MemoryMarketDataStore::new());
        let market_data = Arc::new(MarketDataService::new(store, None));
        let service = PortfolioService::new(storage, market_data);

        let query = PortfolioQuery {
            as_of_date: chrono::NaiveDate::from_ymd_opt(2026, 2, 2).unwrap(),
            currency: "USD".to_string(),
            grouping: Grouping::Both,
            include_detail: true,
        };

        let result = service.calculate(&query, &RefreshPolicy::default()).await?;

        assert_eq!(result.total_value, "6000");

        let by_asset = result.by_asset.unwrap();
        assert_eq!(by_asset.len(), 1);

        let usd_summary = &by_asset[0];
        assert_eq!(usd_summary.total_amount, "6000");
        assert!(usd_summary.holdings.is_some());
        assert_eq!(usd_summary.holdings.as_ref().unwrap().len(), 2);

        Ok(())
    }
```

**Step 7: Run all portfolio tests**

Run: `cargo test -p keepbook --lib portfolio`
Expected: All PASS

**Step 8: Commit**

```bash
git add src/portfolio/service.rs src/storage/memory.rs src/storage/mod.rs
git commit -m "feat(portfolio): implement PortfolioService calculate

- Aggregate balances by asset across all accounts
- Convert to target currency via FX rates
- Support grouping by asset, account, or both
- Include per-account detail when requested
- Add MemoryStorage for testing

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>"
```

---

## Task 3: Add CLI Command

**Files:**
- Modify: `src/main.rs`

**Step 1: Add PortfolioCommand enum after other command enums**

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

        /// Fetch prices/FX if stale
        #[arg(long)]
        refresh: bool,

        /// Always fetch fresh prices/FX
        #[arg(long)]
        force_refresh: bool,

        /// Staleness threshold (e.g., 1d, 4h)
        #[arg(long, default_value = "1d")]
        stale_after: String,
    },
}
```

**Step 2: Add Portfolio variant to Command enum**

```rust
#[derive(Subcommand)]
enum Command {
    // ... existing variants ...
    #[command(subcommand)]
    Portfolio(PortfolioCommand),
}
```

**Step 3: Add handler in main match**

```rust
Some(Command::Portfolio(portfolio_cmd)) => match portfolio_cmd {
    PortfolioCommand::Snapshot {
        currency,
        date,
        group_by,
        detail,
        refresh,
        force_refresh,
        stale_after,
    } => {
        use keepbook::portfolio::{
            Grouping, PortfolioQuery, PortfolioService, RefreshMode, RefreshPolicy,
        };

        // Parse date
        let as_of_date = match date {
            Some(d) => chrono::NaiveDate::parse_from_str(&d, "%Y-%m-%d")
                .with_context(|| format!("Invalid date format: {}", d))?,
            None => chrono::Utc::now().date_naive(),
        };

        // Parse grouping
        let grouping = match group_by.as_str() {
            "asset" => Grouping::Asset,
            "account" => Grouping::Account,
            "both" => Grouping::Both,
            _ => anyhow::bail!("Invalid grouping: {}. Use: asset, account, both", group_by),
        };

        // Parse stale_after duration
        let stale_threshold = parse_duration(&stale_after)
            .with_context(|| format!("Invalid duration: {}", stale_after))?;

        // Build refresh policy
        let refresh_mode = if force_refresh {
            RefreshMode::Force
        } else if refresh {
            RefreshMode::IfStale
        } else {
            RefreshMode::CachedOnly
        };

        let refresh_policy = RefreshPolicy {
            mode: refresh_mode,
            stale_threshold,
        };

        // Build query
        let query = PortfolioQuery {
            as_of_date,
            currency: currency.unwrap_or_else(|| config.reporting_currency.clone()),
            grouping,
            include_detail: detail,
        };

        // Setup service
        let store = Arc::new(keepbook::market_data::JsonFileMarketDataStore::new(
            &config.data_dir,
        ));
        let market_data = Arc::new(keepbook::market_data::MarketDataService::new(store, None));
        let storage_arc: Arc<dyn keepbook::storage::Storage> = Arc::new(storage);
        let service = PortfolioService::new(storage_arc, market_data);

        // Calculate and output
        let snapshot = service.calculate(&query, &refresh_policy).await?;
        println!("{}", serde_json::to_string_pretty(&snapshot)?);
    }
},
```

**Step 4: Add duration parser helper function**

```rust
fn parse_duration(s: &str) -> Result<std::time::Duration> {
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
        "d" => std::time::Duration::from_secs(num * 24 * 60 * 60),
        "h" => std::time::Duration::from_secs(num * 60 * 60),
        "m" => std::time::Duration::from_secs(num * 60),
        "s" => std::time::Duration::from_secs(num),
        _ => unreachable!(),
    })
}
```

**Step 5: Add required imports at top of main.rs**

```rust
use std::sync::Arc;
```

**Step 6: Verify compilation**

Run: `cargo build`
Expected: Compiles successfully

**Step 7: Test CLI help**

Run: `cargo run -- portfolio --help`
Expected: Shows Portfolio subcommands

Run: `cargo run -- portfolio snapshot --help`
Expected: Shows Snapshot options

**Step 8: Commit**

```bash
git add src/main.rs
git commit -m "feat(cli): add portfolio snapshot command

Adds 'keepbook portfolio snapshot' with options:
- --currency: base currency for valuations
- --date: as-of date (YYYY-MM-DD)
- --group-by: asset, account, or both
- --detail: include per-account breakdown
- --refresh / --force-refresh: price refresh policy
- --stale-after: staleness threshold

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>"
```

---

## Task 4: Remove Old NetWorthCalculator

**Files:**
- Remove: `src/market_data/valuation.rs` (or deprecate)
- Modify: `src/market_data/mod.rs`

**Step 1: Check if NetWorthCalculator is used elsewhere**

Run: `grep -r "NetWorthCalculator" src/`

If only used in tests or not at all, proceed to remove.

**Step 2: Remove or deprecate valuation.rs**

Option A (if unused): Delete the file and remove from mod.rs
Option B (if used): Add deprecation notice

For Option A:
```bash
rm src/market_data/valuation.rs
```

Edit `src/market_data/mod.rs` to remove:
```rust
// Remove this line:
// mod valuation;
// pub use valuation::*;
```

**Step 3: Verify compilation**

Run: `cargo build`
Expected: Compiles (if nothing depends on NetWorthCalculator)

**Step 4: Commit**

```bash
git add -A
git commit -m "refactor: remove deprecated NetWorthCalculator

Replaced by PortfolioService in src/portfolio/service.rs

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>"
```

---

## Task 5: Integration Test

**Files:**
- Create: `tests/portfolio_integration.rs`

**Step 1: Write integration test**

```rust
// tests/portfolio_integration.rs
use anyhow::Result;
use std::process::Command;
use tempfile::TempDir;

#[test]
fn portfolio_snapshot_empty() -> Result<()> {
    let temp = TempDir::new()?;
    let config_path = temp.path().join("keepbook.toml");
    std::fs::write(
        &config_path,
        format!(
            r#"
data_dir = "{}"
reporting_currency = "USD"
"#,
            temp.path().display()
        ),
    )?;

    let output = Command::new(env!("CARGO_BIN_EXE_keepbook"))
        .args(["--config", config_path.to_str().unwrap(), "portfolio", "snapshot"])
        .output()?;

    assert!(output.status.success(), "Command failed: {:?}", output);

    let stdout = String::from_utf8(output.stdout)?;
    let json: serde_json::Value = serde_json::from_str(&stdout)?;

    assert_eq!(json["total_value"], "0");
    assert_eq!(json["currency"], "USD");

    Ok(())
}
```

**Step 2: Run integration test**

Run: `cargo test --test portfolio_integration`
Expected: PASS

**Step 3: Commit**

```bash
git add tests/portfolio_integration.rs
git commit -m "test: add portfolio integration test

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>"
```

---

## Task 6: Final Verification

**Step 1: Run all tests**

Run: `cargo test`
Expected: All tests pass

**Step 2: Run clippy**

Run: `cargo clippy -- -D warnings`
Expected: No warnings

**Step 3: Test with real data (manual)**

Run: `cargo run -- portfolio snapshot`
Run: `cargo run -- portfolio snapshot --group-by asset --detail`
Run: `cargo run -- portfolio snapshot --date 2026-01-01 --currency EUR`

**Step 4: Final commit if any fixes needed**

---

## Summary

| Task | Description | Files |
|------|-------------|-------|
| 1 | Create portfolio module structure | `src/portfolio/{mod,models,service}.rs`, `src/lib.rs` |
| 2 | Implement PortfolioService | `src/portfolio/service.rs`, `src/storage/memory.rs` |
| 3 | Add CLI command | `src/main.rs` |
| 4 | Remove NetWorthCalculator | `src/market_data/valuation.rs`, `src/market_data/mod.rs` |
| 5 | Integration test | `tests/portfolio_integration.rs` |
| 6 | Final verification | - |
