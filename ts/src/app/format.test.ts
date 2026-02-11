import { describe, it, expect } from 'vitest';
import { Decimal } from 'decimal.js';
import {
  formatRfc3339,
  formatChronoSerde,
  formatDateYMD,
  parseAsset,
  decStr,
  parseGranularity,
} from './format.js';

// ---------------------------------------------------------------------------
// formatRfc3339
// ---------------------------------------------------------------------------

describe('formatRfc3339', () => {
  it('formats a date with no milliseconds using +00:00 suffix', () => {
    const d = new Date('2024-01-15T10:00:00Z');
    expect(formatRfc3339(d)).toBe('2024-01-15T10:00:00+00:00');
  });

  it('formats a date with milliseconds as 9-digit nanoseconds', () => {
    const d = new Date('2024-01-15T10:00:00.123Z');
    expect(formatRfc3339(d)).toBe('2024-01-15T10:00:00.123000000+00:00');
  });

  it('formats midnight correctly', () => {
    const d = new Date('2024-01-01T00:00:00Z');
    expect(formatRfc3339(d)).toBe('2024-01-01T00:00:00+00:00');
  });

  it('formats end of day correctly', () => {
    const d = new Date('2024-12-31T23:59:59Z');
    expect(formatRfc3339(d)).toBe('2024-12-31T23:59:59+00:00');
  });

  it('formats single-digit ms with leading zeros', () => {
    const d = new Date('2024-01-15T10:00:00.001Z');
    expect(formatRfc3339(d)).toBe('2024-01-15T10:00:00.001000000+00:00');
  });

  it('formats 100ms correctly', () => {
    const d = new Date('2024-01-15T10:00:00.100Z');
    expect(formatRfc3339(d)).toBe('2024-01-15T10:00:00.100000000+00:00');
  });
});

// ---------------------------------------------------------------------------
// formatChronoSerde
// ---------------------------------------------------------------------------

describe('formatChronoSerde', () => {
  it('formats a date with no milliseconds using Z suffix', () => {
    const d = new Date('2024-01-15T10:00:00Z');
    expect(formatChronoSerde(d)).toBe('2024-01-15T10:00:00Z');
  });

  it('formats a date with milliseconds as 9-digit nanoseconds with Z', () => {
    const d = new Date('2024-01-15T10:00:00.123Z');
    expect(formatChronoSerde(d)).toBe('2024-01-15T10:00:00.123000000Z');
  });

  it('formats midnight correctly', () => {
    const d = new Date('2024-01-01T00:00:00Z');
    expect(formatChronoSerde(d)).toBe('2024-01-01T00:00:00Z');
  });

  it('formats end of day correctly', () => {
    const d = new Date('2024-12-31T23:59:59Z');
    expect(formatChronoSerde(d)).toBe('2024-12-31T23:59:59Z');
  });

  it('formats single-digit ms with leading zeros', () => {
    const d = new Date('2024-01-15T10:00:00.007Z');
    expect(formatChronoSerde(d)).toBe('2024-01-15T10:00:00.007000000Z');
  });
});

// ---------------------------------------------------------------------------
// formatDateYMD
// ---------------------------------------------------------------------------

describe('formatDateYMD', () => {
  it('formats a date as YYYY-MM-DD in UTC', () => {
    const d = new Date('2024-01-15T10:00:00Z');
    expect(formatDateYMD(d)).toBe('2024-01-15');
  });

  it('pads single-digit month and day', () => {
    const d = new Date('2024-03-05T00:00:00Z');
    expect(formatDateYMD(d)).toBe('2024-03-05');
  });

  it('handles year boundaries', () => {
    const d = new Date('2024-12-31T23:59:59Z');
    expect(formatDateYMD(d)).toBe('2024-12-31');
  });
});

// ---------------------------------------------------------------------------
// parseAsset
// ---------------------------------------------------------------------------

describe('parseAsset', () => {
  it('parses bare string as currency', () => {
    expect(parseAsset('USD')).toEqual({ type: 'currency', iso_code: 'USD' });
  });

  it('parses equity:TICKER', () => {
    expect(parseAsset('equity:AAPL')).toEqual({ type: 'equity', ticker: 'AAPL' });
  });

  it('parses crypto:SYMBOL', () => {
    expect(parseAsset('crypto:BTC')).toEqual({ type: 'crypto', symbol: 'BTC' });
  });

  it('parses currency:CODE', () => {
    expect(parseAsset('currency:EUR')).toEqual({ type: 'currency', iso_code: 'EUR' });
  });

  it('throws on empty string', () => {
    expect(() => parseAsset('')).toThrow('Asset string cannot be empty');
  });

  it('throws on whitespace-only string', () => {
    expect(() => parseAsset('   ')).toThrow('Asset string cannot be empty');
  });

  it('throws on prefix with empty value', () => {
    expect(() => parseAsset('equity:')).toThrow("Asset value missing for prefix 'equity'");
  });

  it('handles case-insensitive prefix', () => {
    expect(parseAsset('Equity:MSFT')).toEqual({ type: 'equity', ticker: 'MSFT' });
    expect(parseAsset('CRYPTO:ETH')).toEqual({ type: 'crypto', symbol: 'ETH' });
  });

  it('treats unknown prefix as currency', () => {
    // e.g. "unknown:something" has unknown prefix, so the whole thing is treated as currency
    expect(parseAsset('unknown:something')).toEqual({
      type: 'currency',
      iso_code: 'unknown:something',
    });
  });

  it('trims whitespace from input', () => {
    expect(parseAsset('  USD  ')).toEqual({ type: 'currency', iso_code: 'USD' });
  });
});

// ---------------------------------------------------------------------------
// decStr
// ---------------------------------------------------------------------------

describe('decStr', () => {
  it('strips trailing zeros', () => {
    expect(decStr(new Decimal('100.500'))).toBe('100.5');
  });

  it('formats zero', () => {
    expect(decStr(new Decimal('0'))).toBe('0');
  });

  it('formats integer value', () => {
    expect(decStr(new Decimal('42'))).toBe('42');
  });

  it('strips all trailing zeros from decimal', () => {
    expect(decStr(new Decimal('1.000'))).toBe('1');
  });

  it('preserves significant decimal digits', () => {
    expect(decStr(new Decimal('3.14159'))).toBe('3.14159');
  });

  it('formats negative values', () => {
    expect(decStr(new Decimal('-100.500'))).toBe('-100.5');
  });

  it('formats very small values', () => {
    expect(decStr(new Decimal('0.001'))).toBe('0.001');
  });

  it('formats large values', () => {
    expect(decStr(new Decimal('1000000'))).toBe('1000000');
  });
});

// ---------------------------------------------------------------------------
// parseGranularity
// ---------------------------------------------------------------------------

describe('parseGranularity', () => {
  it('maps "none" to "full"', () => {
    expect(parseGranularity('none')).toBe('full');
  });

  it('maps "None" to "full" (case-insensitive)', () => {
    expect(parseGranularity('None')).toBe('full');
  });

  it('accepts "daily"', () => {
    expect(parseGranularity('daily')).toBe('daily');
  });

  it('accepts "full"', () => {
    expect(parseGranularity('full')).toBe('full');
  });

  it('accepts "hourly"', () => {
    expect(parseGranularity('hourly')).toBe('hourly');
  });

  it('accepts "weekly"', () => {
    expect(parseGranularity('weekly')).toBe('weekly');
  });

  it('accepts "monthly"', () => {
    expect(parseGranularity('monthly')).toBe('monthly');
  });

  it('accepts "yearly"', () => {
    expect(parseGranularity('yearly')).toBe('yearly');
  });

  it('throws on invalid value', () => {
    expect(() => parseGranularity('invalid')).toThrow('Invalid granularity');
  });

  it('is case-insensitive', () => {
    expect(parseGranularity('Daily')).toBe('daily');
    expect(parseGranularity('MONTHLY')).toBe('monthly');
  });
});
