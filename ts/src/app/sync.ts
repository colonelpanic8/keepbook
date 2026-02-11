/**
 * Sync and auth stub commands for the CLI.
 *
 * Real synchronizer implementations (Schwab, Chase, Coinbase) are not yet
 * available in the TypeScript library. These stubs return the correct output
 * shapes so the CLI can wire them up now.
 */

import { type Storage } from '../storage/storage.js';
import { findConnection } from '../storage/lookup.js';

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
export async function syncConnection(
  storage: Storage,
  idOrName: string,
): Promise<object> {
  const conn = await findConnection(storage, idOrName);

  if (conn === null) {
    return { success: false, error: `Connection not found: '${idOrName}'` };
  }

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
    error:
      `Synchronizer '${conn.config.synchronizer}' not implemented in TypeScript CLI`,
    connection: {
      id: conn.state.id.asStr(),
      name: conn.config.name,
    },
  };
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
    const result = await syncConnection(storage, conn.state.id.asStr());
    results.push(result);
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
export async function authLogin(
  provider: string,
  _idOrName?: string,
): Promise<object> {
  return {
    success: false,
    error: `Auth login for '${provider}' not yet implemented in TypeScript CLI`,
  };
}
