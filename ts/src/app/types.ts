/**
 * Output types for the CLI.
 *
 * All field names use snake_case to match Rust serde serialization format.
 * These interfaces describe the JSON shapes produced by CLI commands.
 *
 * Nullability rules:
 * - Fields that Rust serializes as `null` (no `skip_serializing_if`) use `| null`.
 * - Fields that Rust skips when absent (`skip_serializing_if`) use `?:` (optional/undefined).
 */

import { type AssetType } from '../models/asset.js';

// ---------------------------------------------------------------------------
// Connection / Account
// ---------------------------------------------------------------------------

export interface ConnectionOutput {
  id: string;
  name: string;
  synchronizer: string;
  status: string;
  account_count: number;
  last_sync: string | null;
}

export interface AccountOutput {
  id: string;
  name: string;
  connection_id: string;
  tags: string[];
  active: boolean;
}

// ---------------------------------------------------------------------------
// Balance
// ---------------------------------------------------------------------------

export interface BalanceOutput {
  account_id: string;
  asset: AssetType;
  amount: string;
  value_in_reporting_currency: string | null;
  reporting_currency: string;
  timestamp: string;
}

// ---------------------------------------------------------------------------
// Transaction
// ---------------------------------------------------------------------------

export interface TransactionOutput {
  id: string;
  account_id: string;
  account_name: string;
  timestamp: string;
  description: string;
  amount: string;
  asset: AssetType;
  status: string;
  annotation?: TransactionAnnotationOutput;
}

export interface TransactionAnnotationOutput {
  description?: string;
  note?: string;
  category?: string;
  tags?: string[];
}

// ---------------------------------------------------------------------------
// Spending
// ---------------------------------------------------------------------------

export type SpendingScopeOutput =
  | { type: 'portfolio' }
  | { type: 'connection'; id: string; name: string }
  | { type: 'account'; id: string; name: string };

export interface SpendingBreakdownEntryOutput {
  key: string;
  total: string;
  transaction_count: number;
}

export interface SpendingPeriodOutput {
  start_date: string;
  end_date: string;
  total: string;
  transaction_count: number;
  breakdown?: SpendingBreakdownEntryOutput[];
}

export interface SpendingOutput {
  scope: SpendingScopeOutput;
  currency: string;
  tz: string;
  start_date: string;
  end_date: string;
  period: string;
  week_start?: string;
  bucket_days?: number;
  direction: string;
  status: string;
  group_by: string;
  total: string;
  transaction_count: number;
  periods: SpendingPeriodOutput[];
  skipped_transaction_count: number;
  missing_price_transaction_count: number;
  missing_fx_transaction_count: number;
}

// ---------------------------------------------------------------------------
// Price Source
// ---------------------------------------------------------------------------

export interface PriceSourceOutput {
  name: string;
  type: string;
  enabled: boolean;
  priority: number;
  has_credentials: boolean;
}

// ---------------------------------------------------------------------------
// All (combined listing)
// ---------------------------------------------------------------------------

export interface AllOutput {
  connections: ConnectionOutput[];
  accounts: AccountOutput[];
  price_sources: PriceSourceOutput[];
  balances: BalanceOutput[];
}

// ---------------------------------------------------------------------------
// Portfolio History
// ---------------------------------------------------------------------------

export interface HistoryPoint {
  timestamp: string;
  date: string;
  total_value: string;
  percentage_change_from_previous: string | null;
  change_triggers?: string[];
}

export interface HistorySummary {
  initial_value: string;
  final_value: string;
  absolute_change: string;
  percentage_change: string;
}

export interface HistoryOutput {
  currency: string;
  start_date: string | null;
  end_date: string | null;
  granularity: string;
  points: HistoryPoint[];
  summary?: HistorySummary;
}

// ---------------------------------------------------------------------------
// Change Points
// ---------------------------------------------------------------------------

export interface SerializedChangePoint {
  timestamp: string;
  triggers: SerializedChangeTrigger[];
}

export type SerializedChangeTrigger =
  | { type: 'balance'; account_id: string; asset: AssetType }
  | { type: 'price'; asset_id: string }
  | { type: 'fx_rate'; base: string; quote: string };

export interface ChangePointsOutput {
  start_date: string | null;
  end_date: string | null;
  granularity: string;
  include_prices: boolean;
  points: SerializedChangePoint[];
}

// ---------------------------------------------------------------------------
// Price History
// ---------------------------------------------------------------------------

export type PriceHistoryScopeOutput =
  | { type: 'portfolio' }
  | { type: 'connection'; id: string; name: string }
  | { type: 'account'; id: string; name: string };

export interface AssetInfoOutput {
  asset: AssetType;
  asset_id: string;
}

export interface PriceHistoryStats {
  attempted: number;
  existing: number;
  fetched: number;
  lookback: number;
  missing: number;
}

export interface PriceHistoryFailure {
  kind: string;
  date: string;
  error: string;
  asset_id?: string;
  asset?: AssetType;
  base?: string;
  quote?: string;
}

export interface PriceHistoryOutput {
  scope: PriceHistoryScopeOutput;
  currency: string;
  interval: string;
  start_date: string;
  end_date: string;
  earliest_balance_date?: string;
  days: number;
  points: number;
  assets: AssetInfoOutput[];
  prices: PriceHistoryStats;
  fx?: PriceHistoryStats;
  failure_count: number;
  failures?: PriceHistoryFailure[];
}
