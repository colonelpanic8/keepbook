import { Id } from '../models/id.js';
import { type AccountType, type AccountConfig } from '../models/account.js';
import { type BalanceSnapshotType } from '../models/balance.js';
import {
  ConnectionState,
  type ConnectionType,
  type ConnectionConfig,
} from '../models/connection.js';
import { type TransactionType } from '../models/transaction.js';
import { type Storage, type CredentialStore } from './storage.js';

/**
 * In-memory storage implementation.
 *
 * Port of the Rust `MemoryStorage` (storage/memory.rs).
 * Uses Maps keyed by the string representation of entity Ids.
 */
export class MemoryStorage implements Storage {
  private readonly connections = new Map<string, ConnectionType>();
  private readonly accounts = new Map<string, AccountType>();
  private readonly accountConfigs = new Map<string, AccountConfig>();
  private readonly balances = new Map<string, BalanceSnapshotType[]>();
  private readonly transactions = new Map<string, TransactionType[]>();

  // -----------------------------------------------------------------------
  // Credentials
  // -----------------------------------------------------------------------

  getCredentialStore(_connectionId: Id): CredentialStore | null {
    return null;
  }

  // -----------------------------------------------------------------------
  // Account Config
  // -----------------------------------------------------------------------

  getAccountConfig(accountId: Id): AccountConfig | null {
    return this.accountConfigs.get(accountId.asStr()) ?? null;
  }

  // -----------------------------------------------------------------------
  // Connections
  // -----------------------------------------------------------------------

  async listConnections(): Promise<ConnectionType[]> {
    return Array.from(this.connections.values());
  }

  async getConnection(id: Id): Promise<ConnectionType | null> {
    return this.connections.get(id.asStr()) ?? null;
  }

  async saveConnection(conn: ConnectionType): Promise<void> {
    this.connections.set(conn.state.id.asStr(), conn);
  }

  async deleteConnection(id: Id): Promise<boolean> {
    return this.connections.delete(id.asStr());
  }

  async saveConnectionConfig(id: Id, config: ConnectionConfig): Promise<void> {
    const existing = this.connections.get(id.asStr());
    if (existing) {
      this.connections.set(id.asStr(), {
        config,
        state: existing.state,
      });
    } else {
      // Create a new connection with fresh state
      const state = ConnectionState.newWith(id, new Date());
      this.connections.set(id.asStr(), { config, state });
    }
  }

  // -----------------------------------------------------------------------
  // Accounts
  // -----------------------------------------------------------------------

  async listAccounts(): Promise<AccountType[]> {
    return Array.from(this.accounts.values());
  }

  async getAccount(id: Id): Promise<AccountType | null> {
    return this.accounts.get(id.asStr()) ?? null;
  }

  async saveAccount(account: AccountType): Promise<void> {
    this.accounts.set(account.id.asStr(), account);
  }

  async deleteAccount(id: Id): Promise<boolean> {
    return this.accounts.delete(id.asStr());
  }

  async saveAccountConfig(id: Id, config: AccountConfig): Promise<void> {
    this.accountConfigs.set(id.asStr(), config);
  }

  // -----------------------------------------------------------------------
  // Balance Snapshots
  // -----------------------------------------------------------------------

  async getBalanceSnapshots(accountId: Id): Promise<BalanceSnapshotType[]> {
    return this.balances.get(accountId.asStr()) ?? [];
  }

  async appendBalanceSnapshot(accountId: Id, snapshot: BalanceSnapshotType): Promise<void> {
    const key = accountId.asStr();
    const existing = this.balances.get(key);
    if (existing) {
      existing.push(snapshot);
    } else {
      this.balances.set(key, [snapshot]);
    }
  }

  async getLatestBalanceSnapshot(accountId: Id): Promise<BalanceSnapshotType | null> {
    const snapshots = this.balances.get(accountId.asStr());
    if (!snapshots || snapshots.length === 0) {
      return null;
    }
    let latest = snapshots[0];
    for (let i = 1; i < snapshots.length; i++) {
      if (snapshots[i].timestamp.getTime() > latest.timestamp.getTime()) {
        latest = snapshots[i];
      }
    }
    return latest;
  }

  async getLatestBalances(): Promise<Array<[Id, BalanceSnapshotType]>> {
    const result: Array<[Id, BalanceSnapshotType]> = [];
    for (const [, account] of this.accounts) {
      const latest = await this.getLatestBalanceSnapshot(account.id);
      if (latest) {
        result.push([account.id, latest]);
      }
    }
    return result;
  }

  async getLatestBalancesForConnection(
    connectionId: Id,
  ): Promise<Array<[Id, BalanceSnapshotType]>> {
    const conn = this.connections.get(connectionId.asStr());
    if (!conn) {
      throw new Error('Connection not found');
    }

    const result: Array<[Id, BalanceSnapshotType]> = [];
    for (const [, account] of this.accounts) {
      if (account.connection_id.equals(connectionId)) {
        const latest = await this.getLatestBalanceSnapshot(account.id);
        if (latest) {
          result.push([account.id, latest]);
        }
      }
    }
    return result;
  }

  // -----------------------------------------------------------------------
  // Transactions
  // -----------------------------------------------------------------------

  async getTransactions(accountId: Id): Promise<TransactionType[]> {
    const raw = this.transactions.get(accountId.asStr());
    if (!raw || raw.length === 0) {
      return [];
    }
    // Deduplicate by id, last-write-wins
    const byId = new Map<string, TransactionType>();
    for (const tx of raw) {
      byId.set(tx.id.asStr(), tx);
    }
    return Array.from(byId.values());
  }

  async getTransactionsRaw(accountId: Id): Promise<TransactionType[]> {
    return this.transactions.get(accountId.asStr()) ?? [];
  }

  async appendTransactions(accountId: Id, txns: TransactionType[]): Promise<void> {
    if (txns.length === 0) {
      return;
    }
    const key = accountId.asStr();
    const existing = this.transactions.get(key);
    if (existing) {
      existing.push(...txns);
    } else {
      this.transactions.set(key, [...txns]);
    }
  }
}
