import { describe, it, expect } from 'vitest';
import { MemoryStorage } from '../storage/memory.js';
import { FixedClock } from '../clock.js';
import { FixedIdGenerator } from '../models/id-generator.js';
import { Id } from '../models/id.js';
import { Connection, type ConnectionType } from '../models/connection.js';
import { Account } from '../models/account.js';
import { addConnection, addAccount, removeConnection, setBalance } from './mutations.js';

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function makeIdGen(...ids: string[]): FixedIdGenerator {
  return new FixedIdGenerator(ids.map(s => Id.fromString(s)));
}

function makeClock(iso: string): FixedClock {
  return new FixedClock(new Date(iso));
}

// ---------------------------------------------------------------------------
// addConnection
// ---------------------------------------------------------------------------

describe('addConnection', () => {
  it('creates a connection and returns correct output', async () => {
    const storage = new MemoryStorage();
    const ids = makeIdGen('conn-1');
    const clock = makeClock('2024-06-01T12:00:00Z');

    const result = await addConnection(storage, 'My Bank', ids, clock);

    expect(result).toEqual({
      success: true,
      connection: {
        id: 'conn-1',
        name: 'My Bank',
        synchronizer: 'manual',
      },
    });

    // Verify persisted to storage
    const conn = await storage.getConnection(Id.fromString('conn-1'));
    expect(conn).not.toBeNull();
    expect(conn!.config.name).toBe('My Bank');
    expect(conn!.config.synchronizer).toBe('manual');
  });

  it('returns error for duplicate name', async () => {
    const storage = new MemoryStorage();
    const clock = makeClock('2024-06-01T12:00:00Z');

    // Create first connection
    await addConnection(storage, 'My Bank', makeIdGen('conn-1'), clock);

    // Attempt duplicate
    const result = await addConnection(storage, 'My Bank', makeIdGen('conn-2'), clock);

    expect(result).toEqual({
      success: false,
      error: "Connection with name 'My Bank' already exists",
    });
  });

  it('returns error for duplicate name case-insensitive', async () => {
    const storage = new MemoryStorage();
    const clock = makeClock('2024-06-01T12:00:00Z');

    await addConnection(storage, 'My Bank', makeIdGen('conn-1'), clock);

    const result = await addConnection(storage, 'my bank', makeIdGen('conn-2'), clock);

    expect(result).toEqual({
      success: false,
      error: "Connection with name 'my bank' already exists",
    });
  });

  it('saves connection config to storage', async () => {
    const storage = new MemoryStorage();
    const ids = makeIdGen('conn-abc');
    const clock = makeClock('2024-06-01T12:00:00Z');

    await addConnection(storage, 'Test', ids, clock);

    const connections = await storage.listConnections();
    expect(connections).toHaveLength(1);
    expect(connections[0].config.name).toBe('Test');
    expect(connections[0].config.synchronizer).toBe('manual');
    expect(connections[0].state.id.asStr()).toBe('conn-abc');
  });
});

// ---------------------------------------------------------------------------
// addAccount
// ---------------------------------------------------------------------------

describe('addAccount', () => {
  it('creates an account linked to a connection', async () => {
    const storage = new MemoryStorage();
    const clock = makeClock('2024-06-01T12:00:00Z');

    // Create a connection first
    await addConnection(storage, 'My Bank', makeIdGen('conn-1'), clock);

    // Add an account
    const result = await addAccount(
      storage, 'My Bank', 'Checking', [],
      makeIdGen('acct-1'), clock,
    );

    expect(result).toEqual({
      success: true,
      account: {
        id: 'acct-1',
        name: 'Checking',
        connection_id: 'conn-1',
      },
    });

    // Verify persisted to storage
    const acct = await storage.getAccount(Id.fromString('acct-1'));
    expect(acct).not.toBeNull();
    expect(acct!.name).toBe('Checking');
    expect(acct!.connection_id.asStr()).toBe('conn-1');
  });

  it('returns error when connection not found', async () => {
    const storage = new MemoryStorage();
    const clock = makeClock('2024-06-01T12:00:00Z');

    const result = await addAccount(
      storage, 'nonexistent', 'Checking', [],
      makeIdGen('acct-1'), clock,
    );

    expect(result).toEqual({
      success: false,
      error: "Connection not found: 'nonexistent'",
    });
  });

  it('preserves tags on the account', async () => {
    const storage = new MemoryStorage();
    const clock = makeClock('2024-06-01T12:00:00Z');

    await addConnection(storage, 'My Bank', makeIdGen('conn-1'), clock);

    await addAccount(
      storage, 'My Bank', 'Checking', ['bank', 'primary'],
      makeIdGen('acct-1'), clock,
    );

    const acct = await storage.getAccount(Id.fromString('acct-1'));
    expect(acct).not.toBeNull();
    expect(acct!.tags).toEqual(['bank', 'primary']);
  });

  it('updates connection state account_ids', async () => {
    const storage = new MemoryStorage();
    const clock = makeClock('2024-06-01T12:00:00Z');

    await addConnection(storage, 'My Bank', makeIdGen('conn-1'), clock);

    await addAccount(
      storage, 'My Bank', 'Checking', [],
      makeIdGen('acct-1'), clock,
    );

    const conn = await storage.getConnection(Id.fromString('conn-1'));
    expect(conn).not.toBeNull();
    const accountIdStrs = conn!.state.account_ids.map(id => id.asStr());
    expect(accountIdStrs).toContain('acct-1');
  });

  it('finds connection by ID', async () => {
    const storage = new MemoryStorage();
    const clock = makeClock('2024-06-01T12:00:00Z');

    await addConnection(storage, 'My Bank', makeIdGen('conn-1'), clock);

    const result = await addAccount(
      storage, 'conn-1', 'Savings', [],
      makeIdGen('acct-2'), clock,
    );

    expect(result).toEqual({
      success: true,
      account: {
        id: 'acct-2',
        name: 'Savings',
        connection_id: 'conn-1',
      },
    });
  });

  it('handles empty tags array', async () => {
    const storage = new MemoryStorage();
    const clock = makeClock('2024-06-01T12:00:00Z');

    await addConnection(storage, 'My Bank', makeIdGen('conn-1'), clock);

    await addAccount(
      storage, 'My Bank', 'Checking', [],
      makeIdGen('acct-1'), clock,
    );

    const acct = await storage.getAccount(Id.fromString('acct-1'));
    expect(acct!.tags).toEqual([]);
  });
});

// ---------------------------------------------------------------------------
// removeConnection
// ---------------------------------------------------------------------------

describe('removeConnection', () => {
  it('deletes connection and its accounts, returns correct output', async () => {
    const storage = new MemoryStorage();
    const clock = makeClock('2024-06-01T12:00:00Z');

    // Create connection and accounts
    await addConnection(storage, 'My Bank', makeIdGen('conn-1'), clock);
    await addAccount(storage, 'conn-1', 'Checking', [], makeIdGen('acct-1'), clock);
    await addAccount(storage, 'conn-1', 'Savings', [], makeIdGen('acct-2'), clock);

    const result = await removeConnection(storage, 'conn-1');

    expect(result).toEqual({
      success: true,
      connection: {
        id: 'conn-1',
        name: 'My Bank',
      },
      deleted_accounts: 2,
      account_ids: expect.arrayContaining(['acct-1', 'acct-2']),
    });

    // Verify connection is gone
    const conn = await storage.getConnection(Id.fromString('conn-1'));
    expect(conn).toBeNull();

    // Verify accounts are gone
    const acct1 = await storage.getAccount(Id.fromString('acct-1'));
    expect(acct1).toBeNull();
    const acct2 = await storage.getAccount(Id.fromString('acct-2'));
    expect(acct2).toBeNull();
  });

  it('returns error for non-existent connection', async () => {
    const storage = new MemoryStorage();

    const result = await removeConnection(storage, 'nonexistent-id');

    expect(result).toEqual({
      success: false,
      error: 'Connection not found',
      id: 'nonexistent-id',
    });
  });

  it('removes connection with no accounts', async () => {
    const storage = new MemoryStorage();
    const clock = makeClock('2024-06-01T12:00:00Z');

    await addConnection(storage, 'Empty Bank', makeIdGen('conn-1'), clock);

    const result = await removeConnection(storage, 'conn-1');

    expect(result).toEqual({
      success: true,
      connection: {
        id: 'conn-1',
        name: 'Empty Bank',
      },
      deleted_accounts: 0,
      account_ids: [],
    });
  });

  it('does not delete accounts from other connections', async () => {
    const storage = new MemoryStorage();
    const clock = makeClock('2024-06-01T12:00:00Z');

    await addConnection(storage, 'Bank A', makeIdGen('conn-a'), clock);
    await addConnection(storage, 'Bank B', makeIdGen('conn-b'), clock);
    await addAccount(storage, 'conn-a', 'Acct A', [], makeIdGen('acct-a'), clock);
    await addAccount(storage, 'conn-b', 'Acct B', [], makeIdGen('acct-b'), clock);

    await removeConnection(storage, 'conn-a');

    // Bank B's account should still exist
    const acctB = await storage.getAccount(Id.fromString('acct-b'));
    expect(acctB).not.toBeNull();
    expect(acctB!.name).toBe('Acct B');
  });
});

// ---------------------------------------------------------------------------
// setBalance
// ---------------------------------------------------------------------------

describe('setBalance', () => {
  it('creates a balance snapshot with correct output', async () => {
    const storage = new MemoryStorage();
    const clock = makeClock('2024-06-01T12:00:00Z');

    // Set up connection + account
    await addConnection(storage, 'My Bank', makeIdGen('conn-1'), clock);
    await addAccount(storage, 'conn-1', 'Checking', [], makeIdGen('acct-1'), clock);

    const balanceClock = makeClock('2024-07-01T10:00:00Z');
    const result = await setBalance(storage, 'acct-1', 'USD', '1234.56', balanceClock);

    expect(result).toEqual({
      success: true,
      balance: {
        account_id: 'acct-1',
        asset: { type: 'currency', iso_code: 'USD' },
        amount: '1234.56',
        timestamp: '2024-07-01T10:00:00+00:00',
      },
    });

    // Verify snapshot saved
    const snapshots = await storage.getBalanceSnapshots(Id.fromString('acct-1'));
    expect(snapshots).toHaveLength(1);
    expect(snapshots[0].balances[0].amount).toBe('1234.56');
  });

  it('returns error for non-existent account', async () => {
    const storage = new MemoryStorage();

    const result = await setBalance(storage, 'nonexistent', 'USD', '100');

    expect(result).toEqual({
      success: false,
      error: "Account not found: 'nonexistent'",
    });
  });

  it('returns error for invalid amount', async () => {
    const storage = new MemoryStorage();
    const clock = makeClock('2024-06-01T12:00:00Z');

    await addConnection(storage, 'My Bank', makeIdGen('conn-1'), clock);
    await addAccount(storage, 'conn-1', 'Checking', [], makeIdGen('acct-1'), clock);

    const result = await setBalance(storage, 'acct-1', 'USD', 'not-a-number');

    expect(result).toEqual({
      success: false,
      error: "Invalid amount: 'not-a-number'",
    });
  });

  it('parses currency asset (bare string)', async () => {
    const storage = new MemoryStorage();
    const clock = makeClock('2024-06-01T12:00:00Z');

    await addConnection(storage, 'My Bank', makeIdGen('conn-1'), clock);
    await addAccount(storage, 'conn-1', 'Checking', [], makeIdGen('acct-1'), clock);

    const balanceClock = makeClock('2024-07-01T10:00:00Z');
    const result = await setBalance(storage, 'acct-1', 'EUR', '500', balanceClock) as any;

    expect(result.success).toBe(true);
    expect(result.balance.asset).toEqual({ type: 'currency', iso_code: 'EUR' });
  });

  it('parses equity asset (equity:AAPL)', async () => {
    const storage = new MemoryStorage();
    const clock = makeClock('2024-06-01T12:00:00Z');

    await addConnection(storage, 'My Bank', makeIdGen('conn-1'), clock);
    await addAccount(storage, 'conn-1', 'Brokerage', [], makeIdGen('acct-1'), clock);

    const balanceClock = makeClock('2024-07-01T10:00:00Z');
    const result = await setBalance(storage, 'acct-1', 'equity:AAPL', '100', balanceClock) as any;

    expect(result.success).toBe(true);
    expect(result.balance.asset).toEqual({ type: 'equity', ticker: 'AAPL' });
  });

  it('parses crypto asset (crypto:BTC)', async () => {
    const storage = new MemoryStorage();
    const clock = makeClock('2024-06-01T12:00:00Z');

    await addConnection(storage, 'My Bank', makeIdGen('conn-1'), clock);
    await addAccount(storage, 'conn-1', 'Wallet', [], makeIdGen('acct-1'), clock);

    const balanceClock = makeClock('2024-07-01T10:00:00Z');
    const result = await setBalance(storage, 'acct-1', 'crypto:BTC', '0.5', balanceClock) as any;

    expect(result.success).toBe(true);
    expect(result.balance.asset).toEqual({ type: 'crypto', symbol: 'BTC' });
    expect(result.balance.amount).toBe('0.5');
  });

  it('strips trailing zeros from amount', async () => {
    const storage = new MemoryStorage();
    const clock = makeClock('2024-06-01T12:00:00Z');

    await addConnection(storage, 'My Bank', makeIdGen('conn-1'), clock);
    await addAccount(storage, 'conn-1', 'Checking', [], makeIdGen('acct-1'), clock);

    const balanceClock = makeClock('2024-07-01T10:00:00Z');
    const result = await setBalance(storage, 'acct-1', 'USD', '100.00', balanceClock) as any;

    expect(result.success).toBe(true);
    expect(result.balance.amount).toBe('100');
  });

  it('finds account by name', async () => {
    const storage = new MemoryStorage();
    const clock = makeClock('2024-06-01T12:00:00Z');

    await addConnection(storage, 'My Bank', makeIdGen('conn-1'), clock);
    await addAccount(storage, 'conn-1', 'Checking', [], makeIdGen('acct-1'), clock);

    const balanceClock = makeClock('2024-07-01T10:00:00Z');
    const result = await setBalance(storage, 'Checking', 'USD', '500', balanceClock) as any;

    expect(result.success).toBe(true);
    expect(result.balance.account_id).toBe('acct-1');
  });

  it('formats timestamp with rfc3339', async () => {
    const storage = new MemoryStorage();
    const clock = makeClock('2024-06-01T12:00:00Z');

    await addConnection(storage, 'My Bank', makeIdGen('conn-1'), clock);
    await addAccount(storage, 'conn-1', 'Checking', [], makeIdGen('acct-1'), clock);

    const balanceClock = makeClock('2024-07-01T10:30:00.123Z');
    const result = await setBalance(storage, 'acct-1', 'USD', '100', balanceClock) as any;

    expect(result.balance.timestamp).toBe('2024-07-01T10:30:00.123000000+00:00');
  });
});
