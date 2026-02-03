# Symlink Directories for Connections and Accounts

## Overview

Add human-readable symlink directories for navigating connections and accounts by name instead of UUID.

## Directory Structure

```
data/
├── connections/
│   ├── by-name/
│   │   ├── Charles Schwab -> ../abc123-uuid/
│   │   └── Coinbase -> ../def456-uuid/
│   ├── abc123-uuid/
│   │   ├── connection.toml
│   │   ├── connection.json
│   │   └── accounts/
│   │       ├── Checking -> ../../../accounts/acct-uuid-1/
│   │       └── Brokerage -> ../../../accounts/acct-uuid-2/
│   └── def456-uuid/
│       ├── connection.toml
│       ├── connection.json
│       └── accounts/
│           └── Main -> ../../../accounts/acct-uuid-3/
└── accounts/
    └── {account-uuid}/
        └── ...
```

- `connections/by-name/` contains symlinks named after connection names pointing to UUID directories
- Each connection's `accounts/` directory contains symlinks named after account names pointing to account UUID directories in `data/accounts/`
- Symlinks use relative paths for portability

## Implementation

### Storage Layer

Add functions to `src/storage/` (trait or JsonFileStorage):

```rust
fn rebuild_all_symlinks(&self) -> Result<()>
fn rebuild_connection_symlinks(&self) -> Result<()>
fn rebuild_account_symlinks_for_connection(&self, connection_id: &Id) -> Result<()>
```

### Automatic Triggers

Symlinks regenerate on create/delete operations:

- `save_connection()` - when creating a new connection
- `delete_connection()` - rebuild connection symlinks after deletion
- `save_account()` - when creating a new account, rebuild that connection's account symlinks
- `delete_account()` - rebuild that connection's account symlinks after deletion

### CLI Command

**Command:** `keepbook sync symlinks`

Behavior:
1. Remove all existing symlinks in `connections/by-name/` and all `connections/*/accounts/` directories
2. Rebuild all symlinks from current connection and account data
3. Report what was created and any warnings

Example output:
```
Rebuilding symlinks...
  connections/by-name/Charles Schwab -> abc123-uuid
  connections/by-name/Coinbase -> def456-uuid
  connections/abc123-uuid/accounts/Checking -> acct-uuid-1
  connections/abc123-uuid/accounts/Brokerage -> acct-uuid-2
  connections/def456-uuid/accounts/Main -> acct-uuid-3
Warning: Skipped duplicate connection name "Schwab" (id: ghi789-uuid)
Done. Created 5 symlinks, 1 warning.
```

## Edge Cases

### Name Collisions

If two connections have the same name, log a warning and skip the duplicate symlink. Same for accounts within a connection.

### Name Sanitization

- Replace `/` and `\0` with `-`
- Trim leading/trailing whitespace
- If result is empty, skip with warning

### Other Cases

- **Missing accounts directory** - Create `connections/{uuid}/accounts/` if it doesn't exist
- **Stale symlinks** - Rebuild process removes all symlinks first, cleaning up stale ones
- **Filesystem errors** - Log error and continue with remaining symlinks
