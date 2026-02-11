import { describe, it, expect, beforeEach } from 'vitest';
import { MemoryStorage } from '../storage/memory.js';
import { Id } from '../models/id.js';
import { Account } from '../models/account.js';
import { Asset } from '../models/asset.js';
import { AssetBalance } from '../models/balance.js';
import { Connection, ConnectionState, type ConnectionConfig } from '../models/connection.js';
import { Transaction } from '../models/transaction.js';
import { FixedIdGenerator } from '../models/id-generator.js';
import { FixedClock } from '../clock.js';
import { AssetId } from '../market-data/asset-id.js';
import type { PricePoint } from '../market-data/models.js';
import {
  SyncedAssetBalanceFactory,
  saveSyncResult,
  type SyncResult,
  type AuthStatus,
} from './mod.js';

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

const fixedDate = new Date('2024-06-15T12:00:00.000Z');

function makeConnection(name: string, id?: Id): ReturnType<typeof Connection.new> {
  const connId = id ?? Id.new();
  const config: ConnectionConfig = { name, synchronizer: 'test-sync' };
  const state = ConnectionState.newWith(connId, new Date('2024-01-01T00:00:00Z'));
  return { config, state };
}

function makeAccount(name: string, connectionId: Id, id?: Id) {
  const acctId = id ?? Id.new();
  return Account.newWith(acctId, new Date('2024-01-01T00:00:00Z'), name, connectionId);
}

function makeTransaction(idStr: string, amount: string, description: string) {
  const ids = new FixedIdGenerator([Id.fromString(idStr)]);
  const clock = new FixedClock(fixedDate);
  return Transaction.newWithGenerator(ids, clock, amount, Asset.currency('USD'), description);
}

function makePricePoint(): PricePoint {
  return {
    asset_id: AssetId.fromAsset(Asset.currency('USD')),
    as_of_date: '2024-06-15',
    timestamp: fixedDate,
    price: '1.00',
    quote_currency: 'USD',
    kind: 'close',
    source: 'test',
  };
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe('SyncedAssetBalance', () => {
  describe('factory new', () => {
    it('creates a SyncedAssetBalance without a price', () => {
      const ab = AssetBalance.new(Asset.currency('USD'), '1000.00');
      const sab = SyncedAssetBalanceFactory.new(ab);

      expect(sab.asset_balance).toBe(ab);
      expect(sab.price).toBeUndefined();
    });
  });

  describe('withPrice builder', () => {
    it('returns a new SyncedAssetBalance with a price set', () => {
      const ab = AssetBalance.new(Asset.currency('USD'), '1000.00');
      const sab = SyncedAssetBalanceFactory.new(ab);
      const price = makePricePoint();

      const sabWithPrice = SyncedAssetBalanceFactory.withPrice(sab, price);

      expect(sabWithPrice.asset_balance).toBe(ab);
      expect(sabWithPrice.price).toBe(price);
      // Original should be unchanged (immutability)
      expect(sab.price).toBeUndefined();
    });
  });
});

describe('AuthStatus', () => {
  it('represents valid status', () => {
    const status: AuthStatus = { type: 'valid' };
    expect(status.type).toBe('valid');
  });

  it('represents missing status', () => {
    const status: AuthStatus = { type: 'missing' };
    expect(status.type).toBe('missing');
  });

  it('represents expired status with reason', () => {
    const status: AuthStatus = { type: 'expired', reason: 'Token expired at 2024-06-15' };
    expect(status.type).toBe('expired');
    expect(status.reason).toBe('Token expired at 2024-06-15');
  });

  it('can be narrowed via discriminated union', () => {
    const status: AuthStatus = { type: 'expired', reason: 'session timeout' };

    // Type narrowing
    if (status.type === 'expired') {
      expect(status.reason).toBe('session timeout');
    } else {
      // Should not reach here
      expect.unreachable('Expected expired status');
    }
  });
});

describe('saveSyncResult', () => {
  let storage: MemoryStorage;
  const clock = new FixedClock(fixedDate);

  beforeEach(() => {
    storage = new MemoryStorage();
  });

  it('saves accounts and connection', async () => {
    const connId = Id.fromString('conn-1');
    const conn = makeConnection('Test Bank', connId);
    const acct1 = makeAccount('Checking', connId, Id.fromString('acct-1'));
    const acct2 = makeAccount('Savings', connId, Id.fromString('acct-2'));

    const result: SyncResult = {
      connection: conn,
      accounts: [acct1, acct2],
      balances: [],
      transactions: [],
    };

    await saveSyncResult(result, storage, clock);

    // Verify accounts were saved
    const savedAcct1 = await storage.getAccount(Id.fromString('acct-1'));
    expect(savedAcct1).not.toBeNull();
    expect(savedAcct1!.name).toBe('Checking');

    const savedAcct2 = await storage.getAccount(Id.fromString('acct-2'));
    expect(savedAcct2).not.toBeNull();
    expect(savedAcct2!.name).toBe('Savings');

    // Verify connection was saved
    const savedConn = await storage.getConnection(connId);
    expect(savedConn).not.toBeNull();
    expect(Connection.name(savedConn!)).toBe('Test Bank');
  });

  it('creates balance snapshots from synced balances', async () => {
    const connId = Id.fromString('conn-1');
    const conn = makeConnection('Test Bank', connId);
    const acctId = Id.fromString('acct-1');
    const acct = makeAccount('Checking', connId, acctId);

    const ab1 = AssetBalance.new(Asset.currency('USD'), '5000.00');
    const ab2 = AssetBalance.new(Asset.crypto('BTC'), '0.5');
    const sab1 = SyncedAssetBalanceFactory.new(ab1);
    const sab2 = SyncedAssetBalanceFactory.withPrice(
      SyncedAssetBalanceFactory.new(ab2),
      makePricePoint(),
    );

    const result: SyncResult = {
      connection: conn,
      accounts: [acct],
      balances: [[acctId, [sab1, sab2]]],
      transactions: [],
    };

    await saveSyncResult(result, storage, clock);

    const snapshots = await storage.getBalanceSnapshots(acctId);
    expect(snapshots).toHaveLength(1);

    const snap = snapshots[0];
    expect(snap.timestamp.getTime()).toBe(fixedDate.getTime());
    expect(snap.balances).toHaveLength(2);
    expect(snap.balances[0].amount).toBe('5000.00');
    expect(snap.balances[1].amount).toBe('0.5');
  });

  it('skips empty balance arrays', async () => {
    const connId = Id.fromString('conn-1');
    const conn = makeConnection('Test Bank', connId);
    const acctId = Id.fromString('acct-1');
    const acct = makeAccount('Checking', connId, acctId);

    const result: SyncResult = {
      connection: conn,
      accounts: [acct],
      balances: [[acctId, []]],
      transactions: [],
    };

    await saveSyncResult(result, storage, clock);

    const snapshots = await storage.getBalanceSnapshots(acctId);
    expect(snapshots).toHaveLength(0);
  });

  it('appends new transactions', async () => {
    const connId = Id.fromString('conn-1');
    const conn = makeConnection('Test Bank', connId);
    const acctId = Id.fromString('acct-1');
    const acct = makeAccount('Checking', connId, acctId);

    const tx1 = makeTransaction('tx-1', '50.00', 'Coffee');
    const tx2 = makeTransaction('tx-2', '100.00', 'Groceries');

    const result: SyncResult = {
      connection: conn,
      accounts: [acct],
      balances: [],
      transactions: [[acctId, [tx1, tx2]]],
    };

    await saveSyncResult(result, storage, clock);

    const txns = await storage.getTransactions(acctId);
    expect(txns).toHaveLength(2);
    const descriptions = txns.map((t) => t.description).sort();
    expect(descriptions).toEqual(['Coffee', 'Groceries']);
  });

  it('skips unchanged transactions (idempotent)', async () => {
    const connId = Id.fromString('conn-1');
    const conn = makeConnection('Test Bank', connId);
    const acctId = Id.fromString('acct-1');
    const acct = makeAccount('Checking', connId, acctId);

    const tx1 = makeTransaction('tx-1', '50.00', 'Coffee');

    // First save: append the transaction
    await storage.saveAccount(acct);
    await storage.appendTransactions(acctId, [tx1]);

    // Now sync with the same transaction
    const result: SyncResult = {
      connection: conn,
      accounts: [acct],
      balances: [],
      transactions: [[acctId, [tx1]]],
    };

    await saveSyncResult(result, storage, clock);

    // Raw transactions should still show only 1 (no duplicate appended)
    const rawTxns = await storage.getTransactionsRaw(acctId);
    expect(rawTxns).toHaveLength(1);
    expect(rawTxns[0].description).toBe('Coffee');
  });

  it('updates changed transactions', async () => {
    const connId = Id.fromString('conn-1');
    const conn = makeConnection('Test Bank', connId);
    const acctId = Id.fromString('acct-1');
    const acct = makeAccount('Checking', connId, acctId);

    const txOriginal = makeTransaction('tx-1', '50.00', 'Coffee');

    // First save
    await storage.saveAccount(acct);
    await storage.appendTransactions(acctId, [txOriginal]);

    // Now sync with the same id but different amount (e.g. pending -> posted adjustment)
    const txUpdated = makeTransaction('tx-1', '55.00', 'Coffee');

    const result: SyncResult = {
      connection: conn,
      accounts: [acct],
      balances: [],
      transactions: [[acctId, [txUpdated]]],
    };

    await saveSyncResult(result, storage, clock);

    // Raw should have 2 entries (original + updated version)
    const rawTxns = await storage.getTransactionsRaw(acctId);
    expect(rawTxns).toHaveLength(2);

    // Deduplicated view should show updated version
    const txns = await storage.getTransactions(acctId);
    expect(txns).toHaveLength(1);
    expect(txns[0].amount).toBe('55.00');
  });

  it('deduplicates within batch (last write wins)', async () => {
    const connId = Id.fromString('conn-1');
    const conn = makeConnection('Test Bank', connId);
    const acctId = Id.fromString('acct-1');
    const acct = makeAccount('Checking', connId, acctId);

    // Two transactions with the same id in the same batch
    const txFirst = makeTransaction('tx-dup', '50.00', 'First version');
    const txSecond = makeTransaction('tx-dup', '75.00', 'Second version');

    const result: SyncResult = {
      connection: conn,
      accounts: [acct],
      balances: [],
      transactions: [[acctId, [txFirst, txSecond]]],
    };

    await saveSyncResult(result, storage, clock);

    // Only the second (last write) should be appended
    const rawTxns = await storage.getTransactionsRaw(acctId);
    expect(rawTxns).toHaveLength(1);
    expect(rawTxns[0].amount).toBe('75.00');
    expect(rawTxns[0].description).toBe('Second version');
  });

  it('uses SystemClock when no clock is provided', async () => {
    const connId = Id.fromString('conn-1');
    const conn = makeConnection('Test Bank', connId);
    const acctId = Id.fromString('acct-1');
    const acct = makeAccount('Checking', connId, acctId);

    const ab = AssetBalance.new(Asset.currency('USD'), '1000.00');
    const sab = SyncedAssetBalanceFactory.new(ab);

    const result: SyncResult = {
      connection: conn,
      accounts: [acct],
      balances: [[acctId, [sab]]],
      transactions: [],
    };

    // Call without clock argument (should use SystemClock)
    await saveSyncResult(result, storage);

    const snapshots = await storage.getBalanceSnapshots(acctId);
    expect(snapshots).toHaveLength(1);
    // The timestamp should be close to "now"
    expect(Date.now() - snapshots[0].timestamp.getTime()).toBeLessThan(2000);
  });

  it('skips empty transaction arrays', async () => {
    const connId = Id.fromString('conn-1');
    const conn = makeConnection('Test Bank', connId);
    const acctId = Id.fromString('acct-1');
    const acct = makeAccount('Checking', connId, acctId);

    const result: SyncResult = {
      connection: conn,
      accounts: [acct],
      balances: [],
      transactions: [[acctId, []]],
    };

    await saveSyncResult(result, storage, clock);

    const rawTxns = await storage.getTransactionsRaw(acctId);
    expect(rawTxns).toHaveLength(0);
  });

  it('detects change in status field', async () => {
    const connId = Id.fromString('conn-1');
    const conn = makeConnection('Test Bank', connId);
    const acctId = Id.fromString('acct-1');
    const acct = makeAccount('Checking', connId, acctId);

    const ids = new FixedIdGenerator([Id.fromString('tx-status')]);
    const txClock = new FixedClock(fixedDate);
    const txPending: ReturnType<typeof Transaction.newWithGenerator> = {
      ...Transaction.newWithGenerator(ids, txClock, '50.00', Asset.currency('USD'), 'Coffee'),
      status: 'pending',
    };

    // First save
    await storage.saveAccount(acct);
    await storage.appendTransactions(acctId, [txPending]);

    // Same tx but now posted
    const txPosted = { ...txPending, status: 'posted' as const };

    const result: SyncResult = {
      connection: conn,
      accounts: [acct],
      balances: [],
      transactions: [[acctId, [txPosted]]],
    };

    await saveSyncResult(result, storage, clock);

    // Should have appended the updated version
    const rawTxns = await storage.getTransactionsRaw(acctId);
    expect(rawTxns).toHaveLength(2);

    const txns = await storage.getTransactions(acctId);
    expect(txns).toHaveLength(1);
    expect(txns[0].status).toBe('posted');
  });

  it('detects change in synchronizer_data field', async () => {
    const connId = Id.fromString('conn-1');
    const conn = makeConnection('Test Bank', connId);
    const acctId = Id.fromString('acct-1');
    const acct = makeAccount('Checking', connId, acctId);

    const ids = new FixedIdGenerator([Id.fromString('tx-sd')]);
    const txClock = new FixedClock(fixedDate);
    const txOriginal = Transaction.newWithGenerator(
      ids,
      txClock,
      '50.00',
      Asset.currency('USD'),
      'Coffee',
    );

    // First save
    await storage.saveAccount(acct);
    await storage.appendTransactions(acctId, [txOriginal]);

    // Same tx but with updated synchronizer_data
    const txUpdated = { ...txOriginal, synchronizer_data: { plaid_id: 'abc123' } };

    const result: SyncResult = {
      connection: conn,
      accounts: [acct],
      balances: [],
      transactions: [[acctId, [txUpdated]]],
    };

    await saveSyncResult(result, storage, clock);

    const rawTxns = await storage.getTransactionsRaw(acctId);
    expect(rawTxns).toHaveLength(2);
  });
});
