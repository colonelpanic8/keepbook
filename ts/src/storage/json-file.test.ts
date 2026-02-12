import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import * as fs from 'node:fs/promises';
import * as path from 'node:path';
import * as os from 'node:os';
import { JsonFileStorage } from './json-file.js';
import { Id } from '../models/id.js';
import { Account, type AccountConfig } from '../models/account.js';
import { Asset } from '../models/asset.js';
import { BalanceSnapshot, AssetBalance } from '../models/balance.js';
import { Connection, ConnectionState, type ConnectionConfig } from '../models/connection.js';
import { Transaction } from '../models/transaction.js';
import { type TransactionAnnotationPatchType } from '../models/transaction-annotation.js';
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

/**
 * Write a TOML connection config file to the proper location within a base dir.
 */
async function writeConnectionConfig(basePath: string, connId: Id, config: ConnectionConfig) {
  const dir = path.join(basePath, 'connections', connId.asStr());
  await fs.mkdir(dir, { recursive: true });
  let toml = `name = "${config.name}"\nsynchronizer = "${config.synchronizer}"\n`;
  if (config.balance_staleness !== undefined) {
    toml += `balance_staleness = ${config.balance_staleness}\n`;
  }
  await fs.writeFile(path.join(dir, 'connection.toml'), toml);
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe('JsonFileStorage', () => {
  let tmpDir: string;
  let storage: JsonFileStorage;

  beforeEach(async () => {
    tmpDir = await fs.mkdtemp(path.join(os.tmpdir(), 'keepbook-test-'));
    storage = new JsonFileStorage(tmpDir);
  });

  afterEach(async () => {
    await fs.rm(tmpDir, { recursive: true, force: true });
  });

  // -----------------------------------------------------------------------
  // Credentials
  // -----------------------------------------------------------------------

  describe('getCredentialStore', () => {
    it('returns null when no credentials configured', async () => {
      const connId = Id.fromString('conn-creds-none');
      await writeConnectionConfig(tmpDir, connId, { name: 'No Creds', synchronizer: 'coinbase' });
      const result = storage.getCredentialStore(connId);
      expect(result).toBeNull();
    });

    it('returns a pass store when inline credentials exist in connection.toml', async () => {
      const connId = Id.fromString('conn-creds-inline');
      const dir = path.join(tmpDir, 'connections', connId.asStr());
      await fs.mkdir(dir, { recursive: true });

      const toml = [
        'name = "Inline Creds"',
        'synchronizer = "coinbase"',
        '',
        '[credentials]',
        'backend = "pass"',
        'path = "finance/coinbase-api"',
        '',
        '[credentials.fields]',
        'key_name = "key-name"',
        'private_key = "private-key"',
        '',
      ].join('\n');
      await fs.writeFile(path.join(dir, 'connection.toml'), toml);

      const store = storage.getCredentialStore(connId);
      expect(store).not.toBeNull();
      expect(store!.supportsWrite()).toBe(true);
    });

    it('falls back to credentials.toml when connection.toml has no inline credentials', async () => {
      const connId = Id.fromString('conn-creds-file');
      await writeConnectionConfig(tmpDir, connId, { name: 'File Creds', synchronizer: 'coinbase' });

      const dir = path.join(tmpDir, 'connections', connId.asStr());
      const credsToml = [
        'backend = "pass"',
        'path = "finance/coinbase-api"',
        '',
        '[fields]',
        'key_name = "key-name"',
        'private_key = "private-key"',
        '',
      ].join('\n');
      await fs.writeFile(path.join(dir, 'credentials.toml'), credsToml);

      const store = storage.getCredentialStore(connId);
      expect(store).not.toBeNull();
      expect(store!.supportsWrite()).toBe(true);
    });
  });

  // -----------------------------------------------------------------------
  // Connections
  // -----------------------------------------------------------------------

  describe('connections', () => {
    it('save and get connection (state JSON + config TOML roundtrip)', async () => {
      const connId = Id.fromString('conn-1');
      const conn = makeConnection('My Bank', connId);

      // Write the TOML config (human-managed)
      await writeConnectionConfig(tmpDir, connId, conn.config);

      // saveConnection only writes state JSON
      await storage.saveConnection(conn);

      const fetched = await storage.getConnection(connId);
      expect(fetched).not.toBeNull();
      expect(Connection.name(fetched!)).toBe('My Bank');
      expect(fetched!.config.synchronizer).toBe('test-sync');
      expect(fetched!.state.id.equals(connId)).toBe(true);
      expect(fetched!.state.status).toBe('active');
    });

    it('get returns null for unknown id', async () => {
      const result = await storage.getConnection(Id.fromString('nonexistent'));
      expect(result).toBeNull();
    });

    it('list connections returns all saved', async () => {
      const id1 = Id.fromString('conn-a');
      const id2 = Id.fromString('conn-b');
      const conn1 = makeConnection('Bank A', id1);
      const conn2 = makeConnection('Bank B', id2);

      await writeConnectionConfig(tmpDir, id1, conn1.config);
      await writeConnectionConfig(tmpDir, id2, conn2.config);
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
      const connId = Id.fromString('conn-del');
      const conn = makeConnection('To Delete', connId);
      await writeConnectionConfig(tmpDir, connId, conn.config);
      await storage.saveConnection(conn);

      const deleted = await storage.deleteConnection(connId);
      expect(deleted).toBe(true);

      const fetched = await storage.getConnection(connId);
      expect(fetched).toBeNull();
    });

    it('delete connection returns false if not found', async () => {
      const deleted = await storage.deleteConnection(Id.fromString('no-such'));
      expect(deleted).toBe(false);
    });

    it('save connection upserts state (overwrites JSON)', async () => {
      const connId = Id.fromString('conn-upsert');
      const conn1 = makeConnection('Original', connId);
      await writeConnectionConfig(tmpDir, connId, conn1.config);
      await storage.saveConnection(conn1);

      // Save with updated account_ids
      const updatedState = {
        ...conn1.state,
        account_ids: [Id.fromString('acct-1')],
      };
      await storage.saveConnection({ config: conn1.config, state: updatedState });

      const fetched = await storage.getConnection(connId);
      expect(fetched).not.toBeNull();
      expect(fetched!.state.account_ids).toHaveLength(1);
      expect(fetched!.state.account_ids[0].asStr()).toBe('acct-1');
    });

    it('saveConnectionConfig writes TOML config', async () => {
      const connId = Id.fromString('conn-cfg');
      // Ensure the connection directory exists
      const connDir = path.join(tmpDir, 'connections', connId.asStr());
      await fs.mkdir(connDir, { recursive: true });

      const config: ConnectionConfig = {
        name: 'New Bank',
        synchronizer: 'plaid',
        balance_staleness: 3600,
      };
      await storage.saveConnectionConfig(connId, config);

      // Verify by reading back
      // First we need a state file to make getConnection work
      const state = ConnectionState.newWith(connId, new Date('2024-01-01T00:00:00Z'));
      await storage.saveConnection({ config, state });

      const fetched = await storage.getConnection(connId);
      expect(fetched).not.toBeNull();
      expect(fetched!.config.name).toBe('New Bank');
      expect(fetched!.config.synchronizer).toBe('plaid');
      expect(fetched!.config.balance_staleness).toBe(3600);
    });

    it('parses connection balance_staleness duration strings from TOML', async () => {
      const connId = Id.fromString('conn-dur');
      const connDir = path.join(tmpDir, 'connections', connId.asStr());
      await fs.mkdir(connDir, { recursive: true });
      await fs.writeFile(
        path.join(connDir, 'connection.toml'),
        'name = "Dur Bank"\nsynchronizer = "manual"\nbalance_staleness = "7d"\n',
      );

      const fetched = await storage.getConnection(connId);
      expect(fetched).not.toBeNull();
      expect(fetched!.config.balance_staleness).toBe(7 * 24 * 60 * 60 * 1000);
    });

    it('listConnections skips dirs without config TOML', async () => {
      // Create a directory without a connection.toml
      const badDir = path.join(tmpDir, 'connections', 'no-config');
      await fs.mkdir(badDir, { recursive: true });

      // Create a valid connection
      const connId = Id.fromString('valid-conn');
      const conn = makeConnection('Valid', connId);
      await writeConnectionConfig(tmpDir, connId, conn.config);
      await storage.saveConnection(conn);

      const all = await storage.listConnections();
      expect(all).toHaveLength(1);
      expect(Connection.name(all[0])).toBe('Valid');
    });

    it('getConnection creates default state if JSON missing', async () => {
      const connId = Id.fromString('conn-no-state');
      // Only write config, no state JSON
      await writeConnectionConfig(tmpDir, connId, { name: 'Config Only', synchronizer: 'manual' });

      const fetched = await storage.getConnection(connId);
      expect(fetched).not.toBeNull();
      expect(fetched!.config.name).toBe('Config Only');
      expect(fetched!.state.id.equals(connId)).toBe(true);
      expect(fetched!.state.status).toBe('active');
      expect(fetched!.state.account_ids).toEqual([]);
    });
  });

  // -----------------------------------------------------------------------
  // Accounts
  // -----------------------------------------------------------------------

  describe('accounts', () => {
    it('save and get account (JSON roundtrip)', async () => {
      const connId = Id.fromString('conn-1');
      const acctId = Id.fromString('acct-1');
      const acct = makeAccount('Checking', connId, acctId);
      await storage.saveAccount(acct);

      const fetched = await storage.getAccount(acctId);
      expect(fetched).not.toBeNull();
      expect(fetched!.name).toBe('Checking');
      expect(fetched!.connection_id.equals(connId)).toBe(true);
      expect(fetched!.active).toBe(true);
    });

    it('get returns null for unknown id', async () => {
      const result = await storage.getAccount(Id.fromString('no-such'));
      expect(result).toBeNull();
    });

    it('list accounts returns all saved', async () => {
      const connId = Id.fromString('conn-1');
      const acct1 = makeAccount('Checking', connId, Id.fromString('acct-1'));
      const acct2 = makeAccount('Savings', connId, Id.fromString('acct-2'));
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
      const acct = makeAccount('ToDelete', Id.fromString('conn-1'), Id.fromString('acct-del'));
      await storage.saveAccount(acct);

      const deleted = await storage.deleteAccount(acct.id);
      expect(deleted).toBe(true);

      const fetched = await storage.getAccount(acct.id);
      expect(fetched).toBeNull();
    });

    it('delete account returns false if not found', async () => {
      const deleted = await storage.deleteAccount(Id.fromString('no-such'));
      expect(deleted).toBe(false);
    });

    it('save account upserts by id', async () => {
      const connId = Id.fromString('conn-1');
      const acctId = Id.fromString('acct-upsert');
      const acct1 = makeAccount('Original', connId, acctId);
      await storage.saveAccount(acct1);

      const acct2 = makeAccount('Updated', connId, acctId);
      await storage.saveAccount(acct2);

      const fetched = await storage.getAccount(acctId);
      expect(fetched!.name).toBe('Updated');

      const all = await storage.listAccounts();
      expect(all).toHaveLength(1);
    });
  });

  // -----------------------------------------------------------------------
  // Account Config
  // -----------------------------------------------------------------------

  describe('account config', () => {
    it('returns null when no config file exists', () => {
      const result = storage.getAccountConfig(Id.fromString('no-config'));
      expect(result).toBeNull();
    });

    it('reads account_config.toml', async () => {
      const acctId = Id.fromString('acct-cfg');
      // First save the account so the directory exists
      const acct = makeAccount('Configured', Id.fromString('conn-1'), acctId);
      await storage.saveAccount(acct);

      // Write the config TOML
      const configToml = `balance_staleness = 7200\nbalance_backfill = "zero"\n`;
      const configPath = path.join(tmpDir, 'accounts', acctId.asStr(), 'account_config.toml');
      await fs.writeFile(configPath, configToml);

      const fetched = storage.getAccountConfig(acctId);
      expect(fetched).not.toBeNull();
      expect(fetched!.balance_staleness).toBe(7200);
      expect(fetched!.balance_backfill).toBe('zero');
    });

    it('saveAccountConfig writes TOML and reads back', async () => {
      const acctId = Id.fromString('acct-cfg2');
      // Create the account dir
      const acct = makeAccount('ForConfig', Id.fromString('conn-1'), acctId);
      await storage.saveAccount(acct);

      const config: AccountConfig = {
        balance_staleness: 1800,
        balance_backfill: 'carry_earliest',
      };
      await storage.saveAccountConfig(acctId, config);

      const fetched = storage.getAccountConfig(acctId);
      expect(fetched).not.toBeNull();
      expect(fetched!.balance_staleness).toBe(1800);
      expect(fetched!.balance_backfill).toBe('carry_earliest');
    });

    it('parses account balance_staleness duration strings from TOML', async () => {
      const acctId = Id.fromString('acct-cfg-dur');
      const acct = makeAccount('Configured', Id.fromString('conn-1'), acctId);
      await storage.saveAccount(acct);

      const configPath = path.join(tmpDir, 'accounts', acctId.asStr(), 'account_config.toml');
      await fs.writeFile(configPath, 'balance_staleness = "12h"\nbalance_backfill = "none"\n');

      const fetched = storage.getAccountConfig(acctId);
      expect(fetched).not.toBeNull();
      expect(fetched!.balance_staleness).toBe(12 * 60 * 60 * 1000);
      expect(fetched!.balance_backfill).toBe('none');
    });
  });

  // -----------------------------------------------------------------------
  // Balance Snapshots
  // -----------------------------------------------------------------------

  describe('balance snapshots', () => {
    it('append and get balance snapshots (JSONL)', async () => {
      const acctId = Id.fromString('acct-bal');
      // Ensure account dir exists
      const acct = makeAccount('Balances', Id.fromString('conn-1'), acctId);
      await storage.saveAccount(acct);

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
      const all = await storage.getBalanceSnapshots(Id.fromString('no-such'));
      expect(all).toEqual([]);
    });

    it('get latest balance snapshot picks max timestamp', async () => {
      const acctId = Id.fromString('acct-latest');
      const acct = makeAccount('Latest', Id.fromString('conn-1'), acctId);
      await storage.saveAccount(acct);

      const older = BalanceSnapshot.new(new Date('2024-01-01T00:00:00Z'), [
        AssetBalance.new(Asset.currency('USD'), '500.00'),
      ]);
      const newer = BalanceSnapshot.new(new Date('2024-06-01T00:00:00Z'), [
        AssetBalance.new(Asset.currency('USD'), '2000.00'),
      ]);
      // Append in non-chronological order
      await storage.appendBalanceSnapshot(acctId, newer);
      await storage.appendBalanceSnapshot(acctId, older);

      const latest = await storage.getLatestBalanceSnapshot(acctId);
      expect(latest).not.toBeNull();
      expect(latest!.balances[0].amount).toBe('2000.00');
      expect(latest!.timestamp.toISOString()).toBe('2024-06-01T00:00:00.000Z');
    });

    it('get latest balance snapshot returns null for unknown account', async () => {
      const latest = await storage.getLatestBalanceSnapshot(Id.fromString('no-such'));
      expect(latest).toBeNull();
    });

    it('get latest balances across all accounts', async () => {
      const connId = Id.fromString('conn-bal');
      const conn = makeConnection('Bank', connId);
      await writeConnectionConfig(tmpDir, connId, conn.config);
      await storage.saveConnection(conn);

      const acct1 = makeAccount('Checking', connId, Id.fromString('acct-b1'));
      const acct2 = makeAccount('Savings', connId, Id.fromString('acct-b2'));
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
      const amounts = result.map(([, snap]) => snap.balances[0].amount).sort();
      expect(amounts).toEqual(['1000.00', '2000.00']);
    });

    it('get latest balances for connection', async () => {
      const connId = Id.fromString('conn-filt');
      const conn = makeConnection('Bank', connId);
      await writeConnectionConfig(tmpDir, connId, conn.config);
      await storage.saveConnection(conn);

      const acct1 = makeAccount('Checking', connId, Id.fromString('acct-f1'));
      const acct2 = makeAccount('Savings', connId, Id.fromString('acct-f2'));
      await storage.saveAccount(acct1);
      await storage.saveAccount(acct2);

      // Other connection's account
      const otherConnId = Id.fromString('conn-other');
      const otherConn = makeConnection('Other', otherConnId);
      await writeConnectionConfig(tmpDir, otherConnId, otherConn.config);
      await storage.saveConnection(otherConn);
      const otherAcct = makeAccount('Other Checking', otherConnId, Id.fromString('acct-other'));
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
      await expect(
        storage.getLatestBalancesForConnection(Id.fromString('no-such')),
      ).rejects.toThrow('Connection not found');
    });
  });

  // -----------------------------------------------------------------------
  // Transactions
  // -----------------------------------------------------------------------

  describe('transactions', () => {
    it('append and get transactions (JSONL)', async () => {
      const acctId = Id.fromString('acct-tx');
      const acct = makeAccount('Txns', Id.fromString('conn-1'), acctId);
      await storage.saveAccount(acct);

      const tx1 = makeTransaction('tx-1', '50.00', 'Coffee');
      const tx2 = makeTransaction('tx-2', '100.00', 'Groceries');

      await storage.appendTransactions(acctId, [tx1, tx2]);

      const all = await storage.getTransactions(acctId);
      expect(all).toHaveLength(2);
    });

    it('getTransactions deduplicates by id (last write wins)', async () => {
      const acctId = Id.fromString('acct-dedup');
      const acct = makeAccount('Dedup', Id.fromString('conn-1'), acctId);
      await storage.saveAccount(acct);

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
      const acctId = Id.fromString('acct-raw');
      const acct = makeAccount('Raw', Id.fromString('conn-1'), acctId);
      await storage.saveAccount(acct);

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
      const all = await storage.getTransactions(Id.fromString('no-such'));
      expect(all).toEqual([]);
    });

    it('getTransactionsRaw returns empty for unknown account', async () => {
      const raw = await storage.getTransactionsRaw(Id.fromString('no-such'));
      expect(raw).toEqual([]);
    });

    it('appendTransactions with empty array is a no-op', async () => {
      const acctId = Id.fromString('acct-empty');
      await storage.appendTransactions(acctId, []);

      const all = await storage.getTransactions(acctId);
      expect(all).toEqual([]);
    });
  });

  // -----------------------------------------------------------------------
  // Transaction annotations
  // -----------------------------------------------------------------------

  describe('transaction annotations', () => {
    it('append and get transaction annotation patches (JSONL)', async () => {
      const acctId = Id.fromString('acct-ann');
      const acct = makeAccount('Ann', Id.fromString('conn-1'), acctId);
      await storage.saveAccount(acct);

      const p1: TransactionAnnotationPatchType = {
        transaction_id: Id.fromString('tx-1'),
        timestamp: new Date('2024-06-15T12:00:00Z'),
        description: 'Coffee shop',
      };
      const p2: TransactionAnnotationPatchType = {
        transaction_id: Id.fromString('tx-1'),
        timestamp: new Date('2024-06-15T12:00:01Z'),
        note: null, // explicit clear
      };

      await storage.appendTransactionAnnotationPatches(acctId, [p1, p2]);

      const all = await storage.getTransactionAnnotationPatches(acctId);
      expect(all).toHaveLength(2);
      expect(all[0].transaction_id.asStr()).toBe('tx-1');
      expect(all[0].description).toBe('Coffee shop');
      expect(all[1].note).toBeNull();
    });

    it('returns empty array for unknown account', async () => {
      const all = await storage.getTransactionAnnotationPatches(Id.fromString('no-such'));
      expect(all).toEqual([]);
    });

    it('appendTransactionAnnotationPatches with empty array is a no-op', async () => {
      const acctId = Id.fromString('acct-ann-empty');
      await storage.appendTransactionAnnotationPatches(acctId, []);
      const all = await storage.getTransactionAnnotationPatches(acctId);
      expect(all).toEqual([]);
    });
  });

  // -----------------------------------------------------------------------
  // Path safety validation
  // -----------------------------------------------------------------------

  describe('path safety', () => {
    it('rejects unsafe id with slashes', async () => {
      await expect(storage.getAccount(Id.fromString('../../etc/passwd'))).rejects.toThrow();
    });

    it('rejects dot id', async () => {
      await expect(storage.getAccount(Id.fromString('.'))).rejects.toThrow();
    });

    it('rejects double dot id', async () => {
      await expect(storage.getAccount(Id.fromString('..'))).rejects.toThrow();
    });
  });

  // -----------------------------------------------------------------------
  // JSONL file format verification
  // -----------------------------------------------------------------------

  describe('JSONL file format', () => {
    it('balances file is valid JSONL (one JSON object per line)', async () => {
      const acctId = Id.fromString('acct-jsonl');
      const acct = makeAccount('JSONL', Id.fromString('conn-1'), acctId);
      await storage.saveAccount(acct);

      const snap1 = BalanceSnapshot.new(new Date('2024-01-01T00:00:00Z'), [
        AssetBalance.new(Asset.currency('USD'), '100.00'),
      ]);
      const snap2 = BalanceSnapshot.new(new Date('2024-02-01T00:00:00Z'), [
        AssetBalance.new(Asset.currency('EUR'), '200.00'),
      ]);

      await storage.appendBalanceSnapshot(acctId, snap1);
      await storage.appendBalanceSnapshot(acctId, snap2);

      // Read the raw file and verify JSONL format
      const filePath = path.join(tmpDir, 'accounts', acctId.asStr(), 'balances.jsonl');
      const content = await fs.readFile(filePath, 'utf-8');
      const lines = content.trim().split('\n');
      expect(lines).toHaveLength(2);

      // Each line should parse as valid JSON
      const parsed1 = JSON.parse(lines[0]);
      expect(parsed1.timestamp).toBe('2024-01-01T00:00:00.000Z');
      const parsed2 = JSON.parse(lines[1]);
      expect(parsed2.timestamp).toBe('2024-02-01T00:00:00.000Z');
    });

    it('transactions file is valid JSONL', async () => {
      const acctId = Id.fromString('acct-txjsonl');
      const acct = makeAccount('TxJSONL', Id.fromString('conn-1'), acctId);
      await storage.saveAccount(acct);

      const tx = makeTransaction('tx-j1', '42.00', 'Test');
      await storage.appendTransactions(acctId, [tx]);

      const filePath = path.join(tmpDir, 'accounts', acctId.asStr(), 'transactions.jsonl');
      const content = await fs.readFile(filePath, 'utf-8');
      const lines = content.trim().split('\n');
      expect(lines).toHaveLength(1);

      const parsed = JSON.parse(lines[0]);
      expect(parsed.id).toBe('tx-j1');
      expect(parsed.amount).toBe('42.00');
    });

    it('transaction annotations file is valid JSONL', async () => {
      const acctId = Id.fromString('acct-annjsonl');
      const acct = makeAccount('AnnJSONL', Id.fromString('conn-1'), acctId);
      await storage.saveAccount(acct);

      const p: TransactionAnnotationPatchType = {
        transaction_id: Id.fromString('tx-1'),
        timestamp: new Date('2024-06-15T12:00:00Z'),
        category: 'food',
        tags: ['coffee', 'treat'],
      };
      await storage.appendTransactionAnnotationPatches(acctId, [p]);

      const filePath = path.join(tmpDir, 'accounts', acctId.asStr(), 'transaction_annotations.jsonl');
      const content = await fs.readFile(filePath, 'utf-8');
      const lines = content.trim().split('\n');
      expect(lines).toHaveLength(1);

      const parsed = JSON.parse(lines[0]);
      expect(parsed.transaction_id).toBe('tx-1');
      expect(parsed.category).toBe('food');
      expect(parsed.tags).toEqual(['coffee', 'treat']);
    });
  });

  // -----------------------------------------------------------------------
  // Directory structure verification
  // -----------------------------------------------------------------------

  describe('directory structure', () => {
    it('creates correct connection directory structure', async () => {
      const connId = Id.fromString('conn-struct');
      const conn = makeConnection('Structure Test', connId);
      await writeConnectionConfig(tmpDir, connId, conn.config);
      await storage.saveConnection(conn);

      // Verify connection.json exists
      const statePath = path.join(tmpDir, 'connections', 'conn-struct', 'connection.json');
      const stat = await fs.stat(statePath);
      expect(stat.isFile()).toBe(true);

      // Verify connection.toml exists
      const configPath = path.join(tmpDir, 'connections', 'conn-struct', 'connection.toml');
      const configStat = await fs.stat(configPath);
      expect(configStat.isFile()).toBe(true);
    });

    it('creates correct account directory structure', async () => {
      const acctId = Id.fromString('acct-struct');
      const acct = makeAccount('Structure', Id.fromString('conn-1'), acctId);
      await storage.saveAccount(acct);

      // Verify account.json exists
      const jsonPath = path.join(tmpDir, 'accounts', 'acct-struct', 'account.json');
      const stat = await fs.stat(jsonPath);
      expect(stat.isFile()).toBe(true);
    });
  });
});
