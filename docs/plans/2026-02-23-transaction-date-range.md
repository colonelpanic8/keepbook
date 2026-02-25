# Transaction Date Range Filter Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add `--start` and `--end` date options to `list transactions` that default to last 30 days.

**Architecture:** Filter transactions by comparing `tx.timestamp` against parsed `NaiveDate` bounds (start-of-day UTC). Both Rust and TypeScript implementations must be updated in lockstep.

**Tech Stack:** Rust (clap, chrono), TypeScript (commander, Date)

---

### Task 1: Rust — Add date args to `ListCommand::Transactions`

**Files:**
- Modify: `src/main.rs:428` (ListCommand enum)
- Modify: `src/main.rs:843-846` (match arm)

**Step 1: Add `start` and `end` fields to `ListCommand::Transactions`**

In `src/main.rs`, change:
```rust
    /// List all transactions
    Transactions,
```
to:
```rust
    /// List all transactions
    Transactions {
        /// Start date (YYYY-MM-DD, default: 30 days ago)
        #[arg(long)]
        start: Option<String>,

        /// End date (YYYY-MM-DD, default: today)
        #[arg(long)]
        end: Option<String>,
    },
```

**Step 2: Update match arm to pass args**

In `src/main.rs`, change:
```rust
ListCommand::Transactions => {
    let transactions = app::list_transactions(storage_arc.as_ref()).await?;
```
to:
```rust
ListCommand::Transactions { start, end } => {
    let transactions = app::list_transactions(storage_arc.as_ref(), start, end).await?;
```

**Step 3: Commit**

```bash
git add src/main.rs
git commit -m "feat(cli): add --start/--end args to list transactions"
```

---

### Task 2: Rust — Add date filtering to `list_transactions`

**Files:**
- Modify: `src/app/list.rs:163-211` (list_transactions fn)

**Step 1: Update function signature and add date parsing + filtering**

Add `use chrono::{NaiveDate, Utc, Duration};` to imports.

Change signature from:
```rust
pub async fn list_transactions(storage: &dyn Storage) -> Result<Vec<TransactionOutput>> {
```
to:
```rust
pub async fn list_transactions(
    storage: &dyn Storage,
    start: Option<String>,
    end: Option<String>,
) -> Result<Vec<TransactionOutput>> {
```

Add date parsing at the top of the function body:
```rust
    let end_date = match &end {
        Some(s) => NaiveDate::parse_from_str(s, "%Y-%m-%d")
            .with_context(|| format!("Invalid end date: {s}"))?,
        None => Utc::now().date_naive(),
    };
    let start_date = match &start {
        Some(s) => NaiveDate::parse_from_str(s, "%Y-%m-%d")
            .with_context(|| format!("Invalid start date: {s}"))?,
        None => end_date - Duration::days(30),
    };
```

In the `for tx in transactions` loop, add a filter before the push:
```rust
    let tx_date = tx.timestamp.date_naive();
    if tx_date < start_date || tx_date > end_date {
        continue;
    }
```

**Step 2: Verify it compiles**

Run: `cargo build`

**Step 3: Commit**

```bash
git add src/app/list.rs
git commit -m "feat(list): filter transactions by date range (default 30d)"
```

---

### Task 3: TypeScript — Add date options to CLI and listTransactions

**Files:**
- Modify: `ts/src/cli/main.ts:449-457` (list transactions command)
- Modify: `ts/src/app/list.ts:263-308` (listTransactions fn)

**Step 1: Add CLI options**

In `ts/src/cli/main.ts`, change:
```typescript
list
  .command('transactions')
  .description('List all transactions')
  .action(async () => {
    await runWithConfig(async (cfg) => {
      const storage = new JsonFileStorage(cfg.config.data_dir);
      return listTransactions(storage);
    });
  });
```
to:
```typescript
list
  .command('transactions')
  .description('List all transactions')
  .option('--start <date>', 'start date (YYYY-MM-DD, default: 30 days ago)')
  .option('--end <date>', 'end date (YYYY-MM-DD, default: today)')
  .action(async (opts: { start?: string; end?: string }) => {
    await runWithConfig(async (cfg) => {
      const storage = new JsonFileStorage(cfg.config.data_dir);
      return listTransactions(storage, opts.start, opts.end);
    });
  });
```

**Step 2: Update listTransactions function**

In `ts/src/app/list.ts`, change signature:
```typescript
export async function listTransactions(
  storage: Storage,
  startStr?: string,
  endStr?: string,
): Promise<TransactionOutput[]> {
```

Add date parsing at top of function body:
```typescript
  const endDate = endStr ? new Date(endStr + 'T00:00:00Z') : new Date();
  const startDate = startStr
    ? new Date(startStr + 'T00:00:00Z')
    : new Date(endDate.getTime() - 30 * 24 * 60 * 60 * 1000);
  // Normalize endDate to end-of-day for inclusive comparison
  const endMs = endStr
    ? new Date(endStr + 'T00:00:00Z').getTime() + 24 * 60 * 60 * 1000 - 1
    : endDate.getTime();
```

Add filter in the tx loop:
```typescript
    const txMs = tx.timestamp.getTime();
    if (txMs < startDate.getTime() || txMs > endMs) continue;
```

**Step 3: Run TS tests**

Run: `cd ts && yarn test`

**Step 4: Commit**

```bash
git add ts/src/cli/main.ts ts/src/app/list.ts
git commit -m "feat(ts): add --start/--end date range to list transactions"
```

---

### Task 4: Verify both implementations

**Step 1:** `cargo test`
**Step 2:** `cd ts && yarn test`
**Step 3:** Manual test: `cargo run -- list transactions --start 2025-01-01 --end 2025-12-31`
