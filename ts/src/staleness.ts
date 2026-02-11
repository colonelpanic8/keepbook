/**
 * Staleness checking module.
 *
 * Port of the Rust `staleness.rs` module. Determines whether balances and
 * prices are stale based on configurable thresholds. Durations are
 * represented as milliseconds (number).
 */

import type { AccountConfig } from './models/account.js';
import type { ConnectionType } from './models/connection.js';
import type { RefreshConfig } from './config.js';
import type { PricePoint } from './market-data/models.js';

// ---------------------------------------------------------------------------
// StalenessCheck
// ---------------------------------------------------------------------------

/** Result of a staleness check. */
export interface StalenessCheck {
  /** Whether the data is considered stale. */
  readonly is_stale: boolean;
  /** Age in milliseconds, or null if the data has never been fetched. */
  readonly age: number | null;
  /** The threshold in milliseconds used for the comparison. */
  readonly threshold: number;
}

/** Factory functions for creating StalenessCheck values. */
export const StalenessCheck = {
  /** Data exists but is older than the threshold. */
  stale(age: number, threshold: number): StalenessCheck {
    return { is_stale: true, age, threshold };
  },

  /** Data exists and is within the threshold. */
  fresh(age: number, threshold: number): StalenessCheck {
    return { is_stale: false, age, threshold };
  },

  /** Data has never been fetched. Always considered stale. */
  missing(threshold: number): StalenessCheck {
    return { is_stale: true, age: null, threshold };
  },
} as const;

// ---------------------------------------------------------------------------
// Resolution
// ---------------------------------------------------------------------------

/**
 * Resolve the effective balance staleness threshold.
 *
 * Priority: account config -> connection config -> global config.
 */
export function resolveBalanceStaleness(
  accountConfig: AccountConfig | null,
  connection: ConnectionType,
  globalConfig: RefreshConfig,
): number {
  if (accountConfig?.balance_staleness !== undefined) {
    return accountConfig.balance_staleness;
  }
  if (connection.config.balance_staleness !== undefined) {
    return connection.config.balance_staleness;
  }
  return globalConfig.balance_staleness;
}

// ---------------------------------------------------------------------------
// Balance staleness
// ---------------------------------------------------------------------------

/**
 * Check whether a connection's balance data is stale relative to a given
 * instant.
 *
 * If the connection has never synced, the result is {@link StalenessCheck.missing}.
 * If the last sync is in the future relative to `now`, the age is clamped to 0.
 */
export function checkBalanceStalenessAt(
  connection: ConnectionType,
  threshold: number,
  now: Date,
): StalenessCheck {
  const lastSync = connection.state.last_sync;
  if (lastSync === undefined) {
    return StalenessCheck.missing(threshold);
  }

  const age = Math.max(0, now.getTime() - lastSync.at.getTime());

  if (age >= threshold) {
    return StalenessCheck.stale(age, threshold);
  }
  return StalenessCheck.fresh(age, threshold);
}

/**
 * Convenience wrapper: check balance staleness using the current wall-clock
 * time.
 */
export function checkBalanceStaleness(
  connection: ConnectionType,
  threshold: number,
): StalenessCheck {
  return checkBalanceStalenessAt(connection, threshold, new Date());
}

// ---------------------------------------------------------------------------
// Price staleness
// ---------------------------------------------------------------------------

/**
 * Check whether a price point is stale relative to a given instant.
 *
 * If no price is provided, the result is {@link StalenessCheck.missing}.
 * If the price timestamp is in the future relative to `now`, the age is
 * clamped to 0.
 */
export function checkPriceStalenessAt(
  price: PricePoint | null,
  threshold: number,
  now: Date,
): StalenessCheck {
  if (price === null) {
    return StalenessCheck.missing(threshold);
  }

  const age = Math.max(0, now.getTime() - price.timestamp.getTime());

  if (age >= threshold) {
    return StalenessCheck.stale(age, threshold);
  }
  return StalenessCheck.fresh(age, threshold);
}

/**
 * Convenience wrapper: check price staleness using the current wall-clock
 * time.
 */
export function checkPriceStaleness(price: PricePoint | null, threshold: number): StalenessCheck {
  return checkPriceStalenessAt(price, threshold, new Date());
}
