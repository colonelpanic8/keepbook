/**
 * Sync and auth stub commands for the CLI.
 *
 * Real synchronizer implementations (Schwab, Chase, Coinbase) are not yet
 * available in the TypeScript library. These stubs return the correct output
 * shapes so the CLI can wire them up now.
 */

import { type Storage } from '../storage/storage.js';
import { findConnection } from '../storage/lookup.js';
import type { RefreshConfig } from '../config.js';
import { checkBalanceStaleness, resolveBalanceStaleness } from '../staleness.js';
import type { ConnectionType } from '../models/connection.js';

function syncConnectionImpl(conn: ConnectionType): object {
  if (conn.config.synchronizer === 'manual') {
    return {
      success: true,
      skipped: true,
      reason: 'manual',
      connection: {
        id: conn.state.id.asStr(),
        name: conn.config.name,
      },
      accounts_synced: 0,
      prices_stored: 0,
      last_sync: null,
    };
  }

  return {
    success: false,
    error: `Synchronizer '${conn.config.synchronizer}' not implemented in TypeScript CLI`,
    connection: {
      id: conn.state.id.asStr(),
      name: conn.config.name,
    },
  };
}

// ---------------------------------------------------------------------------
// syncConnection
// ---------------------------------------------------------------------------

/**
 * Attempt to sync a single connection by ID or name.
 *
 * - Manual connections are skipped with a descriptive result.
 * - Other synchronizers return a "not implemented" error.
 * - Unknown connections return a "not found" error.
 */
export async function syncConnection(storage: Storage, idOrName: string): Promise<object> {
  const conn = await findConnection(storage, idOrName);

  if (conn === null) {
    return { success: false, error: `Connection not found: '${idOrName}'` };
  }

  return syncConnectionImpl(conn);
}

/**
 * Attempt to sync a single connection only if it's stale.
 *
 * Uses balance staleness threshold resolution:
 * connection override -> global refresh config.
 */
export async function syncConnectionIfStale(
  storage: Storage,
  idOrName: string,
  refresh: RefreshConfig,
): Promise<object> {
  const conn = await findConnection(storage, idOrName);

  if (conn === null) {
    return { success: false, error: `Connection not found: '${idOrName}'` };
  }

  const threshold = resolveBalanceStaleness(null, conn, refresh);
  const check = checkBalanceStaleness(conn, threshold);
  if (!check.is_stale) {
    return {
      success: true,
      skipped: true,
      reason: 'not stale',
      connection: conn.config.name,
    };
  }

  return syncConnectionImpl(conn);
}

// ---------------------------------------------------------------------------
// syncAll
// ---------------------------------------------------------------------------

/**
 * Sync every connection. Collects per-connection results.
 */
export async function syncAll(storage: Storage): Promise<object> {
  const connections = await storage.listConnections();
  const results: object[] = [];

  for (const conn of connections) {
    results.push(syncConnectionImpl(conn));
  }

  return { results, total: connections.length };
}

/**
 * Sync every connection only if stale. Fresh connections are skipped.
 */
export async function syncAllIfStale(storage: Storage, refresh: RefreshConfig): Promise<object> {
  const connections = await storage.listConnections();
  const results: object[] = [];

  for (const conn of connections) {
    const threshold = resolveBalanceStaleness(null, conn, refresh);
    const check = checkBalanceStaleness(conn, threshold);
    if (!check.is_stale) {
      results.push({
        success: true,
        skipped: true,
        reason: 'not stale',
        connection: conn.config.name,
      });
      continue;
    }
    results.push(syncConnectionImpl(conn));
  }

  return { results, total: connections.length };
}

// ---------------------------------------------------------------------------
// syncPrices
// ---------------------------------------------------------------------------

/**
 * Stub: price sync is not yet implemented.
 */
export async function syncPrices(): Promise<object> {
  return { success: false, error: 'Price sync not yet implemented in TypeScript CLI' };
}

// ---------------------------------------------------------------------------
// syncSymlinks
// ---------------------------------------------------------------------------

/**
 * Stub: symlink creation returns zeros (no filesystem operations).
 */
export async function syncSymlinks(): Promise<object> {
  return {
    connection_symlinks_created: 0,
    account_symlinks_created: 0,
    warnings: [],
  };
}

// ---------------------------------------------------------------------------
// authLogin
// ---------------------------------------------------------------------------

/**
 * Stub: auth login is not yet implemented for any provider.
 */
export async function authLogin(provider: string, _idOrName?: string): Promise<object> {
  return {
    success: false,
    error: `Auth login for '${provider}' not yet implemented in TypeScript CLI`,
  };
}
