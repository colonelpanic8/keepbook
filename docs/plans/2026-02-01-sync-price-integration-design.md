# Sync + Price Integration Design

## Overview

Integrate price fetching with account synchronization so that:
1. Synchronizers can provide prices alongside balances when available
2. Missing prices are automatically fetched from external PriceSources after sync
3. Prices can be refreshed independently of syncing (daily refresh for existing holdings)
4. All asset values can be converted to a single reporting currency

## Data Model Changes

### Asset Enum

Remove the `Other` variant. New asset types require extending the enum:

```rust
pub enum Asset {
    Currency { iso_code: String },
    Equity { ticker: String, exchange: Option<String> },
    Crypto { symbol: String, network: Option<String> },
}
```

### AssetId Format

Change from SHA256 hash to human-readable path format that maps directly to storage paths:

| Asset | AssetId |
|-------|---------|
| `Currency { iso_code: "USD" }` | `currency/USD` |
| `Equity { ticker: "AAPL", exchange: None }` | `equity/AAPL` |
| `Equity { ticker: "AAPL", exchange: Some("NYSE") }` | `equity/AAPL/NYSE` |
| `Crypto { symbol: "BTC", network: None }` | `crypto/BTC` |
| `Crypto { symbol: "ETH", network: Some("arbitrum") }` | `crypto/ETH/arbitrum` |

Implementation:

```rust
impl AssetId {
    pub fn from_asset(asset: &Asset) -> Self {
        match asset {
            Asset::Currency { iso_code } =>
                Self(format!("currency/{}", iso_code.to_uppercase())),
            Asset::Equity { ticker, exchange: None } =>
                Self(format!("equity/{}", ticker.to_uppercase())),
            Asset::Equity { ticker, exchange: Some(ex) } =>
                Self(format!("equity/{}/{}", ticker.to_uppercase(), ex.to_uppercase())),
            Asset::Crypto { symbol, network: None } =>
                Self(format!("crypto/{}", symbol.to_uppercase())),
            Asset::Crypto { symbol, network: Some(net) } =>
                Self(format!("crypto/{}/{}", symbol.to_uppercase(), net.to_lowercase())),
        }
    }
}
```

### SyncedBalance

Pair balances with optional prices when coming from synchronizers:

```rust
pub struct SyncedBalance {
    pub balance: Balance,
    pub price: Option<PricePoint>,
}
```

### SyncResult

Update to use `SyncedBalance`:

```rust
pub struct SyncResult {
    pub connection: Connection,
    pub accounts: Vec<Account>,
    pub balances: Vec<(Id, Vec<SyncedBalance>)>,
    pub transactions: Vec<(Id, Vec<Transaction>)>,
}
```

### Config

Add global reporting currency:

```rust
pub struct Config {
    // ... existing fields
    pub reporting_currency: String,  // e.g., "USD"
}
```

## SyncOrchestrator

New struct that coordinates sync + price fetching:

```rust
pub struct SyncOrchestrator {
    storage: Arc<dyn Storage>,
    market_data: MarketDataService,
    reporting_currency: String,
}
```

### Methods

#### sync_with_prices

Run sync and fetch any missing prices:

```rust
pub async fn sync_with_prices(
    &self,
    synchronizer: &dyn Synchronizer,
    connection: &mut Connection,
    force_refresh: bool,
) -> Result<SyncResult> {
    // 1. Run the sync
    let result = synchronizer.sync(connection).await?;

    // 2. Store balances and any prices the synchronizer provided
    for (account_id, synced_balances) in &result.balances {
        let balances: Vec<Balance> = synced_balances.iter()
            .map(|sb| sb.balance.clone())
            .collect();
        self.storage.append_balances(account_id, &balances).await?;

        for sb in synced_balances {
            if let Some(price) = &sb.price {
                self.market_data.store_price(price).await?;
            }
        }
    }

    // 3. Collect assets that need prices
    let assets: HashSet<Asset> = result.balances
        .iter()
        .flat_map(|(_, sbs)| sbs.iter().map(|sb| &sb.balance.asset))
        .cloned()
        .collect();

    // 4. Fetch missing prices (skip if already in store, unless force_refresh)
    self.ensure_prices(&assets, Utc::now().date_naive(), force_refresh).await?;

    Ok(result)
}
```

#### Price Refresh Methods

Refresh prices at different scopes without syncing:

```rust
pub async fn refresh_all_prices(
    &self,
    date: NaiveDate,
    force: bool,
) -> Result<PriceRefreshResult>;

pub async fn refresh_connection_prices(
    &self,
    connection_id: &Id,
    date: NaiveDate,
    force: bool,
) -> Result<PriceRefreshResult>;

pub async fn refresh_account_prices(
    &self,
    account_id: &Id,
    date: NaiveDate,
    force: bool,
) -> Result<PriceRefreshResult>;
```

Result type:

```rust
pub struct PriceRefreshResult {
    pub fetched: Vec<PricePoint>,
    pub skipped: Vec<Asset>,       // already had price
    pub failed: Vec<(Asset, String)>,  // asset + error message
}
```

### Internal: ensure_prices

Core logic for fetching missing prices:

```rust
async fn ensure_prices(
    &self,
    assets: &HashSet<Asset>,
    date: NaiveDate,
    force: bool,
) -> Result<PriceRefreshResult> {
    let mut needed_fx_pairs: HashSet<(String, String)> = HashSet::new();

    for asset in assets {
        // Handle currencies - just need FX rate to reporting currency
        if let Asset::Currency { iso_code } = asset {
            if iso_code != &self.reporting_currency {
                needed_fx_pairs.insert((iso_code.clone(), self.reporting_currency.clone()));
            }
            continue;
        }

        // Fetch asset price, note its quote currency
        let price = self.fetch_price_if_missing(asset, date, force).await?;
        if let Some(p) = price {
            if p.quote_currency != self.reporting_currency {
                needed_fx_pairs.insert((p.quote_currency.clone(), self.reporting_currency.clone()));
            }
        }
    }

    // Fetch needed FX rates
    for (base, quote) in needed_fx_pairs {
        self.fetch_fx_rate_if_missing(&base, &quote, date, force).await?;
    }

    // ...
}
```

### Price Source Routing

Route by `Asset` variant:
- `Asset::Currency` → FxRateSource (for conversion to reporting currency)
- `Asset::Equity` → EquityPriceSource
- `Asset::Crypto` → CryptoPriceSource

### Price Lookup Priority

1. Check store first - if price exists for the date, use it
2. Only fetch from external source if missing (unless `force=true`)
3. Synchronizer-provided prices are stored like any other - first one in wins

## Storage Changes

Add method to get latest balances (for price refresh without sync):

```rust
trait Storage {
    // ... existing methods

    /// Get the most recent balance for each (account, asset) pair
    async fn get_latest_balances(&self) -> Result<Vec<Balance>>;

    /// Scoped variants
    async fn get_latest_balances_for_connection(&self, connection_id: &Id) -> Result<Vec<Balance>>;
    async fn get_latest_balances_for_account(&self, account_id: &Id) -> Result<Vec<Balance>>;
}
```

## Synchronizer Updates

### Schwab

Extract price from `Position` struct:

```rust
let price_point = PricePoint {
    asset_id: AssetId::from_asset(&asset),
    as_of_date: Utc::now().date_naive(),
    timestamp: Utc::now(),
    price: position.price.to_string(),
    quote_currency: "USD".to_string(),
    kind: PriceKind::Close,
    source: "schwab".to_string(),
};

SyncedBalance {
    balance: Balance::new(asset, position.quantity.to_string()),
    price: Some(price_point),
}
```

### Coinbase

Similar pattern if price data is available from the API.

### Plaid (banking)

No prices typically available - use `price: None`.

## File Changes Summary

**Modify:**
- `src/models/asset.rs` - Remove `Asset::Other`
- `src/market_data/asset_id.rs` - Human-readable path format
- `src/sync/mod.rs` - Add `SyncedBalance`, update `SyncResult`
- `src/config.rs` - Add `reporting_currency`
- `src/storage/mod.rs` - Add `get_latest_balances()` methods
- `src/sync/schwab.rs` - Extract prices into `SyncedBalance`
- `examples/coinbase.rs` - Extract prices if available

**Create:**
- `src/sync/orchestrator.rs` - New `SyncOrchestrator`
