/**
 * Formatting utilities for CLI output.
 *
 * These functions ensure TypeScript CLI JSON output exactly matches the Rust
 * CLI's serde-serialized format, especially for timestamps and decimals.
 */

import { Decimal } from '../decimal.js';
import { type AssetType, Asset } from '../models/asset.js';
import { type Granularity } from '../portfolio/change-points.js';

// ---------------------------------------------------------------------------
// Timestamp formatting
// ---------------------------------------------------------------------------

/**
 * Pad a number to exactly `digits` characters with leading zeros.
 */
function pad(n: number, digits: number): string {
  return n.toString().padStart(digits, '0');
}

/**
 * Format the subsecond part of a Date.
 *
 * - If milliseconds == 0, returns empty string (no fractional part).
 * - Otherwise returns `.` followed by 9-digit nanosecond representation
 *   (JS only has ms precision, so the last 6 digits are always zero).
 */
function formatSubseconds(date: Date): string {
  const ms = date.getUTCMilliseconds();
  if (ms === 0) return '';
  // 3-digit ms padded, then 6 trailing zeros for nanoseconds
  return '.' + pad(ms, 3) + '000000';
}

/**
 * Format the date/time core (without suffix) in UTC: `YYYY-MM-DDTHH:MM:SS[.nnnnnnnnn]`
 */
function formatCore(date: Date): string {
  const y = pad(date.getUTCFullYear(), 4);
  const mo = pad(date.getUTCMonth() + 1, 2);
  const d = pad(date.getUTCDate(), 2);
  const h = pad(date.getUTCHours(), 2);
  const mi = pad(date.getUTCMinutes(), 2);
  const s = pad(date.getUTCSeconds(), 2);
  return `${y}-${mo}-${d}T${h}:${mi}:${s}${formatSubseconds(date)}`;
}

function formatFromEpochNanos(epochNanos: string, suffix: '+00:00' | 'Z'): string {
  const nanos = BigInt(epochNanos);
  const wholeSeconds = nanos / 1000000000n;
  const fractionNanos = nanos % 1000000000n;
  const date = new Date(Number(wholeSeconds * 1000n));
  const core = formatCore(date);
  if (fractionNanos === 0n) {
    return `${core}${suffix}`;
  }
  const y = pad(date.getUTCFullYear(), 4);
  const mo = pad(date.getUTCMonth() + 1, 2);
  const d = pad(date.getUTCDate(), 2);
  const h = pad(date.getUTCHours(), 2);
  const mi = pad(date.getUTCMinutes(), 2);
  const s = pad(date.getUTCSeconds(), 2);
  const frac = fractionNanos.toString().padStart(9, '0');
  return `${y}-${mo}-${d}T${h}:${mi}:${s}.${frac}${suffix}`;
}

/**
 * Format a Date as RFC 3339 matching Rust's `DateTime::to_rfc3339()`.
 *
 * - Uses `+00:00` suffix (not `Z`).
 * - Omits subsecond digits when ms == 0.
 * - Includes 9-digit nanoseconds when ms != 0.
 *
 * Examples:
 * - `formatRfc3339(new Date('2024-01-15T10:00:00Z'))` -> `"2024-01-15T10:00:00+00:00"`
 * - `formatRfc3339(new Date('2024-01-15T10:00:00.123Z'))` -> `"2024-01-15T10:00:00.123000000+00:00"`
 */
export function formatRfc3339(date: Date): string {
  return formatCore(date) + '+00:00';
}

/**
 * Format an epoch-nanoseconds timestamp as RFC 3339 with `+00:00`.
 */
export function formatRfc3339FromEpochNanos(epochNanos: string): string {
  return formatFromEpochNanos(epochNanos, '+00:00');
}

/**
 * Format a Date matching Rust's chrono serde serialization.
 *
 * - Uses `Z` suffix.
 * - Omits subsecond digits when ms == 0.
 * - Includes 9-digit nanoseconds when ms != 0.
 *
 * Examples:
 * - `formatChronoSerde(new Date('2024-01-15T10:00:00Z'))` -> `"2024-01-15T10:00:00Z"`
 * - `formatChronoSerde(new Date('2024-01-15T10:00:00.123Z'))` -> `"2024-01-15T10:00:00.123000000Z"`
 */
export function formatChronoSerde(date: Date): string {
  return formatCore(date) + 'Z';
}

/**
 * Format an epoch-nanoseconds timestamp with `Z` suffix.
 */
export function formatChronoSerdeFromEpochNanos(epochNanos: string): string {
  return formatFromEpochNanos(epochNanos, 'Z');
}

/**
 * Format a Date as `YYYY-MM-DD` in UTC.
 */
export function formatDateYMD(date: Date): string {
  const y = pad(date.getUTCFullYear(), 4);
  const mo = pad(date.getUTCMonth() + 1, 2);
  const d = pad(date.getUTCDate(), 2);
  return `${y}-${mo}-${d}`;
}

// ---------------------------------------------------------------------------
// Asset parsing
// ---------------------------------------------------------------------------

/**
 * Parse an asset string from CLI input.
 *
 * Handles:
 * - Bare string (e.g. `"USD"`) -> currency
 * - `"equity:AAPL"` -> equity
 * - `"crypto:BTC"` -> crypto
 * - `"currency:EUR"` -> currency
 *
 * Mirrors Rust `parse_asset` at `src/app.rs:2184-2204`.
 */
export function parseAsset(s: string): AssetType {
  const trimmed = s.trim();
  if (trimmed === '') {
    throw new Error('Asset string cannot be empty');
  }

  const colonIdx = trimmed.indexOf(':');
  if (colonIdx !== -1) {
    const prefix = trimmed.slice(0, colonIdx);
    const value = trimmed.slice(colonIdx + 1).trim();
    if (value === '') {
      throw new Error(`Asset value missing for prefix '${prefix}'`);
    }
    switch (prefix.toLowerCase()) {
      case 'equity':
        return Asset.equity(value);
      case 'crypto':
        return Asset.crypto(value);
      case 'currency':
        return Asset.currency(value);
      default:
        break;
    }
  }

  // Default: assume it's a currency code
  return Asset.currency(trimmed);
}

// ---------------------------------------------------------------------------
// Decimal formatting
// ---------------------------------------------------------------------------

/**
 * Format a Decimal to string, stripping trailing zeros.
 *
 * Matches Rust `Decimal::normalize().to_string()`.
 */
export function decStr(d: Decimal): string {
  // Decimal.js toFixed() keeps trailing zeros; we need to strip them.
  // Using the string representation and trimming.
  const s = d.toFixed();
  // If there's no decimal point, return as-is
  if (!s.includes('.')) return s;
  // Strip trailing zeros, then strip trailing dot if present
  let result = s.replace(/0+$/, '');
  if (result.endsWith('.')) {
    result = result.slice(0, -1);
  }
  if (result === '-0') return '0';
  return result;
}

// ---------------------------------------------------------------------------
// Granularity parsing
// ---------------------------------------------------------------------------

const VALID_GRANULARITIES = new Set<string>([
  'full',
  'hourly',
  'daily',
  'weekly',
  'monthly',
  'yearly',
]);

/**
 * Parse a granularity string from CLI input.
 *
 * The Rust CLI accepts "none" to mean `Granularity::Full`.
 */
export function parseGranularity(s: string): Granularity {
  const lower = s.toLowerCase().trim();
  if (lower === 'none') return 'full';
  if (VALID_GRANULARITIES.has(lower)) return lower as Granularity;
  throw new Error(
    `Invalid granularity '${s}'. Valid values: none, full, hourly, daily, weekly, monthly, yearly`,
  );
}
