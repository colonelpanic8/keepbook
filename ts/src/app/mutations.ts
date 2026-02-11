/**
 * Mutation commands for the CLI.
 *
 * Each function takes a Storage (and optional injectable deps) and returns
 * plain result objects. Errors are returned as `{success: false, error: ...}`,
 * NOT thrown, so the CLI can render them as JSON.
 */

import { type Storage } from '../storage/storage.js';
import { findConnection, findAccount } from '../storage/lookup.js';
import { Connection, type ConnectionType } from '../models/connection.js';
import { Account } from '../models/account.js';
import { AssetBalance, BalanceSnapshot } from '../models/balance.js';
import { Id } from '../models/id.js';
import { type IdGenerator, UuidIdGenerator } from '../models/id-generator.js';
import { type Clock, SystemClock } from '../clock.js';
import { parseAsset, formatRfc3339, decStr } from './format.js';
import { Decimal } from '../decimal.js';

// ---------------------------------------------------------------------------
// addConnection
// ---------------------------------------------------------------------------

/**
 * Create a new manual connection.
 *
 * Checks for duplicate names (case-insensitive). Returns an error object
 * if a connection with the same name already exists.
 */
export async function addConnection(
  storage: Storage,
  name: string,
  ids?: IdGenerator,
  clock?: Clock,
): Promise<object> {
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

  const conn = Connection.new(
    { name, synchronizer: 'manual' },
    ids ?? new UuidIdGenerator(),
    clock ?? new SystemClock(),
  );

  const connId = conn.state.id;
  await storage.saveConnectionConfig(connId, conn.config);
  await storage.saveConnection(conn);

  return {
    success: true,
    connection: {
      id: connId.asStr(),
      name,
      synchronizer: 'manual',
    },
  };
}

// ---------------------------------------------------------------------------
// addAccount
// ---------------------------------------------------------------------------

/**
 * Create a new account under an existing connection.
 *
 * Finds the connection by ID or name. If not found, returns an error object.
 * After creating the account, updates the connection's state.account_ids.
 */
export async function addAccount(
  storage: Storage,
  connectionIdOrName: string,
  name: string,
  tags: string[],
  ids?: IdGenerator,
  clock?: Clock,
): Promise<object> {
  const conn = await findConnection(storage, connectionIdOrName);
  if (conn === null) {
    return {
      success: false,
      error: `Connection not found: '${connectionIdOrName}'`,
    };
  }

  const connectionId = conn.state.id;
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
      connection_id: connectionId.asStr(),
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
 * Finds the account by ID or name. Parses the asset string and validates the
 * amount as a valid decimal. Creates a balance snapshot at the current time
 * (or injected clock time).
 */
export async function setBalance(
  storage: Storage,
  accountIdOrName: string,
  assetStr: string,
  amountStr: string,
  clock?: Clock,
): Promise<object> {
  const account = await findAccount(storage, accountIdOrName);
  if (account === null) {
    return {
      success: false,
      error: `Account not found: '${accountIdOrName}'`,
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
