# Balance Snapshot Model Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Replace per-asset balance lines in JSONL with atomic snapshots containing all assets, fixing the bug where sold assets retain stale non-zero balances.

**Architecture:** Introduce `BalanceSnapshot` struct that groups all `AssetBalance` entries under a single timestamp. Storage reads/writes entire snapshots. "Latest balances" means "all assets from the most recent snapshot."

**Tech Stack:** Rust, serde, chrono, tokio async

---

## Task 1: Add New Balance Types

**Files:**
- Modify: `src/models/balance.rs`
- Modify: `src/models/mod.rs`

**Step 1: Add AssetBalance and BalanceSnapshot structs to balance.rs**

Add after the existing `Balance` struct:

```rust
/// A single asset's balance without timestamp (belongs to a snapshot).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetBalance {
    pub asset: Asset,
    /// Amount as string to avoid floating point precision issues
    pub amount: String,
}

impl AssetBalance {
    pub fn new(asset: Asset, amount: impl Into<String>) -> Self {
        Self {
            asset,
            amount: amount.into(),
        }
    }
}

/// A point-in-time snapshot of ALL holdings in an account.
/// One line in the JSONL file = one complete account state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BalanceSnapshot {
    pub timestamp: DateTime<Utc>,
    pub balances: Vec<AssetBalance>,
}

impl BalanceSnapshot {
    pub fn new(timestamp: DateTime<Utc>, balances: Vec<AssetBalance>) -> Self {
        Self { timestamp, balances }
    }

    pub fn now(balances: Vec<AssetBalance>) -> Self {
        Self::new(Utc::now(), balances)
    }
}
```

**Step 2: Export new types from mod.rs**

Change line 10 in `src/models/mod.rs` from:
```rust
pub use balance::Balance;
```
to:
```rust
pub use balance::{AssetBalance, Balance, BalanceSnapshot};
```

**Step 3: Run cargo check**

Run: `cargo check`
Expected: Compiles with no errors (Balance is still there, just new types added)

**Step 4: Commit**

```bash
git add src/models/balance.rs src/models/mod.rs
git commit -m "$(cat <<'EOF'
feat: add AssetBalance and BalanceSnapshot types

Introduces new types for atomic balance snapshots:
- AssetBalance: single asset/amount pair without timestamp
- BalanceSnapshot: timestamp + Vec<AssetBalance>

The old Balance type is preserved for now during migration.

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>
EOF
)"
```

---

## Task 2: Update Storage Trait

**Files:**
- Modify: `src/storage/mod.rs`

**Step 1: Update imports**

Change line 11 from:
```rust
use crate::models::{Account, Balance, Connection, Id, Transaction};
```
to:
```rust
use crate::models::{Account, AssetBalance, Balance, BalanceSnapshot, Connection, Id, Transaction};
```

**Step 2: Replace balance methods in the Storage trait**

Replace lines 30-41 (the balance-related methods):
```rust
    // Balances
    async fn get_balances(&self, account_id: &Id) -> Result<Vec<Balance>>;
    async fn append_balances(&self, account_id: &Id, balances: &[Balance]) -> Result<()>;

    /// Get the most recent balance for each (account, asset) pair across all accounts.
    async fn get_latest_balances(&self) -> Result<Vec<(Id, Balance)>>;

    /// Get the most recent balance for each asset in accounts belonging to a connection.
    async fn get_latest_balances_for_connection(&self, connection_id: &Id) -> Result<Vec<(Id, Balance)>>;

    /// Get the most recent balance for each asset in a specific account.
    async fn get_latest_balances_for_account(&self, account_id: &Id) -> Result<Vec<Balance>>;
```

with:
```rust
    // Balance Snapshots
    async fn get_balance_snapshots(&self, account_id: &Id) -> Result<Vec<BalanceSnapshot>>;
    async fn append_balance_snapshot(&self, account_id: &Id, snapshot: &BalanceSnapshot) -> Result<()>;

    /// Get the most recent balance snapshot for a specific account.
    async fn get_latest_balance_snapshot(&self, account_id: &Id) -> Result<Option<BalanceSnapshot>>;

    /// Get the most recent balance snapshot for each account across all accounts.
    async fn get_latest_balances(&self) -> Result<Vec<(Id, BalanceSnapshot)>>;

    /// Get the most recent balance snapshot for each account belonging to a connection.
    async fn get_latest_balances_for_connection(&self, connection_id: &Id) -> Result<Vec<(Id, BalanceSnapshot)>>;
```

**Step 3: Run cargo check**

Run: `cargo check`
Expected: Fails with many errors (implementations don't match trait) - this is expected

**Step 4: Commit**

```bash
git add src/storage/mod.rs
git commit -m "$(cat <<'EOF'
refactor: update Storage trait for snapshot model

Changes balance methods to work with BalanceSnapshot:
- get_balance_snapshots: returns all snapshots for account
- append_balance_snapshot: appends single snapshot
- get_latest_balance_snapshot: returns most recent snapshot
- get_latest_balances: returns (account_id, snapshot) pairs

This breaks compilation until implementations are updated.

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>
EOF
)"
```

---

## Task 3: Update JsonFileStorage Implementation

**Files:**
- Modify: `src/storage/json_file.rs`

**Step 1: Update imports at top of file**

Change line 8 from:
```rust
use crate::models::{Account, AccountConfig, Balance, Connection, ConnectionConfig, ConnectionState, Id, Transaction};
```
to:
```rust
use crate::models::{Account, AccountConfig, BalanceSnapshot, Connection, ConnectionConfig, ConnectionState, Id, Transaction};
```

**Step 2: Replace get_balances implementation (around line 401)**

Replace:
```rust
    async fn get_balances(&self, account_id: &Id) -> Result<Vec<Balance>> {
        self.read_jsonl(&self.balances_file(account_id)).await
    }
```
with:
```rust
    async fn get_balance_snapshots(&self, account_id: &Id) -> Result<Vec<BalanceSnapshot>> {
        self.read_jsonl(&self.balances_file(account_id)).await
    }
```

**Step 3: Replace append_balances implementation (around line 405)**

Replace:
```rust
    async fn append_balances(&self, account_id: &Id, balances: &[Balance]) -> Result<()> {
        self.append_jsonl(&self.balances_file(account_id), balances).await
    }
```
with:
```rust
    async fn append_balance_snapshot(&self, account_id: &Id, snapshot: &BalanceSnapshot) -> Result<()> {
        self.append_jsonl(&self.balances_file(account_id), &[snapshot]).await
    }
```

**Step 4: Replace get_latest_balances_for_account implementation (around line 417)**

Replace the entire function:
```rust
    async fn get_latest_balances_for_account(&self, account_id: &Id) -> Result<Vec<Balance>> {
        use crate::models::Asset;

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
with:
```rust
    async fn get_latest_balance_snapshot(&self, account_id: &Id) -> Result<Option<BalanceSnapshot>> {
        let snapshots = self.get_balance_snapshots(account_id).await?;
        Ok(snapshots.into_iter().max_by_key(|s| s.timestamp))
    }
```

**Step 5: Replace get_latest_balances_for_connection implementation (around line 438)**

Replace:
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
with:
```rust
    async fn get_latest_balances_for_connection(&self, connection_id: &Id) -> Result<Vec<(Id, BalanceSnapshot)>> {
        let connection = self.get_connection(connection_id).await?
            .ok_or_else(|| anyhow::anyhow!("Connection not found"))?;

        let mut results = Vec::new();
        for account_id in &connection.state.account_ids {
            if let Some(snapshot) = self.get_latest_balance_snapshot(account_id).await? {
                results.push((account_id.clone(), snapshot));
            }
        }

        Ok(results)
    }
```

**Step 6: Replace get_latest_balances implementation (around line 453)**

Replace:
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
with:
```rust
    async fn get_latest_balances(&self) -> Result<Vec<(Id, BalanceSnapshot)>> {
        let connections = self.list_connections().await?;

        let mut results = Vec::new();
        for connection in connections {
            let connection_snapshots = self.get_latest_balances_for_connection(connection.id()).await?;
            results.extend(connection_snapshots);
        }

        Ok(results)
    }
```

**Step 7: Run cargo check**

Run: `cargo check`
Expected: Fails - MemoryStorage and callers not yet updated

**Step 8: Commit**

```bash
git add src/storage/json_file.rs
git commit -m "$(cat <<'EOF'
refactor: update JsonFileStorage for snapshot model

Implements the new Storage trait methods:
- get_balance_snapshots: reads all snapshots from JSONL
- append_balance_snapshot: appends single snapshot line
- get_latest_balance_snapshot: finds most recent by timestamp
- Updated connection/global queries to return snapshots

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>
EOF
)"
```

---

## Task 4: Update MemoryStorage Implementation

**Files:**
- Modify: `src/storage/memory.rs`

**Step 1: Update imports**

Change line 10 from:
```rust
use crate::models::{Account, Asset, Balance, Connection, Id, Transaction};
```
to:
```rust
use crate::models::{Account, BalanceSnapshot, Connection, Id, Transaction};
```

**Step 2: Update the balances field in MemoryStorage struct**

Change line 18 from:
```rust
    balances: Mutex<HashMap<Id, Vec<Balance>>>,
```
to:
```rust
    balances: Mutex<HashMap<Id, Vec<BalanceSnapshot>>>,
```

**Step 3: Remove the save_balance convenience method (lines 32-35)**

Delete:
```rust
    /// Convenience method for tests to save a single balance.
    pub async fn save_balance(&self, account_id: &Id, balance: &Balance) -> Result<()> {
        self.append_balances(account_id, &[balance.clone()]).await
    }
```

**Step 4: Replace get_balances implementation (around line 92)**

Replace:
```rust
    async fn get_balances(&self, account_id: &Id) -> Result<Vec<Balance>> {
        let balances = self.balances.lock().await;
        Ok(balances.get(account_id).cloned().unwrap_or_default())
    }
```
with:
```rust
    async fn get_balance_snapshots(&self, account_id: &Id) -> Result<Vec<BalanceSnapshot>> {
        let balances = self.balances.lock().await;
        Ok(balances.get(account_id).cloned().unwrap_or_default())
    }
```

**Step 5: Replace append_balances implementation (around line 97)**

Replace:
```rust
    async fn append_balances(&self, account_id: &Id, new_balances: &[Balance]) -> Result<()> {
        let mut balances = self.balances.lock().await;
        balances
            .entry(account_id.clone())
            .or_default()
            .extend(new_balances.iter().cloned());
        Ok(())
    }
```
with:
```rust
    async fn append_balance_snapshot(&self, account_id: &Id, snapshot: &BalanceSnapshot) -> Result<()> {
        let mut balances = self.balances.lock().await;
        balances
            .entry(account_id.clone())
            .or_default()
            .push(snapshot.clone());
        Ok(())
    }
```

**Step 6: Replace get_latest_balances implementation (around line 106)**

Replace the entire function:
```rust
    async fn get_latest_balances(&self) -> Result<Vec<(Id, Balance)>> {
        let accounts = self.accounts.lock().await;
        let balances = self.balances.lock().await;

        let mut results = Vec::new();
        for account_id in accounts.keys() {
            if let Some(account_balances) = balances.get(account_id) {
                // Group by asset, keep most recent
                let mut latest: HashMap<Asset, Balance> = HashMap::new();
                for balance in account_balances {
                    latest
                        .entry(balance.asset.clone())
                        .and_modify(|existing| {
                            if balance.timestamp > existing.timestamp {
                                *existing = balance.clone();
                            }
                        })
                        .or_insert(balance.clone());
                }
                for balance in latest.into_values() {
                    results.push((account_id.clone(), balance));
                }
            }
        }

        Ok(results)
    }
```
with:
```rust
    async fn get_latest_balances(&self) -> Result<Vec<(Id, BalanceSnapshot)>> {
        let accounts = self.accounts.lock().await;
        let balances = self.balances.lock().await;

        let mut results = Vec::new();
        for account_id in accounts.keys() {
            if let Some(snapshots) = balances.get(account_id) {
                if let Some(latest) = snapshots.iter().max_by_key(|s| s.timestamp) {
                    results.push((account_id.clone(), latest.clone()));
                }
            }
        }

        Ok(results)
    }
```

**Step 7: Replace get_latest_balances_for_connection implementation (around line 134)**

Replace:
```rust
    async fn get_latest_balances_for_connection(&self, connection_id: &Id) -> Result<Vec<(Id, Balance)>> {
        let connections = self.connections.lock().await;
        let accounts = self.accounts.lock().await;
        let balances = self.balances.lock().await;

        let connection = connections.get(connection_id);
        if connection.is_none() {
            return Ok(Vec::new());
        }

        // Find all accounts for this connection
        let account_ids: Vec<Id> = accounts
            .values()
            .filter(|a| &a.connection_id == connection_id)
            .map(|a| a.id.clone())
            .collect();

        let mut results = Vec::new();
        for account_id in account_ids {
            if let Some(account_balances) = balances.get(&account_id) {
                let mut latest: HashMap<Asset, Balance> = HashMap::new();
                for balance in account_balances {
                    latest
                        .entry(balance.asset.clone())
                        .and_modify(|existing| {
                            if balance.timestamp > existing.timestamp {
                                *existing = balance.clone();
                            }
                        })
                        .or_insert(balance.clone());
                }
                for balance in latest.into_values() {
                    results.push((account_id.clone(), balance));
                }
            }
        }

        Ok(results)
    }
```
with:
```rust
    async fn get_latest_balances_for_connection(&self, connection_id: &Id) -> Result<Vec<(Id, BalanceSnapshot)>> {
        let connections = self.connections.lock().await;
        let accounts = self.accounts.lock().await;
        let balances = self.balances.lock().await;

        if connections.get(connection_id).is_none() {
            return Ok(Vec::new());
        }

        let account_ids: Vec<Id> = accounts
            .values()
            .filter(|a| &a.connection_id == connection_id)
            .map(|a| a.id.clone())
            .collect();

        let mut results = Vec::new();
        for account_id in account_ids {
            if let Some(snapshots) = balances.get(&account_id) {
                if let Some(latest) = snapshots.iter().max_by_key(|s| s.timestamp) {
                    results.push((account_id.clone(), latest.clone()));
                }
            }
        }

        Ok(results)
    }
```

**Step 8: Replace get_latest_balances_for_account implementation (around line 174)**

Replace:
```rust
    async fn get_latest_balances_for_account(&self, account_id: &Id) -> Result<Vec<Balance>> {
        let balances = self.balances.lock().await;

        if let Some(account_balances) = balances.get(account_id) {
            let mut latest: HashMap<Asset, Balance> = HashMap::new();
            for balance in account_balances {
                latest
                    .entry(balance.asset.clone())
                    .and_modify(|existing| {
                        if balance.timestamp > existing.timestamp {
                            *existing = balance.clone();
                        }
                    })
                    .or_insert(balance.clone());
            }
            Ok(latest.into_values().collect())
        } else {
            Ok(Vec::new())
        }
    }
```
with:
```rust
    async fn get_latest_balance_snapshot(&self, account_id: &Id) -> Result<Option<BalanceSnapshot>> {
        let balances = self.balances.lock().await;
        Ok(balances
            .get(account_id)
            .and_then(|snapshots| snapshots.iter().max_by_key(|s| s.timestamp).cloned()))
    }
```

**Step 9: Run cargo check**

Run: `cargo check`
Expected: Fails - callers not yet updated (sync module, main.rs, etc.)

**Step 10: Commit**

```bash
git add src/storage/memory.rs
git commit -m "$(cat <<'EOF'
refactor: update MemoryStorage for snapshot model

Implements snapshot-based Storage trait:
- Stores Vec<BalanceSnapshot> per account instead of Vec<Balance>
- Simplifies latest-balance logic (just find max timestamp)
- Removes save_balance convenience method

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>
EOF
)"
```

---

## Task 5: Update SyncResult and SyncedBalance

**Files:**
- Modify: `src/sync/mod.rs`

**Step 1: Update imports**

Change line 10 from:
```rust
use crate::models::{Account, Balance, Connection, Id, Transaction};
```
to:
```rust
use crate::models::{Account, AssetBalance, BalanceSnapshot, Connection, Id, Transaction};
```

**Step 2: Update SyncedBalance struct**

Replace lines 13-29:
```rust
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
with:
```rust
/// An asset balance paired with optional price data from the synchronizer.
#[derive(Debug, Clone)]
pub struct SyncedAssetBalance {
    pub asset_balance: AssetBalance,
    pub price: Option<PricePoint>,
}

impl SyncedAssetBalance {
    pub fn new(asset_balance: AssetBalance) -> Self {
        Self { asset_balance, price: None }
    }

    pub fn with_price(mut self, price: PricePoint) -> Self {
        self.price = Some(price);
        self
    }
}
```

**Step 3: Update SyncResult struct**

Replace line 35:
```rust
    pub balances: Vec<(Id, Vec<SyncedBalance>)>,
```
with:
```rust
    pub balances: Vec<(Id, Vec<SyncedAssetBalance>)>,
```

**Step 4: Update SyncResult::save method**

Replace lines 48-56:
```rust
        for (account_id, synced_balances) in &self.balances {
            let balances: Vec<Balance> = synced_balances
                .iter()
                .map(|sb| sb.balance.clone())
                .collect();
            if !balances.is_empty() {
                storage.append_balances(account_id, &balances).await?;
            }
        }
```
with:
```rust
        for (account_id, synced_balances) in &self.balances {
            if !synced_balances.is_empty() {
                let asset_balances: Vec<AssetBalance> = synced_balances
                    .iter()
                    .map(|sb| sb.asset_balance.clone())
                    .collect();
                let snapshot = BalanceSnapshot::now(asset_balances);
                storage.append_balance_snapshot(account_id, &snapshot).await?;
            }
        }
```

**Step 5: Run cargo check**

Run: `cargo check`
Expected: Fails - synchronizers and orchestrator use old types

**Step 6: Commit**

```bash
git add src/sync/mod.rs
git commit -m "$(cat <<'EOF'
refactor: update SyncResult for snapshot model

- Rename SyncedBalance to SyncedAssetBalance
- Change balance field to asset_balance: AssetBalance
- SyncResult::save now creates BalanceSnapshot from asset balances

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>
EOF
)"
```

---

## Task 6: Update Schwab Synchronizer

**Files:**
- Modify: `src/sync/synchronizers/schwab.rs`

**Step 1: Update imports**

Change line 22 from:
```rust
use crate::models::{
    Account, Asset, Balance, Connection, ConnectionStatus, Id, LastSync, SyncStatus,
};
```
to:
```rust
use crate::models::{
    Account, Asset, AssetBalance, Connection, ConnectionStatus, Id, LastSync, SyncStatus,
};
```

Change line 26 from:
```rust
use crate::sync::{AuthStatus, InteractiveAuth, SyncResult, SyncedBalance, Synchronizer};
```
to:
```rust
use crate::sync::{AuthStatus, InteractiveAuth, SyncResult, SyncedAssetBalance, Synchronizer};
```

**Step 2: Update account_balances variable type (around line 120)**

Change line 120 from:
```rust
            let mut account_balances = vec![];
```
to:
```rust
            let mut account_balances: Vec<SyncedAssetBalance> = vec![];
```

**Step 3: Update equity position handling (around lines 132-144)**

Replace:
```rust
                    let asset = Asset::equity(&position.default_symbol);
                    let balance = Balance::new(asset.clone(), position.quantity.to_string());

                    let price_point = PricePoint {
                        asset_id: AssetId::from_asset(&asset),
                        as_of_date: Utc::now().date_naive(),
                        timestamp: Utc::now(),
                        price: position.price.to_string(),
                        quote_currency: "USD".to_string(),
                        kind: PriceKind::Close,
                        source: "schwab".to_string(),
                    };
                    account_balances.push(SyncedBalance::new(balance).with_price(price_point));
```
with:
```rust
                    let asset = Asset::equity(&position.default_symbol);
                    let asset_balance = AssetBalance::new(asset.clone(), position.quantity.to_string());

                    let price_point = PricePoint {
                        asset_id: AssetId::from_asset(&asset),
                        as_of_date: Utc::now().date_naive(),
                        timestamp: Utc::now(),
                        price: position.price.to_string(),
                        quote_currency: "USD".to_string(),
                        kind: PriceKind::Close,
                        source: "schwab".to_string(),
                    };
                    account_balances.push(SyncedAssetBalance::new(asset_balance).with_price(price_point));
```

**Step 4: Update cash balance handling (around lines 150-155)**

Replace:
```rust
                        if cash > 0.0 {
                            account_balances.push(SyncedBalance::new(Balance::new(
                                Asset::currency("USD"),
                                cash.to_string(),
                            )));
                        }
```
with:
```rust
                        account_balances.push(SyncedAssetBalance::new(AssetBalance::new(
                            Asset::currency("USD"),
                            cash.to_string(),
                        )));
```

(Note: removed the `if cash > 0.0` check - we want to record zero balances in snapshots)

**Step 5: Update non-brokerage account handling (around lines 160-163)**

Replace:
```rust
                account_balances.push(SyncedBalance::new(Balance::new(
                    Asset::currency("USD"),
                    bal.balance.to_string(),
                )));
```
with:
```rust
                account_balances.push(SyncedAssetBalance::new(AssetBalance::new(
                    Asset::currency("USD"),
                    bal.balance.to_string(),
                )));
```

**Step 6: Run cargo check**

Run: `cargo check`
Expected: Fails - Coinbase synchronizer not yet updated

**Step 7: Commit**

```bash
git add src/sync/synchronizers/schwab.rs
git commit -m "$(cat <<'EOF'
refactor: update Schwab synchronizer for snapshot model

- Use AssetBalance instead of Balance
- Use SyncedAssetBalance instead of SyncedBalance
- Remove cash > 0 check (snapshots should include all assets)

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>
EOF
)"
```

---

## Task 7: Update Coinbase Synchronizer

**Files:**
- Modify: `src/sync/synchronizers/coinbase.rs`

**Step 1: Update imports**

Change line 18 from:
```rust
use crate::models::{
    Account, Asset, Balance, Connection, ConnectionStatus, Id, LastSync, SyncStatus, Transaction,
};
```
to:
```rust
use crate::models::{
    Account, Asset, AssetBalance, Connection, ConnectionStatus, Id, LastSync, SyncStatus, Transaction,
};
```

Change line 22 from:
```rust
use crate::sync::{SyncResult, SyncedBalance, Synchronizer};
```
to:
```rust
use crate::sync::{SyncResult, SyncedAssetBalance, Synchronizer};
```

**Step 2: Update balances variable type (around line 231)**

Change line 231 from:
```rust
        let mut balances: Vec<(Id, Vec<SyncedBalance>)> = Vec::new();
```
to:
```rust
        let mut balances: Vec<(Id, Vec<SyncedAssetBalance>)> = Vec::new();
```

**Step 3: Update balance creation (around line 287)**

Replace:
```rust
            // Record current balance
            let balance = Balance::new(asset.clone(), &cb_account.available_balance.value);
```
with:
```rust
            // Record current balance
            let asset_balance = AssetBalance::new(asset.clone(), &cb_account.available_balance.value);
```

**Step 4: Update balances.push (around line 312)**

Replace:
```rust
            balances.push((account_id.clone(), vec![SyncedBalance::new(balance)]));
```
with:
```rust
            balances.push((account_id.clone(), vec![SyncedAssetBalance::new(asset_balance)]));
```

**Step 5: Run cargo check**

Run: `cargo check`
Expected: Fails - orchestrator not yet updated

**Step 6: Commit**

```bash
git add src/sync/synchronizers/coinbase.rs
git commit -m "$(cat <<'EOF'
refactor: update Coinbase synchronizer for snapshot model

- Use AssetBalance instead of Balance
- Use SyncedAssetBalance instead of SyncedBalance

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>
EOF
)"
```

---

## Task 8: Update SyncOrchestrator

**Files:**
- Modify: `src/sync/orchestrator.rs`

**Step 1: Update imports**

Change line 10 from:
```rust
use crate::models::{Asset, Connection, Id};
```
to:
```rust
use crate::models::{Asset, BalanceSnapshot, Connection, Id};
```

**Step 2: Update refresh_all_prices (around line 109)**

Replace:
```rust
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
```
with:
```rust
    pub async fn refresh_all_prices(
        &self,
        date: NaiveDate,
        force: bool,
    ) -> Result<PriceRefreshResult> {
        let snapshots = self.storage.get_latest_balances().await?;
        let assets: HashSet<Asset> = snapshots
            .into_iter()
            .flat_map(|(_, snapshot)| snapshot.balances.into_iter().map(|ab| ab.asset))
            .collect();
        self.ensure_prices(&assets, date, force).await
    }
```

**Step 3: Update refresh_connection_prices (around line 123)**

Replace:
```rust
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
```
with:
```rust
    pub async fn refresh_connection_prices(
        &self,
        connection_id: &Id,
        date: NaiveDate,
        force: bool,
    ) -> Result<PriceRefreshResult> {
        let snapshots = self.storage.get_latest_balances_for_connection(connection_id).await?;
        let assets: HashSet<Asset> = snapshots
            .into_iter()
            .flat_map(|(_, snapshot)| snapshot.balances.into_iter().map(|ab| ab.asset))
            .collect();
        self.ensure_prices(&assets, date, force).await
    }
```

**Step 4: Update refresh_account_prices (around line 138)**

Replace:
```rust
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
```
with:
```rust
    pub async fn refresh_account_prices(
        &self,
        account_id: &Id,
        date: NaiveDate,
        force: bool,
    ) -> Result<PriceRefreshResult> {
        let snapshot = self.storage.get_latest_balance_snapshot(account_id).await?;
        let assets: HashSet<Asset> = snapshot
            .map(|s| s.balances.into_iter().map(|ab| ab.asset).collect())
            .unwrap_or_default();
        self.ensure_prices(&assets, date, force).await
    }
```

**Step 5: Update sync_with_prices price extraction (around line 161)**

Replace:
```rust
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
```
with:
```rust
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
            .flat_map(|(_, sbs)| sbs.iter().map(|sb| sb.asset_balance.asset.clone()))
            .collect();
```

**Step 6: Run cargo check**

Run: `cargo check`
Expected: Fails - main.rs and portfolio service not yet updated

**Step 7: Commit**

```bash
git add src/sync/orchestrator.rs
git commit -m "$(cat <<'EOF'
refactor: update SyncOrchestrator for snapshot model

- Extract assets from BalanceSnapshot.balances
- Use get_latest_balance_snapshot for account-level queries
- Update sync_with_prices to use asset_balance field

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>
EOF
)"
```

---

## Task 9: Update Portfolio Service

**Files:**
- Modify: `src/portfolio/service.rs`

**Step 1: Update imports**

Change line 11 from:
```rust
use crate::models::{Account, Asset, Balance, Connection, Id};
```
to:
```rust
use crate::models::{Account, Asset, BalanceSnapshot, Connection, Id};
```

**Step 2: Update AssetAggregate struct (around line 35)**

Replace:
```rust
/// Aggregated data for a single asset across all accounts.
struct AssetAggregate {
    total_amount: Decimal,
    latest_balance_date: NaiveDate,
    holdings: Vec<(Id, Balance)>,
}
```
with:
```rust
/// Represents a single asset holding from a snapshot.
struct AssetHolding {
    account_id: Id,
    asset: Asset,
    amount: String,
    timestamp: DateTime<Utc>,
}

/// Aggregated data for a single asset across all accounts.
struct AssetAggregate {
    total_amount: Decimal,
    latest_balance_date: NaiveDate,
    holdings: Vec<AssetHolding>,
}
```

**Step 3: Update CalculationContext (around line 44)**

Replace:
```rust
/// Context loaded from storage for portfolio calculation.
struct CalculationContext {
    account_map: HashMap<Id, Account>,
    connection_map: HashMap<Id, Connection>,
    filtered_balances: Vec<(Id, Balance)>,
}
```
with:
```rust
/// Context loaded from storage for portfolio calculation.
struct CalculationContext {
    account_map: HashMap<Id, Account>,
    connection_map: HashMap<Id, Connection>,
    filtered_snapshots: Vec<(Id, BalanceSnapshot)>,
}
```

**Step 4: Update load_calculation_context (around line 118)**

Replace:
```rust
        let all_balances = self.storage.get_latest_balances().await?;
        let as_of_datetime = as_of_date.and_hms_opt(23, 59, 59).unwrap().and_utc();

        let filtered_balances: Vec<(Id, Balance)> = all_balances
            .into_iter()
            .filter(|(_, balance)| balance.timestamp <= as_of_datetime)
            .collect();

        Ok(CalculationContext {
            account_map,
            connection_map,
            filtered_balances,
        })
```
with:
```rust
        let all_snapshots = self.storage.get_latest_balances().await?;
        let as_of_datetime = as_of_date.and_hms_opt(23, 59, 59).unwrap().and_utc();

        let filtered_snapshots: Vec<(Id, BalanceSnapshot)> = all_snapshots
            .into_iter()
            .filter(|(_, snapshot)| snapshot.timestamp <= as_of_datetime)
            .collect();

        Ok(CalculationContext {
            account_map,
            connection_map,
            filtered_snapshots,
        })
```

**Step 5: Update aggregate_by_asset signature and implementation (around line 134)**

Replace:
```rust
    /// Aggregate balances by asset, tracking totals and holdings.
    fn aggregate_by_asset(
        balances: &[(Id, Balance)],
    ) -> Result<HashMap<String, AssetAggregate>> {
        let mut by_asset: HashMap<String, AssetAggregate> = HashMap::new();

        for (account_id, balance) in balances {
            let asset_key = serde_json::to_string(&balance.asset)?;
            let amount = Decimal::from_str(&balance.amount)?;
            let balance_date = balance.timestamp.date_naive();

            let entry = by_asset.entry(asset_key).or_insert_with(|| AssetAggregate {
                total_amount: Decimal::ZERO,
                latest_balance_date: balance_date,
                holdings: Vec::new(),
            });

            entry.total_amount += amount;
            if balance_date > entry.latest_balance_date {
                entry.latest_balance_date = balance_date;
            }
            entry.holdings.push((account_id.clone(), balance.clone()));
        }

        Ok(by_asset)
    }
```
with:
```rust
    /// Aggregate balances by asset, tracking totals and holdings.
    fn aggregate_by_asset(
        snapshots: &[(Id, BalanceSnapshot)],
    ) -> Result<HashMap<String, AssetAggregate>> {
        let mut by_asset: HashMap<String, AssetAggregate> = HashMap::new();

        for (account_id, snapshot) in snapshots {
            for asset_balance in &snapshot.balances {
                let asset_key = serde_json::to_string(&asset_balance.asset)?;
                let amount = Decimal::from_str(&asset_balance.amount)?;
                let balance_date = snapshot.timestamp.date_naive();

                let entry = by_asset.entry(asset_key).or_insert_with(|| AssetAggregate {
                    total_amount: Decimal::ZERO,
                    latest_balance_date: balance_date,
                    holdings: Vec::new(),
                });

                entry.total_amount += amount;
                if balance_date > entry.latest_balance_date {
                    entry.latest_balance_date = balance_date;
                }
                entry.holdings.push(AssetHolding {
                    account_id: account_id.clone(),
                    asset: asset_balance.asset.clone(),
                    amount: asset_balance.amount.clone(),
                    timestamp: snapshot.timestamp,
                });
            }
        }

        Ok(by_asset)
    }
```

**Step 6: Update the calculate method call (around line 61)**

Change:
```rust
        let by_asset_agg = Self::aggregate_by_asset(&ctx.filtered_balances)?;
```
to:
```rust
        let by_asset_agg = Self::aggregate_by_asset(&ctx.filtered_snapshots)?;
```

**Step 7: Run cargo check to find remaining issues**

Run: `cargo check 2>&1 | head -50`

This will reveal any remaining places that need updating (build_asset_summaries, build_account_summaries). Update those based on the errors.

**Step 8: Commit**

```bash
git add src/portfolio/service.rs
git commit -m "$(cat <<'EOF'
refactor: update PortfolioService for snapshot model

- Replace filtered_balances with filtered_snapshots
- Add AssetHolding struct for flattened snapshot data
- Update aggregate_by_asset to iterate snapshot.balances

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>
EOF
)"
```

---

## Task 10: Update main.rs

**Files:**
- Modify: `src/main.rs`

**Step 1: Find and update list_balances function**

Search for the `list_balances` function and update it to work with snapshots. The function signature returns `Vec<BalanceOutput>` which flattens snapshots.

Replace the implementation:
```rust
async fn list_balances(storage: &JsonFileStorage) -> Result<Vec<BalanceOutput>> {
    let balances = storage.get_latest_balances().await?;
    Ok(balances
        .into_iter()
        .map(|(account_id, balance)| BalanceOutput {
            account_id: account_id.to_string(),
            asset: serde_json::to_value(&balance.asset).unwrap_or_default(),
            amount: balance.amount,
            timestamp: balance.timestamp.to_rfc3339(),
        })
        .collect())
}
```
with:
```rust
async fn list_balances(storage: &JsonFileStorage) -> Result<Vec<BalanceOutput>> {
    let snapshots = storage.get_latest_balances().await?;
    Ok(snapshots
        .into_iter()
        .flat_map(|(account_id, snapshot)| {
            snapshot.balances.into_iter().map(move |ab| BalanceOutput {
                account_id: account_id.to_string(),
                asset: serde_json::to_value(&ab.asset).unwrap_or_default(),
                amount: ab.amount,
                timestamp: snapshot.timestamp.to_rfc3339(),
            })
        })
        .collect())
}
```

Note: This requires cloning account_id and snapshot.timestamp for each iteration. Adjust the closure captures as needed.

**Step 2: Find and update price dry-run code (around line 515)**

Replace:
```rust
                    let balances = storage.get_latest_balances().await?;
                    let mut seen_assets: HashSet<String> = HashSet::new();

                    for (_, balance) in &balances {
                        match &balance.asset {
```
with:
```rust
                    let snapshots = storage.get_latest_balances().await?;
                    let mut seen_assets: HashSet<String> = HashSet::new();

                    for (_, snapshot) in &snapshots {
                        for asset_balance in &snapshot.balances {
                            match &asset_balance.asset {
```

And close the extra brace at the end of the match block.

**Step 3: Run cargo check**

Run: `cargo check`
Expected: Should compile (or reveal final issues)

**Step 4: Run cargo test**

Run: `cargo test`
Expected: Tests pass (may need test updates)

**Step 5: Commit**

```bash
git add src/main.rs
git commit -m "$(cat <<'EOF'
refactor: update main.rs for snapshot model

- list_balances flattens snapshots to individual balance outputs
- Price dry-run iterates snapshot.balances

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>
EOF
)"
```

---

## Task 11: Remove Old Balance Type (Optional Cleanup)

**Files:**
- Modify: `src/models/balance.rs`
- Modify: `src/models/mod.rs`

**Step 1: Check for remaining Balance usages**

Run: `cargo check 2>&1 | grep -i "Balance"` and `grep -r "Balance" src/ --include="*.rs" | grep -v BalanceSnapshot | grep -v AssetBalance`

If no remaining usages of the old `Balance` type, remove it.

**Step 2: Remove Balance struct from balance.rs**

Delete lines 6-29 (the old Balance struct and impl).

**Step 3: Update mod.rs export**

Change:
```rust
pub use balance::{AssetBalance, Balance, BalanceSnapshot};
```
to:
```rust
pub use balance::{AssetBalance, BalanceSnapshot};
```

**Step 4: Run cargo check and cargo test**

Run: `cargo check && cargo test`
Expected: Compiles and tests pass

**Step 5: Commit**

```bash
git add src/models/balance.rs src/models/mod.rs
git commit -m "$(cat <<'EOF'
chore: remove unused Balance type

The old per-asset Balance type is no longer needed now that
all code uses BalanceSnapshot with Vec<AssetBalance>.

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>
EOF
)"
```

---

## Task 12: Delete Old Balance Files and Test

**Step 1: Delete old balances.jsonl files**

Run: `find data/accounts -name "balances.jsonl" -delete`

**Step 2: Run a sync to populate new format**

Run: `cargo run -- sync` (or whatever command triggers sync)

**Step 3: Verify new format**

Run: `head -1 data/accounts/*/balances.jsonl`

Expected: Each line is a JSON object with `{"timestamp":"...","balances":[...]}`

**Step 4: Run full test suite**

Run: `cargo test`
Expected: All tests pass

**Step 5: Final commit**

```bash
git add -A
git commit -m "$(cat <<'EOF'
chore: migrate to balance snapshot format

Deleted old per-asset balance files. New syncs write atomic
snapshots containing all assets for an account.

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>
EOF
)"
```

---

## Summary

This plan converts the balance storage from per-asset lines to atomic snapshots:

1. **Tasks 1-4**: Add new types and update storage trait/implementations
2. **Tasks 5-7**: Update sync infrastructure (SyncResult, synchronizers)
3. **Tasks 8-10**: Update consumers (orchestrator, portfolio, CLI)
4. **Tasks 11-12**: Cleanup and migration

Each task builds on the previous, maintaining a compilable (though possibly failing) state at each commit.
