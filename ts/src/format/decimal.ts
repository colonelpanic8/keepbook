/**
 * Decimal formatting utilities shared by the library and CLI layers.
 *
 * Keep these semantics in sync with Rust `Decimal::normalize().to_string()`.
 */

import { Decimal } from '../decimal.js';

/**
 * Format a Decimal to string, stripping trailing zeros.
 *
 * Matches Rust `Decimal::normalize().to_string()`.
 */
export function decStr(d: Decimal): string {
  // Decimal.js toFixed() keeps trailing zeros; we need to strip them.
  const s = d.toFixed();
  if (!s.includes('.')) return s;
  let result = s.replace(/0+$/, '');
  if (result.endsWith('.')) {
    result = result.slice(0, -1);
  }
  if (result === '-0') return '0';
  return result;
}

/**
 * Round a Decimal to at most `dp` decimal places (half-up) and format it with
 * {@link decStr}.
 */
export function decStrRounded(d: Decimal, dp: number | undefined): string {
  if (dp === undefined) return decStr(d);
  if (!Number.isInteger(dp) || dp < 0) return decStr(d);
  return decStr(d.toDecimalPlaces(dp, Decimal.ROUND_HALF_UP));
}

