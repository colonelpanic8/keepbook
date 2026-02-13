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

function groupIntDigits(intPart: string): string {
  // Insert commas every 3 digits, preserving leading zeros.
  if (intPart.length <= 3) return intPart;
  let out = '';
  for (let i = 0; i < intPart.length; i++) {
    out += intPart[i];
    const remaining = intPart.length - i - 1;
    if (remaining > 0 && remaining % 3 === 0) out += ',';
  }
  return out;
}

function groupNumberString(s: string): string {
  const dot = s.indexOf('.');
  if (dot === -1) return groupIntDigits(s);
  const intPart = s.slice(0, dot);
  const frac = s.slice(dot + 1);
  const grouped = groupIntDigits(intPart);
  return frac.length > 0 ? `${grouped}.${frac}` : grouped;
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

export type CurrencyDisplayOptions = {
  currency_decimals?: number;
  currency_grouping?: boolean;
  currency_symbol?: string;
  currency_fixed_decimals?: boolean;
};

/**
 * Format a base-currency Decimal for human display.
 *
 * This is intended for UI surfaces. It does not change any canonical JSON
 * numeric string fields.
 */
export function formatCurrencyDisplay(d: Decimal, opts: CurrencyDisplayOptions): string {
  const dp = opts.currency_decimals;
  const fixed = opts.currency_fixed_decimals === true && dp !== undefined;
  const grouping = opts.currency_grouping === true;
  const symbol = typeof opts.currency_symbol === 'string' ? opts.currency_symbol : undefined;

  const rounded = dp !== undefined ? d.toDecimalPlaces(dp, Decimal.ROUND_HALF_UP) : d;
  const negative = rounded.isNeg() && !rounded.isZero();
  const abs = rounded.abs();

  let s = fixed && dp !== undefined ? abs.toFixed(dp) : decStr(abs);
  if (s === '-0') s = '0';
  if (grouping) s = groupNumberString(s);

  return `${negative ? '-' : ''}${symbol ?? ''}${s}`;
}
