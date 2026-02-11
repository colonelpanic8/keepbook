import { describe, it, expect } from 'vitest';
import { MemoryStorage } from '../storage/memory.js';
import { FixedClock } from '../clock.js';
import { FixedIdGenerator } from '../models/id-generator.js';
import { Id } from '../models/id.js';
import { Connection } from '../models/connection.js';
import { syncConnection, syncAll, syncPrices, syncSymlinks, authLogin } from './sync.js';

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function makeIdGen(...ids: string[]): FixedIdGenerator {
  return new FixedIdGenerator(ids.map((s) => Id.fromString(s)));
}

function makeClock(iso: string): FixedClock {
  return new FixedClock(new Date(iso));
}

type SyncConnectionResultShape = {
  success: boolean;
  skipped?: boolean;
  connection?: {
    id: string;
    name?: string;
  };
};

type SyncAllResultShape = {
  total: number;
  results: SyncConnectionResultShape[];
};

async function addManualConnection(
  storage: MemoryStorage,
  name: string,
  id: string,
): Promise<void> {
  const conn = Connection.new(
    { name, synchronizer: 'manual' },
    makeIdGen(id),
    makeClock('2024-06-01T12:00:00Z'),
  );
  await storage.saveConnection(conn);
}

async function addNonManualConnection(
  storage: MemoryStorage,
  name: string,
  id: string,
  synchronizer: string,
): Promise<void> {
  const conn = Connection.new(
    { name, synchronizer },
    makeIdGen(id),
    makeClock('2024-06-01T12:00:00Z'),
  );
  await storage.saveConnection(conn);
}

// ---------------------------------------------------------------------------
// syncConnection
// ---------------------------------------------------------------------------

describe('syncConnection', () => {
  it('returns skipped output for manual connection', async () => {
    const storage = new MemoryStorage();
    await addManualConnection(storage, 'My Bank', 'conn-1');

    const result = await syncConnection(storage, 'conn-1');

    expect(result).toEqual({
      success: true,
      skipped: true,
      reason: 'manual',
      connection: {
        id: 'conn-1',
        name: 'My Bank',
      },
      accounts_synced: 0,
      prices_stored: 0,
      last_sync: null,
    });
  });

  it('returns error with synchronizer name for non-manual connection', async () => {
    const storage = new MemoryStorage();
    await addNonManualConnection(storage, 'Coinbase', 'conn-2', 'coinbase');

    const result = await syncConnection(storage, 'conn-2');

    expect(result).toEqual({
      success: false,
      error: "Synchronizer 'coinbase' not implemented in TypeScript CLI",
      connection: {
        id: 'conn-2',
        name: 'Coinbase',
      },
    });
  });

  it('returns not-found error for missing connection', async () => {
    const storage = new MemoryStorage();

    const result = await syncConnection(storage, 'nonexistent');

    expect(result).toEqual({
      success: false,
      error: "Connection not found: 'nonexistent'",
    });
  });

  it('finds connection by name', async () => {
    const storage = new MemoryStorage();
    await addManualConnection(storage, 'My Bank', 'conn-1');

    const result = (await syncConnection(storage, 'My Bank')) as SyncConnectionResultShape;

    expect(result.success).toBe(true);
    expect(result.skipped).toBe(true);
    expect(result.connection).toBeDefined();
    if (result.connection === undefined) {
      throw new Error('Expected connection in sync result');
    }
    expect(result.connection.id).toBe('conn-1');
  });
});

// ---------------------------------------------------------------------------
// syncAll
// ---------------------------------------------------------------------------

describe('syncAll', () => {
  it('returns empty results for no connections', async () => {
    const storage = new MemoryStorage();

    const result = await syncAll(storage);

    expect(result).toEqual({ results: [], total: 0 });
  });

  it('returns results for two connections (one manual, one non-manual)', async () => {
    const storage = new MemoryStorage();
    await addManualConnection(storage, 'My Bank', 'conn-1');
    await addNonManualConnection(storage, 'Coinbase', 'conn-2', 'coinbase');

    const result = (await syncAll(storage)) as SyncAllResultShape;

    expect(result.total).toBe(2);
    expect(result.results).toHaveLength(2);

    // Find results by connection id (order may vary)
    const manualResult = result.results.find((r) => r.connection?.id === 'conn-1');
    const coinbaseResult = result.results.find((r) => r.connection?.id === 'conn-2');

    expect(manualResult).toEqual({
      success: true,
      skipped: true,
      reason: 'manual',
      connection: { id: 'conn-1', name: 'My Bank' },
      accounts_synced: 0,
      prices_stored: 0,
      last_sync: null,
    });

    expect(coinbaseResult).toEqual({
      success: false,
      error: "Synchronizer 'coinbase' not implemented in TypeScript CLI",
      connection: { id: 'conn-2', name: 'Coinbase' },
    });
  });
});

// ---------------------------------------------------------------------------
// syncPrices
// ---------------------------------------------------------------------------

describe('syncPrices', () => {
  it('returns not-implemented error', async () => {
    const result = await syncPrices();

    expect(result).toEqual({
      success: false,
      error: 'Price sync not yet implemented in TypeScript CLI',
    });
  });
});

// ---------------------------------------------------------------------------
// syncSymlinks
// ---------------------------------------------------------------------------

describe('syncSymlinks', () => {
  it('returns correct output shape with zeros', async () => {
    const result = await syncSymlinks();

    expect(result).toEqual({
      connection_symlinks_created: 0,
      account_symlinks_created: 0,
      warnings: [],
    });
  });
});

// ---------------------------------------------------------------------------
// authLogin
// ---------------------------------------------------------------------------

describe('authLogin', () => {
  it('returns error with provider name', async () => {
    const result = await authLogin('schwab');

    expect(result).toEqual({
      success: false,
      error: "Auth login for 'schwab' not yet implemented in TypeScript CLI",
    });
  });

  it('returns error with different provider name', async () => {
    const result = await authLogin('plaid');

    expect(result).toEqual({
      success: false,
      error: "Auth login for 'plaid' not yet implemented in TypeScript CLI",
    });
  });
});
