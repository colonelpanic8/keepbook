# Symlink Directories Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add human-readable symlink directories for connections (`by-name/`) and accounts (within each connection).

**Architecture:** Add symlink management functions to `JsonFileStorage`, integrate with save/delete operations, and add `sync symlinks` CLI command.

**Tech Stack:** Rust, tokio::fs, std::os::unix::fs::symlink

---

### Task 1: Add name sanitization helper

**Files:**
- Modify: `src/storage/json_file.rs`

**Step 1: Add the sanitize_name function**

Add this function near the top of the impl block (after the path helper methods):

```rust
/// Sanitize a name for use as a symlink filename.
/// Returns None if the result would be empty.
fn sanitize_name(name: &str) -> Option<String> {
    let sanitized: String = name
        .trim()
        .chars()
        .map(|c| if c == '/' || c == '\0' { '-' } else { c })
        .collect();
    if sanitized.is_empty() {
        None
    } else {
        Some(sanitized)
    }
}
```

**Step 2: Build and verify no compilation errors**

Run: `cargo build`
Expected: Compiles successfully (warning about unused function is fine)

**Step 3: Commit**

```bash
git add src/storage/json_file.rs
git commit -m "feat(storage): add name sanitization helper for symlinks"
```

---

### Task 2: Add connections by-name symlink directory path helper

**Files:**
- Modify: `src/storage/json_file.rs`

**Step 1: Add the path helper**

Add this method to the `impl JsonFileStorage` block, near the other path helpers:

```rust
fn connections_by_name_dir(&self) -> PathBuf {
    self.connections_dir().join("by-name")
}
```

**Step 2: Build and verify**

Run: `cargo build`
Expected: Compiles successfully

**Step 3: Commit**

```bash
git add src/storage/json_file.rs
git commit -m "feat(storage): add connections by-name directory path helper"
```

---

### Task 3: Implement rebuild_connection_symlinks

**Files:**
- Modify: `src/storage/json_file.rs`

**Step 1: Add the rebuild_connection_symlinks method**

Add this public async method to the `impl JsonFileStorage` block:

```rust
/// Rebuild all symlinks in connections/by-name/ directory.
/// Removes stale symlinks and creates symlinks for all connections by name.
/// Returns the number of symlinks created and warnings for collisions.
pub async fn rebuild_connection_symlinks(&self) -> Result<(usize, Vec<String>)> {
    let by_name_dir = self.connections_by_name_dir();

    // Create by-name directory if needed
    fs::create_dir_all(&by_name_dir)
        .await
        .context("Failed to create connections/by-name directory")?;

    // Remove all existing symlinks in by-name/
    let mut entries = match fs::read_dir(&by_name_dir).await {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok((0, Vec::new()));
        }
        Err(e) => return Err(e).context("Failed to read by-name directory"),
    };

    while let Some(entry) = entries.next_entry().await? {
        if let Ok(file_type) = entry.file_type().await {
            if file_type.is_symlink() {
                let _ = fs::remove_file(entry.path()).await;
            }
        }
    }

    // Load all connections and create symlinks
    let connections = self.list_connections().await?;
    let mut created = 0;
    let mut warnings = Vec::new();
    let mut seen_names: std::collections::HashMap<String, Id> = std::collections::HashMap::new();

    for conn in connections {
        let name = conn.name();
        let Some(sanitized) = sanitize_name(name) else {
            warnings.push(format!("Skipped connection with empty name (id: {})", conn.id()));
            continue;
        };

        if let Some(existing_id) = seen_names.get(&sanitized) {
            warnings.push(format!(
                "Skipped duplicate connection name \"{}\" (id: {}, conflicts with {})",
                sanitized, conn.id(), existing_id
            ));
            continue;
        }

        let link_path = by_name_dir.join(&sanitized);
        let target = PathBuf::from("..").join(conn.id().to_string());

        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            if let Err(e) = symlink(&target, &link_path) {
                warnings.push(format!(
                    "Failed to create symlink for \"{}\": {}",
                    sanitized, e
                ));
                continue;
            }
        }

        seen_names.insert(sanitized, conn.id().clone());
        created += 1;
    }

    Ok((created, warnings))
}
```

**Step 2: Build and verify**

Run: `cargo build`
Expected: Compiles successfully

**Step 3: Commit**

```bash
git add src/storage/json_file.rs
git commit -m "feat(storage): implement rebuild_connection_symlinks"
```

---

### Task 4: Update existing update_account_symlinks to use names

**Files:**
- Modify: `src/storage/json_file.rs`

**Step 1: Replace the existing update_account_symlinks method**

Replace the existing `update_account_symlinks` method with this version that uses account names instead of IDs:

```rust
/// Update symlinks from connection's accounts/ dir to the actual account directories.
///
/// Creates symlinks like:
///   connections/{conn-id}/accounts/{account-name} -> ../../../accounts/{account-id}
async fn update_account_symlinks(&self, conn: &Connection) -> Result<()> {
    let accounts_dir = self.connection_accounts_dir(conn.id());

    // Create or ensure the accounts directory exists
    fs::create_dir_all(&accounts_dir)
        .await
        .context("Failed to create connection accounts directory")?;

    // Remove all existing symlinks
    if let Ok(mut entries) = fs::read_dir(&accounts_dir).await {
        while let Ok(Some(entry)) = entries.next_entry().await {
            if let Ok(file_type) = entry.file_type().await {
                if file_type.is_symlink() {
                    let _ = fs::remove_file(entry.path()).await;
                }
            }
        }
    }

    // Load accounts and create symlinks by name
    let mut seen_names: std::collections::HashSet<String> = std::collections::HashSet::new();

    for account_id in &conn.state.account_ids {
        let account = match self.get_account(account_id).await? {
            Some(a) => a,
            None => continue,
        };

        let Some(sanitized) = sanitize_name(&account.name) else {
            log::warn!(
                "Skipped account with empty name (id: {}, connection: {})",
                account_id, conn.id()
            );
            continue;
        };

        if seen_names.contains(&sanitized) {
            log::warn!(
                "Skipped duplicate account name \"{}\" (id: {}, connection: {})",
                sanitized, account_id, conn.id()
            );
            continue;
        }

        let link_path = accounts_dir.join(&sanitized);
        // Relative path: ../../../accounts/{account-id}
        let target = PathBuf::from("../../../accounts").join(account_id.to_string());

        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            // Ignore errors - log them
            if let Err(e) = symlink(&target, &link_path) {
                log::warn!(
                    "Failed to create symlink for account \"{}\": {}",
                    sanitized, e
                );
                continue;
            }
        }

        seen_names.insert(sanitized);
    }

    Ok(())
}
```

**Step 2: Build and verify**

Run: `cargo build`
Expected: Compiles successfully

**Step 3: Commit**

```bash
git add src/storage/json_file.rs
git commit -m "feat(storage): update account symlinks to use names instead of IDs"
```

---

### Task 5: Implement rebuild_all_symlinks

**Files:**
- Modify: `src/storage/json_file.rs`

**Step 1: Add the rebuild_all_symlinks method**

Add this public async method to the `impl JsonFileStorage` block:

```rust
/// Rebuild all symlinks: connections/by-name/ and all connection accounts/ directories.
/// Returns (connections_created, accounts_created, warnings).
pub async fn rebuild_all_symlinks(&self) -> Result<(usize, usize, Vec<String>)> {
    let (conn_created, mut warnings) = self.rebuild_connection_symlinks().await?;

    let connections = self.list_connections().await?;
    let mut account_created = 0;

    for conn in connections {
        if let Err(e) = self.update_account_symlinks(&conn).await {
            warnings.push(format!(
                "Failed to update account symlinks for connection {}: {}",
                conn.id(), e
            ));
            continue;
        }
        account_created += conn.state.account_ids.len();
    }

    Ok((conn_created, account_created, warnings))
}
```

**Step 2: Build and verify**

Run: `cargo build`
Expected: Compiles successfully

**Step 3: Commit**

```bash
git add src/storage/json_file.rs
git commit -m "feat(storage): implement rebuild_all_symlinks"
```

---

### Task 6: Add automatic symlink rebuild on connection create/delete

**Files:**
- Modify: `src/storage/json_file.rs`

**Step 1: Update save_connection to rebuild connection symlinks**

The existing `save_connection` already calls `update_account_symlinks`. Update it to also rebuild connection symlinks. Replace the `save_connection` implementation in the `impl Storage for JsonFileStorage` block:

```rust
async fn save_connection(&self, conn: &Connection) -> Result<()> {
    // Only save state - config is human-managed
    self.write_json(&self.connection_state_file(conn.id()), &conn.state).await?;
    self.update_account_symlinks(conn).await?;
    // Rebuild connection by-name symlinks (handles creates and name changes)
    let _ = self.rebuild_connection_symlinks().await;
    Ok(())
}
```

**Step 2: Update delete_connection to rebuild connection symlinks**

Replace the `delete_connection` implementation:

```rust
async fn delete_connection(&self, id: &Id) -> Result<bool> {
    let dir = self.connection_dir(id);
    if dir.exists() {
        fs::remove_dir_all(&dir)
            .await
            .with_context(|| format!("Failed to delete connection directory: {}", dir.display()))?;
        // Rebuild symlinks to remove stale one
        let _ = self.rebuild_connection_symlinks().await;
        Ok(true)
    } else {
        Ok(false)
    }
}
```

**Step 3: Build and verify**

Run: `cargo build`
Expected: Compiles successfully

**Step 4: Commit**

```bash
git add src/storage/json_file.rs
git commit -m "feat(storage): auto-rebuild connection symlinks on save/delete"
```

---

### Task 7: Add sync symlinks CLI command

**Files:**
- Modify: `src/main.rs`

**Step 1: Add Symlinks variant to SyncCommand enum**

Find the `SyncCommand` enum (around line 105) and add a new variant:

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
    /// Rebuild all symlinks (connections/by-name and account directories)
    Symlinks,
}
```

**Step 2: Add the match arm for Symlinks in main()**

Find the match for `SyncCommand` (around line 317) and add the new arm. The existing code looks like:

```rust
Some(Command::Sync(sync_cmd)) => match sync_cmd {
    SyncCommand::Connection { id_or_name, if_stale } => {
        ...
    }
    SyncCommand::All { if_stale } => {
        ...
    }
}
```

Add this arm after `SyncCommand::All`:

```rust
SyncCommand::Symlinks => {
    let (conn_created, acct_created, warnings) = storage.rebuild_all_symlinks().await?;
    for warning in &warnings {
        eprintln!("Warning: {}", warning);
    }
    let result = serde_json::json!({
        "connection_symlinks_created": conn_created,
        "account_symlinks_created": acct_created,
        "warnings": warnings.len()
    });
    println!("{}", serde_json::to_string_pretty(&result)?);
}
```

**Step 3: Build and verify**

Run: `cargo build`
Expected: Compiles successfully

**Step 4: Test the command**

Run: `cargo run -- sync symlinks`
Expected: JSON output showing symlinks created

**Step 5: Commit**

```bash
git add src/main.rs
git commit -m "feat(cli): add 'sync symlinks' command"
```

---

### Task 8: Manual integration test

**Files:** None (manual testing)

**Step 1: Verify by-name symlinks work**

Run: `ls -la data/connections/by-name/`
Expected: Symlinks pointing to connection UUID directories

**Step 2: Verify account symlinks work**

Run: `ls -la data/connections/*/accounts/`
Expected: Symlinks with account names pointing to account UUID directories

**Step 3: Verify symlinks are functional**

Run: `cat data/connections/by-name/<connection-name>/connection.toml`
Expected: Connection config displayed (symlink resolves correctly)

---

### Task 9: Final cleanup and commit

**Files:**
- Modify: `docs/plans/2026-02-03-symlink-directories-design.md`

**Step 1: Update design doc status**

Add a status line at the top of the design doc:

```markdown
**Status:** Implemented
```

**Step 2: Run clippy**

Run: `cargo clippy`
Expected: No errors (warnings are OK)

**Step 3: Commit**

```bash
git add docs/plans/2026-02-03-symlink-directories-design.md
git commit -m "docs: mark symlink directories design as implemented"
```
