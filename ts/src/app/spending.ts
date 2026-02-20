import type { Storage } from '../storage/storage.js';
import type { MarketDataStore } from '../market-data/store.js';
import { MarketDataService } from '../market-data/service.js';
import { Decimal } from '../decimal.js';
import { parseDuration } from '../duration.js';
import type { TransactionType, TransactionStatus } from '../models/transaction.js';
import { Asset, type AssetType } from '../models/asset.js';
import type { AccountType } from '../models/account.js';
import type { ResolvedConfig } from '../config.js';
import { findAccount, findConnection } from '../storage/lookup.js';
import {
  applyTransactionAnnotationPatch,
  isEmptyTransactionAnnotation,
  type TransactionAnnotationType,
} from '../models/transaction-annotation.js';
import { decStrRounded } from './format.js';
import { valueInReportingCurrencyDetailed } from './value.js';
import type {
  SpendingOutput,
  SpendingScopeOutput,
  SpendingPeriodOutput,
  SpendingBreakdownEntryOutput,
} from './types.js';

type Ymd = { y: number; m: number; d: number };

function pad2(n: number): string {
  return n.toString().padStart(2, '0');
}

function ymdToString(ymd: Ymd): string {
  return `${ymd.y.toString().padStart(4, '0')}-${pad2(ymd.m)}-${pad2(ymd.d)}`;
}

function parseYmd(s: string): Ymd {
  const m = s.trim().match(/^(\d{4})-(\d{2})-(\d{2})$/);
  if (!m) throw new Error(`Invalid date '${s}' (expected YYYY-MM-DD)`);
  const y = Number.parseInt(m[1], 10);
  const mo = Number.parseInt(m[2], 10);
  const d = Number.parseInt(m[3], 10);
  return { y, m: mo, d };
}

function ymdToUtcDate(ymd: Ymd): Date {
  return new Date(Date.UTC(ymd.y, ymd.m - 1, ymd.d, 0, 0, 0));
}

function utcDateToYmd(d: Date): Ymd {
  return { y: d.getUTCFullYear(), m: d.getUTCMonth() + 1, d: d.getUTCDate() };
}

function addDays(ymd: Ymd, days: number): Ymd {
  const d = ymdToUtcDate(ymd);
  d.setUTCDate(d.getUTCDate() + days);
  return utcDateToYmd(d);
}

function compareYmd(a: Ymd, b: Ymd): number {
  if (a.y !== b.y) return a.y < b.y ? -1 : 1;
  if (a.m !== b.m) return a.m < b.m ? -1 : 1;
  if (a.d !== b.d) return a.d < b.d ? -1 : 1;
  return 0;
}

function clampYmd(x: Ymd, min: Ymd, max: Ymd): Ymd {
  if (compareYmd(x, min) < 0) return min;
  if (compareYmd(x, max) > 0) return max;
  return x;
}

function diffDays(a: Ymd, b: Ymd): number {
  const da = Date.UTC(a.y, a.m - 1, a.d);
  const db = Date.UTC(b.y, b.m - 1, b.d);
  return Math.floor((da - db) / 86400000);
}

function lastDayOfMonth(y: number, m: number): Ymd {
  // month param is 1-12, Date.UTC month is 0-11; using (m, 0) yields last day of month m.
  const d = new Date(Date.UTC(y, m, 0));
  return utcDateToYmd(d);
}

function weekdayFromYmd(ymd: Ymd): number {
  // 0=Sunday .. 6=Saturday
  return new Date(Date.UTC(ymd.y, ymd.m - 1, ymd.d)).getUTCDay();
}

function ymdFromTimestampInTimeZone(date: Date, timeZone: string): Ymd {
  const dtf = new Intl.DateTimeFormat('en-CA', {
    timeZone,
    year: 'numeric',
    month: '2-digit',
    day: '2-digit',
  });
  const parts = dtf.formatToParts(date);
  const get = (type: string): string => {
    const p = parts.find((x) => x.type === type);
    if (!p) throw new Error(`Failed to format date in timezone '${timeZone}'`);
    return p.value;
  };
  return {
    y: Number.parseInt(get('year'), 10),
    m: Number.parseInt(get('month'), 10),
    d: Number.parseInt(get('day'), 10),
  };
}

type Period =
  | 'daily'
  | 'weekly'
  | 'monthly'
  | 'quarterly'
  | 'yearly'
  | 'range'
  | { custom_days: number };

type Direction = 'outflow' | 'inflow' | 'net';
type StatusFilter = 'posted' | 'posted+pending' | 'all';
type GroupBy = 'none' | 'category' | 'merchant' | 'account' | 'tag';
type WeekStart = 'sunday' | 'monday';

function parsePeriod(period: string, bucket?: string): { period: Period; label: string; bucketDays?: number } {
  const p = period.trim().toLowerCase();
  switch (p) {
    case 'daily':
    case 'weekly':
    case 'monthly':
    case 'quarterly':
    case 'range':
      return { period: p, label: p };
    case 'yearly':
    case 'annual':
      return { period: 'yearly', label: 'yearly' };
    case 'custom': {
      if (bucket === undefined) throw new Error('Missing --bucket for period=custom');
      const ms = parseDuration(bucket);
      if (ms <= 0 || ms % 86400000 !== 0) {
        throw new Error("Custom bucket duration must be a positive multiple of 1d (e.g. '14d')");
      }
      const days = Math.trunc(ms / 86400000);
      return { period: { custom_days: days }, label: 'custom', bucketDays: days };
    }
    default:
      throw new Error(
        `Invalid period '${period}'. Valid values: daily, weekly, monthly, quarterly, yearly, range, custom`,
      );
  }
}

function parseDirection(s: string | undefined): Direction {
  const v = (s ?? 'outflow').trim().toLowerCase();
  if (v === 'outflow' || v === 'inflow' || v === 'net') return v;
  throw new Error(`Invalid direction '${s}'. Valid values: outflow, inflow, net`);
}

function parseStatusFilter(s: string | undefined): StatusFilter {
  const v = (s ?? 'posted').trim().toLowerCase();
  if (v === 'posted' || v === 'all') return v;
  if (v === 'posted+pending' || v === 'posted_pending' || v === 'posted-pending') return 'posted+pending';
  throw new Error(`Invalid status '${s}'. Valid values: posted, posted+pending, all`);
}

function parseGroupBy(s: string | undefined): GroupBy {
  const v = (s ?? 'none').trim().toLowerCase();
  if (v === 'none' || v === 'category' || v === 'merchant' || v === 'account' || v === 'tag') return v;
  throw new Error(`Invalid group_by '${s}'. Valid values: none, category, merchant, account, tag`);
}

function parseWeekStart(s: string | undefined): WeekStart {
  const v = (s ?? 'sunday').trim().toLowerCase();
  if (v === 'sunday' || v === 'sun') return 'sunday';
  if (v === 'monday' || v === 'mon') return 'monday';
  throw new Error(`Invalid week_start '${s}'. Valid values: sunday, monday`);
}

function normalizeRule(value: string): string | null {
  const trimmed = value.trim();
  return trimmed.length > 0 ? trimmed.toLowerCase() : null;
}

async function ignoredAccountIdsForPortfolioSpending(
  storage: Storage,
  config: ResolvedConfig,
  accounts: AccountType[],
): Promise<Set<string>> {
  const ignoreAccounts = new Set(
    config.spending.ignore_accounts.map((v) => normalizeRule(v)).filter((v): v is string => v !== null),
  );
  const ignoreConnectionsRaw = new Set(
    config.spending.ignore_connections.map((v) => normalizeRule(v)).filter((v): v is string => v !== null),
  );
  const ignoreTags = new Set(
    config.spending.ignore_tags.map((v) => normalizeRule(v)).filter((v): v is string => v !== null),
  );

  if (ignoreAccounts.size === 0 && ignoreConnectionsRaw.size === 0 && ignoreTags.size === 0) {
    return new Set();
  }

  const ignoreConnections = new Set(ignoreConnectionsRaw);
  const connections = await storage.listConnections();
  for (const connection of connections) {
    const connectionId = connection.state.id.asStr().toLowerCase();
    const connectionName = connection.config.name.toLowerCase();
    if (ignoreConnectionsRaw.has(connectionId) || ignoreConnectionsRaw.has(connectionName)) {
      ignoreConnections.add(connectionId);
    }
  }

  const ignoredAccountIds = new Set<string>();
  for (const account of accounts) {
    const accountId = account.id.asStr().toLowerCase();
    const accountName = account.name.toLowerCase();
    const connectionId = account.connection_id.asStr().toLowerCase();
    const hasIgnoredTag = account.tags
      .map((tag) => normalizeRule(tag))
      .filter((tag): tag is string => tag !== null)
      .some((tag) => ignoreTags.has(tag));
    if (
      ignoreAccounts.has(accountId) ||
      ignoreAccounts.has(accountName) ||
      ignoreConnections.has(connectionId) ||
      hasIgnoredTag
    ) {
      ignoredAccountIds.add(account.id.asStr());
    }
  }

  return ignoredAccountIds;
}

function includeStatus(status: TransactionStatus, filter: StatusFilter): boolean {
  switch (filter) {
    case 'all':
      return true;
    case 'posted':
      return status === 'posted';
    case 'posted+pending':
      return status === 'posted' || status === 'pending';
  }
}

function applyDirection(valueInBase: Decimal, direction: Direction): Decimal {
  switch (direction) {
    case 'net':
      return valueInBase;
    case 'outflow':
      return valueInBase.isNeg() && !valueInBase.isZero() ? valueInBase.neg() : new Decimal(0);
    case 'inflow':
      return valueInBase.isPos() && !valueInBase.isZero() ? valueInBase : new Decimal(0);
  }
}

function bucketStartFor(date: Ymd, period: Period, weekStart: WeekStart, rangeStart: Ymd): Ymd {
  if (typeof period === 'object') {
    const days = period.custom_days;
    const delta = diffDays(date, rangeStart);
    if (delta < 0) return rangeStart;
    const buckets = Math.floor(delta / days);
    return addDays(rangeStart, buckets * days);
  }
  switch (period) {
    case 'daily':
      return date;
    case 'weekly': {
      const wd = weekdayFromYmd(date); // 0..6 (Sun..Sat)
      const offset = weekStart === 'sunday' ? wd : (wd + 6) % 7;
      return addDays(date, -offset);
    }
    case 'monthly':
      return { y: date.y, m: date.m, d: 1 };
    case 'quarterly': {
      const q0 = Math.floor((date.m - 1) / 3) * 3;
      return { y: date.y, m: q0 + 1, d: 1 };
    }
    case 'yearly':
      return { y: date.y, m: 1, d: 1 };
    case 'range':
      return rangeStart;
  }
}

function bucketEndFor(start: Ymd, period: Period, rangeEnd: Ymd): Ymd {
  if (typeof period === 'object') {
    return addDays(start, period.custom_days - 1);
  }
  switch (period) {
    case 'daily':
      return start;
    case 'weekly':
      return addDays(start, 6);
    case 'monthly':
      return lastDayOfMonth(start.y, start.m);
    case 'quarterly':
      return lastDayOfMonth(start.y, start.m + 2);
    case 'yearly':
      return { y: start.y, m: 12, d: 31 };
    case 'range':
      return rangeEnd;
  }
}

function nextBucketStart(start: Ymd, period: Period): Ymd {
  if (typeof period === 'object') {
    return addDays(start, period.custom_days);
  }
  switch (period) {
    case 'daily':
      return addDays(start, 1);
    case 'weekly':
      return addDays(start, 7);
    case 'monthly': {
      const y = start.y;
      const m = start.m;
      if (m === 12) return { y: y + 1, m: 1, d: 1 };
      return { y, m: m + 1, d: 1 };
    }
    case 'quarterly': {
      let y = start.y;
      let m = start.m + 3;
      while (m > 12) {
        m -= 12;
        y += 1;
      }
      return { y, m, d: 1 };
    }
    case 'yearly':
      return { y: start.y + 1, m: 1, d: 1 };
    case 'range':
      return start;
  }
}

export type SpendingReportOptions = {
  currency?: string;
  start?: string;
  end?: string;
  period: string;
  tz?: string;
  week_start?: string;
  bucket?: string;
  account?: string;
  connection?: string;
  status?: string;
  direction?: string;
  group_by?: string;
  top?: number;
  lookback_days?: number;
  include_noncurrency?: boolean;
  include_empty?: boolean;
};

export async function spendingReport(
  storage: Storage,
  marketDataStore: MarketDataStore,
  config: ResolvedConfig,
  options: SpendingReportOptions,
): Promise<SpendingOutput> {
  const reportingCurrency = (options.currency ?? config.reporting_currency).trim().toUpperCase();

  // Timezone: output uses "local" unless explicitly set.
  const tzRaw = options.tz?.trim();
  const tzLabel = tzRaw && tzRaw.length > 0 && tzRaw.toLowerCase() !== 'local' && tzRaw.toLowerCase() !== 'current'
    ? tzRaw
    : 'local';
  const effectiveTimeZone =
    tzLabel === 'local' ? Intl.DateTimeFormat().resolvedOptions().timeZone : tzLabel === 'UTC' ? 'UTC' : tzLabel;

  // Validate timezone early.
  try {
    // eslint-disable-next-line no-new
    new Intl.DateTimeFormat('en-CA', { timeZone: effectiveTimeZone }).format(new Date());
  } catch {
    throw new Error(`Invalid timezone '${tzRaw}' (expected IANA name, e.g. America/New_York)`);
  }

  const { period, label: periodLabel, bucketDays } = parsePeriod(options.period, options.bucket);
  const direction = parseDirection(options.direction);
  const status = parseStatusFilter(options.status);
  const groupBy = parseGroupBy(options.group_by);
  const weekStart = parseWeekStart(options.week_start);

  if (options.account !== undefined && options.connection !== undefined) {
    throw new Error('--account and --connection are mutually exclusive');
  }

  // Resolve scope + accounts.
  let scope: SpendingScopeOutput = { type: 'portfolio' };
  let accountIds: string[] = [];
  if (options.account !== undefined) {
    const acct = await findAccount(storage, options.account);
    if (acct === null) throw new Error(`Account not found: ${options.account}`);
    scope = { type: 'account', id: acct.id.asStr(), name: acct.name };
    accountIds = [acct.id.asStr()];
  } else if (options.connection !== undefined) {
    const conn = await findConnection(storage, options.connection);
    if (conn === null) throw new Error(`Connection not found: ${options.connection}`);
    scope = { type: 'connection', id: conn.state.id.asStr(), name: conn.config.name };
    const accounts = await storage.listAccounts();
    accountIds = accounts.filter((a) => a.connection_id.equals(conn.state.id)).map((a) => a.id.asStr());
  } else {
    const accounts = await storage.listAccounts();
    const ignoredIds = await ignoredAccountIdsForPortfolioSpending(storage, config, accounts);
    accountIds = accounts.filter((a) => !ignoredIds.has(a.id.asStr())).map((a) => a.id.asStr());
  }

  const marketData = new MarketDataService(marketDataStore).withQuoteStaleness(
    config.refresh.price_staleness,
  );
  if (typeof options.lookback_days === 'number' && Number.isFinite(options.lookback_days)) {
    marketData.withLookbackDays(options.lookback_days);
  } else {
    marketData.withLookbackDays(7);
  }

  const startOpt = options.start !== undefined ? parseYmd(options.start) : undefined;
  const endOpt = options.end !== undefined ? parseYmd(options.end) : undefined;

  type Row = {
    account_id: string;
    local_date: Ymd;
    as_of_date: string;
    asset: AssetType;
    amount: string;
    raw_description: string;
    annotation: TransactionAnnotationType | null;
  };

  const rows: Row[] = [];
  let minDate: Ymd | undefined;
  const includeNoncurrency = options.include_noncurrency === true;

  for (const accountId of accountIds) {
    const account = await findAccount(storage, accountId);
    if (account === null) continue;

    const txns = await storage.getTransactions(account.id);
    const patches = await storage.getTransactionAnnotationPatches(account.id);

    const annByTx = new Map<string, TransactionAnnotationType>();
    for (const p of patches) {
      const key = p.transaction_id.asStr();
      const base = annByTx.get(key) ?? { transaction_id: p.transaction_id };
      annByTx.set(key, applyTransactionAnnotationPatch(base, p));
    }

    for (const tx of txns) {
      if (!includeStatus(tx.status, status)) continue;
      const localYmd = ymdFromTimestampInTimeZone(tx.timestamp, effectiveTimeZone);
      minDate = minDate === undefined ? localYmd : compareYmd(localYmd, minDate) < 0 ? localYmd : minDate;

      const normalizedAsset = Asset.normalized(tx.asset);
      if (!includeNoncurrency && normalizedAsset.type !== 'currency') {
        continue;
      }

      const ann = annByTx.get(tx.id.asStr());
      const annotation = ann && !isEmptyTransactionAnnotation(ann) ? ann : null;
      rows.push({
        account_id: account.id.asStr(),
        local_date: localYmd,
        as_of_date: ymdToString(localYmd),
        asset: normalizedAsset,
        amount: tx.amount,
        raw_description: tx.description,
        annotation,
      });
    }
  }

  const today = ymdFromTimestampInTimeZone(new Date(), effectiveTimeZone);
  const startDate = startOpt ?? minDate ?? today;
  const endDate = endOpt ?? today;
  if (compareYmd(endDate, startDate) < 0) {
    throw new Error(`end date ${ymdToString(endDate)} is before start date ${ymdToString(startDate)}`);
  }

  type BucketAgg = {
    total: Decimal;
    tx_count: number;
    breakdown: Map<string, { total: Decimal; tx_count: number }>;
  };

  const buckets = new Map<string, { start: Ymd; agg: BucketAgg }>();
  let skipped = 0;
  let missingPrice = 0;
  let missingFx = 0;
  let includedTx = 0;
  let grandTotal = new Decimal(0);

  for (const row of rows) {
    if (compareYmd(row.local_date, startDate) < 0 || compareYmd(row.local_date, endDate) > 0) continue;

    // Direction prefilter: valuation is linear with positive prices/FX, so sign is preserved.
    // Avoid counting missing market data for transactions that couldn't contribute.
    const amt = new Decimal(row.amount);
    if (amt.isZero()) continue;
    if (direction === 'outflow' && !amt.isNeg()) continue;
    if (direction === 'inflow' && !amt.isPos()) continue;

    const conv = await valueInReportingCurrencyDetailed(
      marketData,
      row.asset,
      row.amount,
      reportingCurrency,
      row.as_of_date,
      config.display.currency_decimals,
    );

    if (conv.value === null) {
      skipped += 1;
      if (conv.missing === 'price') missingPrice += 1;
      if (conv.missing === 'fx') missingFx += 1;
      continue;
    }

    const valueDec = new Decimal(conv.value);
    const directed = applyDirection(valueDec, direction);
    if (directed.isZero()) continue;

    includedTx += 1;
    grandTotal = grandTotal.plus(directed);

    const bstart = bucketStartFor(row.local_date, period, weekStart, startDate);
    const bkey = ymdToString(bstart);
    const existing = buckets.get(bkey);
    const agg: BucketAgg =
      existing?.agg ?? { total: new Decimal(0), tx_count: 0, breakdown: new Map() };
    agg.total = agg.total.plus(directed);
    agg.tx_count += 1;

    if (groupBy !== 'none') {
      let keys: string[] = [];
      switch (groupBy) {
        case 'category': {
          const cat = row.annotation?.category;
          keys = [cat !== undefined ? cat : 'uncategorized'];
          break;
        }
        case 'merchant': {
          const desc = row.annotation?.description;
          keys = [desc !== undefined ? desc : row.raw_description];
          break;
        }
        case 'account':
          keys = [row.account_id];
          break;
        case 'tag': {
          const tags = row.annotation?.tags;
          keys = tags && tags.length > 0 ? tags : ['untagged'];
          break;
        }
      }

      for (const key of keys) {
        const e = agg.breakdown.get(key) ?? { total: new Decimal(0), tx_count: 0 };
        e.total = e.total.plus(directed);
        e.tx_count += 1;
        agg.breakdown.set(key, e);
      }
    }

    buckets.set(bkey, { start: bstart, agg });
  }

  const keysSorted = Array.from(buckets.keys()).sort();
  const periods: SpendingPeriodOutput[] = [];

  const includeEmpty = options.include_empty === true;
  const bucketKeys: string[] = includeEmpty
    ? (() => {
        const keys: string[] = [];
        let s = bucketStartFor(startDate, period, weekStart, startDate);
        if (typeof period !== 'object' && period === 'range') {
          keys.push(ymdToString(s));
          return keys;
        }
        while (compareYmd(s, endDate) <= 0) {
          keys.push(ymdToString(s));
          s = nextBucketStart(s, period);
        }
        return keys;
      })()
    : keysSorted;

  for (const bkey of bucketKeys) {
    const b = buckets.get(bkey);
    const start = b?.start ?? parseYmd(bkey);
    const bend = bucketEndFor(start, period, endDate);
    const clampedStart = clampYmd(start, startDate, endDate);
    const clampedEnd = clampYmd(bend, startDate, endDate);

    let breakdown: SpendingBreakdownEntryOutput[] = [];
    if (groupBy !== 'none' && b !== undefined) {
      const entries: Array<[string, { total: Decimal; tx_count: number }]> = Array.from(
        b.agg.breakdown.entries(),
      );
      entries.sort((a, c) => {
        const totalB = c[1].total;
        const totalA = a[1].total;
        const cmp = totalB.comparedTo(totalA);
        if (cmp !== 0) return cmp;
        return a[0].localeCompare(c[0]);
      });
      if (typeof options.top === 'number' && Number.isFinite(options.top) && options.top > 0) {
        entries.splice(options.top);
      }
      breakdown = entries.map(([k, v]) => ({
        key: k,
        total: decStrRounded(v.total, config.display.currency_decimals),
        transaction_count: v.tx_count,
      }));
    }

    const out: SpendingPeriodOutput = {
      start_date: ymdToString(clampedStart),
      end_date: ymdToString(clampedEnd),
      total: decStrRounded(b?.agg.total ?? new Decimal(0), config.display.currency_decimals),
      transaction_count: b?.agg.tx_count ?? 0,
    };
    if (breakdown.length > 0) out.breakdown = breakdown;
    periods.push(out);
  }

  const out: SpendingOutput = {
    scope,
    currency: reportingCurrency,
    tz: tzLabel === 'local' ? 'local' : tzLabel,
    start_date: ymdToString(startDate),
    end_date: ymdToString(endDate),
    period: periodLabel,
    direction,
    status,
    group_by: groupBy,
    total: decStrRounded(grandTotal, config.display.currency_decimals),
    transaction_count: includedTx,
    periods,
    skipped_transaction_count: skipped,
    missing_price_transaction_count: missingPrice,
    missing_fx_transaction_count: missingFx,
  };

  if (periodLabel === 'weekly') out.week_start = weekStart;
  if (bucketDays !== undefined) out.bucket_days = bucketDays;

  return out;
}
