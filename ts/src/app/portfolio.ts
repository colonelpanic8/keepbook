/**
 * Portfolio snapshot command.
 *
 * Provides `serializeSnapshot` (convert library types to plain JSON-serializable
 * objects) and `portfolioSnapshot` (the top-level command handler).
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
import { formatChronoSerde } from './format.js';
import type { ResolvedConfig } from '../config.js';

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
  const portfolioService = new PortfolioService(
    storage,
    marketDataService,
    effectiveClock,
  );

  const snapshot = await portfolioService.calculate({
    as_of_date: asOfDate,
    currency,
    grouping,
    include_detail: includeDetail,
  });

  return serializeSnapshot(snapshot);
}
