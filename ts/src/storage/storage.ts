import { Id } from '../models/id.js';
import { type AccountType, type AccountConfig } from '../models/account.js';
import { type BalanceSnapshotType } from '../models/balance.js';
import { type ConnectionType, type ConnectionConfig } from '../models/connection.js';
import { type TransactionType } from '../models/transaction.js';

// ---------------------------------------------------------------------------
// CredentialStore
// ---------------------------------------------------------------------------

/**
 * Abstraction for reading/writing credentials associated with a connection.
 */
export interface CredentialStore {
  get(key: string): Promise<string | null>;
  set(key: string, value: string): Promise<void>;
  supportsWrite(): boolean;
}

// ---------------------------------------------------------------------------
// Storage
// ---------------------------------------------------------------------------

/**
 * Async CRUD storage interface for keepbook entities.
 *
 * Port of the Rust `Storage` trait (storage/mod.rs).
 */
export interface Storage {
  // Credentials
  getCredentialStore(connectionId: Id): CredentialStore | null;

  // Account config
  getAccountConfig(accountId: Id): AccountConfig | null;

  // Connections
  listConnections(): Promise<ConnectionType[]>;
  getConnection(id: Id): Promise<ConnectionType | null>;
  saveConnection(conn: ConnectionType): Promise<void>;
  deleteConnection(id: Id): Promise<boolean>;
  saveConnectionConfig(id: Id, config: ConnectionConfig): Promise<void>;

  // Accounts
  listAccounts(): Promise<AccountType[]>;
  getAccount(id: Id): Promise<AccountType | null>;
  saveAccount(account: AccountType): Promise<void>;
  deleteAccount(id: Id): Promise<boolean>;
  saveAccountConfig(id: Id, config: AccountConfig): Promise<void>;

  // Balance Snapshots
  getBalanceSnapshots(accountId: Id): Promise<BalanceSnapshotType[]>;
  appendBalanceSnapshot(accountId: Id, snapshot: BalanceSnapshotType): Promise<void>;
  getLatestBalanceSnapshot(accountId: Id): Promise<BalanceSnapshotType | null>;
  getLatestBalances(): Promise<Array<[Id, BalanceSnapshotType]>>;
  getLatestBalancesForConnection(connectionId: Id): Promise<Array<[Id, BalanceSnapshotType]>>;

  // Transactions
  getTransactions(accountId: Id): Promise<TransactionType[]>;
  getTransactionsRaw(accountId: Id): Promise<TransactionType[]>;
  appendTransactions(accountId: Id, txns: TransactionType[]): Promise<void>;
}
