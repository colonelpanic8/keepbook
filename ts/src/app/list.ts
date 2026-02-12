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
import { type Storage } from '../storage/storage.js';
import { type ConnectionType } from '../models/connection.js';
import { type AccountType } from '../models/account.js';
import { type AssetType } from '../models/asset.js';
import { Id } from '../models/id.js';
import { MarketDataService } from '../market-data/service.js';
import { NullMarketDataStore, type MarketDataStore } from '../market-data/store.js';
import { Decimal } from '../decimal.js';
import { formatDateYMD, formatRfc3339, formatRfc3339FromEpochNanos, decStr } from './format.js';
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

async function valueInReportingCurrency(
  marketData: MarketDataService,
  asset: AssetType,
  amount: string,
  reportingCurrency: string,
  asOfDate: string,
): Promise<string | null> {
  const amountValue = new Decimal(amount);

  if (asset.type === 'currency') {
    if (asset.iso_code.toUpperCase() === reportingCurrency) {
      return decStr(amountValue);
    }

    const rate = await marketData.fxFromStore(asset.iso_code, reportingCurrency, asOfDate);
    if (rate === null) return null;
    return decStr(amountValue.times(new Decimal(rate.rate)));
  }

  const price = await marketData.priceFromStore(asset, asOfDate);
  if (price === null) return null;

  const valueInQuote = amountValue.times(new Decimal(price.price));
  if (price.quote_currency.toUpperCase() === reportingCurrency) {
    return decStr(valueInQuote);
  }

  const rate = await marketData.fxFromStore(price.quote_currency, reportingCurrency, asOfDate);
  if (rate === null) return null;
  return decStr(valueInQuote.times(new Decimal(rate.rate)));
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
 * - equities/crypto use cached close prices (and FX when needed)
 * - missing price/FX data returns `null`
 */
export async function listBalances(
  storage: Storage,
  reportingCurrency: string,
  marketDataStore?: MarketDataStore,
): Promise<BalanceOutput[]> {
  const [connections, accounts] = await Promise.all([
    storage.listConnections(),
    storage.listAccounts(),
  ]);
  const accountsByConnection = buildAccountsByConnection(accounts);
  const reportingCurrencyUpper = reportingCurrency.trim().toUpperCase();
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
        const valueInReportingCurrencyValue = await valueInReportingCurrency(
          marketData,
          balance.asset,
          balance.amount,
          reportingCurrencyUpper,
          asOfDate,
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
        timestamp: formatRfc3339PreservingRaw(tx.timestamp, tx.timestamp_raw),
        description: tx.description,
        amount: tx.amount,
        asset: assetForListOutput(tx.asset),
        status: tx.status,
      });
    }
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
  reportingCurrency: string,
  marketDataStore?: MarketDataStore,
  dataDir?: string,
): Promise<AllOutput> {
  const [connections, accounts, balances, priceSources] = await Promise.all([
    listConnections(storage),
    listAccounts(storage),
    listBalances(storage, reportingCurrency, marketDataStore),
    listPriceSources(dataDir),
  ]);

  return {
    connections,
    accounts,
    price_sources: priceSources,
    balances,
  };
}
