import { describe, it, expect } from 'vitest';
import { Asset } from '../models/asset.js';
import { AssetId } from './asset-id.js';
import {
  type PriceKind,
  type FxRateKind,
  type PricePoint,
  type FxRatePoint,
  type AssetRegistryEntry,
  AssetRegistryEntryFactory,
  pricePointToJSON,
  pricePointFromJSON,
  fxRatePointToJSON,
  fxRatePointFromJSON,
  assetRegistryEntryToJSON,
  assetRegistryEntryFromJSON,
} from './models.js';

// ---------------------------------------------------------------------------
// PriceKind / FxRateKind
// ---------------------------------------------------------------------------

describe('PriceKind', () => {
  it('accepts "close" as a valid PriceKind', () => {
    const kind: PriceKind = 'close';
    expect(kind).toBe('close');
  });

  it('accepts "adj_close" as a valid PriceKind', () => {
    const kind: PriceKind = 'adj_close';
    expect(kind).toBe('adj_close');
  });

  it('accepts "quote" as a valid PriceKind', () => {
    const kind: PriceKind = 'quote';
    expect(kind).toBe('quote');
  });
});

describe('FxRateKind', () => {
  it('accepts "close" as a valid FxRateKind', () => {
    const kind: FxRateKind = 'close';
    expect(kind).toBe('close');
  });
});

// ---------------------------------------------------------------------------
// PricePoint
// ---------------------------------------------------------------------------

describe('PricePoint', () => {
  it('can be created with expected fields', () => {
    const assetId = AssetId.fromAsset(Asset.equity('AAPL'));
    const now = new Date('2024-01-15T12:00:00Z');

    const point: PricePoint = {
      asset_id: assetId,
      as_of_date: '2024-01-15',
      timestamp: now,
      price: '185.50',
      quote_currency: 'USD',
      kind: 'close',
      source: 'yahoo',
    };

    expect(point.asset_id.asStr()).toBe('equity/AAPL');
    expect(point.as_of_date).toBe('2024-01-15');
    expect(point.timestamp).toEqual(now);
    expect(point.price).toBe('185.50');
    expect(point.quote_currency).toBe('USD');
    expect(point.kind).toBe('close');
    expect(point.source).toBe('yahoo');
  });
});

// ---------------------------------------------------------------------------
// FxRatePoint
// ---------------------------------------------------------------------------

describe('FxRatePoint', () => {
  it('can be created with expected fields', () => {
    const now = new Date('2024-01-15T12:00:00Z');

    const point: FxRatePoint = {
      base: 'USD',
      quote: 'EUR',
      as_of_date: '2024-01-15',
      timestamp: now,
      rate: '0.9150',
      kind: 'close',
      source: 'ecb',
    };

    expect(point.base).toBe('USD');
    expect(point.quote).toBe('EUR');
    expect(point.as_of_date).toBe('2024-01-15');
    expect(point.timestamp).toEqual(now);
    expect(point.rate).toBe('0.9150');
    expect(point.kind).toBe('close');
    expect(point.source).toBe('ecb');
  });
});

// ---------------------------------------------------------------------------
// AssetRegistryEntry
// ---------------------------------------------------------------------------

describe('AssetRegistryEntry', () => {
  it('new() auto-generates id from asset', () => {
    const asset = Asset.equity('AAPL', 'NASDAQ');
    const entry = AssetRegistryEntryFactory.new(asset);

    expect(entry.id.asStr()).toBe(AssetId.fromAsset(asset).asStr());
    expect(entry.asset).toEqual(asset);
    expect(entry.provider_ids).toEqual({});
    expect(entry.tz).toBeUndefined();
  });

  it('new() generates correct id for currency asset', () => {
    const asset = Asset.currency('USD');
    const entry = AssetRegistryEntryFactory.new(asset);

    expect(entry.id.asStr()).toBe('currency/USD');
    expect(entry.asset).toEqual(asset);
  });

  it('new() generates correct id for crypto asset', () => {
    const asset = Asset.crypto('BTC', 'bitcoin');
    const entry = AssetRegistryEntryFactory.new(asset);

    expect(entry.id.asStr()).toBe('crypto/BTC/bitcoin');
    expect(entry.asset).toEqual(asset);
  });

  it('can have custom provider_ids and tz', () => {
    const asset = Asset.equity('AAPL');
    const entry: AssetRegistryEntry = {
      id: AssetId.fromAsset(asset),
      asset,
      provider_ids: { yahoo: 'AAPL', polygon: 'AAPL' },
      tz: 'America/New_York',
    };

    expect(entry.provider_ids).toEqual({ yahoo: 'AAPL', polygon: 'AAPL' });
    expect(entry.tz).toBe('America/New_York');
  });
});

// ---------------------------------------------------------------------------
// JSON serialization round-trips
// ---------------------------------------------------------------------------

describe('JSON serialization', () => {
  describe('PricePoint', () => {
    it('round-trips through toJSON/fromJSON', () => {
      const assetId = AssetId.fromAsset(Asset.equity('AAPL'));
      const now = new Date('2024-01-15T12:00:00.000Z');

      const original: PricePoint = {
        asset_id: assetId,
        as_of_date: '2024-01-15',
        timestamp: now,
        price: '185.50',
        quote_currency: 'USD',
        kind: 'close',
        source: 'yahoo',
      };

      const json = pricePointToJSON(original);
      const restored = pricePointFromJSON(json);

      expect(restored.asset_id.asStr()).toBe(original.asset_id.asStr());
      expect(restored.as_of_date).toBe(original.as_of_date);
      expect(restored.timestamp.getTime()).toBe(original.timestamp.getTime());
      expect(restored.price).toBe(original.price);
      expect(restored.quote_currency).toBe(original.quote_currency);
      expect(restored.kind).toBe(original.kind);
      expect(restored.source).toBe(original.source);
    });

    it('serializes through JSON.stringify/parse', () => {
      const assetId = AssetId.fromAsset(Asset.equity('MSFT'));
      const now = new Date('2024-06-01T09:30:00.000Z');

      const original: PricePoint = {
        asset_id: assetId,
        as_of_date: '2024-06-01',
        timestamp: now,
        price: '420.00',
        quote_currency: 'USD',
        kind: 'adj_close',
        source: 'polygon',
      };

      const jsonStr = JSON.stringify(pricePointToJSON(original));
      const parsed = JSON.parse(jsonStr);
      const restored = pricePointFromJSON(parsed);

      expect(restored.asset_id.asStr()).toBe('equity/MSFT');
      expect(restored.timestamp.toISOString()).toBe('2024-06-01T09:30:00.000Z');
      expect(restored.kind).toBe('adj_close');
    });
  });

  describe('FxRatePoint', () => {
    it('round-trips through toJSON/fromJSON', () => {
      const now = new Date('2024-01-15T18:00:00.000Z');

      const original: FxRatePoint = {
        base: 'USD',
        quote: 'EUR',
        as_of_date: '2024-01-15',
        timestamp: now,
        rate: '0.9150',
        kind: 'close',
        source: 'ecb',
      };

      const json = fxRatePointToJSON(original);
      const restored = fxRatePointFromJSON(json);

      expect(restored.base).toBe(original.base);
      expect(restored.quote).toBe(original.quote);
      expect(restored.as_of_date).toBe(original.as_of_date);
      expect(restored.timestamp.getTime()).toBe(original.timestamp.getTime());
      expect(restored.rate).toBe(original.rate);
      expect(restored.kind).toBe(original.kind);
      expect(restored.source).toBe(original.source);
    });

    it('serializes through JSON.stringify/parse', () => {
      const now = new Date('2024-03-20T14:00:00.000Z');

      const original: FxRatePoint = {
        base: 'GBP',
        quote: 'JPY',
        as_of_date: '2024-03-20',
        timestamp: now,
        rate: '190.25',
        kind: 'close',
        source: 'boe',
      };

      const jsonStr = JSON.stringify(fxRatePointToJSON(original));
      const parsed = JSON.parse(jsonStr);
      const restored = fxRatePointFromJSON(parsed);

      expect(restored.base).toBe('GBP');
      expect(restored.quote).toBe('JPY');
      expect(restored.timestamp.toISOString()).toBe('2024-03-20T14:00:00.000Z');
    });
  });

  describe('AssetRegistryEntry', () => {
    it('round-trips through toJSON/fromJSON', () => {
      const asset = Asset.equity('AAPL', 'NASDAQ');
      const original: AssetRegistryEntry = {
        id: AssetId.fromAsset(asset),
        asset,
        provider_ids: { yahoo: 'AAPL', polygon: 'O:AAPL' },
        tz: 'America/New_York',
      };

      const json = assetRegistryEntryToJSON(original);
      const restored = assetRegistryEntryFromJSON(json);

      expect(restored.id.asStr()).toBe(original.id.asStr());
      expect(restored.asset).toEqual(original.asset);
      expect(restored.provider_ids).toEqual(original.provider_ids);
      expect(restored.tz).toBe(original.tz);
    });

    it('round-trips entry without optional tz', () => {
      const asset = Asset.currency('USD');
      const original: AssetRegistryEntry = {
        id: AssetId.fromAsset(asset),
        asset,
        provider_ids: {},
      };

      const json = assetRegistryEntryToJSON(original);
      const restored = assetRegistryEntryFromJSON(json);

      expect(restored.id.asStr()).toBe('currency/USD');
      expect(restored.asset).toEqual(asset);
      expect(restored.provider_ids).toEqual({});
      expect(restored.tz).toBeUndefined();
    });

    it('serializes through JSON.stringify/parse', () => {
      const asset = Asset.crypto('ETH', 'ethereum');
      const entry = AssetRegistryEntryFactory.new(asset);
      entry.provider_ids['coingecko'] = 'ethereum';

      const jsonStr = JSON.stringify(assetRegistryEntryToJSON(entry));
      const parsed = JSON.parse(jsonStr);
      const restored = assetRegistryEntryFromJSON(parsed);

      expect(restored.id.asStr()).toBe('crypto/ETH/ethereum');
      expect(restored.provider_ids).toEqual({ coingecko: 'ethereum' });
    });
  });
});
