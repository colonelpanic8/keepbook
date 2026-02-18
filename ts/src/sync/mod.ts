/**
 * Sync types and interfaces.
 *
 * Port of the Rust `sync/mod.rs` module. Defines the core types for
 * synchronization: SyncedAssetBalance, SyncResult, AuthStatus, and the
 * Synchronizer / InteractiveAuth interfaces.
 */

import { type AssetBalanceType, BalanceSnapshot } from '../models/balance.js';
import { type PricePoint } from '../market-data/models.js';
import { type ConnectionType } from '../models/connection.js';
import { type AccountType } from '../models/account.js';
import { type TransactionType } from '../models/transaction.js';
import { type Id } from '../models/id.js';
import { type Storage } from '../storage/storage.js';
import { type Clock, SystemClock } from '../clock.js';
import { Asset } from '../models/asset.js';

// ---------------------------------------------------------------------------
// AuthStatus
// ---------------------------------------------------------------------------

/** Authentication status for synchronizers requiring interactive auth. */
export type AuthStatus =
  | { type: 'valid' }
  | { type: 'missing' }
  | { type: 'expired'; reason: string };

// ---------------------------------------------------------------------------
// SyncedAssetBalance
// ---------------------------------------------------------------------------

/** An asset balance paired with optional price data from the synchronizer. */
export interface SyncedAssetBalance {
  readonly asset_balance: AssetBalanceType;
  readonly price?: PricePoint;
}

/** Factory functions for SyncedAssetBalance. */
export const SyncedAssetBalanceFactory = {
  /** Create a SyncedAssetBalance without price data. */
  new(assetBalance: AssetBalanceType): SyncedAssetBalance {
    return { asset_balance: assetBalance };
  },

  /** Return a new SyncedAssetBalance with the given price set. */
  withPrice(sab: SyncedAssetBalance, price: PricePoint): SyncedAssetBalance {
    return { ...sab, price };
  },
} as const;

// ---------------------------------------------------------------------------
// SyncResult
// ---------------------------------------------------------------------------

/** Result of a sync operation. */
export interface SyncResult {
  readonly connection: ConnectionType;
  readonly accounts: AccountType[];
  readonly balances: Array<[Id, SyncedAssetBalance[]]>;
  readonly transactions: Array<[Id, TransactionType[]]>;
}

// ---------------------------------------------------------------------------
// SyncOptions
// ---------------------------------------------------------------------------

export type TransactionSyncMode = 'auto' | 'full';

export interface SyncOptions {
  readonly transactions: TransactionSyncMode;
}

export const DefaultSyncOptions: SyncOptions = { transactions: 'auto' } as const;

// ---------------------------------------------------------------------------
// saveSyncResult
// ---------------------------------------------------------------------------

/**
 * Compare two unknown values for deep equality (used for synchronizer_data).
 */
function deepEqual(a: unknown, b: unknown): boolean {
  if (a === b) return true;
  if (a === null || b === null) return a === b;
  if (typeof a !== typeof b) return false;
  if (typeof a !== 'object') return a === b;

  // Both are objects (and non-null)
  const objA = a as Record<string, unknown>;
  const objB = b as Record<string, unknown>;

  const keysA = Object.keys(objA);
  const keysB = Object.keys(objB);
  if (keysA.length !== keysB.length) return false;

  for (const key of keysA) {
    if (!Object.prototype.hasOwnProperty.call(objB, key)) return false;
    if (!deepEqual(objA[key], objB[key])) return false;
  }
  return true;
}

/**
 * Check if two transactions are identical for the fields we care about.
 * Matches the Rust logic: timestamp, amount, asset, description, status, synchronizer_data.
 */
function transactionsEqual(a: TransactionType, b: TransactionType): boolean {
  return (
    a.timestamp.getTime() === b.timestamp.getTime() &&
    a.amount === b.amount &&
    Asset.equals(a.asset, b.asset) &&
    a.description === b.description &&
    a.status === b.status &&
    deepEqual(a.synchronizer_data, b.synchronizer_data)
  );
}

/**
 * Save a sync result to storage.
 *
 * Matches the Rust `SyncResult::save_with_clock` logic:
 * 1. Save all accounts
 * 2. Save the connection
 * 3. For each (account_id, synced_balances): if non-empty, extract asset_balances,
 *    create BalanceSnapshot using clock, append to storage
 * 4. For each (account_id, txns): if non-empty, do idempotent append with dedup
 */
export async function saveSyncResult(
  result: SyncResult,
  storage: Storage,
  clock?: Clock,
): Promise<void> {
  const effectiveClock = clock ?? new SystemClock();

  // 1. Save all accounts
  for (const account of result.accounts) {
    await storage.saveAccount(account);
  }

  // 2. Save the connection
  await storage.saveConnection(result.connection);

  // 3. Save balance snapshots
  for (const [accountId, syncedBalances] of result.balances) {
    if (syncedBalances.length > 0) {
      const assetBalances = syncedBalances.map((sb) => sb.asset_balance);
      const snapshot = BalanceSnapshot.nowWith(effectiveClock, assetBalances);
      await storage.appendBalanceSnapshot(accountId, snapshot);
    }
  }

  // 4. Save transactions with idempotent dedup logic
  for (const [accountId, txns] of result.transactions) {
    if (txns.length > 0) {
      // Get existing transactions (deduplicated view)
      const existing = await storage.getTransactions(accountId);
      const existingById = new Map<string, TransactionType>();
      for (const t of existing) {
        existingById.set(t.id.asStr(), t);
      }

      // Collapse duplicates within this batch: last write wins, preserve first-seen order
      const candidateTxns: TransactionType[] = [];
      const idxById = new Map<string, number>();
      for (const txn of txns) {
        const key = txn.id.asStr();
        const existingIdx = idxById.get(key);
        if (existingIdx !== undefined) {
          candidateTxns[existingIdx] = txn;
        } else {
          idxById.set(key, candidateTxns.length);
          candidateTxns.push(txn);
        }
      }

      // Only append transactions that are actually new or changed
      const toAppend: TransactionType[] = [];
      for (const txn of candidateTxns) {
        const existingTxn = existingById.get(txn.id.asStr());
        if (existingTxn !== undefined) {
          if (transactionsEqual(existingTxn, txn)) {
            continue;
          }
        }
        toAppend.push(txn);
      }

      if (toAppend.length > 0) {
        await storage.appendTransactions(accountId, toAppend);
      }
    }
  }
}

// ---------------------------------------------------------------------------
// Synchronizer interface
// ---------------------------------------------------------------------------

/** Trait for synchronizers - fetches data from external sources. */
export interface Synchronizer {
  /** Human-readable name for this synchronizer. */
  name(): string;

  /** Perform a full sync, returning all accounts, balances, and transactions. */
  sync(connection: ConnectionType, storage: Storage): Promise<SyncResult>;

  /**
   * Perform a sync with options.
   *
   * If omitted, callers should fall back to `sync`.
   */
  syncWithOptions?(connection: ConnectionType, storage: Storage, options: SyncOptions): Promise<SyncResult>;

  /** Return interactive auth support if this synchronizer needs it. */
  interactive?(): InteractiveAuth;
}

// ---------------------------------------------------------------------------
// InteractiveAuth interface
// ---------------------------------------------------------------------------

/** Trait for synchronizers that require interactive (browser-based) authentication. */
export interface InteractiveAuth extends Synchronizer {
  /**
   * Whether auth is required before running `sync`.
   *
   * Some synchronizers (e.g. Chase) can proceed without a cached session because
   * sync itself is interactive and the user will log in during the flow.
   */
  authRequiredForSync?(): boolean;

  /** Check if the current authentication is valid. */
  checkAuth(): Promise<AuthStatus>;

  /** Perform interactive login (typically opens a browser). */
  login(): Promise<void>;
}
