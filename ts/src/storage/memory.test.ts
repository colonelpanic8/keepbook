import { describe, it, expect, beforeEach } from 'vitest';
import { MemoryStorage } from './memory.js';
import { Id } from '../models/id.js';
import { Account, type AccountConfig } from '../models/account.js';
import { Asset } from '../models/asset.js';
import { BalanceSnapshot, AssetBalance } from '../models/balance.js';
import { Connection, ConnectionState, type ConnectionConfig } from '../models/connection.js';
import { Transaction } from '../models/transaction.js';
import { FixedIdGenerator } from '../models/id-generator.js';
import { FixedClock } from '../clock.js';

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function makeConnection(name: string, id?: Id, createdAt?: Date) {
  const connId = id ?? Id.new();
  const ts = createdAt ?? new Date('2024-01-01T00:00:00Z');
  const config: ConnectionConfig = { name, synchronizer: 'test-sync' };
  const state = ConnectionState.newWith(connId, ts);
  return { config, state };
}

function makeAccount(name: string, connectionId: Id, id?: Id, createdAt?: Date) {
  const acctId = id ?? Id.new();
  const ts = createdAt ?? new Date('2024-01-01T00:00:00Z');
  return Account.newWith(acctId, ts, name, connectionId);
}

function makeTransaction(idStr: string, amount: string, description: string) {
  const ids = new FixedIdGenerator([Id.fromString(idStr)]);
  const clock = new FixedClock(new Date('2024-06-15T12:00:00Z'));
  return Transaction.newWithGenerator(ids, clock, amount, Asset.currency('USD'), description);
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe('MemoryStorage', () => {
  let storage: MemoryStorage;

  beforeEach(() => {
    storage = new MemoryStorage();
  });

  // -----------------------------------------------------------------------
  // Credentials
  // -----------------------------------------------------------------------

  describe('getCredentialStore', () => {
    it('always returns null', () => {
      const result = storage.getCredentialStore(Id.new());
      expect(result).toBeNull();
    });
  });

  // -----------------------------------------------------------------------
  // Connections
  // -----------------------------------------------------------------------

  describe('connections', () => {
    it('save and get connection', async () => {
      const conn = makeConnection('My Bank');
      await storage.saveConnection(conn);

      const fetched = await storage.getConnection(Connection.id(conn));
      expect(fetched).not.toBeNull();
      expect(Connection.name(fetched!)).toBe('My Bank');
    });

    it('get returns null for unknown id', async () => {
      const result = await storage.getConnection(Id.new());
      expect(result).toBeNull();
    });

    it('list connections returns all saved', async () => {
      const conn1 = makeConnection('Bank A');
      const conn2 = makeConnection('Bank B');
      await storage.saveConnection(conn1);
      await storage.saveConnection(conn2);

      const all = await storage.listConnections();
      expect(all).toHaveLength(2);
      const names = all.map((c) => Connection.name(c)).sort();
      expect(names).toEqual(['Bank A', 'Bank B']);
    });

    it('list connections returns empty when none saved', async () => {
      const all = await storage.listConnections();
      expect(all).toEqual([]);
    });

    it('delete connection returns true if existed', async () => {
      const conn = makeConnection('To Delete');
      await storage.saveConnection(conn);

      const deleted = await storage.deleteConnection(Connection.id(conn));
      expect(deleted).toBe(true);

      const fetched = await storage.getConnection(Connection.id(conn));
      expect(fetched).toBeNull();
    });

    it('delete connection returns false if not found', async () => {
      const deleted = await storage.deleteConnection(Id.new());
      expect(deleted).toBe(false);
    });

    it('save connection upserts (overwrites) by id', async () => {
      const id = Id.new();
      const conn1 = makeConnection('Original', id);
      await storage.saveConnection(conn1);

      const conn2 = makeConnection('Updated', id);
      await storage.saveConnection(conn2);

      const fetched = await storage.getConnection(id);
      expect(Connection.name(fetched!)).toBe('Updated');

      const all = await storage.listConnections();
      expect(all).toHaveLength(1);
    });

    it('saveConnectionConfig updates existing connection config', async () => {
      const conn = makeConnection('My Bank');
      await storage.saveConnection(conn);

      const newConfig: ConnectionConfig = {
        name: 'Renamed Bank',
        synchronizer: 'new-sync',
        balance_staleness: 3600,
      };
      await storage.saveConnectionConfig(Connection.id(conn), newConfig);

      const fetched = await storage.getConnection(Connection.id(conn));
      expect(fetched).not.toBeNull();
      expect(fetched!.config.name).toBe('Renamed Bank');
      expect(fetched!.config.synchronizer).toBe('new-sync');
      expect(fetched!.config.balance_staleness).toBe(3600);
      // State should be preserved
      expect(fetched!.state.id.equals(Connection.id(conn))).toBe(true);
    });

    it('saveConnectionConfig creates new connection when id not found', async () => {
      const id = Id.new();
      const config: ConnectionConfig = {
        name: 'Brand New',
        synchronizer: 'auto-sync',
      };
      await storage.saveConnectionConfig(id, config);

      const fetched = await storage.getConnection(id);
      expect(fetched).not.toBeNull();
      expect(fetched!.config.name).toBe('Brand New');
      expect(fetched!.state.id.equals(id)).toBe(true);
      expect(fetched!.state.status).toBe('active');
    });
  });

  // -----------------------------------------------------------------------
  // Accounts
  // -----------------------------------------------------------------------

  describe('accounts', () => {
    it('save and get account', async () => {
      const connId = Id.new();
      const acct = makeAccount('Checking', connId);
      await storage.saveAccount(acct);

      const fetched = await storage.getAccount(acct.id);
      expect(fetched).not.toBeNull();
      expect(fetched!.name).toBe('Checking');
      expect(fetched!.connection_id.equals(connId)).toBe(true);
    });

    it('get returns null for unknown id', async () => {
      const result = await storage.getAccount(Id.new());
      expect(result).toBeNull();
    });

    it('list accounts returns all saved', async () => {
      const connId = Id.new();
      const acct1 = makeAccount('Checking', connId);
      const acct2 = makeAccount('Savings', connId);
      await storage.saveAccount(acct1);
      await storage.saveAccount(acct2);

      const all = await storage.listAccounts();
      expect(all).toHaveLength(2);
      const names = all.map((a) => a.name).sort();
      expect(names).toEqual(['Checking', 'Savings']);
    });

    it('list accounts returns empty when none saved', async () => {
      const all = await storage.listAccounts();
      expect(all).toEqual([]);
    });

    it('delete account returns true if existed', async () => {
      const acct = makeAccount('ToDelete', Id.new());
      await storage.saveAccount(acct);

      const deleted = await storage.deleteAccount(acct.id);
      expect(deleted).toBe(true);

      const fetched = await storage.getAccount(acct.id);
      expect(fetched).toBeNull();
    });

    it('delete account returns false if not found', async () => {
      const deleted = await storage.deleteAccount(Id.new());
      expect(deleted).toBe(false);
    });

    it('save account upserts by id', async () => {
      const id = Id.new();
      const connId = Id.new();
      const acct1 = makeAccount('Original', connId, id);
      await storage.saveAccount(acct1);

      const acct2 = makeAccount('Updated', connId, id);
      await storage.saveAccount(acct2);

      const fetched = await storage.getAccount(id);
      expect(fetched!.name).toBe('Updated');

      const all = await storage.listAccounts();
      expect(all).toHaveLength(1);
    });
  });

  // -----------------------------------------------------------------------
  // Account Config
  // -----------------------------------------------------------------------

  describe('account config', () => {
    it('returns null when no config set', () => {
      const result = storage.getAccountConfig(Id.new());
      expect(result).toBeNull();
    });

    it('save and get account config', async () => {
      const id = Id.new();
      const config: AccountConfig = {
        balance_staleness: 7200,
        balance_backfill: 'zero',
      };
      await storage.saveAccountConfig(id, config);

      const fetched = storage.getAccountConfig(id);
      expect(fetched).not.toBeNull();
      expect(fetched!.balance_staleness).toBe(7200);
      expect(fetched!.balance_backfill).toBe('zero');
    });

    it('saveAccountConfig overwrites previous config', async () => {
      const id = Id.new();
      await storage.saveAccountConfig(id, { balance_backfill: 'none' });
      await storage.saveAccountConfig(id, { balance_backfill: 'carry_earliest' });

      const fetched = storage.getAccountConfig(id);
      expect(fetched!.balance_backfill).toBe('carry_earliest');
    });
  });

  // -----------------------------------------------------------------------
  // Balance Snapshots
  // -----------------------------------------------------------------------

  describe('balance snapshots', () => {
    it('append and get balance snapshots', async () => {
      const acctId = Id.new();
      const snap1 = BalanceSnapshot.new(new Date('2024-01-01T00:00:00Z'), [
        AssetBalance.new(Asset.currency('USD'), '1000.00'),
      ]);
      const snap2 = BalanceSnapshot.new(new Date('2024-02-01T00:00:00Z'), [
        AssetBalance.new(Asset.currency('USD'), '1500.00'),
      ]);

      await storage.appendBalanceSnapshot(acctId, snap1);
      await storage.appendBalanceSnapshot(acctId, snap2);

      const all = await storage.getBalanceSnapshots(acctId);
      expect(all).toHaveLength(2);
      expect(all[0].balances[0].amount).toBe('1000.00');
      expect(all[1].balances[0].amount).toBe('1500.00');
    });

    it('get balance snapshots returns empty for unknown account', async () => {
      const all = await storage.getBalanceSnapshots(Id.new());
      expect(all).toEqual([]);
    });

    it('get latest balance snapshot picks max timestamp', async () => {
      const acctId = Id.new();
      const older = BalanceSnapshot.new(new Date('2024-01-01T00:00:00Z'), [
        AssetBalance.new(Asset.currency('USD'), '500.00'),
      ]);
      const newer = BalanceSnapshot.new(new Date('2024-06-01T00:00:00Z'), [
        AssetBalance.new(Asset.currency('USD'), '2000.00'),
      ]);
      // Append in non-chronological order to verify max-timestamp logic
      await storage.appendBalanceSnapshot(acctId, newer);
      await storage.appendBalanceSnapshot(acctId, older);

      const latest = await storage.getLatestBalanceSnapshot(acctId);
      expect(latest).not.toBeNull();
      expect(latest!.balances[0].amount).toBe('2000.00');
      expect(latest!.timestamp.toISOString()).toBe('2024-06-01T00:00:00.000Z');
    });

    it('get latest balance snapshot returns null for unknown account', async () => {
      const latest = await storage.getLatestBalanceSnapshot(Id.new());
      expect(latest).toBeNull();
    });

    it('get latest balances across all accounts', async () => {
      const connId = Id.new();
      const acct1 = makeAccount('Checking', connId);
      const acct2 = makeAccount('Savings', connId);
      await storage.saveAccount(acct1);
      await storage.saveAccount(acct2);

      await storage.appendBalanceSnapshot(
        acct1.id,
        BalanceSnapshot.new(new Date('2024-01-01T00:00:00Z'), [
          AssetBalance.new(Asset.currency('USD'), '1000.00'),
        ]),
      );
      await storage.appendBalanceSnapshot(
        acct2.id,
        BalanceSnapshot.new(new Date('2024-02-01T00:00:00Z'), [
          AssetBalance.new(Asset.currency('USD'), '2000.00'),
        ]),
      );

      const result = await storage.getLatestBalances();
      expect(result).toHaveLength(2);

      // Results are [Id, BalanceSnapshotType] tuples
      const byAmount = new Map(result.map(([id, snap]) => [snap.balances[0].amount, id]));
      expect(byAmount.has('1000.00')).toBe(true);
      expect(byAmount.has('2000.00')).toBe(true);
    });

    it('get latest balances only includes accounts that exist', async () => {
      // Add balance for an account that is NOT saved in the accounts map
      const orphanId = Id.new();
      await storage.appendBalanceSnapshot(
        orphanId,
        BalanceSnapshot.new(new Date('2024-01-01T00:00:00Z'), [
          AssetBalance.new(Asset.currency('USD'), '999.00'),
        ]),
      );

      const result = await storage.getLatestBalances();
      expect(result).toHaveLength(0);
    });

    it('get latest balances for connection', async () => {
      const connId = Id.new();
      const conn = makeConnection('Bank', connId);
      await storage.saveConnection(conn);

      const acct1 = makeAccount('Checking', connId);
      const acct2 = makeAccount('Savings', connId);
      await storage.saveAccount(acct1);
      await storage.saveAccount(acct2);

      // Another connection's account should not appear
      const otherConnId = Id.new();
      const otherConn = makeConnection('Other', otherConnId);
      await storage.saveConnection(otherConn);
      const otherAcct = makeAccount('Other Checking', otherConnId);
      await storage.saveAccount(otherAcct);

      await storage.appendBalanceSnapshot(
        acct1.id,
        BalanceSnapshot.new(new Date('2024-01-01T00:00:00Z'), [
          AssetBalance.new(Asset.currency('USD'), '100.00'),
        ]),
      );
      await storage.appendBalanceSnapshot(
        acct2.id,
        BalanceSnapshot.new(new Date('2024-02-01T00:00:00Z'), [
          AssetBalance.new(Asset.currency('USD'), '200.00'),
        ]),
      );
      await storage.appendBalanceSnapshot(
        otherAcct.id,
        BalanceSnapshot.new(new Date('2024-03-01T00:00:00Z'), [
          AssetBalance.new(Asset.currency('USD'), '9999.00'),
        ]),
      );

      const result = await storage.getLatestBalancesForConnection(connId);
      expect(result).toHaveLength(2);
      const amounts = result.map(([, snap]) => snap.balances[0].amount).sort();
      expect(amounts).toEqual(['100.00', '200.00']);
    });

    it('get latest balances for connection throws if connection not found', async () => {
      await expect(storage.getLatestBalancesForConnection(Id.new())).rejects.toThrow(
        'Connection not found',
      );
    });
  });

  // -----------------------------------------------------------------------
  // Transactions
  // -----------------------------------------------------------------------

  describe('transactions', () => {
    it('append and get transactions (deduplicated)', async () => {
      const acctId = Id.new();
      const tx1 = makeTransaction('tx-1', '50.00', 'Coffee');
      const tx2 = makeTransaction('tx-2', '100.00', 'Groceries');

      await storage.appendTransactions(acctId, [tx1, tx2]);

      const all = await storage.getTransactions(acctId);
      expect(all).toHaveLength(2);
    });

    it('getTransactions deduplicates by id (last write wins)', async () => {
      const acctId = Id.new();
      const tx1 = makeTransaction('tx-dup', '50.00', 'First version');
      const tx2 = makeTransaction('tx-dup', '75.00', 'Updated version');

      await storage.appendTransactions(acctId, [tx1]);
      await storage.appendTransactions(acctId, [tx2]);

      const all = await storage.getTransactions(acctId);
      expect(all).toHaveLength(1);
      expect(all[0].amount).toBe('75.00');
      expect(all[0].description).toBe('Updated version');
    });

    it('getTransactionsRaw preserves all including duplicates', async () => {
      const acctId = Id.new();
      const tx1 = makeTransaction('tx-dup', '50.00', 'First version');
      const tx2 = makeTransaction('tx-dup', '75.00', 'Updated version');

      await storage.appendTransactions(acctId, [tx1]);
      await storage.appendTransactions(acctId, [tx2]);

      const raw = await storage.getTransactionsRaw(acctId);
      expect(raw).toHaveLength(2);
      expect(raw[0].amount).toBe('50.00');
      expect(raw[1].amount).toBe('75.00');
    });

    it('getTransactions returns empty for unknown account', async () => {
      const all = await storage.getTransactions(Id.new());
      expect(all).toEqual([]);
    });

    it('getTransactionsRaw returns empty for unknown account', async () => {
      const raw = await storage.getTransactionsRaw(Id.new());
      expect(raw).toEqual([]);
    });

    it('appendTransactions with empty array is a no-op', async () => {
      const acctId = Id.new();
      await storage.appendTransactions(acctId, []);

      const all = await storage.getTransactions(acctId);
      expect(all).toEqual([]);
    });
  });
});
