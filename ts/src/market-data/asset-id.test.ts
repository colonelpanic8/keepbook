import { describe, it, expect } from 'vitest';
import { Asset } from '../models/asset.js';
import {
  AssetId,
  sanitizeSegment,
  normalizeUpperSegment,
  normalizeLowerSegment,
} from './asset-id.js';

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

describe('sanitizeSegment', () => {
  it('replaces forward slashes with dashes', () => {
    expect(sanitizeSegment('BRK/B')).toBe('BRK-B');
  });

  it('replaces backslashes with dashes', () => {
    expect(sanitizeSegment('A\\B')).toBe('A-B');
  });

  it('replaces null characters with dashes', () => {
    expect(sanitizeSegment('A\0B')).toBe('A-B');
  });

  it('trims whitespace', () => {
    expect(sanitizeSegment('  AAPL  ')).toBe('AAPL');
  });

  it('returns underscore for empty string', () => {
    expect(sanitizeSegment('')).toBe('_');
  });

  it('returns underscore for single dot', () => {
    expect(sanitizeSegment('.')).toBe('_');
  });

  it('returns underscore for double dot', () => {
    expect(sanitizeSegment('..')).toBe('_');
  });

  it('returns underscore for whitespace-only string', () => {
    expect(sanitizeSegment('   ')).toBe('_');
  });

  it('preserves normal characters', () => {
    expect(sanitizeSegment('AAPL')).toBe('AAPL');
  });
});

describe('normalizeUpperSegment', () => {
  it('uppercases and sanitizes', () => {
    expect(normalizeUpperSegment('aapl')).toBe('AAPL');
  });

  it('uppercases with slash sanitization', () => {
    expect(normalizeUpperSegment('brk/b')).toBe('BRK-B');
  });
});

describe('normalizeLowerSegment', () => {
  it('lowercases and sanitizes', () => {
    expect(normalizeLowerSegment('ARBITRUM')).toBe('arbitrum');
  });

  it('lowercases with slash sanitization', () => {
    expect(normalizeLowerSegment('NET/WORK')).toBe('net-work');
  });
});

// ---------------------------------------------------------------------------
// AssetId
// ---------------------------------------------------------------------------

describe('AssetId', () => {
  // 1. Deterministic
  describe('fromAsset is deterministic', () => {
    it('produces the same id for the same currency asset', () => {
      const a = AssetId.fromAsset(Asset.currency('USD'));
      const b = AssetId.fromAsset(Asset.currency('USD'));
      expect(a.asStr()).toBe(b.asStr());
    });

    it('produces the same id for the same equity asset', () => {
      const a = AssetId.fromAsset(Asset.equity('AAPL'));
      const b = AssetId.fromAsset(Asset.equity('AAPL'));
      expect(a.asStr()).toBe(b.asStr());
    });

    it('produces the same id for the same crypto asset', () => {
      const a = AssetId.fromAsset(Asset.crypto('BTC'));
      const b = AssetId.fromAsset(Asset.crypto('BTC'));
      expect(a.asStr()).toBe(b.asStr());
    });
  });

  // 2. Different assets produce different ids
  describe('different assets produce different ids', () => {
    it('currency vs equity are different', () => {
      const currency = AssetId.fromAsset(Asset.currency('USD'));
      const equity = AssetId.fromAsset(Asset.equity('AAPL'));
      expect(currency.asStr()).not.toBe(equity.asStr());
    });

    it('currency vs crypto are different', () => {
      const currency = AssetId.fromAsset(Asset.currency('USD'));
      const crypto = AssetId.fromAsset(Asset.crypto('BTC'));
      expect(currency.asStr()).not.toBe(crypto.asStr());
    });

    it('equity vs crypto are different', () => {
      const equity = AssetId.fromAsset(Asset.equity('AAPL'));
      const crypto = AssetId.fromAsset(Asset.crypto('BTC'));
      expect(equity.asStr()).not.toBe(crypto.asStr());
    });

    it('same type different values are different', () => {
      const usd = AssetId.fromAsset(Asset.currency('USD'));
      const eur = AssetId.fromAsset(Asset.currency('EUR'));
      expect(usd.asStr()).not.toBe(eur.asStr());
    });
  });

  // 3. Case normalization
  describe('case normalization', () => {
    it('lowercased currency is normalized to uppercase', () => {
      const id = AssetId.fromAsset(Asset.currency('usd'));
      expect(id.asStr()).toBe('currency/USD');
    });

    it('lowercased equity ticker is normalized to uppercase', () => {
      const id = AssetId.fromAsset(Asset.equity('aapl'));
      expect(id.asStr()).toBe('equity/AAPL');
    });

    it('mixed-case crypto symbol is normalized to uppercase', () => {
      const id = AssetId.fromAsset(Asset.crypto('Btc'));
      expect(id.asStr()).toBe('crypto/BTC');
    });

    it('same asset different case produces same id', () => {
      const a = AssetId.fromAsset(Asset.currency('usd'));
      const b = AssetId.fromAsset(Asset.currency('USD'));
      expect(a.asStr()).toBe(b.asStr());
    });
  });

  // 4. Human-readable format for each asset type
  describe('human-readable format', () => {
    it('currency/USD', () => {
      const id = AssetId.fromAsset(Asset.currency('USD'));
      expect(id.asStr()).toBe('currency/USD');
    });

    it('equity/AAPL', () => {
      const id = AssetId.fromAsset(Asset.equity('AAPL'));
      expect(id.asStr()).toBe('equity/AAPL');
    });

    it('equity/AAPL/NYSE with exchange', () => {
      const id = AssetId.fromAsset(Asset.equity('AAPL', 'NYSE'));
      expect(id.asStr()).toBe('equity/AAPL/NYSE');
    });

    it('crypto/BTC', () => {
      const id = AssetId.fromAsset(Asset.crypto('BTC'));
      expect(id.asStr()).toBe('crypto/BTC');
    });

    it('crypto/ETH/arbitrum with network in lowercase', () => {
      const id = AssetId.fromAsset(Asset.crypto('ETH', 'Arbitrum'));
      expect(id.asStr()).toBe('crypto/ETH/arbitrum');
    });
  });

  // 5. Empty/whitespace exchange and network are ignored
  describe('empty/whitespace optional fields are ignored', () => {
    it('empty exchange is ignored for equity', () => {
      const id = AssetId.fromAsset(Asset.equity('AAPL', ''));
      expect(id.asStr()).toBe('equity/AAPL');
    });

    it('whitespace-only exchange is ignored for equity', () => {
      const id = AssetId.fromAsset(Asset.equity('AAPL', '   '));
      expect(id.asStr()).toBe('equity/AAPL');
    });

    it('empty network is ignored for crypto', () => {
      const id = AssetId.fromAsset(Asset.crypto('BTC', ''));
      expect(id.asStr()).toBe('crypto/BTC');
    });

    it('whitespace-only network is ignored for crypto', () => {
      const id = AssetId.fromAsset(Asset.crypto('BTC', '   '));
      expect(id.asStr()).toBe('crypto/BTC');
    });
  });

  // 6. Path segments are sanitized
  describe('path segment sanitization', () => {
    it('forward slash in ticker is replaced with dash', () => {
      const id = AssetId.fromAsset(Asset.equity('BRK/B'));
      expect(id.asStr()).toBe('equity/BRK-B');
    });

    it('backslash in ticker is replaced with dash', () => {
      const id = AssetId.fromAsset(Asset.equity('BRK\\B'));
      expect(id.asStr()).toBe('equity/BRK-B');
    });

    it('forward slash in crypto symbol is replaced with dash', () => {
      const id = AssetId.fromAsset(Asset.crypto('A/B'));
      expect(id.asStr()).toBe('crypto/A-B');
    });

    it('forward slash in exchange is replaced with dash', () => {
      const id = AssetId.fromAsset(Asset.equity('AAPL', 'NY/SE'));
      expect(id.asStr()).toBe('equity/AAPL/NY-SE');
    });
  });

  // 7. Sanitization of special values (., .., empty)
  describe('special value sanitization', () => {
    it('dot iso_code becomes underscore', () => {
      const asset = { type: 'currency' as const, iso_code: '.' };
      const id = AssetId.fromAsset(asset);
      expect(id.asStr()).toBe('currency/_');
    });

    it('double-dot ticker becomes underscore', () => {
      const asset = { type: 'equity' as const, ticker: '..' };
      const id = AssetId.fromAsset(asset);
      expect(id.asStr()).toBe('equity/_');
    });
  });

  // 8. fromString creates from raw string
  describe('fromString', () => {
    it('creates an AssetId from a raw string', () => {
      const id = AssetId.fromString('currency/USD');
      expect(id.asStr()).toBe('currency/USD');
    });

    it('preserves the exact string given', () => {
      const id = AssetId.fromString('arbitrary/string/value');
      expect(id.asStr()).toBe('arbitrary/string/value');
    });
  });

  // 9. JSON serialization
  describe('toJSON', () => {
    it('returns the plain id string', () => {
      const id = AssetId.fromAsset(Asset.currency('USD'));
      expect(id.toJSON()).toBe('currency/USD');
    });

    it('serializes as a plain string in JSON.stringify', () => {
      const id = AssetId.fromAsset(Asset.equity('AAPL'));
      const json = JSON.stringify({ id });
      expect(json).toBe('{"id":"equity/AAPL"}');
    });

    it('serializes correctly with fromString', () => {
      const id = AssetId.fromString('crypto/BTC');
      expect(JSON.stringify(id)).toBe('"crypto/BTC"');
    });
  });

  // 10. toString
  describe('toString', () => {
    it('returns the id string', () => {
      const id = AssetId.fromAsset(Asset.currency('USD'));
      expect(id.toString()).toBe('currency/USD');
    });

    it('works in template literals', () => {
      const id = AssetId.fromAsset(Asset.equity('AAPL'));
      expect(`asset: ${id}`).toBe('asset: equity/AAPL');
    });
  });

  // 11. equals
  describe('equals', () => {
    it('returns true for same inner value', () => {
      const a = AssetId.fromAsset(Asset.currency('USD'));
      const b = AssetId.fromAsset(Asset.currency('USD'));
      expect(a.equals(b)).toBe(true);
    });

    it('returns true for fromString with same value', () => {
      const a = AssetId.fromAsset(Asset.currency('USD'));
      const b = AssetId.fromString('currency/USD');
      expect(a.equals(b)).toBe(true);
    });

    it('returns false for different inner values', () => {
      const a = AssetId.fromAsset(Asset.currency('USD'));
      const b = AssetId.fromAsset(Asset.currency('EUR'));
      expect(a.equals(b)).toBe(false);
    });

    it('returns false for different asset types with same suffix', () => {
      const a = AssetId.fromString('equity/BTC');
      const b = AssetId.fromString('crypto/BTC');
      expect(a.equals(b)).toBe(false);
    });
  });
});
