/**
 * Mutation commands for the CLI.
 *
 * Each function takes a Storage (and optional injectable deps) and returns
 * plain result objects. Errors are returned as `{success: false, error: ...}`,
 * NOT thrown, so the CLI can render them as JSON.
 */

import { type Storage } from '../storage/storage.js';
import { Connection, type ConnectionType } from '../models/connection.js';
import { Account } from '../models/account.js';
import { AssetBalance, BalanceSnapshot } from '../models/balance.js';
import { Id } from '../models/id.js';
import { type IdGenerator, UuidIdGenerator } from '../models/id-generator.js';
import { type Clock, SystemClock } from '../clock.js';
import { parseAsset, formatRfc3339, decStr } from './format.js';
import { Decimal } from '../decimal.js';
import {
  TransactionAnnotationPatch,
  applyTransactionAnnotationPatch,
  isEmptyTransactionAnnotation,
  type TransactionAnnotationPatchType,
  type TransactionAnnotationType,
} from '../models/transaction-annotation.js';
import { type BalanceBackfillPolicy } from '../models/account.js';
import { findAccount } from '../storage/lookup.js';

// ---------------------------------------------------------------------------
// addConnection
// ---------------------------------------------------------------------------

/**
 * Create a new connection.
 *
 * Checks for duplicate names (case-insensitive). Returns an error object
 * if a connection with the same name already exists.
 */
export function addConnection(
  storage: Storage,
  name: string,
  ids?: IdGenerator,
  clock?: Clock,
): Promise<object>;
export function addConnection(
  storage: Storage,
  name: string,
  synchronizer: string,
  ids?: IdGenerator,
  clock?: Clock,
): Promise<object>;
export async function addConnection(
  storage: Storage,
  name: string,
  synchronizerOrIds?: string | IdGenerator,
  idsOrClock?: IdGenerator | Clock,
  clock?: Clock,
): Promise<object> {
  // Backwards-compatible arg parsing:
  // Old: addConnection(storage, name, ids?, clock?)
  // New: addConnection(storage, name, synchronizer, ids?, clock?)
  let syncArg: string | undefined;
  let idsArg: IdGenerator | undefined;
  let clockArg: Clock | undefined;
  if (typeof synchronizerOrIds === 'string' || synchronizerOrIds === undefined) {
    syncArg = synchronizerOrIds;
    idsArg = idsOrClock as IdGenerator | undefined;
    clockArg = clock;
  } else {
    syncArg = undefined;
    idsArg = synchronizerOrIds;
    clockArg = idsOrClock as Clock | undefined;
  }

  // Check for duplicate name (case-insensitive)
  const existing = await storage.listConnections();
  const needle = name.toLowerCase();
  for (const conn of existing) {
    if (conn.config.name.toLowerCase() === needle) {
      return {
        success: false,
        error: `Connection with name '${name}' already exists`,
      };
    }
  }

  const syncName = typeof syncArg === 'string' && syncArg.trim() !== '' ? syncArg.trim() : 'manual';
  const conn = Connection.new(
    { name, synchronizer: syncName },
    idsArg ?? new UuidIdGenerator(),
    clockArg ?? new SystemClock(),
  );

  const connId = conn.state.id;
  await storage.saveConnectionConfig(connId, conn.config);
  await storage.saveConnection(conn);

  return {
    success: true,
    connection: {
      id: connId.asStr(),
      name,
      synchronizer: conn.config.synchronizer,
    },
  };
}

// ---------------------------------------------------------------------------
// addAccount
// ---------------------------------------------------------------------------

/**
 * Create a new account under an existing connection.
 *
 * Finds the connection by ID. If not found, returns an error object.
 * After creating the account, updates the connection's state.account_ids.
 */
export async function addAccount(
  storage: Storage,
  connectionIdStr: string,
  name: string,
  tags: string[],
  ids?: IdGenerator,
  clock?: Clock,
): Promise<object> {
  let connectionId: Id;
  try {
    connectionId = Id.fromStringChecked(connectionIdStr);
  } catch {
    return {
      success: false,
      error: `Invalid connection id: ${connectionIdStr}`,
    };
  }

  const conn = await storage.getConnection(connectionId);
  if (conn === null) {
    return {
      success: false,
      error: `Connection not found: '${connectionIdStr}'`,
    };
  }

  const account = Account.newWithGenerator(
    ids ?? new UuidIdGenerator(),
    clock ?? new SystemClock(),
    name,
    connectionId,
  );

  // Set tags on the account
  const accountWithTags = { ...account, tags: [...tags] };
  await storage.saveAccount(accountWithTags);

  // Update connection state's account_ids
  const updatedConn: ConnectionType = {
    config: conn.config,
    state: {
      ...conn.state,
      account_ids: [...conn.state.account_ids, accountWithTags.id],
    },
  };
  await storage.saveConnection(updatedConn);

  return {
    success: true,
    account: {
      id: accountWithTags.id.asStr(),
      name,
      connection_id: connectionIdStr,
    },
  };
}

// ---------------------------------------------------------------------------
// removeConnection
// ---------------------------------------------------------------------------

/**
 * Remove a connection and all its associated accounts.
 *
 * Finds the connection by ID string. If not found, returns an error object.
 * Deletes all accounts whose connection_id matches, then deletes the connection.
 */
export async function removeConnection(storage: Storage, idStr: string): Promise<object> {
  const id = Id.fromString(idStr);
  const conn = await storage.getConnection(id);
  if (conn === null) {
    return {
      success: false,
      error: 'Connection not found',
      id: idStr,
    };
  }

  // Find all accounts belonging to this connection
  const allAccounts = await storage.listAccounts();
  const matchingAccounts = allAccounts.filter((a) => a.connection_id.equals(id));

  const deletedAccountIds: string[] = [];
  for (const account of matchingAccounts) {
    await storage.deleteAccount(account.id);
    deletedAccountIds.push(account.id.asStr());
  }

  await storage.deleteConnection(id);

  return {
    success: true,
    connection: {
      id: id.asStr(),
      name: conn.config.name,
    },
    deleted_accounts: matchingAccounts.length,
    account_ids: deletedAccountIds,
  };
}

// ---------------------------------------------------------------------------
// setBalance
// ---------------------------------------------------------------------------

/**
 * Set a balance for an account.
 *
 * Finds the account by ID. Parses the asset string and validates the
 * amount as a valid decimal. Creates a balance snapshot at the current time
 * (or injected clock time).
 */
export async function setBalance(
  storage: Storage,
  accountIdStr: string,
  assetStr: string,
  amountStr: string,
  clock?: Clock,
): Promise<object> {
  let accountId: Id;
  try {
    accountId = Id.fromStringChecked(accountIdStr);
  } catch {
    return {
      success: false,
      error: `Invalid account id: ${accountIdStr}`,
    };
  }

  const account = await storage.getAccount(accountId);
  if (account === null) {
    return {
      success: false,
      error: `Account not found: '${accountIdStr}'`,
    };
  }

  let asset;
  try {
    asset = parseAsset(assetStr);
  } catch {
    return {
      success: false,
      error: `Invalid asset: '${assetStr}'`,
    };
  }

  let amount: Decimal;
  try {
    amount = new Decimal(amountStr);
  } catch {
    return {
      success: false,
      error: `Invalid amount: '${amountStr}'`,
    };
  }

  const balance = AssetBalance.new(asset, decStr(amount));
  const snapshot = BalanceSnapshot.nowWith(clock ?? new SystemClock(), [balance]);

  await storage.appendBalanceSnapshot(account.id, snapshot);

  return {
    success: true,
    balance: {
      account_id: account.id.asStr(),
      asset,
      amount: decStr(amount),
      timestamp: formatRfc3339(snapshot.timestamp),
    },
  };
}

function parseBalanceBackfillPolicy(value: string): BalanceBackfillPolicy | null {
  const normalized = value.trim().toLowerCase();
  if (normalized === 'none') return 'none';
  if (normalized === 'zero') return 'zero';
  if (normalized === 'carry_earliest' || normalized === 'carry-earliest') return 'carry_earliest';
  return null;
}

export async function setAccountConfig(
  storage: Storage,
  accountIdOrName: string,
  args: {
    balance_backfill?: string;
    clear_balance_backfill?: boolean;
  },
): Promise<object> {
  const balanceBackfill = args.balance_backfill;
  const clearBalanceBackfill = args.clear_balance_backfill ?? false;

  if (balanceBackfill !== undefined && clearBalanceBackfill) {
    return {
      success: false,
      error: 'Cannot use balance_backfill and clear_balance_backfill together',
    };
  }

  if (balanceBackfill === undefined && !clearBalanceBackfill) {
    return {
      success: false,
      error: 'No account config fields specified',
    };
  }

  const account = await findAccount(storage, accountIdOrName);
  if (account === null) {
    return {
      success: false,
      error: `Account not found: '${accountIdOrName}'`,
    };
  }

  const baseConfig = storage.getAccountConfig(account.id) ?? {};
  let nextBalanceBackfill = baseConfig.balance_backfill;
  if (clearBalanceBackfill) {
    nextBalanceBackfill = undefined;
  } else if (balanceBackfill !== undefined) {
    const policy = parseBalanceBackfillPolicy(balanceBackfill);
    if (policy === null) {
      return {
        success: false,
        error: `Invalid balance backfill policy: '${balanceBackfill}'. Use: none, zero, carry_earliest`,
      };
    }
    nextBalanceBackfill = policy;
  }

  const nextConfig = {
    ...baseConfig,
    ...(nextBalanceBackfill !== undefined
      ? { balance_backfill: nextBalanceBackfill }
      : { balance_backfill: undefined }),
  };

  await storage.saveAccountConfig(account.id, nextConfig);

  return {
    success: true,
    account: {
      id: account.id.asStr(),
      name: account.name,
    },
    config: {
      balance_backfill: nextConfig.balance_backfill ?? null,
    },
  };
}

export type SetTransactionAnnotationArgs = {
  description?: string;
  clear_description?: boolean;
  note?: string;
  clear_note?: boolean;
  category?: string;
  clear_category?: boolean;
  tags?: string[];
  tags_empty?: boolean;
  clear_tags?: boolean;
};

/**
 * Append a transaction annotation patch for a transaction in an account.
 *
 * This does not modify the underlying transaction record; it stores a separate
 * append-only patch stream.
 */
export async function setTransactionAnnotation(
  storage: Storage,
  accountIdStr: string,
  transactionIdStr: string,
  args: SetTransactionAnnotationArgs,
  clock?: Clock,
): Promise<object> {
  const description = args.description;
  const clearDescription = args.clear_description ?? false;
  const note = args.note;
  const clearNote = args.clear_note ?? false;
  const category = args.category;
  const clearCategory = args.clear_category ?? false;
  const tags = args.tags ?? [];
  const tagsEmpty = args.tags_empty ?? false;
  const clearTags = args.clear_tags ?? false;

  if (clearDescription && description !== undefined) {
    return { success: false, error: 'Cannot use description and clear_description together' };
  }
  if (clearNote && note !== undefined) {
    return { success: false, error: 'Cannot use note and clear_note together' };
  }
  if (clearCategory && category !== undefined) {
    return { success: false, error: 'Cannot use category and clear_category together' };
  }
  if (clearTags && (tagsEmpty || tags.length > 0)) {
    return { success: false, error: 'Cannot use clear_tags with tags/tags_empty' };
  }

  const hasChange =
    description !== undefined ||
    clearDescription ||
    note !== undefined ||
    clearNote ||
    category !== undefined ||
    clearCategory ||
    tags.length > 0 ||
    tagsEmpty ||
    clearTags;
  if (!hasChange) {
    return { success: false, error: 'No annotation fields specified' };
  }

  let accountId: Id;
  let transactionId: Id;
  try {
    accountId = Id.fromStringChecked(accountIdStr);
  } catch {
    return { success: false, error: `Invalid account id: ${accountIdStr}` };
  }
  try {
    transactionId = Id.fromStringChecked(transactionIdStr);
  } catch {
    return { success: false, error: `Invalid transaction id: ${transactionIdStr}` };
  }

  const account = await storage.getAccount(accountId);
  if (account === null) {
    return { success: false, error: `Account not found: '${accountIdStr}'` };
  }

  const txns = await storage.getTransactions(accountId);
  if (!txns.some((t) => t.id.equals(transactionId))) {
    return { success: false, error: `Transaction not found for account: '${transactionIdStr}'` };
  }

  const now = (clock ?? new SystemClock()).now();
  const patch: TransactionAnnotationPatchType = {
    transaction_id: transactionId,
    timestamp: now,
    ...(clearDescription ? { description: null } : description !== undefined ? { description } : {}),
    ...(clearNote ? { note: null } : note !== undefined ? { note } : {}),
    ...(clearCategory ? { category: null } : category !== undefined ? { category } : {}),
    ...(clearTags
      ? { tags: null }
      : tagsEmpty
        ? { tags: [] }
        : tags.length > 0
          ? { tags: [...tags] }
          : {}),
  };

  await storage.appendTransactionAnnotationPatches(accountId, [patch]);

  // Materialize current annotation state for the transaction.
  const patches = await storage.getTransactionAnnotationPatches(accountId);
  let ann: TransactionAnnotationType = { transaction_id: transactionId };
  for (const p of patches) {
    if (!p.transaction_id.equals(transactionId)) continue;
    ann = applyTransactionAnnotationPatch(ann, p);
  }

  const patchJson = TransactionAnnotationPatch.toJSON(patch);
  const patchOut: Record<string, unknown> = { timestamp: formatRfc3339(now) };
  if (patchJson.description !== undefined) patchOut.description = patchJson.description;
  if (patchJson.note !== undefined) patchOut.note = patchJson.note;
  if (patchJson.category !== undefined) patchOut.category = patchJson.category;
  if (patchJson.tags !== undefined) patchOut.tags = patchJson.tags;

  let annotationOut: Record<string, unknown> | null = null;
  if (!isEmptyTransactionAnnotation(ann)) {
    const m: Record<string, unknown> = {};
    if (ann.description !== undefined) m.description = ann.description;
    if (ann.note !== undefined) m.note = ann.note;
    if (ann.category !== undefined) m.category = ann.category;
    if (ann.tags !== undefined) m.tags = ann.tags;
    annotationOut = m;
  }

  return {
    success: true,
    account_id: account.id.asStr(),
    transaction_id: transactionIdStr,
    patch: patchOut,
    annotation: annotationOut,
  };
}
