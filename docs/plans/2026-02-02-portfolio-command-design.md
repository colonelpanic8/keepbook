# Portfolio Command Design

## Overview

Add a `keepbook portfolio` command that aggregates all assets across all accounts, converts them to a specified base currency, and outputs a JSON summary. The core logic is decoupled from the CLI for reuse in other contexts (graphs, UIs, batch processing).

## CLI Interface

```
keepbook portfolio [OPTIONS]
```

**Flags:**
- `--currency <CODE>` — Override reporting currency (default: from config)
- `--date <DATE>` — Calculate portfolio as of this date (default: today)
- `--group-by <MODE>` — Output grouping: `asset`, `account`, or `both` (default: `both`)
- `--detail` — Include per-account breakdown when grouping by asset
- `--refresh` — Fetch prices/FX rates if stale
- `--force-refresh` — Always fetch fresh prices/FX rates
- `--stale-after <DURATION>` — Staleness threshold for `--refresh` (default: `1d`)

**Examples:**
```bash
# Basic usage with defaults
keepbook portfolio

# Historical snapshot in EUR
keepbook portfolio --date 2025-12-31 --currency EUR

# Just asset totals, with refresh
keepbook portfolio --group-by asset --refresh

# Full detail with custom staleness
keepbook portfolio --detail --refresh --stale-after 4h
```

## Library Architecture

Core logic lives in `src/portfolio/`, separate from CLI:

```
src/portfolio/
  mod.rs           -- Public API
  service.rs       -- PortfolioService orchestrates calculations
  models.rs        -- Output structs (PortfolioSnapshot, AssetHolding, etc.)
```

### Core API

```rust
pub struct PortfolioService {
    storage: Arc<dyn Storage>,
    market_data: Arc<MarketDataService>,
}

pub struct PortfolioQuery {
    pub as_of_date: NaiveDate,
    pub currency: String,
    pub grouping: Grouping,         // Asset, Account, Both
    pub include_detail: bool,
}

pub struct RefreshPolicy {
    pub mode: RefreshMode,          // CachedOnly, IfStale, Force
    pub stale_threshold: Duration,  // e.g., 1 day
}

impl PortfolioService {
    pub async fn calculate(
        &self,
        query: &PortfolioQuery,
        refresh: &RefreshPolicy,
    ) -> Result<PortfolioSnapshot>;
}
```

The CLI is a thin wrapper that parses args, calls the service, and serializes to JSON.

## Output Model

```rust
pub struct PortfolioSnapshot {
    pub as_of_date: NaiveDate,
    pub currency: String,
    pub total_value: Decimal,

    // Present based on grouping
    pub by_asset: Option<Vec<AssetSummary>>,
    pub by_account: Option<Vec<AccountSummary>>,
}

pub struct AssetSummary {
    pub asset: Asset,
    pub total_amount: Decimal,
    pub price: Option<Decimal>,        // None for currencies
    pub price_date: Option<NaiveDate>,
    pub fx_rate: Option<Decimal>,      // None if asset is already in target currency
    pub fx_date: Option<NaiveDate>,
    pub value_in_base: Decimal,

    // Present if --detail flag
    pub holdings: Option<Vec<AccountHolding>>,
}

pub struct AccountHolding {
    pub account_id: Id,
    pub account_name: String,
    pub amount: Decimal,
    pub balance_date: NaiveDate,
}

pub struct AccountSummary {
    pub account_id: Id,
    pub account_name: String,
    pub connection_name: String,
    pub value_in_base: Decimal,
}
```

### JSON Output Example

With `--group-by both --detail`:

```json
{
  "as_of_date": "2026-02-02",
  "currency": "USD",
  "total_value": "125432.50",
  "by_asset": [
    {
      "asset": {"type": "equity", "ticker": "AAPL"},
      "total_amount": "150",
      "price": "185.50",
      "price_date": "2026-02-01",
      "value_in_base": "27825.00",
      "holdings": [
        {"account_id": "...", "account_name": "Schwab", "amount": "100", "balance_date": "2026-01-28"},
        {"account_id": "...", "account_name": "Fidelity", "amount": "50", "balance_date": "2026-01-30"}
      ]
    }
  ],
  "by_account": [
    {"account_id": "...", "account_name": "Schwab", "connection_name": "Schwab Brokerage", "value_in_base": "50000.00"}
  ]
}
```

## Calculation Flow

When `PortfolioService::calculate()` is called:

1. **Fetch all accounts** from storage

2. **For each account, get balances as of query date**
   - Query: "most recent balance for each asset at or before `as_of_date`"
   - Returns: `Vec<(Asset, Decimal, NaiveDate)>` — asset, amount, balance date

3. **Aggregate balances by asset** (across all accounts)
   - Build map: `Asset → Vec<(AccountId, Decimal, NaiveDate)>`

4. **For each unique asset, get valuation data:**
   - **Currencies**: Get FX rate to target currency (if different)
   - **Equities/Crypto**: Get price, then FX rate if price currency differs from target
   - Lookup: "most recent price/rate at or before `as_of_date`"
   - Apply `RefreshPolicy`: fetch if stale or forced

5. **Calculate values:**
   - For each asset: `value = amount × price × fx_rate`
   - Sum by asset (for `by_asset`)
   - Sum by account (for `by_account`)
   - Sum total

6. **Build and return `PortfolioSnapshot`**

## Date Handling

Two independent lookups against the query date:

- **Balance date**: Most recent balance for each asset at or before query date
- **Price/FX date**: Most recent price/rate at or before query date

These are decoupled. Example: querying portfolio for Feb 5th might use a balance from Feb 1st and a price from Feb 3rd.

## Refresh Policy

- **Default (CachedOnly)**: Use only prices/rates already in storage; no network calls
- **`--refresh` (IfStale)**: Fetch if data is older than `--stale-after` threshold (default 1 day)
- **`--force-refresh` (Force)**: Always fetch fresh data from providers

## File Changes

**New files:**
- `src/portfolio/mod.rs` — Module exports
- `src/portfolio/service.rs` — `PortfolioService` implementation
- `src/portfolio/models.rs` — `PortfolioSnapshot`, `AssetSummary`, etc.

**Modified files:**
- `src/main.rs` — Add `portfolio` command, wire up CLI args to service
- `src/lib.rs` — Export `portfolio` module

**Removed/deprecated:**
- `src/market_data/valuation.rs` — `NetWorthCalculator` (remove after new implementation works)

## Dependencies

Uses existing code:
- `Storage` trait for fetching accounts/balances
- `MarketDataService` for price/FX lookups (may need minor extensions for refresh policy)
- `Asset`, `Balance`, `Account` models unchanged
