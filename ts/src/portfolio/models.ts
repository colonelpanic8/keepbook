/**
 * Portfolio model types.
 *
 * Port of the Rust `portfolio::models` module.
 */

import type { AssetType } from '../models/asset.js';

// ---------------------------------------------------------------------------
// Grouping
// ---------------------------------------------------------------------------

export type Grouping = 'asset' | 'account' | 'both';

// ---------------------------------------------------------------------------
// Query
// ---------------------------------------------------------------------------

export interface PortfolioQuery {
  /** Date in "YYYY-MM-DD" format. */
  as_of_date: string;
  /** Target currency ISO code (e.g. "USD", "EUR"). */
  currency: string;
  /**
   * If set, values denominated in `currency` are rounded to this many decimal
   * places before being rendered as strings.
   */
  currency_decimals?: number;
  /** Which breakdowns to include. */
  grouping: Grouping;
  /** Whether to include per-account holdings detail in asset summaries. */
  include_detail: boolean;
}

// ---------------------------------------------------------------------------
// Snapshot (result)
// ---------------------------------------------------------------------------

export interface PortfolioSnapshot {
  as_of_date: string;
  currency: string;
  /** Decimal string (normalized, no trailing zeros). */
  total_value: string;
  by_asset?: AssetSummary[];
  by_account?: AccountSummary[];
}

// ---------------------------------------------------------------------------
// Asset summary
// ---------------------------------------------------------------------------

export interface AssetSummary {
  asset: AssetType;
  /** Decimal string. */
  total_amount: string;
  /** "YYYY-MM-DD" date of the most recent balance contributing to this amount. */
  amount_date: string;
  /** Decimal string price per unit, if available. */
  price?: string;
  /** "YYYY-MM-DD" date of the price observation. */
  price_date?: string;
  /** Exact timestamp of the price observation. */
  price_timestamp?: Date;
  /** Decimal string FX rate, if FX conversion was needed. */
  fx_rate?: string;
  /** "YYYY-MM-DD" date of the FX rate observation. */
  fx_date?: string;
  /** Decimal string value in base/target currency. Undefined if price unavailable. */
  value_in_base?: string;
  /** Per-account holdings detail (only when include_detail is true). */
  holdings?: AccountHolding[];
}

// ---------------------------------------------------------------------------
// Account holding (detail within an asset summary)
// ---------------------------------------------------------------------------

export interface AccountHolding {
  account_id: string;
  account_name: string;
  /** Decimal string. */
  amount: string;
  /** "YYYY-MM-DD" date of the balance snapshot. */
  balance_date: string;
}

// ---------------------------------------------------------------------------
// Account summary
// ---------------------------------------------------------------------------

export interface AccountSummary {
  account_id: string;
  account_name: string;
  connection_name: string;
  /** Decimal string value in base/target currency. Undefined if any asset lacks price data. */
  value_in_base?: string;
}
