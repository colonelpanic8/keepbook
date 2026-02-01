# Synchronizer CLI Integration Design

## Overview

Move Schwab and Coinbase synchronizers from examples into the main library, and add CLI commands to invoke them.

## Library Structure

```
src/sync/
├── mod.rs              # Synchronizer trait, SyncResult, SyncedBalance, InteractiveAuth
├── orchestrator.rs     # SyncOrchestrator
└── synchronizers/
    ├── mod.rs          # exports, create_synchronizer() lookup
    ├── schwab.rs       # SchwabClient + SchwabSynchronizer + browser automation
    └── coinbase.rs     # CoinbaseSynchronizer
```

The existing `src/sync/schwab.rs` moves to `src/sync/synchronizers/schwab.rs`.

## CLI Commands

```rust
#[derive(Subcommand)]
enum Command {
    // ... existing commands ...

    /// Sync data from connections
    #[command(subcommand)]
    Sync(SyncCommand),

    /// Schwab-specific commands
    #[command(subcommand)]
    Schwab(SchwabCommand),

    /// Coinbase-specific commands
    #[command(subcommand)]
    Coinbase(CoinbaseCommand),
}

#[derive(Subcommand)]
enum SyncCommand {
    /// Sync a specific connection
    Connection {
        /// Connection ID or name
        id_or_name: String,
    },
    /// Sync all connections
    All,
}

#[derive(Subcommand)]
enum SchwabCommand {
    /// Login via browser to capture session
    Login {
        /// Connection ID or name (optional if only one Schwab connection)
        id_or_name: Option<String>,
    },
}

#[derive(Subcommand)]
enum CoinbaseCommand {
    // TBD - credentials come from connection config
}
```

**Command behavior:**
- `keepbook sync connection <id-or-name>` - looks up by ID then name, calls synchronizer
- `keepbook sync all` - iterates all connections, syncs each
- `keepbook schwab login` - finds the one Schwab connection, opens browser
- `keepbook schwab login <id-or-name>` - explicit when multiple Schwab connections exist

## Synchronizer Lookup

```rust
pub async fn create_synchronizer(
    connection: &Connection,
    storage: &dyn Storage,
) -> Result<Box<dyn Synchronizer>> {
    match connection.config.synchronizer.as_str() {
        "schwab" => Ok(Box::new(SchwabSynchronizer::new(connection, storage).await?)),
        "coinbase" => Ok(Box::new(CoinbaseSynchronizer::new(connection, storage).await?)),
        other => Err(anyhow!("Unknown synchronizer: {}", other)),
    }
}
```

## Interactive Auth

For synchronizers requiring browser-based authentication (like Schwab):

```rust
#[async_trait]
pub trait InteractiveAuth {
    async fn check_auth(&self) -> Result<AuthStatus>;
    async fn login(&mut self) -> Result<()>;
}

pub enum AuthStatus {
    Valid,
    Missing,
    Expired { reason: String },
}
```

**Sync flow with auth check:**

1. Look up connection by ID or name
2. Call `create_synchronizer()` to get the synchronizer
3. If synchronizer implements `InteractiveAuth`, call `check_auth()`
4. If `AuthStatus::Missing` or `AuthStatus::Expired`:
   - Prompt: "Session expired. Run login now? [Y/n]"
   - If yes, call `synchronizer.login()` (opens browser)
   - If no, exit with error
5. Call `synchronizer.sync()`

## Synchronizer Implementations

### SchwabSynchronizer

- Reuses existing `SchwabClient` for API calls
- Loads session from `SessionCache` (credentials module)
- Implements `Synchronizer` trait
- Implements `InteractiveAuth` trait for browser login
- Browser automation (Chrome CDP) moves from example

### CoinbaseSynchronizer

- JWT generation and API calls (from example)
- Loads credentials from connection's credential store (`key-name`, `private-key`)
- Implements `Synchronizer` trait only (no interactive auth)

## File Changes Summary

**Create:**
- `src/sync/synchronizers/mod.rs`
- `src/sync/synchronizers/coinbase.rs`

**Move:**
- `src/sync/schwab.rs` → `src/sync/synchronizers/schwab.rs`

**Modify:**
- `src/sync/mod.rs` - add `synchronizers` module, `InteractiveAuth` trait
- `src/main.rs` - add `Sync`, `Schwab`, `Coinbase` subcommands

**Delete (after migration):**
- `examples/coinbase.rs`
- `examples/schwab.rs`
