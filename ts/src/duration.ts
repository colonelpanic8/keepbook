/**
 * Duration parsing utilities for human-readable durations like "14d", "24h".
 *
 * All durations are represented as milliseconds (number).
 */

const MS_PER_SECOND = 1_000;
const MS_PER_MINUTE = 60 * MS_PER_SECOND;
const MS_PER_HOUR = 60 * MS_PER_MINUTE;
const MS_PER_DAY = 24 * MS_PER_HOUR;

const UNIT_MULTIPLIERS: Record<string, number> = {
  d: MS_PER_DAY,
  h: MS_PER_HOUR,
  m: MS_PER_MINUTE,
  s: MS_PER_SECOND,
};

/**
 * Parse a duration string like "14d", "24h", "30m", "60s" into milliseconds.
 *
 * Supported units:
 * - `d` — days (24 hours)
 * - `h` — hours
 * - `m` — minutes
 * - `s` — seconds
 *
 * The input is case-insensitive and leading/trailing whitespace is trimmed.
 *
 * @throws {Error} on invalid input (unknown unit, non-integer, negative, empty, etc.)
 */
export function parseDuration(s: string): number {
  const trimmed = s.trim().toLowerCase();

  if (trimmed.length === 0) {
    throw new Error('Duration string must not be empty');
  }

  const unit = trimmed[trimmed.length - 1];
  const multiplier = UNIT_MULTIPLIERS[unit];

  if (multiplier === undefined) {
    throw new Error(`Duration must end with d, h, m, or s, got '${unit}'`);
  }

  const numStr = trimmed.slice(0, -1);

  if (numStr.length === 0) {
    throw new Error('Invalid number in duration: empty numeric part');
  }

  // Reject anything that isn't a non-negative integer (no floats, no signs)
  if (!/^\d+$/.test(numStr)) {
    throw new Error(`Invalid number in duration: '${numStr}'`);
  }

  const num = Number(numStr);

  if (!Number.isSafeInteger(num) || num < 0) {
    throw new Error(`Invalid number in duration: '${numStr}'`);
  }

  const ms = num * multiplier;

  if (!Number.isSafeInteger(ms)) {
    throw new Error('Duration is too large');
  }

  return ms;
}

/**
 * Format a duration in milliseconds to a human-readable string.
 *
 * Uses the largest appropriate unit (days, hours, minutes, or seconds).
 * For durations that don't divide evenly into a larger unit, falls back to seconds.
 *
 * @example
 * formatDuration(14 * 86400_000) // "14d"
 * formatDuration(90_000)         // "90s"
 * formatDuration(0)              // "0s"
 */
export function formatDuration(ms: number): string {
  // Convert to whole seconds first (our unit of granularity)
  const secs = Math.floor(ms / MS_PER_SECOND);

  const SECS_PER_DAY = 86400;
  const SECS_PER_HOUR = 3600;
  const SECS_PER_MINUTE = 60;

  if (secs >= SECS_PER_DAY && secs % SECS_PER_DAY === 0) {
    return `${secs / SECS_PER_DAY}d`;
  }
  if (secs >= SECS_PER_HOUR && secs % SECS_PER_HOUR === 0) {
    return `${secs / SECS_PER_HOUR}h`;
  }
  if (secs >= SECS_PER_MINUTE && secs % SECS_PER_MINUTE === 0) {
    return `${secs / SECS_PER_MINUTE}m`;
  }
  return `${secs}s`;
}
