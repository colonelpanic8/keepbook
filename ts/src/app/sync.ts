/**
 * Sync and auth stub commands for the CLI.
 *
 * The TypeScript CLI supports a subset of synchronizers.
 * - `manual`: skipped
 * - `coinbase`: implemented
 * - `schwab`: implemented (requires session capture via `auth schwab login`)
 * - others: not implemented
 */

import { type Storage } from '../storage/storage.js';
import { findConnection } from '../storage/lookup.js';
import type { RefreshConfig } from '../config.js';
import { checkBalanceStaleness, resolveBalanceStaleness } from '../staleness.js';
import type { ConnectionType } from '../models/connection.js';
import { CoinbaseSynchronizer } from '../sync/synchronizers/coinbase.js';
import { SchwabSynchronizer } from '../sync/synchronizers/schwab.js';
import { DefaultSyncOptions, saveSyncResult, type SyncOptions, type Synchronizer } from '../sync/mod.js';
import { SessionCache } from '../credentials/session.js';
import { parseExportedSession } from '../sync/schwab.js';

async function readStdinOnce(timeoutMs: number): Promise<string | null> {
  // Avoid hanging indefinitely in non-interactive contexts.
  // We accept piped input; for TTY we return null (caller can instruct).
  if (process.stdin.isTTY) return null;

  return await new Promise<string | null>((resolve) => {
    let settled = false;
    let data = '';

    const timer = setTimeout(() => {
      if (settled) return;
      settled = true;
      cleanup();
      resolve(null);
    }, timeoutMs);

    function cleanup() {
      clearTimeout(timer);
      process.stdin.off('data', onData);
      process.stdin.off('end', onEnd);
      process.stdin.off('error', onEnd);
    }

    function onData(chunk: Buffer | string) {
      data += chunk.toString();
    }

    function onEnd() {
      if (settled) return;
      settled = true;
      cleanup();
      const trimmed = data.trim();
      resolve(trimmed === '' ? null : trimmed);
    }

    process.stdin.on('data', onData);
    process.stdin.on('end', onEnd);
    process.stdin.on('error', onEnd);

    // Ensure flowing mode so 'data' events arrive for piped stdin.
    process.stdin.resume();
  });
}

async function syncConnectionImpl(
  storage: Storage,
  conn: ConnectionType,
  options: SyncOptions,
): Promise<object> {
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

  if (conn.config.synchronizer === 'schwab') {
    try {
      const synchronizer: Synchronizer = new SchwabSynchronizer(conn.state.id);
      const result = await (synchronizer.syncWithOptions
        ? synchronizer.syncWithOptions(conn, storage, options)
        : synchronizer.sync(conn, storage));
      await saveSyncResult(result, storage);

      return {
        success: true,
        connection: {
          id: result.connection.state.id.asStr(),
          name: result.connection.config.name,
        },
        accounts_synced: result.accounts.length,
        prices_stored: 0,
        last_sync: result.connection.state.last_sync?.at_raw ?? null,
      };
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : String(e);
      return {
        success: false,
        error: msg,
        connection: {
          id: conn.state.id.asStr(),
          name: conn.config.name,
        },
      };
    }
  }

  if (conn.config.synchronizer === 'coinbase') {
    const creds = storage.getCredentialStore(conn.state.id);
    if (creds === null) {
      return {
        success: false,
        error: 'Missing credential store for connection',
        connection: {
          id: conn.state.id.asStr(),
          name: conn.config.name,
        },
      };
    }

    try {
      const synchronizer: Synchronizer = await CoinbaseSynchronizer.fromCredentials(creds);
      const result = await (synchronizer.syncWithOptions
        ? synchronizer.syncWithOptions(conn, storage, options)
        : synchronizer.sync(conn, storage));
      await saveSyncResult(result, storage);

      return {
        success: true,
        connection: {
          id: result.connection.state.id.asStr(),
          name: result.connection.config.name,
        },
        accounts_synced: result.accounts.length,
        prices_stored: 0,
        last_sync: result.connection.state.last_sync?.at_raw ?? null,
      };
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : String(e);
      return {
        success: false,
        error: msg,
        connection: {
          id: conn.state.id.asStr(),
          name: conn.config.name,
        },
      };
    }
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
  return await syncConnectionWithOptions(storage, idOrName, DefaultSyncOptions);
}

export async function syncConnectionWithOptions(
  storage: Storage,
  idOrName: string,
  options: Partial<SyncOptions>,
): Promise<object> {
  const merged: SyncOptions = { ...DefaultSyncOptions, ...options };
  const conn = await findConnection(storage, idOrName);

  if (conn === null) {
    return { success: false, error: `Connection not found: '${idOrName}'` };
  }

  return syncConnectionImpl(storage, conn, merged);
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
  return await syncConnectionIfStaleWithOptions(storage, idOrName, refresh, DefaultSyncOptions);
}

export async function syncConnectionIfStaleWithOptions(
  storage: Storage,
  idOrName: string,
  refresh: RefreshConfig,
  options: Partial<SyncOptions>,
): Promise<object> {
  const merged: SyncOptions = { ...DefaultSyncOptions, ...options };
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

  return syncConnectionImpl(storage, conn, merged);
}

// ---------------------------------------------------------------------------
// syncAll
// ---------------------------------------------------------------------------

/**
 * Sync every connection. Collects per-connection results.
 */
export async function syncAll(storage: Storage): Promise<object> {
  return await syncAllWithOptions(storage, DefaultSyncOptions);
}

export async function syncAllWithOptions(
  storage: Storage,
  options: Partial<SyncOptions>,
): Promise<object> {
  const merged: SyncOptions = { ...DefaultSyncOptions, ...options };
  const connections = await storage.listConnections();
  const results: object[] = [];

  for (const conn of connections) {
    results.push(await syncConnectionImpl(storage, conn, merged));
  }

  return { results, total: connections.length };
}

/**
 * Sync every connection only if stale. Fresh connections are skipped.
 */
export async function syncAllIfStale(storage: Storage, refresh: RefreshConfig): Promise<object> {
  return await syncAllIfStaleWithOptions(storage, refresh, DefaultSyncOptions);
}

export async function syncAllIfStaleWithOptions(
  storage: Storage,
  refresh: RefreshConfig,
  options: Partial<SyncOptions>,
): Promise<object> {
  const merged: SyncOptions = { ...DefaultSyncOptions, ...options };
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
    results.push(await syncConnectionImpl(storage, conn, merged));
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

function formatNamesDebug(names: string[]): string {
  return `[${names.map((n) => JSON.stringify(n)).join(', ')}]`;
}

async function pickProviderConnection(
  storage: Storage,
  synchronizerName: string,
  idOrName?: string,
): Promise<ConnectionType> {
  const connections = await storage.listConnections();
  const matching = connections.filter((c) => c.config.synchronizer === synchronizerName);

  if (idOrName !== undefined) {
    const conn = await findConnection(storage, idOrName);
    if (conn === null || conn.config.synchronizer !== synchronizerName) {
      throw new Error(`${synchronizerName} connection not found: ${idOrName}`);
    }
    return conn;
  }

  if (matching.length === 1) return matching[0];
  if (matching.length === 0) {
    throw new Error(`No ${synchronizerName} connections found`);
  }

  const names = matching.map((c) => c.config.name);
  throw new Error(
    `Multiple ${synchronizerName} connections found (${matching.length}). Specify one: ${formatNamesDebug(names)}`,
  );
}

/**
 * Auth login for providers requiring interactive session capture.
 *
 * Rust uses browser automation; this TS CLI accepts an exported session JSON
 * document via stdin (pipe a file) and stores it in the local session cache.
 */
export async function authLogin(
  storage: Storage,
  provider: string,
  idOrName?: string,
): Promise<object> {
  if (provider !== 'schwab') {
    return {
      success: false,
      error: `Auth login for '${provider}' not yet implemented in TypeScript CLI`,
    };
  }

  let connection: ConnectionType;
  try {
    connection = await pickProviderConnection(storage, 'schwab', idOrName);
  } catch (e: unknown) {
    const msg = e instanceof Error ? e.message : String(e);
    return { success: false, error: msg };
  }

  const input = await readStdinOnce(200);
  if (input === null) {
    return {
      success: false,
      error: 'Schwab login requires exported session JSON on stdin',
    };
  }

  let session;
  try {
    session = parseExportedSession(input);
  } catch (e: unknown) {
    const msg = e instanceof Error ? e.message : String(e);
    return { success: false, error: `Failed to parse exported session JSON: ${msg}` };
  }

  const cache = SessionCache.new();
  cache.set(connection.state.id.asStr(), session);

  return {
    success: true,
    connection: {
      id: connection.state.id.asStr(),
      name: connection.config.name,
    },
    message: 'Session captured successfully',
  };
}
