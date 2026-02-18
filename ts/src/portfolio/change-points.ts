/**
 * Change points module.
 *
 * Port of the Rust `change_points` module. Tracks when portfolio state
 * changes occur (balance updates, price changes, FX rate changes) and
 * provides filtering by granularity and date range.
 */

import { Id } from '../models/id.js';
import { type AssetType } from '../models/asset.js';
import { AssetId } from '../market-data/asset-id.js';
import { type MarketDataStore } from '../market-data/store.js';
import { type Storage } from '../storage/storage.js';

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface ChangePoint {
  timestamp: Date;
  timestamp_nanos?: string;
  triggers: ChangeTrigger[];
}

export type ChangeTrigger =
  | { type: 'balance'; account_id: Id; asset: AssetType }
  | { type: 'price'; asset_id: AssetId }
  | { type: 'fx_rate'; base: string; quote: string };

export type Granularity =
  | 'full'
  | 'hourly'
  | 'daily'
  | 'weekly'
  | 'monthly'
  | 'yearly'
  | { custom_ms: number };

export type CoalesceStrategy = 'first' | 'last';

export interface CollectOptions {
  accountIds?: Id[];
  includePrices?: boolean;
  includeFx?: boolean;
  targetCurrency?: string;
}

// ---------------------------------------------------------------------------
// ChangePointCollector
// ---------------------------------------------------------------------------

/**
 * Collects change points from various sources and merges triggers
 * that share the same timestamp. Equivalent to a BTreeMap<timestamp, Vec<ChangeTrigger>>
 * in Rust, using a Map<number, ChangeTrigger[]> keyed by epoch milliseconds.
 */
export class ChangePointCollector {
  private readonly points: Map<
    string,
    { timestamp: Date; timestamp_nanos: string; triggers: ChangeTrigger[] }
  > = new Map();
  private readonly held: Set<string> = new Set();

  addBalanceChange(timestamp: Date, accountId: Id, asset: AssetType, timestampRaw?: string): void {
    const key = timestampToNanos(timestamp, timestampRaw);
    const trigger: ChangeTrigger = { type: 'balance', account_id: accountId, asset };
    const existing = this.points.get(key);
    if (existing) {
      existing.triggers.push(trigger);
    } else {
      this.points.set(key, {
        timestamp,
        timestamp_nanos: key,
        triggers: [trigger],
      });
    }

    // Track the held asset
    const assetId = AssetId.fromAsset(asset);
    this.held.add(assetId.asStr());
  }

  addPriceChange(timestamp: Date, assetId: AssetId): void {
    const key = timestampToNanos(timestamp);
    const trigger: ChangeTrigger = { type: 'price', asset_id: assetId };
    const existing = this.points.get(key);
    if (existing) {
      existing.triggers.push(trigger);
    } else {
      this.points.set(key, {
        timestamp,
        timestamp_nanos: key,
        triggers: [trigger],
      });
    }
  }

  addFxChange(timestamp: Date, base: string, quote: string): void {
    const key = timestampToNanos(timestamp);
    const trigger: ChangeTrigger = { type: 'fx_rate', base, quote };
    const existing = this.points.get(key);
    if (existing) {
      existing.triggers.push(trigger);
    } else {
      this.points.set(key, {
        timestamp,
        timestamp_nanos: key,
        triggers: [trigger],
      });
    }
  }

  heldAssets(): Set<string> {
    return new Set(this.held);
  }

  /**
   * Convert to sorted change points. Sorts entries by timestamp (ascending).
   */
  intoChangePoints(): ChangePoint[] {
    const entries = Array.from(this.points.entries());
    entries.sort((a, b) => {
      const an = BigInt(a[0]);
      const bn = BigInt(b[0]);
      return an < bn ? -1 : an > bn ? 1 : 0;
    });
    return entries.map(([, point]) => ({
      timestamp: point.timestamp,
      timestamp_nanos: point.timestamp_nanos,
      triggers: point.triggers,
    }));
  }

  get length(): number {
    return this.points.size;
  }

  get isEmpty(): boolean {
    return this.points.size === 0;
  }
}

// ---------------------------------------------------------------------------
// dateToTimestamp
// ---------------------------------------------------------------------------

/**
 * Convert a "YYYY-MM-DD" date string to a Date at end of day (23:59:59 UTC).
 */
export function dateToTimestamp(date: string): Date {
  const [yearStr, monthStr, dayStr] = date.split('-');
  const year = parseInt(yearStr, 10);
  const month = parseInt(monthStr, 10) - 1; // 0-indexed
  const day = parseInt(dayStr, 10);
  return new Date(Date.UTC(year, month, day, 23, 59, 59));
}

// ---------------------------------------------------------------------------
// filterByGranularity
// ---------------------------------------------------------------------------

const MS_PER_HOUR = 3600000;
const MS_PER_DAY = 86400000;
const MS_PER_WEEK = MS_PER_DAY * 7;

/**
 * Compute a bucket key for a timestamp given a granularity.
 * Returns a string or number that groups timestamps into the same bucket.
 */
function bucketKey(ts: number, granularity: Granularity): string | number {
  if (typeof granularity === 'object') {
    // custom_ms
    return Math.floor(ts / granularity.custom_ms);
  }
  switch (granularity) {
    case 'full':
      return ts; // unique per timestamp
    case 'hourly':
      return Math.floor(ts / MS_PER_HOUR);
    case 'daily':
      return Math.floor(ts / MS_PER_DAY);
    case 'weekly':
      return Math.floor(ts / MS_PER_WEEK);
    case 'monthly': {
      const d = new Date(ts);
      return `${d.getUTCFullYear()}-${d.getUTCMonth()}`;
    }
    case 'yearly': {
      const d = new Date(ts);
      return `${d.getUTCFullYear()}`;
    }
  }
}

/**
 * Filter change points by granularity, keeping either the first or last
 * point in each bucket.
 */
export function filterByGranularity(
  points: ChangePoint[],
  granularity: Granularity,
  strategy: CoalesceStrategy,
): ChangePoint[] {
  // Pass-through cases
  if (granularity === 'full') {
    return points;
  }
  if (typeof granularity === 'object' && granularity.custom_ms <= 0) {
    return points;
  }
  if (points.length === 0) {
    return [];
  }

  // Group by bucket, preserving insertion order (which should be sorted)
  const buckets = new Map<string | number, ChangePoint>();
  for (const point of points) {
    const key = bucketKey(point.timestamp.getTime(), granularity);
    if (strategy === 'first') {
      // Keep first seen
      if (!buckets.has(key)) {
        buckets.set(key, point);
      }
    } else {
      // 'last': always overwrite
      buckets.set(key, point);
    }
  }

  return Array.from(buckets.values());
}

// ---------------------------------------------------------------------------
// filterByDateRange
// ---------------------------------------------------------------------------

/**
 * Extract the "YYYY-MM-DD" date portion of a Date (UTC).
 */
function formatDateUTC(d: Date): string {
  const year = d.getUTCFullYear().toString().padStart(4, '0');
  const month = (d.getUTCMonth() + 1).toString().padStart(2, '0');
  const day = d.getUTCDate().toString().padStart(2, '0');
  return `${year}-${month}-${day}`;
}

function timestampToNanos(timestamp: Date, rawTimestamp?: string): string {
  if (rawTimestamp !== undefined) {
    const parsed = parseRfc3339ToEpochNanos(rawTimestamp);
    if (parsed !== null) {
      return parsed.toString();
    }
  }
  return (BigInt(timestamp.getTime()) * 1000000n).toString();
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

/**
 * Filter change points to those whose date portion (UTC) falls
 * within [start, end]. Both start and end are inclusive "YYYY-MM-DD" strings.
 */
export function filterByDateRange(
  points: ChangePoint[],
  start?: string,
  end?: string,
): ChangePoint[] {
  if (start === undefined && end === undefined) {
    return points;
  }

  return points.filter((point) => {
    const dateStr = formatDateUTC(point.timestamp);
    if (start !== undefined && dateStr < start) {
      return false;
    }
    if (end !== undefined && dateStr > end) {
      return false;
    }
    return true;
  });
}

// ---------------------------------------------------------------------------
// collectChangePoints
// ---------------------------------------------------------------------------

/**
 * Collect change points from storage and market data.
 *
 * 1. If accountIds provided, get those accounts; else list all.
 * 2. For each account, get balance snapshots. For each snapshot/balance,
 *    add a balance change trigger.
 * 3. If includePrices, for each held asset, get all prices from store
 *    and add price change triggers.
 * 4. Return sorted change points.
 */
export async function collectChangePoints(
  storage: Storage,
  marketData: MarketDataStore,
  options: CollectOptions,
): Promise<ChangePoint[]> {
  const collector = new ChangePointCollector();

  // Step 1: determine accounts
  let accounts;
  if (options.accountIds && options.accountIds.length > 0) {
    const results = await Promise.all(options.accountIds.map((id) => storage.getAccount(id)));
    accounts = results.filter((a) => {
      if (a === null) return false;
      return storage.getAccountConfig(a.id)?.exclude_from_portfolio !== true;
    });
  } else {
    const all = await storage.listAccounts();
    accounts = all.filter((a) => storage.getAccountConfig(a.id)?.exclude_from_portfolio !== true);
  }

  // Step 2: collect balance changes
  for (const account of accounts) {
    const snapshots = await storage.getBalanceSnapshots(account.id);
    for (const snapshot of snapshots) {
      for (const balance of snapshot.balances) {
        collector.addBalanceChange(
          snapshot.timestamp,
          account.id,
          balance.asset,
          snapshot.timestamp_raw,
        );
      }
    }
  }

  // Step 3: if includePrices, add price changes for held assets
  if (options.includePrices) {
    const heldAssets = Array.from(collector.heldAssets()).sort((a, b) => a.localeCompare(b));
    for (const assetIdStr of heldAssets) {
      const assetId = AssetId.fromString(assetIdStr);
      const prices = await marketData.get_all_prices(assetId);
      for (const price of prices) {
        // Match Rust behavior: price change points are keyed by as_of_date
        // (end-of-day), not by quote ingest timestamp.
        collector.addPriceChange(dateToTimestamp(price.as_of_date), assetId);
      }
    }
  }

  // Step 4: return sorted
  return collector.intoChangePoints();
}
