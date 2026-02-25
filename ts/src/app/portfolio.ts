/**
 * Portfolio snapshot and history commands.
 *
 * Provides `serializeSnapshot` (convert library types to plain JSON-serializable
 * objects), `portfolioSnapshot` (the top-level snapshot command handler), and
 * `portfolioHistory` (the top-level history command handler).
 */

import type { Storage } from '../storage/storage.js';
import type { MarketDataStore } from '../market-data/store.js';
import { MarketDataService } from '../market-data/service.js';
import { JsonlMarketDataStore } from '../market-data/jsonl-store.js';
import { AssetId } from '../market-data/asset-id.js';
import type { PricePoint, FxRatePoint } from '../market-data/models.js';
import { PortfolioService } from '../portfolio/service.js';
import { Asset, type AssetType } from '../models/asset.js';
import { Id } from '../models/id.js';
import type { AccountType } from '../models/account.js';
import { findAccount, findConnection } from '../storage/lookup.js';
import type {
  PortfolioSnapshot,
  AssetSummary,
  AccountSummary,
  AccountHolding,
  Grouping,
} from '../portfolio/models.js';
import { type Clock, SystemClock } from '../clock.js';
import {
  formatChronoSerde,
  formatChronoSerdeFromEpochNanos,
  parseGranularity,
  formatDateYMD,
  formatRfc3339,
  formatRfc3339FromEpochNanos,
  decStr,
  decStrRounded,
} from './format.js';
import type { ResolvedConfig } from '../config.js';
import {
  collectChangePoints,
  filterByDateRange,
  filterByGranularity,
  type ChangePoint,
  type ChangeTrigger,
} from '../portfolio/change-points.js';
import type {
  HistoryOutput,
  HistoryPoint,
  HistorySummary,
  SerializedChangePoint,
  SerializedChangeTrigger,
  ChangePointsOutput,
  PriceHistoryOutput,
  PriceHistoryScopeOutput,
  PriceHistoryStats,
  PriceHistoryFailure,
} from './types.js';
import { Decimal } from '../decimal.js';
import { tryAutoCommit } from '../git.js';

// ---------------------------------------------------------------------------
// Serialization
// ---------------------------------------------------------------------------

/**
 * Serialize an `AccountHolding` to a plain object.
 * All fields are already JSON-safe strings.
 */
function serializeHolding(h: AccountHolding): object {
  return {
    account_id: h.account_id,
    account_name: h.account_name,
    amount: h.amount,
    balance_date: h.balance_date,
  };
}

/**
 * Serialize an `AssetSummary` to a plain JSON-serializable object.
 *
 * - `price_timestamp` (Date) is formatted via `formatChronoSerde`.
 * - `undefined` fields are omitted (matches Rust `skip_serializing_if`).
 */
function serializeAssetSummary(s: AssetSummary): object {
  const out: Record<string, unknown> = {
    asset: s.asset,
    total_amount: s.total_amount,
    amount_date: s.amount_date,
  };

  if (s.price !== undefined) out.price = s.price;
  if (s.price_date !== undefined) out.price_date = s.price_date;
  if (s.price_timestamp !== undefined) {
    out.price_timestamp = formatChronoSerde(s.price_timestamp);
  }
  if (s.fx_rate !== undefined) out.fx_rate = s.fx_rate;
  if (s.fx_date !== undefined) out.fx_date = s.fx_date;
  if (s.value_in_base !== undefined) out.value_in_base = s.value_in_base;
  if (s.holdings !== undefined) {
    out.holdings = s.holdings.map(serializeHolding);
  }

  return out;
}

/**
 * Serialize an `AccountSummary` to a plain object.
 *
 * `value_in_base` is omitted when undefined.
 */
function serializeAccountSummary(s: AccountSummary): object {
  const out: Record<string, unknown> = {
    account_id: s.account_id,
    account_name: s.account_name,
    connection_name: s.connection_name,
  };

  if (s.value_in_base !== undefined) out.value_in_base = s.value_in_base;

  return out;
}

/**
 * Deep-convert a `PortfolioSnapshot` to a plain JSON-serializable object.
 *
 * - `price_timestamp: Date` fields are formatted via `formatChronoSerde`.
 * - `undefined` fields are omitted by construction (matches Rust `skip_serializing_if`).
 */
export function serializeSnapshot(snapshot: PortfolioSnapshot): object {
  const out: Record<string, unknown> = {
    as_of_date: snapshot.as_of_date,
    currency: snapshot.currency,
    total_value: snapshot.total_value,
  };

  if (snapshot.by_asset !== undefined) {
    out.by_asset = snapshot.by_asset.map(serializeAssetSummary);
  }
  if (snapshot.by_account !== undefined) {
    out.by_account = snapshot.by_account.map(serializeAccountSummary);
  }

  return out;
}

// ---------------------------------------------------------------------------
// Grouping parser
// ---------------------------------------------------------------------------

function parseGrouping(s: string | undefined): Grouping {
  if (s === undefined) return 'both';
  switch (s.toLowerCase()) {
    case 'asset':
      return 'asset';
    case 'account':
      return 'account';
    case 'both':
      return 'both';
    default:
      return 'both';
  }
}

// ---------------------------------------------------------------------------
// Command handler
// ---------------------------------------------------------------------------

export interface PortfolioSnapshotOptions {
  currency?: string;
  date?: string;
  groupBy?: string;
  detail?: boolean;
}

/**
 * Execute the portfolio snapshot command.
 *
 * Creates a store-only `MarketDataService` (no external sources) and a
 * `PortfolioService`, computes the snapshot, and serializes it.
 */
export async function portfolioSnapshot(
  storage: Storage,
  marketDataStore: MarketDataStore,
  config: ResolvedConfig,
  options: PortfolioSnapshotOptions,
  clock?: Clock,
): Promise<object> {
  const effectiveClock = clock ?? new SystemClock();
  const currency = options.currency ?? config.reporting_currency;
  const asOfDate = options.date ?? effectiveClock.today();
  const grouping = parseGrouping(options.groupBy);
  const includeDetail = options.detail ?? false;

  const marketDataService = new MarketDataService(marketDataStore);
  const portfolioService = new PortfolioService(storage, marketDataService, effectiveClock);

  const snapshot = await portfolioService.calculate({
    as_of_date: asOfDate,
    currency,
    currency_decimals: config.display.currency_decimals,
    grouping,
    include_detail: includeDetail,
  });

  return serializeSnapshot(snapshot);
}

// ---------------------------------------------------------------------------
// Market Data Price History
// ---------------------------------------------------------------------------

type PriceHistoryInterval = 'daily' | 'weekly' | 'monthly' | 'yearly';

type AssetPriceCache = {
  asset: AssetType;
  asset_id: AssetId;
  prices: Map<string, PricePoint>;
};

type FxCache = Map<string, Map<string, FxRatePoint>>;

type FailureCounter = { count: number };

type FxRateContext = {
  marketData: MarketDataService;
  store: JsonlMarketDataStore;
  fxCache: FxCache;
  stats: PriceHistoryStats;
  failures: PriceHistoryFailure[];
  failureCount: FailureCounter;
  failureLimit: number;
  lookbackDays: number;
};

const MS_PER_DAY = 24 * 60 * 60 * 1000;

const YMD_REGEX = /^(\d{4})-(\d{2})-(\d{2})$/;

export interface PriceHistoryOptions {
  account?: string;
  connection?: string;
  start?: string;
  end?: string;
  interval?: string;
  lookback_days?: number;
  request_delay_ms?: number;
  currency?: string;
  include_fx?: boolean;
}

function emptyPriceHistoryStats(): PriceHistoryStats {
  return { attempted: 0, existing: 0, fetched: 0, lookback: 0, missing: 0 };
}

function parseHistoryInterval(value: string): PriceHistoryInterval {
  switch (value.trim().toLowerCase()) {
    case 'daily':
      return 'daily';
    case 'weekly':
      return 'weekly';
    case 'monthly':
      return 'monthly';
    case 'yearly':
    case 'annual':
    case 'annually':
      return 'yearly';
    default:
      throw new Error(
        `Invalid interval: ${value}. Use: daily, weekly, monthly, yearly, annual`,
      );
  }
}

function intervalAsString(interval: PriceHistoryInterval): string {
  return interval;
}

function parseYmdOrThrow(value: string, kind: 'start' | 'end'): string {
  const trimmed = value.trim();
  const match = YMD_REGEX.exec(trimmed);
  if (match === null) {
    throw new Error(`Invalid ${kind} date: ${value}`);
  }

  const year = Number.parseInt(match[1], 10);
  const month = Number.parseInt(match[2], 10);
  const day = Number.parseInt(match[3], 10);
  const parsed = new Date(Date.UTC(year, month - 1, day));
  if (
    parsed.getUTCFullYear() !== year ||
    parsed.getUTCMonth() !== month - 1 ||
    parsed.getUTCDate() !== day
  ) {
    throw new Error(`Invalid ${kind} date: ${value}`);
  }

  return formatDateYMD(parsed);
}

function parseYmdDate(value: string): Date {
  const match = YMD_REGEX.exec(value);
  if (match === null) {
    throw new Error(`Invalid date value: ${value}`);
  }
  const year = Number.parseInt(match[1], 10);
  const month = Number.parseInt(match[2], 10);
  const day = Number.parseInt(match[3], 10);
  return new Date(Date.UTC(year, month - 1, day));
}

function compareYmd(a: string, b: string): number {
  return a.localeCompare(b);
}

function addDaysYmd(value: string, days: number): string {
  const parsed = parseYmdDate(value);
  parsed.setUTCDate(parsed.getUTCDate() + days);
  return formatDateYMD(parsed);
}

function daysInMonth(year: number, month: number): number {
  return new Date(Date.UTC(year, month, 0)).getUTCDate();
}

function monthEnd(date: string): string {
  const parsed = parseYmdDate(date);
  const year = parsed.getUTCFullYear();
  const month = parsed.getUTCMonth() + 1;
  const day = daysInMonth(year, month);
  return `${year.toString().padStart(4, '0')}-${month.toString().padStart(2, '0')}-${day
    .toString()
    .padStart(2, '0')}`;
}

function yearEnd(year: number): string {
  return `${year.toString().padStart(4, '0')}-12-31`;
}

function nextMonthEnd(date: string): string {
  const parsed = parseYmdDate(date);
  let year = parsed.getUTCFullYear();
  let month = parsed.getUTCMonth() + 1;
  if (month === 12) {
    year += 1;
    month = 1;
  } else {
    month += 1;
  }
  const day = daysInMonth(year, month);
  return `${year.toString().padStart(4, '0')}-${month.toString().padStart(2, '0')}-${day
    .toString()
    .padStart(2, '0')}`;
}

function nextYearEnd(date: string): string {
  const parsed = parseYmdDate(date);
  return yearEnd(parsed.getUTCFullYear() + 1);
}

function alignStartDate(date: string, interval: PriceHistoryInterval): string {
  switch (interval) {
    case 'monthly':
      return monthEnd(date);
    case 'yearly':
      return yearEnd(parseYmdDate(date).getUTCFullYear());
    default:
      return date;
  }
}

function advanceIntervalDate(date: string, interval: PriceHistoryInterval): string {
  switch (interval) {
    case 'daily':
      return addDaysYmd(date, 1);
    case 'weekly':
      return addDaysYmd(date, 7);
    case 'monthly':
      return nextMonthEnd(date);
    case 'yearly':
      return nextYearEnd(date);
  }
}

function daysInclusive(startDate: string, endDate: string): number {
  const start = parseYmdDate(startDate).getTime();
  const end = parseYmdDate(endDate).getTime();
  return Math.floor((end - start) / MS_PER_DAY) + 1;
}

function fxKey(base: string, quote: string): string {
  return `${base}|${quote}`;
}

function errorMessage(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

function upsertPriceCache(cache: Map<string, PricePoint>, point: PricePoint): void {
  const existing = cache.get(point.as_of_date);
  if (existing === undefined || existing.timestamp.getTime() < point.timestamp.getTime()) {
    cache.set(point.as_of_date, point);
  }
}

function upsertFxCache(cache: Map<string, FxRatePoint>, point: FxRatePoint): void {
  const existing = cache.get(point.as_of_date);
  if (existing === undefined || existing.timestamp.getTime() < point.timestamp.getTime()) {
    cache.set(point.as_of_date, point);
  }
}

function resolveCachedPrice(
  cache: Map<string, PricePoint>,
  date: string,
  lookbackDays: number,
): { point: PricePoint; exact: boolean } | null {
  const exact = cache.get(date);
  if (exact !== undefined) {
    return { point: exact, exact: true };
  }

  for (let offset = 1; offset <= lookbackDays; offset++) {
    const target = addDaysYmd(date, -offset);
    const point = cache.get(target);
    if (point !== undefined) {
      return { point, exact: false };
    }
  }

  return null;
}

function resolveCachedFx(
  cache: Map<string, FxRatePoint>,
  date: string,
  lookbackDays: number,
): { point: FxRatePoint; exact: boolean } | null {
  const exact = cache.get(date);
  if (exact !== undefined) {
    return { point: exact, exact: true };
  }

  for (let offset = 1; offset <= lookbackDays; offset++) {
    const target = addDaysYmd(date, -offset);
    const point = cache.get(target);
    if (point !== undefined) {
      return { point, exact: false };
    }
  }

  return null;
}

async function loadPriceCache(
  store: MarketDataStore,
  assetId: AssetId,
): Promise<Map<string, PricePoint>> {
  const all = await store.get_all_prices(assetId);
  const cache = new Map<string, PricePoint>();
  for (const point of all) {
    if (point.kind !== 'close') continue;
    upsertPriceCache(cache, point);
  }
  return cache;
}

async function loadFxCache(
  store: MarketDataStore,
  base: string,
  quote: string,
): Promise<Map<string, FxRatePoint>> {
  const all = await store.get_all_fx_rates(base, quote);
  const cache = new Map<string, FxRatePoint>();
  for (const point of all) {
    if (point.kind !== 'close') continue;
    upsertFxCache(cache, point);
  }
  return cache;
}

async function resolvePriceHistoryScope(
  storage: Storage,
  account: string | undefined,
  connection: string | undefined,
): Promise<{ scope: PriceHistoryScopeOutput; accounts: AccountType[] }> {
  if (account !== undefined && connection !== undefined) {
    throw new Error('Specify only one of --account or --connection');
  }

  if (account !== undefined) {
    const found = await findAccount(storage, account);
    if (found === null) {
      throw new Error(`Account not found: ${account}`);
    }
    return {
      scope: { type: 'account', id: found.id.asStr(), name: found.name },
      accounts: [found],
    };
  }

  if (connection !== undefined) {
    const found = await findConnection(storage, connection);
    if (found === null) {
      throw new Error(`Connection not found: ${connection}`);
    }

    const accounts: AccountType[] = [];
    const seenIds = new Set<string>();

    for (const accountId of found.state.account_ids) {
      const accountIdStr = accountId.asStr();
      if (seenIds.has(accountIdStr)) continue;
      seenIds.add(accountIdStr);

      if (!Id.isPathSafe(accountIdStr)) {
        continue;
      }

      const accountFromStorage = await storage.getAccount(accountId);
      if (accountFromStorage === null) continue;
      if (!accountFromStorage.connection_id.equals(found.state.id)) continue;
      accounts.push(accountFromStorage);
    }

    const allAccounts = await storage.listAccounts();
    for (const accountEntry of allAccounts) {
      if (!accountEntry.connection_id.equals(found.state.id)) continue;
      const accountIdStr = accountEntry.id.asStr();
      if (seenIds.has(accountIdStr)) continue;
      seenIds.add(accountIdStr);
      accounts.push(accountEntry);
    }

    if (accounts.length === 0) {
      throw new Error(`No accounts found for connection ${found.config.name}`);
    }

    return {
      scope: { type: 'connection', id: found.state.id.asStr(), name: found.config.name },
      accounts,
    };
  }

  const accounts = await storage.listAccounts();
  if (accounts.length === 0) {
    throw new Error('No accounts found');
  }
  return { scope: { type: 'portfolio' }, accounts };
}

async function ensureFxRate(
  ctx: FxRateContext,
  base: string,
  quote: string,
  date: string,
): Promise<void> {
  ctx.stats.attempted += 1;

  const baseUpper = base.toUpperCase();
  const quoteUpper = quote.toUpperCase();
  const pairKey = fxKey(baseUpper, quoteUpper);

  if (!ctx.fxCache.has(pairKey)) {
    ctx.fxCache.set(pairKey, await loadFxCache(ctx.store, baseUpper, quoteUpper));
  }

  const pairCache = ctx.fxCache.get(pairKey);
  if (pairCache === undefined) {
    return;
  }

  const cached = resolveCachedFx(pairCache, date, ctx.lookbackDays);
  if (cached !== null) {
    if (cached.exact) {
      ctx.stats.existing += 1;
    } else {
      ctx.stats.lookback += 1;
    }
    return;
  }

  try {
    const fetched = await ctx.marketData.fxClose(baseUpper, quoteUpper, date);
    if (fetched.as_of_date === date) {
      ctx.stats.fetched += 1;
    } else {
      ctx.stats.lookback += 1;
    }

    upsertFxCache(pairCache, fetched);
  } catch (err) {
    ctx.stats.missing += 1;
    ctx.failureCount.count += 1;
    if (ctx.failures.length < ctx.failureLimit) {
      ctx.failures.push({
        kind: 'fx',
        date,
        error: errorMessage(err),
        base: baseUpper,
        quote: quoteUpper,
      });
    }
  }
}

/**
 * Fetch historical prices for assets in scope.
 *
 * Mirrors Rust `app::fetch_historical_prices` output shape and semantics.
 */
export async function fetchHistoricalPrices(
  storage: Storage,
  config: ResolvedConfig,
  options: PriceHistoryOptions,
  clock?: Clock,
): Promise<PriceHistoryOutput> {
  const effectiveClock = clock ?? new SystemClock();

  const lookbackDaysRaw = options.lookback_days ?? 7;
  if (!Number.isFinite(lookbackDaysRaw) || lookbackDaysRaw < 0) {
    throw new Error('lookback_days must be a non-negative number');
  }
  const lookbackDays = Math.trunc(lookbackDaysRaw);

  const requestDelayMsRaw = options.request_delay_ms ?? 0;
  if (!Number.isFinite(requestDelayMsRaw) || requestDelayMsRaw < 0) {
    throw new Error('request_delay_ms must be a non-negative number');
  }
  const requestDelayMs = Math.trunc(requestDelayMsRaw);
  const includeFx = options.include_fx ?? true;

  const { scope, accounts } = await resolvePriceHistoryScope(
    storage,
    options.account,
    options.connection,
  );

  const assetsByHash = new Map<string, AssetType>();
  let earliestBalanceDate: string | undefined;

  for (const account of accounts) {
    const snapshots = await storage.getBalanceSnapshots(account.id);
    for (const snapshot of snapshots) {
      const date = formatDateYMD(snapshot.timestamp);
      if (earliestBalanceDate === undefined || compareYmd(date, earliestBalanceDate) < 0) {
        earliestBalanceDate = date;
      }
      for (const balance of snapshot.balances) {
        const normalizedAsset = Asset.normalized(balance.asset);
        assetsByHash.set(Asset.hash(normalizedAsset), normalizedAsset);
      }
    }
  }

  if (assetsByHash.size === 0) {
    throw new Error('No balances found for selected scope');
  }

  const startDate =
    options.start !== undefined
      ? parseYmdOrThrow(options.start, 'start')
      : (earliestBalanceDate ?? (() => {
          throw new Error('No balances found to infer start date');
        })());
  const endDate =
    options.end !== undefined ? parseYmdOrThrow(options.end, 'end') : effectiveClock.today();

  if (compareYmd(startDate, endDate) > 0) {
    throw new Error('Start date must be on or before end date');
  }

  const interval = parseHistoryInterval(options.interval ?? 'monthly');
  const alignedStart = alignStartDate(startDate, interval);

  const targetCurrency = options.currency ?? config.reporting_currency;
  const targetCurrencyUpper = targetCurrency.toUpperCase();

  const store = new JsonlMarketDataStore(config.data_dir);
  const marketData = new MarketDataService(store).withLookbackDays(lookbackDays);

  const assetCaches: AssetPriceCache[] = [];
  for (const asset of assetsByHash.values()) {
    const assetId = AssetId.fromAsset(asset);
    assetCaches.push({
      asset,
      asset_id: assetId,
      prices: await loadPriceCache(store, assetId),
    });
  }

  assetCaches.sort((a, b) => a.asset_id.asStr().localeCompare(b.asset_id.asStr()));

  const fxCache: FxCache = new Map();
  if (includeFx) {
    for (const assetCache of assetCaches) {
      if (assetCache.asset.type !== 'currency') continue;
      const base = assetCache.asset.iso_code.toUpperCase();
      if (base === targetCurrencyUpper) continue;
      const key = fxKey(base, targetCurrencyUpper);
      if (!fxCache.has(key)) {
        fxCache.set(key, await loadFxCache(store, base, targetCurrencyUpper));
      }
    }
  }

  const prices = emptyPriceHistoryStats();
  const fx = emptyPriceHistoryStats();
  const failures: PriceHistoryFailure[] = [];
  const failureCount: FailureCounter = { count: 0 };
  const failureLimit = 50;
  const shouldDelayRequests = requestDelayMs > 0;

  const fxCtx: FxRateContext = {
    marketData,
    store,
    fxCache,
    stats: fx,
    failures,
    failureCount,
    failureLimit,
    lookbackDays,
  };

  let current = alignedStart;
  let points = 0;
  while (compareYmd(current, endDate) <= 0) {
    points += 1;

    for (const assetCache of assetCaches) {
      let shouldDelay = false;

      switch (assetCache.asset.type) {
        case 'currency': {
          if (includeFx) {
            const base = assetCache.asset.iso_code.toUpperCase();
            if (base !== targetCurrencyUpper) {
              await ensureFxRate(fxCtx, base, targetCurrencyUpper, current);
            }
          }
          break;
        }

        case 'equity':
        case 'crypto': {
          prices.attempted += 1;
          const cached = resolveCachedPrice(assetCache.prices, current, lookbackDays);
          if (cached !== null) {
            if (cached.exact) {
              prices.existing += 1;
            } else {
              prices.lookback += 1;
            }

            if (includeFx && cached.point.quote_currency.toUpperCase() !== targetCurrencyUpper) {
              await ensureFxRate(
                fxCtx,
                cached.point.quote_currency.toUpperCase(),
                targetCurrencyUpper,
                current,
              );
            }
            break;
          }

          try {
            const fetched = await marketData.priceClose(assetCache.asset, current);
            if (fetched.as_of_date === current) {
              prices.fetched += 1;
            } else {
              prices.lookback += 1;
            }
            upsertPriceCache(assetCache.prices, fetched);
            shouldDelay = shouldDelayRequests;

            if (includeFx && fetched.quote_currency.toUpperCase() !== targetCurrencyUpper) {
              await ensureFxRate(
                fxCtx,
                fetched.quote_currency.toUpperCase(),
                targetCurrencyUpper,
                current,
              );
            }
          } catch (err) {
            prices.missing += 1;
            failureCount.count += 1;
            if (failures.length < failureLimit) {
              failures.push({
                kind: 'price',
                date: current,
                error: errorMessage(err),
                asset_id: assetCache.asset_id.asStr(),
                asset: assetCache.asset,
              });
            }
            shouldDelay = shouldDelayRequests;
          }

          break;
        }
      }

      if (shouldDelay) {
        await new Promise<void>((resolve) => {
          setTimeout(resolve, requestDelayMs);
        });
      }
    }

    current = advanceIntervalDate(current, interval);
  }

  if (config.git.auto_commit) {
    try {
      await tryAutoCommit(config.data_dir, 'market data fetch', config.git.auto_push);
    } catch {
      // Keep command behavior best-effort for auto-commit.
    }
  }

  return {
    scope,
    currency: targetCurrency,
    interval: intervalAsString(interval),
    start_date: startDate,
    end_date: endDate,
    earliest_balance_date: earliestBalanceDate,
    days: daysInclusive(startDate, endDate),
    points,
    assets: assetCaches.map((cache) => ({ asset: cache.asset, asset_id: cache.asset_id.asStr() })),
    prices,
    fx: includeFx ? fx : undefined,
    failure_count: failureCount.count,
    failures: failures.length > 0 ? failures : undefined,
  };
}

// ---------------------------------------------------------------------------
// Portfolio History
// ---------------------------------------------------------------------------

export interface PortfolioHistoryOptions {
  currency?: string;
  start?: string;
  end?: string;
  granularity?: string;
  includePrices?: boolean;
}

/**
 * Format a ChangeTrigger into its string representation.
 *
 * - Balance trigger: `"balance:<account_id>:<json_asset>"` (compact JSON, no spaces)
 * - Price trigger: `"price:<asset_id_string>"`
 * - FxRate trigger: `"fx:<base>/<quote>"`
 */
function formatTrigger(trigger: ChangeTrigger): string {
  switch (trigger.type) {
    case 'balance':
      return `balance:${trigger.account_id}:${JSON.stringify(trigger.asset)}`;
    case 'price':
      return `price:${trigger.asset_id.asStr()}`;
    case 'fx_rate':
      return `fx:${trigger.base}/${trigger.quote}`;
  }
}

function computeHistoryTotalValueWithCarryForward(
  byAsset: AssetSummary[],
  carryForwardUnitValues: Map<string, Decimal>,
  currencyDecimals: number | undefined,
): string | undefined {
  try {
    let totalValue = new Decimal(0);

    for (const summary of byAsset) {
      const assetId = AssetId.fromAsset(summary.asset).asStr();
      const totalAmount = new Decimal(summary.total_amount);
      let assetValue: Decimal;

      if (summary.value_in_base !== undefined) {
        assetValue = new Decimal(summary.value_in_base);
        if (!totalAmount.isZero()) {
          carryForwardUnitValues.set(assetId, assetValue.div(totalAmount));
        }
      } else if (totalAmount.isZero()) {
        assetValue = new Decimal(0);
      } else {
        const unitValue = carryForwardUnitValues.get(assetId);
        assetValue = unitValue !== undefined ? unitValue.times(totalAmount) : new Decimal(0);
      }

      totalValue = totalValue.plus(assetValue);
    }

    return decStrRounded(totalValue, currencyDecimals);
  } catch {
    return undefined;
  }
}

/**
 * Execute the portfolio history command.
 *
 * Collects change points from storage and market data, calculates portfolio
 * value at each point, and returns the history with optional summary statistics.
 */
export async function portfolioHistory(
  storage: Storage,
  marketDataStore: MarketDataStore,
  config: ResolvedConfig,
  options: PortfolioHistoryOptions,
  clock?: Clock,
): Promise<HistoryOutput> {
  const effectiveClock = clock ?? new SystemClock();
  const currency = options.currency ?? config.reporting_currency;
  const granularity = parseGranularity(options.granularity ?? 'none');

  const marketDataService = new MarketDataService(marketDataStore);

  // Collect change points
  const allPoints = await collectChangePoints(storage, marketDataStore, {
    includePrices: options.includePrices ?? true,
  });

  // Filter by date range
  const dateFiltered = filterByDateRange(allPoints, options.start, options.end);

  // Filter by granularity
  const points = filterByGranularity(dateFiltered, granularity, 'last');

  // Calculate portfolio value at each change point
  const historyPoints: HistoryPoint[] = [];
  let previousTotalValue: Decimal | undefined;
  const carryForwardUnitValues = new Map<string, Decimal>();
  for (const point of points) {
    const portfolioService = new PortfolioService(storage, marketDataService, effectiveClock);

    const snapshot = await portfolioService.calculate({
      as_of_date: formatDateYMD(point.timestamp),
      currency,
      currency_decimals: config.display.currency_decimals,
      grouping: 'both',
      include_detail: false,
    });

    const totalValue =
      snapshot.by_asset !== undefined
        ? computeHistoryTotalValueWithCarryForward(
            snapshot.by_asset,
            carryForwardUnitValues,
            config.display.currency_decimals,
          ) ?? snapshot.total_value
        : snapshot.total_value;
    const currentTotalValue = new Decimal(totalValue);

    // Format triggers
    const triggers = point.triggers.map(formatTrigger);

    let percentageChangeFromPrevious: string | null = null;
    if (previousTotalValue !== undefined) {
      if (previousTotalValue.isZero()) {
        percentageChangeFromPrevious = 'N/A';
      } else {
        percentageChangeFromPrevious = currentTotalValue
          .minus(previousTotalValue)
          .div(previousTotalValue)
          .times(100)
          .toDecimalPlaces(2)
          .toFixed(2);
      }
    }

    const historyPoint: HistoryPoint = {
      timestamp:
        point.timestamp_nanos !== undefined
          ? formatRfc3339FromEpochNanos(point.timestamp_nanos)
          : formatRfc3339(point.timestamp),
      date: formatDateYMD(point.timestamp),
      total_value: totalValue,
      percentage_change_from_previous: percentageChangeFromPrevious,
      change_triggers: triggers.length > 0 ? triggers : undefined,
    };

    historyPoints.push(historyPoint);
    previousTotalValue = currentTotalValue;
  }

  // Calculate summary if 2+ points
  let summary: HistorySummary | undefined;
  if (historyPoints.length >= 2) {
    const initialValue = new Decimal(historyPoints[0].total_value);
    const finalValue = new Decimal(historyPoints[historyPoints.length - 1].total_value);
    const absoluteChange = finalValue.minus(initialValue);

    let percentageChange: string;
    if (initialValue.isZero()) {
      percentageChange = 'N/A';
    } else {
      const pct = finalValue.minus(initialValue).div(initialValue).times(100);
      percentageChange = pct.toDecimalPlaces(2).toFixed(2);
    }

    summary = {
      initial_value: decStr(initialValue),
      final_value: decStr(finalValue),
      absolute_change: decStr(absoluteChange),
      percentage_change: percentageChange,
    };
  }

  return {
    currency,
    start_date: options.start ?? null,
    end_date: options.end ?? null,
    granularity: options.granularity ?? 'none',
    points: historyPoints,
    summary,
  };
}

// ---------------------------------------------------------------------------
// Portfolio Change Points
// ---------------------------------------------------------------------------

/**
 * Serialize a `ChangeTrigger` to its JSON-serializable form.
 *
 * - Balance: `{type: 'balance', account_id: "<id>", asset: <asset>}`
 * - Price: `{type: 'price', asset_id: "<asset_id_string>"}`
 * - FxRate: `{type: 'fx_rate', base: "<base>", quote: "<quote>"}`
 */
export function serializeChangeTrigger(trigger: ChangeTrigger): SerializedChangeTrigger {
  switch (trigger.type) {
    case 'balance':
      return { type: 'balance', account_id: trigger.account_id.asStr(), asset: trigger.asset };
    case 'price':
      return { type: 'price', asset_id: trigger.asset_id.asStr() };
    case 'fx_rate':
      return { type: 'fx_rate', base: trigger.base, quote: trigger.quote };
  }
}

/**
 * Serialize a `ChangePoint` to its JSON-serializable form.
 *
 * Uses `formatChronoSerde` (Z suffix) because Rust's `ChangePoint.timestamp`
 * is serialized via chrono serde derive, not manual `to_rfc3339`.
 */
export function serializeChangePoint(point: ChangePoint): SerializedChangePoint {
  return {
    timestamp:
      point.timestamp_nanos !== undefined
        ? formatChronoSerdeFromEpochNanos(point.timestamp_nanos)
        : formatChronoSerde(point.timestamp),
    triggers: point.triggers.map(serializeChangeTrigger),
  };
}

export interface PortfolioChangePointsOptions {
  start?: string;
  end?: string;
  granularity?: string;
  includePrices?: boolean;
}

/**
 * Execute the portfolio change-points command.
 *
 * Collects change points from storage and market data, filters by date range
 * and granularity, serializes each point, and returns the output.
 */
export async function portfolioChangePoints(
  storage: Storage,
  marketDataStore: MarketDataStore,
  _config: ResolvedConfig,
  options: PortfolioChangePointsOptions,
  _clock?: Clock,
): Promise<ChangePointsOutput> {
  const granularity = parseGranularity(options.granularity ?? 'none');

  // Collect change points
  const allPoints = await collectChangePoints(storage, marketDataStore, {
    includePrices: options.includePrices ?? true,
  });

  // Filter by date range
  const dateFiltered = filterByDateRange(allPoints, options.start, options.end);

  // Filter by granularity
  const points = filterByGranularity(dateFiltered, granularity, 'last');

  // Serialize each change point
  const serialized = points.map(serializeChangePoint);

  return {
    start_date: options.start ?? null,
    end_date: options.end ?? null,
    granularity: options.granularity ?? 'none',
    include_prices: options.includePrices ?? true,
    points: serialized,
  };
}
