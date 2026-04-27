/**
 * Portfolio model types.
 *
 * Port of the Rust `portfolio::models` module.
 */

import type { Decimal } from '../decimal.js';
import type { AssetType } from '../models/asset.js';
import type { Id } from '../models/id.js';

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
  /**
   * Optional capital gains tax rate as a decimal fraction (for example,
   * 0.238 for 23.8%).
   */
  capital_gains_tax_rate?: Decimal;
  /**
   * Optional scenario that changes equity valuations before totals,
   * unrealized gains, and prospective tax are calculated.
   */
  equity_valuation_adjustment?: EquityValuationAdjustment;
  /**
   * Restrict valuation to these accounts. Empty/undefined means all
   * non-excluded accounts in the portfolio.
   */
  account_ids?: Id[];
}

export type EquityValuationAdjustment =
  | { type: 'percent_change'; percent: Decimal }
  | { type: 'target_pre_tax_total_value'; amount: Decimal };

// ---------------------------------------------------------------------------
// Snapshot (result)
// ---------------------------------------------------------------------------

export interface PortfolioSnapshot {
  as_of_date: string;
  currency: string;
  /** Decimal string (normalized, no trailing zeros). */
  total_value: string;
  total_cost_basis?: string;
  total_unrealized_gain?: string;
  prospective_capital_gains_tax?: string;
  valuation_scenario?: PortfolioValuationScenario;
  by_asset?: AssetSummary[];
  by_account?: AccountSummary[];
}

export interface PortfolioValuationScenario {
  equity_multiplier: string;
  equity_change_percent: string;
  pre_tax_total_value: string;
  equity_value_before: string;
  equity_value_after: string;
  target_pre_tax_total_value?: string;
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
  /** Decimal string total known cost basis. */
  cost_basis?: string;
  /** Decimal string unrealized gain for holdings with known cost basis. */
  unrealized_gain?: string;
  /** Decimal string estimated tax on positive unrealized gains when a tax rate is supplied. */
  prospective_capital_gains_tax?: string;
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
  /** Decimal string known cost basis for this holding. */
  cost_basis?: string;
  /** Decimal string unrealized gain for this holding. */
  unrealized_gain?: string;
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
