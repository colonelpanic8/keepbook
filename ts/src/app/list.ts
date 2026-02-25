/**
 * List commands for the CLI.
 *
 * Each function takes a Storage (and optional config parameters) and returns
 * plain objects that can be JSON.stringify'd to match the Rust CLI output.
 */

import { readdir, readFile } from 'node:fs/promises';
import type { Dirent } from 'node:fs';
import path from 'node:path';
import toml from 'toml';
import { Decimal } from '../decimal.js';
import { type Storage } from '../storage/storage.js';
import { type ConnectionType } from '../models/connection.js';
import { type AccountType } from '../models/account.js';
import { type AssetType } from '../models/asset.js';
import { Id } from '../models/id.js';
import { MarketDataService } from '../market-data/service.js';
import { NullMarketDataStore, type MarketDataStore } from '../market-data/store.js';
import {
  formatDateYMD,
  formatRfc3339,
  formatRfc3339FromEpochNanos,
} from './format.js';
import { DEFAULT_IGNORE_CONFIG, type ResolvedConfig } from '../config.js';
import {
  applyTransactionAnnotationPatch,
  isEmptyTransactionAnnotation,
  type TransactionAnnotationType,
} from '../models/transaction-annotation.js';
import { valueInReportingCurrencyBestEffort } from './value.js';
import { compileTransactionIgnoreRules, shouldIgnoreTransaction } from './ignore-rules.js';
import {
  type ConnectionOutput,
  type AccountOutput,
  type BalanceOutput,
  type TransactionOutput,
  type PriceSourceOutput,
  type AllOutput,
} from './types.js';

function buildAccountsByConnection(accounts: AccountType[]): Map<string, Set<string>> {
  const accountsByConnection = new Map<string, Set<string>>();
  for (const account of accounts) {
    const connectionId = account.connection_id.asStr();
    const set = accountsByConnection.get(connectionId);
    if (set !== undefined) {
      set.add(account.id.asStr());
    } else {
      accountsByConnection.set(connectionId, new Set([account.id.asStr()]));
    }
  }
  return accountsByConnection;
}

function parseRfc3339ToEpochNanos(value: string): bigint | null {
  const m = value.match(
    /^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2}):(\d{2})(?:\.(\d{1,9}))?(Z|[+-]\d{2}:\d{2})$/,
  );
  if (m === null) {
    return null;
  }

  const year = Number.parseInt(m[1], 10);
  const month = Number.parseInt(m[2], 10);
  const day = Number.parseInt(m[3], 10);
  const hour = Number.parseInt(m[4], 10);
  const minute = Number.parseInt(m[5], 10);
  const second = Number.parseInt(m[6], 10);
  const fractional = m[7] ?? '';
  const tz = m[8];

  const fractionNanos = BigInt((fractional + '000000000').slice(0, 9));
  const localMs = Date.UTC(year, month - 1, day, hour, minute, second);

  let offsetMinutes = 0;
  if (tz !== 'Z') {
    const sign = tz.startsWith('-') ? -1 : 1;
    const tzHour = Number.parseInt(tz.slice(1, 3), 10);
    const tzMinute = Number.parseInt(tz.slice(4, 6), 10);
    offsetMinutes = sign * (tzHour * 60 + tzMinute);
  }

  const utcMs = localMs - offsetMinutes * 60 * 1000;
  return BigInt(utcMs) * 1000000n + fractionNanos;
}

function formatRfc3339PreservingRaw(date: Date, raw?: string): string {
  if (raw !== undefined) {
    const parsed = parseRfc3339ToEpochNanos(raw);
    if (parsed !== null) {
      return formatRfc3339FromEpochNanos(parsed.toString());
    }
  }
  return formatRfc3339(date);
}

function assetForListOutput(asset: AssetType): AssetType {
  switch (asset.type) {
    case 'currency':
      return { iso_code: asset.iso_code, type: 'currency' };
    case 'equity': {
      if (asset.exchange !== undefined) {
        return { exchange: asset.exchange, ticker: asset.ticker, type: 'equity' };
      }
      return { ticker: asset.ticker, type: 'equity' };
    }
    case 'crypto': {
      if (asset.network !== undefined) {
        return { network: asset.network, symbol: asset.symbol, type: 'crypto' };
      }
      return { symbol: asset.symbol, type: 'crypto' };
    }
  }
}

function resolveConnectionAccountIds(
  connection: ConnectionType,
  accounts: AccountType[],
  validIds: Set<string>,
): string[] {
  const result: string[] = [];
  const seen = new Set<string>();

  for (const accountId of connection.state.account_ids) {
    const accountIdStr = accountId.asStr();
    if (!validIds.has(accountIdStr)) continue;
    if (seen.has(accountIdStr)) continue;
    seen.add(accountIdStr);
    result.push(accountIdStr);
  }

  for (const account of accounts) {
    if (!account.connection_id.equals(connection.state.id)) continue;
    const accountIdStr = account.id.asStr();
    if (!validIds.has(accountIdStr)) continue;
    if (seen.has(accountIdStr)) continue;
    seen.add(accountIdStr);
    result.push(accountIdStr);
  }

  return result;
}

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
  const accountsByConnection = buildAccountsByConnection(accounts);

  return connections.map((conn) => {
    const validIds = accountsByConnection.get(conn.state.id.asStr()) ?? new Set<string>();
    const accountIds = resolveConnectionAccountIds(conn, accounts, validIds);

    const last_sync = conn.state.last_sync
      ? formatRfc3339PreservingRaw(conn.state.last_sync.at, conn.state.last_sync.at_raw)
      : null;

    return {
      id: conn.state.id.asStr(),
      name: conn.config.name,
      synchronizer: conn.config.synchronizer,
      status: conn.state.status,
      account_count: accountIds.length,
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
 * `value_in_reporting_currency` mirrors Rust behavior:
 * - same-currency amounts are normalized and returned
 * - equities/crypto use cached close prices and may fall back to same-day quotes
 *   when close is missing (and FX when needed)
 * - missing price/FX data returns `null`
 */
export async function listBalances(
  storage: Storage,
  config: ResolvedConfig,
  marketDataStore?: MarketDataStore,
): Promise<BalanceOutput[]> {
  const [connections, accounts] = await Promise.all([
    storage.listConnections(),
    storage.listAccounts(),
  ]);
  const accountsByConnection = buildAccountsByConnection(accounts);
  const reportingCurrencyUpper = config.reporting_currency.trim().toUpperCase();
  const currencyDecimals = config.display.currency_decimals;
  const marketData = new MarketDataService(marketDataStore ?? new NullMarketDataStore());
  const result: BalanceOutput[] = [];

  for (const connection of connections) {
    const validIds = accountsByConnection.get(connection.state.id.asStr()) ?? new Set<string>();
    const accountIds = resolveConnectionAccountIds(connection, accounts, validIds);

    for (const accountId of accountIds) {
      const snapshot = await storage.getLatestBalanceSnapshot(Id.fromString(accountId));
      if (!snapshot) continue;

      const asOfDate = formatDateYMD(snapshot.timestamp);
      for (const balance of snapshot.balances) {
        const valueInReportingCurrencyValue = await valueInReportingCurrencyBestEffort(
          marketData,
          balance.asset,
          balance.amount,
          reportingCurrencyUpper,
          asOfDate,
          currencyDecimals,
        );

        result.push({
          account_id: accountId,
          asset: assetForListOutput(balance.asset),
          amount: balance.amount,
          value_in_reporting_currency: valueInReportingCurrencyValue,
          reporting_currency: reportingCurrencyUpper,
          timestamp: formatRfc3339PreservingRaw(snapshot.timestamp, snapshot.timestamp_raw),
        });
      }
    }
  }

  return result;
}

// ---------------------------------------------------------------------------
// listTransactions
// ---------------------------------------------------------------------------

/**
 * List all transactions for all accounts, filtered by date range.
 */
export async function listTransactions(
  storage: Storage,
  startStr?: string,
  endStr?: string,
  config?: ResolvedConfig,
  sortByAmount = false,
  skipIgnored = true,
): Promise<TransactionOutput[]> {
  const endDate = endStr ? new Date(endStr + 'T23:59:59.999Z') : new Date();
  const startDate = startStr
    ? new Date(startStr + 'T00:00:00Z')
    : new Date(endDate.getTime() - 30 * 24 * 60 * 60 * 1000);

  const [accounts, connections] = await Promise.all([
    storage.listAccounts(),
    storage.listConnections(),
  ]);
  const connectionsById = new Map(connections.map((connection) => [connection.state.id.asStr(), connection]));
  const ignoreRules = skipIgnored
    ? compileTransactionIgnoreRules(config?.ignore ?? DEFAULT_IGNORE_CONFIG)
    : [];
  const result: TransactionOutput[] = [];

  for (const account of accounts) {
    const connection = connectionsById.get(account.connection_id.asStr());
    const connection_id = account.connection_id.asStr();
    const connection_name = connection?.config.name ?? '';
    const synchronizer = connection?.config.synchronizer ?? '';

    const transactions = await storage.getTransactions(account.id);
    const patches = await storage.getTransactionAnnotationPatches(account.id);

    // Materialize last-write-wins annotation state per transaction id.
    const annByTx = new Map<string, TransactionAnnotationType>();
    for (const p of patches) {
      const key = p.transaction_id.asStr();
      const base = annByTx.get(key) ?? { transaction_id: p.transaction_id };
      annByTx.set(key, applyTransactionAnnotationPatch(base, p));
    }

    for (const tx of transactions) {
      if (tx.timestamp < startDate || tx.timestamp > endDate) continue;
      if (
        skipIgnored &&
        shouldIgnoreTransaction(ignoreRules, {
          account_id: account.id.asStr(),
          account_name: account.name,
          connection_id,
          connection_name,
          synchronizer,
          description: tx.description,
          status: tx.status,
          amount: tx.amount,
        })
      ) {
        continue;
      }

      const ann = annByTx.get(tx.id.asStr());
      const annotation =
        ann && !isEmptyTransactionAnnotation(ann)
          ? {
              ...(ann.description !== undefined ? { description: ann.description } : {}),
              ...(ann.note !== undefined ? { note: ann.note } : {}),
              ...(ann.category !== undefined ? { category: ann.category } : {}),
              ...(ann.tags !== undefined ? { tags: ann.tags } : {}),
            }
          : undefined;

      const out: TransactionOutput = {
        id: tx.id.asStr(),
        account_id: account.id.asStr(),
        account_name: account.name,
        timestamp: formatRfc3339PreservingRaw(tx.timestamp, tx.timestamp_raw),
        description: tx.description,
        amount: tx.amount,
        asset: assetForListOutput(tx.asset),
        status: tx.status,
      };
      if (annotation !== undefined) {
        out.annotation = annotation;
      }
      result.push(out);
    }
  }

  if (sortByAmount) {
    result.sort((a, b) => {
      let left: Decimal | null = null;
      let right: Decimal | null = null;
      try {
        left = new Decimal(a.amount);
      } catch {
        left = null;
      }
      try {
        right = new Decimal(b.amount);
      } catch {
        right = null;
      }

      if (left !== null && right !== null) return left.comparedTo(right);
      if (left === null && right !== null) return 1;
      if (left !== null && right === null) return -1;
      return a.amount.localeCompare(b.amount);
    });
  }

  return result;
}

// ---------------------------------------------------------------------------
// listPriceSources
// ---------------------------------------------------------------------------

const KNOWN_SOURCE_TYPES = new Set<string>([
  'eodhd',
  'twelve_data',
  'alpha_vantage',
  'marketstack',
  'coingecko',
  'cryptocompare',
  'coincap',
  'frankfurter',
]);

function toOutputSourceType(value: string): string | null {
  const normalized = value.trim().toLowerCase();
  if (!KNOWN_SOURCE_TYPES.has(normalized)) return null;
  // Rust currently formats source type with Debug + lowercase, so underscores are removed.
  return normalized.replaceAll('_', '');
}

function parsePriority(value: unknown): number {
  if (typeof value !== 'number' || !Number.isFinite(value)) return 100;
  return Math.trunc(value);
}

type PriceSourceToml = {
  type?: unknown;
  enabled?: unknown;
  priority?: unknown;
  credentials?: unknown;
};

/**
 * List configured price sources from `data_dir/price_sources/<name>/source.toml`.
 *
 * Mirrors Rust behavior:
 * - include only enabled sources (enabled defaults to true)
 * - skip invalid configs
 * - sort by ascending priority
 */
export async function listPriceSources(dataDir?: string): Promise<PriceSourceOutput[]> {
  if (dataDir === undefined) return [];

  const sourcesDir = path.join(dataDir, 'price_sources');
  let entries: Dirent<string>[];
  try {
    entries = await readdir(sourcesDir, { withFileTypes: true, encoding: 'utf8' });
  } catch (err: unknown) {
    if ((err as NodeJS.ErrnoException).code === 'ENOENT') {
      return [];
    }
    throw err;
  }

  const output: PriceSourceOutput[] = [];

  for (const entry of entries) {
    if (!entry.isDirectory()) continue;

    const sourceTomlPath = path.join(sourcesDir, entry.name, 'source.toml');
    let content: string;
    try {
      content = await readFile(sourceTomlPath, 'utf8');
    } catch (err: unknown) {
      if ((err as NodeJS.ErrnoException).code === 'ENOENT') continue;
      continue;
    }

    let parsed: PriceSourceToml;
    try {
      parsed = toml.parse(content) as PriceSourceToml;
    } catch {
      continue;
    }

    if (typeof parsed.type !== 'string') continue;
    const outputType = toOutputSourceType(parsed.type);
    if (outputType === null) continue;

    const enabled = typeof parsed.enabled === 'boolean' ? parsed.enabled : true;
    if (!enabled) continue;

    output.push({
      name: entry.name,
      type: outputType,
      enabled,
      priority: parsePriority(parsed.priority),
      has_credentials: parsed.credentials !== undefined && parsed.credentials !== null,
    });
  }

  output.sort((a, b) => a.priority - b.priority);
  return output;
}

// ---------------------------------------------------------------------------
// listAll
// ---------------------------------------------------------------------------

/**
 * Combine all list outputs into a single object.
 */
export async function listAll(
  storage: Storage,
  config: ResolvedConfig,
  marketDataStore?: MarketDataStore,
): Promise<AllOutput> {
  const [connections, accounts, balances, priceSources] = await Promise.all([
    listConnections(storage),
    listAccounts(storage),
    listBalances(storage, config, marketDataStore),
    listPriceSources(config.data_dir),
  ]);

  return {
    connections,
    accounts,
    price_sources: priceSources,
    balances,
  };
}
