import { describe, it, expect } from 'vitest';
import { MemoryStorage } from '../storage/memory.js';
import { Id } from '../models/id.js';
import { Connection, type ConnectionType } from '../models/connection.js';
import { Account } from '../models/account.js';
import { BalanceSnapshot, AssetBalance } from '../models/balance.js';
import { Transaction } from '../models/transaction.js';
import { FixedClock } from '../clock.js';
import { FixedIdGenerator } from '../models/id-generator.js';
import { Asset } from '../models/asset.js';
import {
  listConnections,
  listAccounts,
  listBalances,
  listTransactions,
  listPriceSources,
  listAll,
} from './list.js';

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
// listConnections
// ---------------------------------------------------------------------------

describe('listConnections', () => {
  it('returns [] when storage is empty', async () => {
    const storage = new MemoryStorage();
    const result = await listConnections(storage);
    expect(result).toEqual([]);
  });

  it('returns correct fields for a connection with accounts', async () => {
    const storage = new MemoryStorage();
    const clock = makeClock('2024-06-01T12:00:00Z');
    const connIdGen = makeIdGen('conn-1');
    const conn = Connection.new(
      { name: 'My Bank', synchronizer: 'plaid' },
      connIdGen,
      clock,
    );
    await storage.saveConnection(conn);

    // Create two accounts linked to this connection
    const acctIdGen1 = makeIdGen('acct-1');
    const acct1 = Account.newWithGenerator(acctIdGen1, clock, 'Checking', Id.fromString('conn-1'));
    await storage.saveAccount(acct1);

    const acctIdGen2 = makeIdGen('acct-2');
    const acct2 = Account.newWithGenerator(acctIdGen2, clock, 'Savings', Id.fromString('conn-1'));
    await storage.saveAccount(acct2);

    const result = await listConnections(storage);
    expect(result).toHaveLength(1);
    expect(result[0]).toEqual({
      id: 'conn-1',
      name: 'My Bank',
      synchronizer: 'plaid',
      status: 'active',
      account_count: 2,
      last_sync: null,
    });
  });

  it('unions state.account_ids with actual accounts for account_count', async () => {
    const storage = new MemoryStorage();
    const clock = makeClock('2024-06-01T12:00:00Z');
    const connIdGen = makeIdGen('conn-1');
    const conn = Connection.new(
      { name: 'Bank', synchronizer: 'plaid' },
      connIdGen,
      clock,
    );

    // Add an account_id in state that doesn't exist as a real account
    const connWithStateIds: ConnectionType = {
      config: conn.config,
      state: {
        ...conn.state,
        account_ids: [Id.fromString('phantom-acct')],
      },
    };
    await storage.saveConnection(connWithStateIds);

    // And a real account linked to this connection
    const acctIdGen = makeIdGen('real-acct');
    const acct = Account.newWithGenerator(acctIdGen, clock, 'Checking', Id.fromString('conn-1'));
    await storage.saveAccount(acct);

    const result = await listConnections(storage);
    // phantom-acct + real-acct = 2
    expect(result[0].account_count).toBe(2);
  });

  it('formats last_sync as rfc3339 with +00:00', async () => {
    const storage = new MemoryStorage();
    const clock = makeClock('2024-06-01T12:00:00Z');
    const connIdGen = makeIdGen('conn-1');
    const conn = Connection.new(
      { name: 'Bank', synchronizer: 'plaid' },
      connIdGen,
      clock,
    );

    const connWithSync: ConnectionType = {
      config: conn.config,
      state: {
        ...conn.state,
        last_sync: {
          at: new Date('2024-07-15T09:30:00Z'),
          status: 'success',
        },
      },
    };
    await storage.saveConnection(connWithSync);

    const result = await listConnections(storage);
    expect(result[0].last_sync).toBe('2024-07-15T09:30:00+00:00');
  });

  it('returns status as lowercase string', async () => {
    const storage = new MemoryStorage();
    const clock = makeClock('2024-06-01T12:00:00Z');

    // Connection with 'error' status
    const connIdGen = makeIdGen('conn-err');
    const conn = Connection.new(
      { name: 'Broken', synchronizer: 'plaid' },
      connIdGen,
      clock,
    );
    const connWithError: ConnectionType = {
      config: conn.config,
      state: {
        ...conn.state,
        status: 'error',
      },
    };
    await storage.saveConnection(connWithError);

    const result = await listConnections(storage);
    expect(result[0].status).toBe('error');
  });

  it('returns last_sync null when no last_sync', async () => {
    const storage = new MemoryStorage();
    const clock = makeClock('2024-06-01T12:00:00Z');
    const connIdGen = makeIdGen('conn-1');
    const conn = Connection.new(
      { name: 'Bank', synchronizer: 'plaid' },
      connIdGen,
      clock,
    );
    await storage.saveConnection(conn);

    const result = await listConnections(storage);
    expect(result[0].last_sync).toBeNull();
  });
});

// ---------------------------------------------------------------------------
// listAccounts
// ---------------------------------------------------------------------------

describe('listAccounts', () => {
  it('returns [] when storage is empty', async () => {
    const storage = new MemoryStorage();
    const result = await listAccounts(storage);
    expect(result).toEqual([]);
  });

  it('returns correct fields including tags and active status', async () => {
    const storage = new MemoryStorage();
    const clock = makeClock('2024-06-01T12:00:00Z');
    const idGen = makeIdGen('acct-1');
    const acct = Account.newWithGenerator(idGen, clock, 'Checking', Id.fromString('conn-1'));

    // Add tags and set inactive
    const acctWithTags = {
      ...acct,
      tags: ['bank', 'primary'],
      active: false,
    };
    await storage.saveAccount(acctWithTags);

    const result = await listAccounts(storage);
    expect(result).toHaveLength(1);
    expect(result[0]).toEqual({
      id: 'acct-1',
      name: 'Checking',
      connection_id: 'conn-1',
      tags: ['bank', 'primary'],
      active: false,
    });
  });

  it('returns a copy of tags (not the original array)', async () => {
    const storage = new MemoryStorage();
    const clock = makeClock('2024-06-01T12:00:00Z');
    const idGen = makeIdGen('acct-1');
    const acct = Account.newWithGenerator(idGen, clock, 'Test', Id.fromString('conn-1'));
    const acctWithTags = { ...acct, tags: ['tag1'] };
    await storage.saveAccount(acctWithTags);

    const result = await listAccounts(storage);
    expect(result[0].tags).toEqual(['tag1']);
    // Mutating the result should not affect the source
    result[0].tags.push('tag2');
    const result2 = await listAccounts(storage);
    expect(result2[0].tags).toEqual(['tag1']);
  });

  it('returns active true by default', async () => {
    const storage = new MemoryStorage();
    const clock = makeClock('2024-06-01T12:00:00Z');
    const idGen = makeIdGen('acct-1');
    const acct = Account.newWithGenerator(idGen, clock, 'Default', Id.fromString('conn-1'));
    await storage.saveAccount(acct);

    const result = await listAccounts(storage);
    expect(result[0].active).toBe(true);
  });
});

// ---------------------------------------------------------------------------
// listBalances
// ---------------------------------------------------------------------------

describe('listBalances', () => {
  it('returns [] when storage is empty', async () => {
    const storage = new MemoryStorage();
    const result = await listBalances(storage, 'USD');
    expect(result).toEqual([]);
  });

  it('returns correct balance with null value_in_reporting_currency for non-matching asset', async () => {
    const storage = new MemoryStorage();
    const clock = makeClock('2024-06-01T12:00:00Z');

    // Create account
    const idGen = makeIdGen('acct-1');
    const acct = Account.newWithGenerator(idGen, clock, 'Checking', Id.fromString('conn-1'));
    await storage.saveAccount(acct);

    // Add a BTC balance
    const snapshot = BalanceSnapshot.new(
      new Date('2024-07-01T10:00:00Z'),
      [AssetBalance.new(Asset.crypto('BTC'), '1.5')],
    );
    await storage.appendBalanceSnapshot(acct.id, snapshot);

    const result = await listBalances(storage, 'USD');
    expect(result).toHaveLength(1);
    expect(result[0]).toEqual({
      account_id: 'acct-1',
      asset: { type: 'crypto', symbol: 'BTC' },
      amount: '1.5',
      value_in_reporting_currency: null,
      reporting_currency: 'USD',
      timestamp: '2024-07-01T10:00:00+00:00',
    });
  });

  it('returns amount as value_in_reporting_currency when asset matches reporting currency', async () => {
    const storage = new MemoryStorage();
    const clock = makeClock('2024-06-01T12:00:00Z');

    const idGen = makeIdGen('acct-1');
    const acct = Account.newWithGenerator(idGen, clock, 'Checking', Id.fromString('conn-1'));
    await storage.saveAccount(acct);

    const snapshot = BalanceSnapshot.new(
      new Date('2024-07-01T10:00:00Z'),
      [AssetBalance.new(Asset.currency('USD'), '1000.50')],
    );
    await storage.appendBalanceSnapshot(acct.id, snapshot);

    const result = await listBalances(storage, 'USD');
    expect(result).toHaveLength(1);
    expect(result[0].value_in_reporting_currency).toBe('1000.50');
  });

  it('returns null value_in_reporting_currency when currency does not match', async () => {
    const storage = new MemoryStorage();
    const clock = makeClock('2024-06-01T12:00:00Z');

    const idGen = makeIdGen('acct-1');
    const acct = Account.newWithGenerator(idGen, clock, 'Euro Account', Id.fromString('conn-1'));
    await storage.saveAccount(acct);

    const snapshot = BalanceSnapshot.new(
      new Date('2024-07-01T10:00:00Z'),
      [AssetBalance.new(Asset.currency('EUR'), '500')],
    );
    await storage.appendBalanceSnapshot(acct.id, snapshot);

    const result = await listBalances(storage, 'USD');
    expect(result).toHaveLength(1);
    expect(result[0].value_in_reporting_currency).toBeNull();
  });

  it('formats timestamp with formatRfc3339', async () => {
    const storage = new MemoryStorage();
    const clock = makeClock('2024-06-01T12:00:00Z');

    const idGen = makeIdGen('acct-1');
    const acct = Account.newWithGenerator(idGen, clock, 'Test', Id.fromString('conn-1'));
    await storage.saveAccount(acct);

    const snapshot = BalanceSnapshot.new(
      new Date('2024-07-01T10:30:00.123Z'),
      [AssetBalance.new(Asset.currency('USD'), '100')],
    );
    await storage.appendBalanceSnapshot(acct.id, snapshot);

    const result = await listBalances(storage, 'USD');
    expect(result[0].timestamp).toBe('2024-07-01T10:30:00.123000000+00:00');
  });

  it('returns multiple balances from a single snapshot', async () => {
    const storage = new MemoryStorage();
    const clock = makeClock('2024-06-01T12:00:00Z');

    const idGen = makeIdGen('acct-1');
    const acct = Account.newWithGenerator(idGen, clock, 'Multi', Id.fromString('conn-1'));
    await storage.saveAccount(acct);

    const snapshot = BalanceSnapshot.new(
      new Date('2024-07-01T10:00:00Z'),
      [
        AssetBalance.new(Asset.currency('USD'), '1000'),
        AssetBalance.new(Asset.crypto('BTC'), '0.5'),
      ],
    );
    await storage.appendBalanceSnapshot(acct.id, snapshot);

    const result = await listBalances(storage, 'USD');
    expect(result).toHaveLength(2);
    expect(result[0].asset).toEqual({ type: 'currency', iso_code: 'USD' });
    expect(result[0].value_in_reporting_currency).toBe('1000');
    expect(result[1].asset).toEqual({ type: 'crypto', symbol: 'BTC' });
    expect(result[1].value_in_reporting_currency).toBeNull();
  });
});

// ---------------------------------------------------------------------------
// listTransactions
// ---------------------------------------------------------------------------

describe('listTransactions', () => {
  it('returns [] when storage is empty', async () => {
    const storage = new MemoryStorage();
    const result = await listTransactions(storage);
    expect(result).toEqual([]);
  });

  it('returns correct fields with formatted timestamp', async () => {
    const storage = new MemoryStorage();
    const clock = makeClock('2024-06-15T14:30:00Z');

    // Create account
    const acctIdGen = makeIdGen('acct-1');
    const acct = Account.newWithGenerator(acctIdGen, clock, 'Checking', Id.fromString('conn-1'));
    await storage.saveAccount(acct);

    // Create transaction
    const txIdGen = makeIdGen('tx-1');
    const tx = Transaction.newWithGenerator(
      txIdGen,
      clock,
      '-50.00',
      Asset.currency('USD'),
      'Coffee shop',
    );
    await storage.appendTransactions(acct.id, [tx]);

    const result = await listTransactions(storage);
    expect(result).toHaveLength(1);
    expect(result[0]).toEqual({
      id: 'tx-1',
      account_id: 'acct-1',
      timestamp: '2024-06-15T14:30:00+00:00',
      description: 'Coffee shop',
      amount: '-50.00',
      asset: { type: 'currency', iso_code: 'USD' },
      status: 'posted',
    });
  });

  it('includes transactions from multiple accounts', async () => {
    const storage = new MemoryStorage();
    const clock = makeClock('2024-06-15T14:30:00Z');

    const acctIdGen1 = makeIdGen('acct-1');
    const acct1 = Account.newWithGenerator(acctIdGen1, clock, 'Acct1', Id.fromString('conn-1'));
    await storage.saveAccount(acct1);

    const acctIdGen2 = makeIdGen('acct-2');
    const acct2 = Account.newWithGenerator(acctIdGen2, clock, 'Acct2', Id.fromString('conn-1'));
    await storage.saveAccount(acct2);

    const txIdGen1 = makeIdGen('tx-1');
    const tx1 = Transaction.newWithGenerator(txIdGen1, clock, '100', Asset.currency('USD'), 'Deposit');
    await storage.appendTransactions(acct1.id, [tx1]);

    const txIdGen2 = makeIdGen('tx-2');
    const tx2 = Transaction.newWithGenerator(txIdGen2, clock, '0.01', Asset.crypto('BTC'), 'Mining');
    await storage.appendTransactions(acct2.id, [tx2]);

    const result = await listTransactions(storage);
    expect(result).toHaveLength(2);
    expect(result[0].account_id).toBe('acct-1');
    expect(result[1].account_id).toBe('acct-2');
  });

  it('formats timestamp with rfc3339 including subseconds', async () => {
    const storage = new MemoryStorage();
    const clock = makeClock('2024-06-15T14:30:00.456Z');

    const acctIdGen = makeIdGen('acct-1');
    const acct = Account.newWithGenerator(acctIdGen, clock, 'Test', Id.fromString('conn-1'));
    await storage.saveAccount(acct);

    const txIdGen = makeIdGen('tx-1');
    const tx = Transaction.newWithGenerator(txIdGen, clock, '10', Asset.currency('USD'), 'Test');
    await storage.appendTransactions(acct.id, [tx]);

    const result = await listTransactions(storage);
    expect(result[0].timestamp).toBe('2024-06-15T14:30:00.456000000+00:00');
  });

  it('returns status as lowercase string', async () => {
    const storage = new MemoryStorage();
    const clock = makeClock('2024-06-15T14:30:00Z');

    const acctIdGen = makeIdGen('acct-1');
    const acct = Account.newWithGenerator(acctIdGen, clock, 'Test', Id.fromString('conn-1'));
    await storage.saveAccount(acct);

    const txIdGen = makeIdGen('tx-1');
    const tx = Transaction.newWithGenerator(txIdGen, clock, '10', Asset.currency('USD'), 'Test');
    await storage.appendTransactions(acct.id, [tx]);

    const result = await listTransactions(storage);
    expect(result[0].status).toBe('posted');
  });
});

// ---------------------------------------------------------------------------
// listPriceSources
// ---------------------------------------------------------------------------

describe('listPriceSources', () => {
  it('returns empty array', () => {
    const result = listPriceSources();
    expect(result).toEqual([]);
  });
});

// ---------------------------------------------------------------------------
// listAll
// ---------------------------------------------------------------------------

describe('listAll', () => {
  it('combines all sublists', async () => {
    const storage = new MemoryStorage();
    const clock = makeClock('2024-06-01T12:00:00Z');

    // Connection
    const connIdGen = makeIdGen('conn-1');
    const conn = Connection.new(
      { name: 'Bank', synchronizer: 'plaid' },
      connIdGen,
      clock,
    );
    await storage.saveConnection(conn);

    // Account
    const acctIdGen = makeIdGen('acct-1');
    const acct = Account.newWithGenerator(acctIdGen, clock, 'Checking', Id.fromString('conn-1'));
    await storage.saveAccount(acct);

    // Balance
    const snapshot = BalanceSnapshot.new(
      new Date('2024-07-01T10:00:00Z'),
      [AssetBalance.new(Asset.currency('USD'), '500')],
    );
    await storage.appendBalanceSnapshot(acct.id, snapshot);

    const result = await listAll(storage, 'USD');

    expect(result.connections).toHaveLength(1);
    expect(result.connections[0].id).toBe('conn-1');

    expect(result.accounts).toHaveLength(1);
    expect(result.accounts[0].id).toBe('acct-1');

    expect(result.price_sources).toEqual([]);

    expect(result.balances).toHaveLength(1);
    expect(result.balances[0].account_id).toBe('acct-1');
    expect(result.balances[0].value_in_reporting_currency).toBe('500');
  });

  it('returns all empty arrays when storage is empty', async () => {
    const storage = new MemoryStorage();
    const result = await listAll(storage, 'USD');
    expect(result).toEqual({
      connections: [],
      accounts: [],
      price_sources: [],
      balances: [],
    });
  });
});

// ---------------------------------------------------------------------------
// JSON output compatibility
// ---------------------------------------------------------------------------

describe('JSON output format', () => {
  it('matches expected Rust JSON format for listConnections', async () => {
    const storage = new MemoryStorage();
    const clock = makeClock('2024-06-01T12:00:00Z');
    const connIdGen = makeIdGen('conn-abc');
    const conn = Connection.new(
      { name: 'Test Bank', synchronizer: 'plaid' },
      connIdGen,
      clock,
    );
    const connWithSync: ConnectionType = {
      config: conn.config,
      state: {
        ...conn.state,
        last_sync: {
          at: new Date('2024-07-15T09:30:00Z'),
          status: 'success',
        },
      },
    };
    await storage.saveConnection(connWithSync);

    const result = await listConnections(storage);
    const json = JSON.stringify(result);

    // Verify the JSON matches the expected Rust format exactly
    const expected = JSON.stringify([{
      id: 'conn-abc',
      name: 'Test Bank',
      synchronizer: 'plaid',
      status: 'active',
      account_count: 0,
      last_sync: '2024-07-15T09:30:00+00:00',
    }]);
    expect(json).toBe(expected);
  });

  it('matches expected Rust JSON format for listBalances', async () => {
    const storage = new MemoryStorage();
    const clock = makeClock('2024-06-01T12:00:00Z');

    const idGen = makeIdGen('acct-xyz');
    const acct = Account.newWithGenerator(idGen, clock, 'Savings', Id.fromString('conn-1'));
    await storage.saveAccount(acct);

    const snapshot = BalanceSnapshot.new(
      new Date('2024-07-01T10:00:00Z'),
      [AssetBalance.new(Asset.currency('USD'), '1234.56')],
    );
    await storage.appendBalanceSnapshot(acct.id, snapshot);

    const result = await listBalances(storage, 'USD');
    const json = JSON.stringify(result);

    const expected = JSON.stringify([{
      account_id: 'acct-xyz',
      asset: { type: 'currency', iso_code: 'USD' },
      amount: '1234.56',
      value_in_reporting_currency: '1234.56',
      reporting_currency: 'USD',
      timestamp: '2024-07-01T10:00:00+00:00',
    }]);
    expect(json).toBe(expected);
  });

  it('matches expected Rust JSON format for listAll with empty storage', async () => {
    const storage = new MemoryStorage();
    const result = await listAll(storage, 'USD');
    const json = JSON.stringify(result);

    const expected = JSON.stringify({
      connections: [],
      accounts: [],
      price_sources: [],
      balances: [],
    });
    expect(json).toBe(expected);
  });
});
