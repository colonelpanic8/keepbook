/**
 * List commands for the CLI.
 *
 * Each function takes a Storage (and optional config parameters) and returns
 * plain objects that can be JSON.stringify'd to match the Rust CLI output.
 */

import { type Storage } from '../storage/storage.js';
import { formatRfc3339 } from './format.js';
import {
  type ConnectionOutput,
  type AccountOutput,
  type BalanceOutput,
  type TransactionOutput,
  type PriceSourceOutput,
  type AllOutput,
} from './types.js';

// ---------------------------------------------------------------------------
// listConnections
// ---------------------------------------------------------------------------

/**
 * List all connections with account counts and formatted timestamps.
 */
export async function listConnections(storage: Storage): Promise<ConnectionOutput[]> {
  const [connections, accounts] = await Promise.all([
    storage.listConnections(),
    storage.listAccounts(),
  ]);

  return connections.map((conn) => {
    // Union of account_ids from connection state and accounts whose connection_id matches
    const accountIdSet = new Set<string>();
    for (const id of conn.state.account_ids) {
      accountIdSet.add(id.asStr());
    }
    for (const account of accounts) {
      if (account.connection_id.equals(conn.state.id)) {
        accountIdSet.add(account.id.asStr());
      }
    }

    const last_sync = conn.state.last_sync ? formatRfc3339(conn.state.last_sync.at) : null;

    return {
      id: conn.state.id.asStr(),
      name: conn.config.name,
      synchronizer: conn.config.synchronizer,
      status: conn.state.status,
      account_count: accountIdSet.size,
      last_sync,
    };
  });
}

// ---------------------------------------------------------------------------
// listAccounts
// ---------------------------------------------------------------------------

/**
 * List all accounts with basic fields.
 */
export async function listAccounts(storage: Storage): Promise<AccountOutput[]> {
  const accounts = await storage.listAccounts();
  return accounts.map((a) => ({
    id: a.id.asStr(),
    name: a.name,
    connection_id: a.connection_id.asStr(),
    tags: [...a.tags],
    active: a.active,
  }));
}

// ---------------------------------------------------------------------------
// listBalances
// ---------------------------------------------------------------------------

/**
 * List latest balances for all accounts.
 *
 * `value_in_reporting_currency` is set to the amount when the asset is the
 * reporting currency (same iso_code); otherwise `null`.
 */
export async function listBalances(
  storage: Storage,
  reportingCurrency: string,
): Promise<BalanceOutput[]> {
  const accounts = await storage.listAccounts();
  const result: BalanceOutput[] = [];

  for (const account of accounts) {
    const snapshot = await storage.getLatestBalanceSnapshot(account.id);
    if (!snapshot) continue;

    for (const balance of snapshot.balances) {
      const valueInReportingCurrency =
        balance.asset.type === 'currency' && balance.asset.iso_code === reportingCurrency
          ? balance.amount
          : null;

      result.push({
        account_id: account.id.asStr(),
        asset: balance.asset,
        amount: balance.amount,
        value_in_reporting_currency: valueInReportingCurrency,
        reporting_currency: reportingCurrency,
        timestamp: formatRfc3339(snapshot.timestamp),
      });
    }
  }

  return result;
}

// ---------------------------------------------------------------------------
// listTransactions
// ---------------------------------------------------------------------------

/**
 * List all transactions for all accounts.
 */
export async function listTransactions(storage: Storage): Promise<TransactionOutput[]> {
  const accounts = await storage.listAccounts();
  const result: TransactionOutput[] = [];

  for (const account of accounts) {
    const transactions = await storage.getTransactions(account.id);
    for (const tx of transactions) {
      result.push({
        id: tx.id.asStr(),
        account_id: account.id.asStr(),
        timestamp: formatRfc3339(tx.timestamp),
        description: tx.description,
        amount: tx.amount,
        asset: tx.asset,
        status: tx.status,
      });
    }
  }

  return result;
}

// ---------------------------------------------------------------------------
// listPriceSources
// ---------------------------------------------------------------------------

/**
 * List price sources. Returns empty array (PriceSourceRegistry not yet in TS).
 */
export function listPriceSources(): PriceSourceOutput[] {
  return [];
}

// ---------------------------------------------------------------------------
// listAll
// ---------------------------------------------------------------------------

/**
 * Combine all list outputs into a single object.
 */
export async function listAll(storage: Storage, reportingCurrency: string): Promise<AllOutput> {
  const [connections, accounts, balances] = await Promise.all([
    listConnections(storage),
    listAccounts(storage),
    listBalances(storage, reportingCurrency),
  ]);

  return {
    connections,
    accounts,
    price_sources: listPriceSources(),
    balances,
  };
}
