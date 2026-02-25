import * as fs from 'node:fs/promises';
import * as fsSync from 'node:fs';
import * as path from 'node:path';
import TOML from 'toml';

import { Id, IdError } from '../models/id.js';
import { parseDuration } from '../duration.js';
import {
  Account,
  type AccountType,
  type AccountConfig,
  type AccountJSON,
} from '../models/account.js';
import {
  BalanceSnapshot,
  type BalanceSnapshotType,
  type BalanceSnapshotJSON,
} from '../models/balance.js';
import {
  ConnectionState,
  type ConnectionType,
  type ConnectionConfig,
  type ConnectionStateJSON,
} from '../models/connection.js';
import { Transaction, type TransactionType, type TransactionJSON } from '../models/transaction.js';
import {
  TransactionAnnotationPatch,
  applyTransactionAnnotationPatch,
  isEmptyTransactionAnnotation,
  type TransactionAnnotationType,
  type TransactionAnnotationPatchType,
  type TransactionAnnotationPatchJSON,
} from '../models/transaction-annotation.js';
import { dedupeTransactionsLastWriteWins } from './dedupe.js';
import {
  type Storage,
  type CredentialStore,
  type CompactionStorage,
  type JsonlCompactionStats,
} from './storage.js';
import { parseCredentialConfigValue } from '../credentials/credential-config.js';
import { PassCredentialStore } from '../credentials/pass.js';

// ---------------------------------------------------------------------------
// TOML serialization helper
// ---------------------------------------------------------------------------

/**
 * Simple TOML serializer for flat objects with string, number, and boolean values.
 * Sufficient for ConnectionConfig and AccountConfig which have simple flat fields.
 */
function toToml(obj: Record<string, unknown>): string {
  const lines: string[] = [];
  for (const [key, value] of Object.entries(obj)) {
    if (value === undefined) continue;
    if (typeof value === 'string') lines.push(`${key} = "${value}"`);
    else if (typeof value === 'number') lines.push(`${key} = ${value}`);
    else if (typeof value === 'boolean') lines.push(`${key} = ${value}`);
  }
  return lines.join('\n') + '\n';
}

function compactTransactionAnnotationPatches(
  patches: TransactionAnnotationPatchType[],
): TransactionAnnotationPatchType[] {
  const sorted = [...patches].sort((a, b) => a.timestamp.getTime() - b.timestamp.getTime());
  const byTx = new Map<
    string,
    { annotation: TransactionAnnotationType; timestamp: Date; timestampRaw?: string }
  >();

  for (const patch of sorted) {
    const key = patch.transaction_id.asStr();
    const existing = byTx.get(key);
    const base: TransactionAnnotationType =
      existing?.annotation ?? { transaction_id: patch.transaction_id };
    const next = applyTransactionAnnotationPatch(base, patch);
    byTx.set(key, {
      annotation: next,
      timestamp: patch.timestamp,
      timestampRaw: patch.timestamp_raw,
    });
  }

  const compacted: TransactionAnnotationPatchType[] = [];
  for (const state of byTx.values()) {
    if (isEmptyTransactionAnnotation(state.annotation)) continue;
    compacted.push({
      transaction_id: state.annotation.transaction_id,
      timestamp: state.timestamp,
      timestamp_raw: state.timestampRaw,
      ...(state.annotation.description !== undefined
        ? { description: state.annotation.description }
        : {}),
      ...(state.annotation.note !== undefined ? { note: state.annotation.note } : {}),
      ...(state.annotation.category !== undefined ? { category: state.annotation.category } : {}),
      ...(state.annotation.tags !== undefined ? { tags: state.annotation.tags } : {}),
    });
  }

  compacted.sort((a, b) => {
    const tsCmp = a.timestamp.getTime() - b.timestamp.getTime();
    if (tsCmp !== 0) return tsCmp;
    return a.transaction_id.asStr().localeCompare(b.transaction_id.asStr());
  });
  return compacted;
}

// ---------------------------------------------------------------------------
// JsonFileStorage
// ---------------------------------------------------------------------------

/**
 * JSON file-based storage implementation.
 *
 * Port of the Rust `JsonFileStorage` (storage/json_file.rs).
 *
 * Directory structure:
 * ```
 * data/
 *   connections/
 *     {id}/
 *       connection.toml   # human-declared config (ConnectionConfig)
 *       connection.json   # machine-managed state (ConnectionState)
 *   accounts/
 *     {id}/
 *       account.json
 *       account_config.toml   # optional
 *       balances.jsonl
 *       transactions.jsonl
 *       transaction_annotations.jsonl
 * ```
 */
export class JsonFileStorage implements Storage, CompactionStorage {
  private readonly basePath: string;

  constructor(basePath: string) {
    this.basePath = basePath;
  }

  // -----------------------------------------------------------------------
  // Path helpers
  // -----------------------------------------------------------------------

  private connectionsDir(): string {
    return path.join(this.basePath, 'connections');
  }

  private accountsDir(): string {
    return path.join(this.basePath, 'accounts');
  }

  private ensureIdPathSafe(id: Id): void {
    const value = id.asStr();
    if (!Id.isPathSafe(value)) {
      throw new IdError(value);
    }
  }

  private connectionDir(id: Id): string {
    this.ensureIdPathSafe(id);
    return path.join(this.connectionsDir(), id.asStr());
  }

  private connectionConfigFile(id: Id): string {
    return path.join(this.connectionDir(id), 'connection.toml');
  }

  private connectionStateFile(id: Id): string {
    return path.join(this.connectionDir(id), 'connection.json');
  }

  private accountDir(id: Id): string {
    this.ensureIdPathSafe(id);
    return path.join(this.accountsDir(), id.asStr());
  }

  private accountFile(id: Id): string {
    return path.join(this.accountDir(id), 'account.json');
  }

  private accountConfigFile(id: Id): string {
    return path.join(this.accountDir(id), 'account_config.toml');
  }

  private balancesFile(accountId: Id): string {
    return path.join(this.accountDir(accountId), 'balances.jsonl');
  }

  private transactionsFile(accountId: Id): string {
    return path.join(this.accountDir(accountId), 'transactions.jsonl');
  }

  private transactionAnnotationsFile(accountId: Id): string {
    return path.join(this.accountDir(accountId), 'transaction_annotations.jsonl');
  }

  // -----------------------------------------------------------------------
  // I/O helpers
  // -----------------------------------------------------------------------

  private async ensureDir(filePath: string): Promise<void> {
    const dir = path.dirname(filePath);
    await fs.mkdir(dir, { recursive: true });
  }

  private async readJson<T>(filePath: string): Promise<T | null> {
    try {
      const content = await fs.readFile(filePath, 'utf-8');
      return JSON.parse(content) as T;
    } catch (e: unknown) {
      if ((e as NodeJS.ErrnoException).code === 'ENOENT') {
        return null;
      }
      throw e;
    }
  }

  private async writeJson(filePath: string, value: unknown): Promise<void> {
    await this.ensureDir(filePath);
    const content = JSON.stringify(value, null, 2);
    await fs.writeFile(filePath, content, 'utf-8');
  }

  private readTomlSync<T>(filePath: string): T | null {
    try {
      const content = fsSync.readFileSync(filePath, 'utf-8');
      return TOML.parse(content) as T;
    } catch (e: unknown) {
      if ((e as NodeJS.ErrnoException).code === 'ENOENT') {
        return null;
      }
      throw e;
    }
  }

  private parseOptionalDuration(value: unknown, fieldName: string): number | undefined {
    if (value === undefined || value === null) {
      return undefined;
    }
    if (typeof value === 'number') {
      return value;
    }
    if (typeof value === 'string') {
      return parseDuration(value);
    }
    throw new Error(`Invalid ${fieldName}: expected number or duration string`);
  }

  private normalizeConnectionConfig(config: ConnectionConfig): ConnectionConfig {
    const balanceStaleness = this.parseOptionalDuration(
      (config as { balance_staleness?: unknown }).balance_staleness,
      'connection balance_staleness',
    );
    return {
      ...config,
      ...(balanceStaleness !== undefined ? { balance_staleness: balanceStaleness } : {}),
    };
  }

  private normalizeAccountConfig(config: AccountConfig): AccountConfig {
    const balanceStaleness = this.parseOptionalDuration(
      (config as { balance_staleness?: unknown }).balance_staleness,
      'account balance_staleness',
    );
    return {
      ...config,
      ...(balanceStaleness !== undefined ? { balance_staleness: balanceStaleness } : {}),
    };
  }

  private async readJsonl<T>(filePath: string): Promise<T[]> {
    try {
      const content = await fs.readFile(filePath, 'utf-8');
      const lines = content.split('\n');
      const items: T[] = [];
      for (const line of lines) {
        const trimmed = line.trim();
        if (trimmed === '') continue;
        items.push(JSON.parse(trimmed) as T);
      }
      return items;
    } catch (e: unknown) {
      if ((e as NodeJS.ErrnoException).code === 'ENOENT') {
        return [];
      }
      throw e;
    }
  }

  private async appendJsonl(filePath: string, items: unknown[]): Promise<void> {
    if (items.length === 0) return;
    await this.ensureDir(filePath);
    const lines = items.map((item) => JSON.stringify(item)).join('\n') + '\n';
    await fs.appendFile(filePath, lines, 'utf-8');
  }

  private async writeJsonl(filePath: string, items: unknown[]): Promise<void> {
    await this.ensureDir(filePath);
    const lines = items.map((item) => JSON.stringify(item)).join('\n');
    const content = lines === '' ? '' : `${lines}\n`;
    await fs.writeFile(filePath, content, 'utf-8');
  }

  /**
   * List subdirectories under a path and return their names as Ids.
   * Skips entries that are not directories or not path-safe.
   */
  private async listDirs(dirPath: string): Promise<Id[]> {
    let names: string[];
    try {
      names = await fs.readdir(dirPath);
    } catch (e: unknown) {
      if ((e as NodeJS.ErrnoException).code === 'ENOENT') {
        return [];
      }
      throw e;
    }

    const ids: Id[] = [];
    const sortedNames = [...names].sort((a, b) => a.localeCompare(b));
    for (const name of sortedNames) {
      if (!name || !Id.isPathSafe(name)) continue;
      try {
        const stat = await fs.stat(path.join(dirPath, name));
        if (stat.isDirectory()) {
          ids.push(Id.fromString(name));
        }
      } catch {
        // Skip entries we cannot stat
      }
    }
    return ids;
  }

  // -----------------------------------------------------------------------
  // Connection loading
  // -----------------------------------------------------------------------

  /**
   * Load a connection by reading config (TOML) and state (JSON).
   * Returns null if the config TOML does not exist.
   */
  private async loadConnection(id: Id): Promise<ConnectionType | null> {
    const configPath = this.connectionConfigFile(id);
    const statePath = this.connectionStateFile(id);

    // Config is required
    const rawConfig = this.readTomlSync<ConnectionConfig>(configPath);
    if (rawConfig === null) {
      return null;
    }
    const config = this.normalizeConnectionConfig(rawConfig);

    // State may not exist yet (new connection with only config TOML)
    const stateJson = await this.readJson<ConnectionStateJSON>(statePath);
    let state;
    if (stateJson !== null) {
      state = ConnectionState.fromJSON(stateJson);
      // Verify id matches directory name
      if (!state.id.equals(id)) {
        // Use directory id
        state = { ...state, id };
      }
    } else {
      // Create default state with the directory name as ID
      state = ConnectionState.newWith(id, new Date());
    }

    return { config, state };
  }

  // -----------------------------------------------------------------------
  // Credentials
  // -----------------------------------------------------------------------

  getCredentialStore(connectionId: Id): CredentialStore | null {
    try {
      // First: inline credentials in connection.toml ([credentials] table).
      const configPath = this.connectionConfigFile(connectionId);
      const rawConfig = this.readTomlSync<ConnectionConfig>(configPath);
      const inlineCreds = rawConfig !== null ? (rawConfig as { credentials?: unknown }).credentials : undefined;
      if (inlineCreds !== undefined && inlineCreds !== null) {
        const cfg = parseCredentialConfigValue(inlineCreds);
        if (cfg.backend === 'pass') {
          return new PassCredentialStore(cfg);
        }
      }

      // Fallback: separate credentials.toml (backwards compatibility with Rust).
      const credsPath = path.join(this.connectionDir(connectionId), 'credentials.toml');
      const rawCreds = this.readTomlSync<Record<string, unknown>>(credsPath);
      if (rawCreds !== null) {
        const cfg = parseCredentialConfigValue(rawCreds);
        if (cfg.backend === 'pass') {
          return new PassCredentialStore(cfg);
        }
      }

      return null;
    } catch (e: unknown) {
      if (e instanceof IdError) return null;
      // If TOML exists but is invalid, treat as "no credentials" rather than crashing CLI commands.
      return null;
    }
  }

  // -----------------------------------------------------------------------
  // Account Config
  // -----------------------------------------------------------------------

  getAccountConfig(accountId: Id): AccountConfig | null {
    try {
      const configPath = this.accountConfigFile(accountId);
      const config = this.readTomlSync<AccountConfig>(configPath);
      if (config === null) {
        return null;
      }
      return this.normalizeAccountConfig(config);
    } catch (e: unknown) {
      if (e instanceof IdError) {
        return null;
      }
      throw e;
    }
  }

  // -----------------------------------------------------------------------
  // Connections
  // -----------------------------------------------------------------------

  async listConnections(): Promise<ConnectionType[]> {
    const ids = await this.listDirs(this.connectionsDir());
    const connections: ConnectionType[] = [];

    for (const id of ids) {
      try {
        const conn = await this.loadConnection(id);
        if (conn !== null) {
          connections.push(conn);
        }
      } catch {
        // Skip connections with invalid config/state
      }
    }

    return connections;
  }

  async getConnection(id: Id): Promise<ConnectionType | null> {
    return this.loadConnection(id);
  }

  async saveConnection(conn: ConnectionType): Promise<void> {
    // Only save state - config is human-managed TOML
    const statePath = this.connectionStateFile(conn.state.id);
    const stateJson = ConnectionState.toJSON(conn.state);
    await this.writeJson(statePath, stateJson);
  }

  async deleteConnection(id: Id): Promise<boolean> {
    const dir = this.connectionDir(id);
    try {
      await fs.rm(dir, { recursive: true });
      return true;
    } catch (e: unknown) {
      if ((e as NodeJS.ErrnoException).code === 'ENOENT') {
        return false;
      }
      throw e;
    }
  }

  async saveConnectionConfig(id: Id, config: ConnectionConfig): Promise<void> {
    const configPath = this.connectionConfigFile(id);
    await this.ensureDir(configPath);
    const content = toToml(config as unknown as Record<string, unknown>);
    await fs.writeFile(configPath, content, 'utf-8');
  }

  // -----------------------------------------------------------------------
  // Accounts
  // -----------------------------------------------------------------------

  async listAccounts(): Promise<AccountType[]> {
    const ids = await this.listDirs(this.accountsDir());
    const accounts: AccountType[] = [];

    for (const id of ids) {
      try {
        const account = await this.getAccount(id);
        if (account !== null) {
          accounts.push(account);
        }
      } catch {
        // Skip accounts with invalid json
      }
    }

    return accounts;
  }

  async getAccount(id: Id): Promise<AccountType | null> {
    const filePath = this.accountFile(id);
    const json = await this.readJson<AccountJSON>(filePath);
    if (json === null) return null;

    const account = Account.fromJSON(json);
    // Verify id matches directory name
    if (!account.id.equals(id)) {
      return { ...account, id };
    }
    return account;
  }

  async saveAccount(account: AccountType): Promise<void> {
    const filePath = this.accountFile(account.id);
    const json = Account.toJSON(account);
    await this.writeJson(filePath, json);
  }

  async deleteAccount(id: Id): Promise<boolean> {
    const dir = this.accountDir(id);
    try {
      await fs.rm(dir, { recursive: true });
      return true;
    } catch (e: unknown) {
      if ((e as NodeJS.ErrnoException).code === 'ENOENT') {
        return false;
      }
      throw e;
    }
  }

  async saveAccountConfig(id: Id, config: AccountConfig): Promise<void> {
    const configPath = this.accountConfigFile(id);
    await this.ensureDir(configPath);
    const content = toToml(config as unknown as Record<string, unknown>);
    await fs.writeFile(configPath, content, 'utf-8');
  }

  // -----------------------------------------------------------------------
  // Balance Snapshots
  // -----------------------------------------------------------------------

  async getBalanceSnapshots(accountId: Id): Promise<BalanceSnapshotType[]> {
    const filePath = this.balancesFile(accountId);
    const jsonItems = await this.readJsonl<BalanceSnapshotJSON>(filePath);
    return jsonItems.map(BalanceSnapshot.fromJSON);
  }

  async appendBalanceSnapshot(accountId: Id, snapshot: BalanceSnapshotType): Promise<void> {
    const filePath = this.balancesFile(accountId);
    const json = BalanceSnapshot.toJSON(snapshot);
    await this.appendJsonl(filePath, [json]);
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
    const filePath = this.transactionsFile(accountId);
    const jsonItems = await this.readJsonl<TransactionJSON>(filePath);
    return jsonItems.map(Transaction.fromJSON);
  }

  async appendTransactions(accountId: Id, txns: TransactionType[]): Promise<void> {
    if (txns.length === 0) return;
    const filePath = this.transactionsFile(accountId);
    const jsonItems = txns.map(Transaction.toJSON);
    await this.appendJsonl(filePath, jsonItems);
  }

  // -----------------------------------------------------------------------
  // Transaction annotations
  // -----------------------------------------------------------------------

  async getTransactionAnnotationPatches(accountId: Id): Promise<TransactionAnnotationPatchType[]> {
    const filePath = this.transactionAnnotationsFile(accountId);
    const jsonItems = await this.readJsonl<TransactionAnnotationPatchJSON>(filePath);
    return jsonItems.map(TransactionAnnotationPatch.fromJSON);
  }

  async appendTransactionAnnotationPatches(
    accountId: Id,
    patches: TransactionAnnotationPatchType[],
  ): Promise<void> {
    if (patches.length === 0) return;
    const filePath = this.transactionAnnotationsFile(accountId);
    const jsonItems = patches.map(TransactionAnnotationPatch.toJSON);
    await this.appendJsonl(filePath, jsonItems);
  }

  async recompactAllJsonl(): Promise<JsonlCompactionStats> {
    const accountIds = await this.listDirs(this.accountsDir());
    const stats: JsonlCompactionStats = {
      accounts_processed: accountIds.length,
      files_rewritten: 0,
      balance_snapshots_before: 0,
      balance_snapshots_after: 0,
      transactions_before: 0,
      transactions_after: 0,
      annotation_patches_before: 0,
      annotation_patches_after: 0,
    };

    for (const accountId of accountIds) {
      const balancesPath = this.balancesFile(accountId);
      if (fsSync.existsSync(balancesPath)) {
        const balances = await this.getBalanceSnapshots(accountId);
        stats.balance_snapshots_before += balances.length;
        const sorted = [...balances].sort(
          (a, b) => a.timestamp.getTime() - b.timestamp.getTime(),
        );
        stats.balance_snapshots_after += sorted.length;
        await this.writeJsonl(
          balancesPath,
          sorted.map((s) => BalanceSnapshot.toJSON(s)),
        );
        stats.files_rewritten += 1;
      }

      const txPath = this.transactionsFile(accountId);
      if (fsSync.existsSync(txPath)) {
        const raw = await this.getTransactionsRaw(accountId);
        stats.transactions_before += raw.length;
        const compacted = dedupeTransactionsLastWriteWins(raw).sort((a, b) => {
          const tsCmp = a.timestamp.getTime() - b.timestamp.getTime();
          if (tsCmp !== 0) return tsCmp;
          return a.id.asStr().localeCompare(b.id.asStr());
        });
        stats.transactions_after += compacted.length;
        await this.writeJsonl(txPath, compacted.map((t) => Transaction.toJSON(t)));
        stats.files_rewritten += 1;
      }

      const annotationsPath = this.transactionAnnotationsFile(accountId);
      if (fsSync.existsSync(annotationsPath)) {
        const raw = await this.getTransactionAnnotationPatches(accountId);
        stats.annotation_patches_before += raw.length;
        const compacted = compactTransactionAnnotationPatches(raw);
        stats.annotation_patches_after += compacted.length;
        await this.writeJsonl(
          annotationsPath,
          compacted.map((p) => TransactionAnnotationPatch.toJSON(p)),
        );
        stats.files_rewritten += 1;
      }
    }

    return stats;
  }
}
