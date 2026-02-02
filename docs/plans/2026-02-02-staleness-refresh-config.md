# Hierarchical Staleness/Refresh Configuration

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add configurable staleness thresholds for balances and prices, with auto-refresh when data is stale.

**Architecture:** Hierarchical config resolution (global → connection → account) for balance staleness; global-only config for price staleness (per asset-type). Staleness checked before fetching; only stale data is refreshed.

**Tech Stack:** Rust, serde for config parsing, tracing for staleness logging

---

## Configuration Structure

### Global Config (`keepbook.toml`)

```toml
data_dir = "data"
reporting_currency = "USD"

[refresh]
balance_staleness = "14d"    # default for all connections
price_staleness = "24h"      # default for all asset types (equity, crypto, fx)
```

### Connection Config (`data/connections/<id>/config.toml`)

```toml
name = "Schwab"
synchronizer = "schwab"
balance_staleness = "7d"     # override for this connection
```

### Account Config (`data/accounts/<id>/config.toml`) - New File

```toml
balance_staleness = "30d"    # override for this specific account
```

### Resolution Order (most specific wins)

1. Account's `balance_staleness`
2. Connection's `balance_staleness`
3. Global `refresh.balance_staleness`
4. Hardcoded default (`14d` for balances, `24h` for prices)

---

## Staleness Detection

### Balances

- Use connection's `last_sync.at` timestamp
- Stale if `now - last_sync.at > balance_staleness`
- If `last_sync` is `None`, always considered stale

### Prices

- Use `timestamp` field on cached `PricePoint`
- Stale if `now - price.timestamp > price_staleness`
- If no cached price exists, considered stale

### FX Rates

- Same as prices - use `timestamp` on cached `FxRatePoint`

---

## CLI Interface

### Portfolio Command

```bash
keepbook portfolio snapshot                 # auto-refresh stale data (default)
keepbook portfolio snapshot --auto          # explicit auto-refresh
keepbook portfolio snapshot --offline       # cache only, no network
keepbook portfolio snapshot --dry-run       # log staleness, use cache
keepbook portfolio snapshot --force-refresh # refresh everything
```

### Sync Command

```bash
keepbook sync connection schwab             # force sync (current behavior)
keepbook sync connection schwab --if-stale  # sync only if stale
```

### Dry-Run Output

Logs staleness status to stderr as structured JSON:

```json
{"level":"INFO","message":"stale check","connection":"Schwab","last_sync":"2026-01-20T...","staleness":"13d","threshold":"7d","status":"stale"}
{"level":"INFO","message":"stale check","asset":"equity/AAPL","last_price":"2026-02-01T...","staleness":"1d","threshold":"24h","status":"stale"}
```

---

## Default Values

| Type | Default |
|------|---------|
| Balance staleness | 14 days |
| Price staleness (all types) | 24 hours |

---

## Implementation

### Files to Modify

1. **`src/config.rs`** - Add `RefreshConfig` struct with global defaults
2. **`src/models/connection.rs`** - Add optional `balance_staleness` to `ConnectionConfig`
3. **`src/models/account.rs`** - Add optional `balance_staleness` and account config loading
4. **`src/portfolio/models.rs`** - Update `RefreshPolicy` with separate thresholds
5. **`src/portfolio/service.rs`** - Add staleness checking before fetching
6. **`src/main.rs`** - Update CLI flags, wire up new behavior

### New Module

**`src/staleness.rs`** - Centralizes staleness resolution:

```rust
pub fn resolve_balance_staleness(
    account: &Account,
    connection: &Connection,
    config: &RefreshConfig,
) -> Duration;

pub fn is_balance_stale(connection: &Connection, threshold: Duration) -> bool;

pub fn is_price_stale(price_point: Option<&PricePoint>, threshold: Duration) -> bool;
```

### Duration Parsing

Move existing `parse_duration` from `main.rs` to shared location. Add serde deserializer:

```rust
#[derive(Deserialize)]
pub struct RefreshConfig {
    #[serde(default = "default_balance_staleness", deserialize_with = "deserialize_duration")]
    pub balance_staleness: Duration,

    #[serde(default = "default_price_staleness", deserialize_with = "deserialize_duration")]
    pub price_staleness: Duration,
}
```

---

## Behavior Summary

| Flag | Sync Stale Balances | Fetch Stale Prices | Use Cache |
|------|---------------------|-------------------|-----------|
| (default) / `--auto` | Yes | Yes | For fresh data |
| `--offline` | No | No | Always |
| `--dry-run` | No (log only) | No (log only) | Always |
| `--force-refresh` | Yes (all) | Yes (all) | No |
