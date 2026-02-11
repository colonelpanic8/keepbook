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
import { PortfolioService } from '../portfolio/service.js';
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
} from './types.js';
import { Decimal } from '../decimal.js';

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
    grouping,
    include_detail: includeDetail,
  });

  return serializeSnapshot(snapshot);
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
  for (const point of points) {
    const portfolioService = new PortfolioService(storage, marketDataService, effectiveClock);

    const snapshot = await portfolioService.calculate({
      as_of_date: formatDateYMD(point.timestamp),
      currency,
      grouping: 'both',
      include_detail: false,
    });

    const totalValue = snapshot.total_value;

    // Format triggers
    const triggers = point.triggers.map(formatTrigger);

    const historyPoint: HistoryPoint = {
      timestamp:
        point.timestamp_nanos !== undefined
          ? formatRfc3339FromEpochNanos(point.timestamp_nanos)
          : formatRfc3339(point.timestamp),
      date: formatDateYMD(point.timestamp),
      total_value: totalValue,
      change_triggers: triggers.length > 0 ? triggers : undefined,
    };

    historyPoints.push(historyPoint);
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
