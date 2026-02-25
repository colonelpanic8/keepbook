/**
 * AsyncStorage-backed implementation of the keepbook Storage interface.
 *
 * This adapter reads financial data that was synced from a git repository
 * and stored in AsyncStorage with keys like:
 *   keepbook.file.{dataDir}.data/accounts/{id}/balances.jsonl
 *   keepbook.file.{dataDir}.data/connections/{id}/connection.json
 *   keepbook.manifest.{dataDir}  (JSON array of all relative paths)
 *
 * This is a READ-ONLY adapter. All write methods throw.
 */

import AsyncStorage from '@react-native-async-storage/async-storage';
import type { Storage, CredentialStore } from '@keepbook/storage/storage';
import { Id } from '@keepbook/models/id';
import { Account, type AccountType, type AccountConfig } from '@keepbook/models/account';
import {
  BalanceSnapshot,
  type BalanceSnapshotType,
  type BalanceSnapshotJSON,
} from '@keepbook/models/balance';
import {
  ConnectionState,
  type ConnectionType,
  type ConnectionConfig,
  type ConnectionStateJSON,
} from '@keepbook/models/connection';
import { Transaction, type TransactionType, type TransactionJSON } from '@keepbook/models/transaction';
import {
  TransactionAnnotationPatch,
  type TransactionAnnotationPatchType,
  type TransactionAnnotationPatchJSON,
} from '@keepbook/models/transaction-annotation';
import { dedupeTransactionsLastWriteWins } from '@keepbook/storage/dedupe';
import type { AccountJSON } from '@keepbook/models/account';

// ---------------------------------------------------------------------------
// Simple TOML parser for connection config
// ---------------------------------------------------------------------------

function parseTomlStringValue(toml: string, key: string): string | null {
  const re = new RegExp(`^\\s*${key}\\s*=\\s*"([^"]*)"\\s*$`, 'm');
  const m = toml.match(re);
  return m ? m[1] : null;
}

function parseConnectionConfigFromToml(toml: string): ConnectionConfig {
  return {
    name: parseTomlStringValue(toml, 'name') ?? 'unknown',
    synchronizer: parseTomlStringValue(toml, 'synchronizer') ?? 'unknown',
  };
}

// ---------------------------------------------------------------------------
// AsyncStorageStorage
// ---------------------------------------------------------------------------

export class AsyncStorageStorage implements Storage {
  private dataDir: string;
  private manifestCache: string[] | null = null;

  constructor(dataDir: string) {
    this.dataDir = dataDir;
  }

  /** Invalidate cached manifest (e.g. after a sync). */
  clearCache(): void {
    this.manifestCache = null;
  }

  private fileKey(relativePath: string): string {
    return `keepbook.file.${this.dataDir}.${relativePath}`;
  }

  private manifestKey(): string {
    return `keepbook.manifest.${this.dataDir}`;
  }

  private async getManifest(): Promise<string[]> {
    if (this.manifestCache) return this.manifestCache;
    const raw = await AsyncStorage.getItem(this.manifestKey());
    this.manifestCache = raw ? JSON.parse(raw) : [];
    return this.manifestCache!;
  }

  private async readFile(relativePath: string): Promise<string | null> {
    return AsyncStorage.getItem(this.fileKey(relativePath));
  }

  private parseJsonl<T>(content: string): T[] {
    return content
      .split('\n')
      .filter((line) => line.trim())
      .map((line) => JSON.parse(line) as T);
  }

  // -----------------------------------------------------------------------
  // Credentials (not supported)
  // -----------------------------------------------------------------------

  getCredentialStore(_connectionId: Id): CredentialStore | null {
    return null;
  }

  // -----------------------------------------------------------------------
  // Account Config (not supported -- would need TOML parsing of account_config.toml)
  // -----------------------------------------------------------------------

  getAccountConfig(_accountId: Id): AccountConfig | null {
    return null;
  }

  // -----------------------------------------------------------------------
  // Connections
  // -----------------------------------------------------------------------

  async listConnections(): Promise<ConnectionType[]> {
    const manifest = await this.getManifest();

    // Find all connection.json files
    const connJsonPaths = manifest.filter((p) =>
      /^data\/connections\/[^/]+\/connection\.json$/.test(p),
    );
    const connTomlSet = new Set(
      manifest.filter((p) => /^data\/connections\/[^/]+\/connection\.toml$/.test(p)),
    );

    const connections: ConnectionType[] = [];
    for (const jsonPath of connJsonPaths) {
      try {
        const id = jsonPath.split('/')[2];
        if (!id) continue;

        const stateRaw = await this.readFile(jsonPath);
        if (!stateRaw) continue;

        const stateJson = JSON.parse(stateRaw) as ConnectionStateJSON;
        const state = ConnectionState.fromJSON(stateJson);

        // Parse config from TOML if available
        const tomlPath = `data/connections/${id}/connection.toml`;
        let config: ConnectionConfig;
        if (connTomlSet.has(tomlPath)) {
          const tomlRaw = await this.readFile(tomlPath);
          config = tomlRaw
            ? parseConnectionConfigFromToml(tomlRaw)
            : { name: id, synchronizer: 'unknown' };
        } else {
          config = { name: id, synchronizer: 'unknown' };
        }

        connections.push({ config, state });
      } catch {
        // Skip invalid connections
      }
    }

    return connections;
  }

  async getConnection(id: Id): Promise<ConnectionType | null> {
    const idStr = id.asStr();
    const jsonPath = `data/connections/${idStr}/connection.json`;
    const stateRaw = await this.readFile(jsonPath);
    if (!stateRaw) return null;

    try {
      const stateJson = JSON.parse(stateRaw) as ConnectionStateJSON;
      const state = ConnectionState.fromJSON(stateJson);

      const tomlPath = `data/connections/${idStr}/connection.toml`;
      const tomlRaw = await this.readFile(tomlPath);
      const config: ConnectionConfig = tomlRaw
        ? parseConnectionConfigFromToml(tomlRaw)
        : { name: idStr, synchronizer: 'unknown' };

      return { config, state };
    } catch {
      return null;
    }
  }

  async saveConnection(_conn: ConnectionType): Promise<void> {
    throw new Error('AsyncStorageStorage is read-only');
  }

  async deleteConnection(_id: Id): Promise<boolean> {
    throw new Error('AsyncStorageStorage is read-only');
  }

  async saveConnectionConfig(_id: Id, _config: ConnectionConfig): Promise<void> {
    throw new Error('AsyncStorageStorage is read-only');
  }

  // -----------------------------------------------------------------------
  // Accounts
  // -----------------------------------------------------------------------

  async listAccounts(): Promise<AccountType[]> {
    const manifest = await this.getManifest();
    const acctJsonPaths = manifest.filter((p) =>
      /^data\/accounts\/[^/]+\/account\.json$/.test(p),
    );

    const accounts: AccountType[] = [];
    for (const jsonPath of acctJsonPaths) {
      try {
        const raw = await this.readFile(jsonPath);
        if (!raw) continue;
        const json = JSON.parse(raw) as AccountJSON;
        accounts.push(Account.fromJSON(json));
      } catch {
        // Skip invalid accounts
      }
    }

    return accounts;
  }

  async getAccount(id: Id): Promise<AccountType | null> {
    const jsonPath = `data/accounts/${id.asStr()}/account.json`;
    const raw = await this.readFile(jsonPath);
    if (!raw) return null;

    try {
      const json = JSON.parse(raw) as AccountJSON;
      return Account.fromJSON(json);
    } catch {
      return null;
    }
  }

  async saveAccount(_account: AccountType): Promise<void> {
    throw new Error('AsyncStorageStorage is read-only');
  }

  async deleteAccount(_id: Id): Promise<boolean> {
    throw new Error('AsyncStorageStorage is read-only');
  }

  async saveAccountConfig(_id: Id, _config: AccountConfig): Promise<void> {
    throw new Error('AsyncStorageStorage is read-only');
  }

  // -----------------------------------------------------------------------
  // Balance Snapshots
  // -----------------------------------------------------------------------

  async getBalanceSnapshots(accountId: Id): Promise<BalanceSnapshotType[]> {
    const filePath = `data/accounts/${accountId.asStr()}/balances.jsonl`;
    const raw = await this.readFile(filePath);
    if (!raw) return [];

    const jsonItems = this.parseJsonl<BalanceSnapshotJSON>(raw);
    return jsonItems.map(BalanceSnapshot.fromJSON);
  }

  async appendBalanceSnapshot(_accountId: Id, _snapshot: BalanceSnapshotType): Promise<void> {
    throw new Error('AsyncStorageStorage is read-only');
  }

  async getLatestBalanceSnapshot(accountId: Id): Promise<BalanceSnapshotType | null> {
    const snapshots = await this.getBalanceSnapshots(accountId);
    if (snapshots.length === 0) return null;

    let latest = snapshots[0];
    for (let i = 1; i < snapshots.length; i++) {
      if (snapshots[i].timestamp.getTime() > latest.timestamp.getTime()) {
        latest = snapshots[i];
      }
    }
    return latest;
  }

  async getLatestBalances(): Promise<Array<[Id, BalanceSnapshotType]>> {
    const accounts = await this.listAccounts();
    const results: Array<[Id, BalanceSnapshotType]> = [];

    for (const account of accounts) {
      const latest = await this.getLatestBalanceSnapshot(account.id);
      if (latest !== null) {
        results.push([account.id, latest]);
      }
    }

    return results;
  }

  async getLatestBalancesForConnection(
    connectionId: Id,
  ): Promise<Array<[Id, BalanceSnapshotType]>> {
    const conn = await this.getConnection(connectionId);
    if (conn === null) {
      throw new Error('Connection not found');
    }

    const accounts = await this.listAccounts();
    const results: Array<[Id, BalanceSnapshotType]> = [];

    for (const account of accounts) {
      if (account.connection_id.equals(connectionId)) {
        const latest = await this.getLatestBalanceSnapshot(account.id);
        if (latest !== null) {
          results.push([account.id, latest]);
        }
      }
    }

    return results;
  }

  // -----------------------------------------------------------------------
  // Transactions
  // -----------------------------------------------------------------------

  async getTransactions(accountId: Id): Promise<TransactionType[]> {
    const raw = await this.getTransactionsRaw(accountId);
    if (raw.length === 0) return [];
    return dedupeTransactionsLastWriteWins(raw);
  }

  async getTransactionsRaw(accountId: Id): Promise<TransactionType[]> {
    const filePath = `data/accounts/${accountId.asStr()}/transactions.jsonl`;
    const raw = await this.readFile(filePath);
    if (!raw) return [];

    const jsonItems = this.parseJsonl<TransactionJSON>(raw);
    return jsonItems.map(Transaction.fromJSON);
  }

  async appendTransactions(_accountId: Id, _txns: TransactionType[]): Promise<void> {
    throw new Error('AsyncStorageStorage is read-only');
  }

  // -----------------------------------------------------------------------
  // Transaction Annotations
  // -----------------------------------------------------------------------

  async getTransactionAnnotationPatches(
    accountId: Id,
  ): Promise<TransactionAnnotationPatchType[]> {
    const filePath = `data/accounts/${accountId.asStr()}/transaction_annotations.jsonl`;
    const raw = await this.readFile(filePath);
    if (!raw) return [];

    const jsonItems = this.parseJsonl<TransactionAnnotationPatchJSON>(raw);
    return jsonItems.map(TransactionAnnotationPatch.fromJSON);
  }

  async appendTransactionAnnotationPatches(
    _accountId: Id,
    _patches: TransactionAnnotationPatchType[],
  ): Promise<void> {
    throw new Error('AsyncStorageStorage is read-only');
  }
}
