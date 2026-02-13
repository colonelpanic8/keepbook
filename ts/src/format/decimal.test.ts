import { describe, it, expect } from 'vitest';
import { Decimal } from '../decimal.js';
import { formatCurrencyDisplay } from './decimal.js';

describe('formatCurrencyDisplay', () => {
  it('defaults to decStr-like output without grouping/symbol', () => {
    expect(formatCurrencyDisplay(new Decimal('1234.500'), {})).toBe('1234.5');
  });

  it('supports grouping + symbol + fixed decimals', () => {
    expect(
      formatCurrencyDisplay(new Decimal('1234567.5'), {
        currency_decimals: 2,
        currency_grouping: true,
        currency_symbol: '$',
        currency_fixed_decimals: true,
      }),
    ).toBe('$1,234,567.50');
  });

  it('puts negative sign before symbol', () => {
    expect(
      formatCurrencyDisplay(new Decimal('-1234.5'), {
        currency_decimals: 2,
        currency_grouping: true,
        currency_symbol: '$',
        currency_fixed_decimals: true,
      }),
    ).toBe('-$1,234.50');
  });
});

