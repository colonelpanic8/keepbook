import { describe, it, expect, beforeEach } from 'vitest';
import { Asset } from '../models/asset.js';
import { AssetId } from './asset-id.js';
import type { PricePoint, FxRatePoint } from './models.js';
import { AssetRegistryEntryFactory } from './models.js';
import { type MarketDataStore, NullMarketDataStore, MemoryMarketDataStore } from './store.js';

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function makePricePoint(overrides: Partial<PricePoint> = {}): PricePoint {
  return {
    asset_id: AssetId.fromAsset(Asset.equity('AAPL')),
    as_of_date: '2024-01-15',
    timestamp: new Date('2024-01-15T21:00:00Z'),
    price: '185.50',
    quote_currency: 'USD',
    kind: 'close',
    source: 'yahoo',
    ...overrides,
  };
}

function makeFxRatePoint(overrides: Partial<FxRatePoint> = {}): FxRatePoint {
  return {
    base: 'USD',
    quote: 'EUR',
    as_of_date: '2024-01-15',
    timestamp: new Date('2024-01-15T18:00:00Z'),
    rate: '0.9150',
    kind: 'close',
    source: 'ecb',
    ...overrides,
  };
}

// ---------------------------------------------------------------------------
// NullMarketDataStore
// ---------------------------------------------------------------------------

describe('NullMarketDataStore', () => {
  it('get_price returns null', async () => {
    const store: MarketDataStore = new NullMarketDataStore();
    const result = await store.get_price(
      AssetId.fromAsset(Asset.equity('AAPL')),
      '2024-01-15',
      'close',
    );
    expect(result).toBeNull();
  });

  it('get_all_prices returns empty array', async () => {
    const store = new NullMarketDataStore();
    const result = await store.get_all_prices(AssetId.fromAsset(Asset.equity('AAPL')));
    expect(result).toEqual([]);
  });

  it('put_prices is a no-op', async () => {
    const store = new NullMarketDataStore();
    await expect(store.put_prices([makePricePoint()])).resolves.toBeUndefined();
  });

  it('get_fx_rate returns null', async () => {
    const store = new NullMarketDataStore();
    const result = await store.get_fx_rate('USD', 'EUR', '2024-01-15', 'close');
    expect(result).toBeNull();
  });

  it('get_all_fx_rates returns empty array', async () => {
    const store = new NullMarketDataStore();
    const result = await store.get_all_fx_rates('USD', 'EUR');
    expect(result).toEqual([]);
  });

  it('put_fx_rates is a no-op', async () => {
    const store = new NullMarketDataStore();
    await expect(store.put_fx_rates([makeFxRatePoint()])).resolves.toBeUndefined();
  });

  it('get_asset_entry returns null', async () => {
    const store = new NullMarketDataStore();
    const result = await store.get_asset_entry(AssetId.fromAsset(Asset.equity('AAPL')));
    expect(result).toBeNull();
  });

  it('upsert_asset_entry is a no-op', async () => {
    const store = new NullMarketDataStore();
    const entry = AssetRegistryEntryFactory.new(Asset.equity('AAPL'));
    await expect(store.upsert_asset_entry(entry)).resolves.toBeUndefined();
  });
});

// ---------------------------------------------------------------------------
// MemoryMarketDataStore
// ---------------------------------------------------------------------------

describe('MemoryMarketDataStore', () => {
  let store: MemoryMarketDataStore;

  beforeEach(() => {
    store = new MemoryMarketDataStore();
  });

  // -- Prices ---------------------------------------------------------------

  describe('prices', () => {
    it('put then get price', async () => {
      const price = makePricePoint();
      await store.put_prices([price]);

      const result = await store.get_price(price.asset_id, '2024-01-15', 'close');

      expect(result).not.toBeNull();
      expect(result!.asset_id.asStr()).toBe(price.asset_id.asStr());
      expect(result!.price).toBe('185.50');
      expect(result!.as_of_date).toBe('2024-01-15');
      expect(result!.kind).toBe('close');
    });

    it('get_price returns null for non-existent', async () => {
      const result = await store.get_price(
        AssetId.fromAsset(Asset.equity('AAPL')),
        '2024-01-15',
        'close',
      );
      expect(result).toBeNull();
    });

    it('get_all_prices returns all dates for an asset', async () => {
      const assetId = AssetId.fromAsset(Asset.equity('AAPL'));
      const p1 = makePricePoint({ asset_id: assetId, as_of_date: '2024-01-15' });
      const p2 = makePricePoint({ asset_id: assetId, as_of_date: '2024-01-16' });
      const p3 = makePricePoint({ asset_id: assetId, as_of_date: '2024-01-17' });

      // Also insert a price for a different asset to ensure filtering works
      const otherAssetId = AssetId.fromAsset(Asset.equity('MSFT'));
      const pOther = makePricePoint({
        asset_id: otherAssetId,
        as_of_date: '2024-01-15',
      });

      await store.put_prices([p1, p2, p3, pOther]);

      const results = await store.get_all_prices(assetId);
      expect(results).toHaveLength(3);

      const dates = results.map((r) => r.as_of_date).sort();
      expect(dates).toEqual(['2024-01-15', '2024-01-16', '2024-01-17']);
    });

    it('put overwrites existing price (same key)', async () => {
      const assetId = AssetId.fromAsset(Asset.equity('AAPL'));
      const original = makePricePoint({
        asset_id: assetId,
        price: '185.50',
      });
      await store.put_prices([original]);

      const updated = makePricePoint({
        asset_id: assetId,
        price: '190.00',
      });
      await store.put_prices([updated]);

      const result = await store.get_price(assetId, '2024-01-15', 'close');
      expect(result).not.toBeNull();
      expect(result!.price).toBe('190.00');
    });

    it('different kinds are stored separately', async () => {
      const assetId = AssetId.fromAsset(Asset.equity('AAPL'));
      const closePrice = makePricePoint({
        asset_id: assetId,
        kind: 'close',
        price: '185.50',
      });
      const adjClosePrice = makePricePoint({
        asset_id: assetId,
        kind: 'adj_close',
        price: '184.00',
      });

      await store.put_prices([closePrice, adjClosePrice]);

      const closeResult = await store.get_price(assetId, '2024-01-15', 'close');
      const adjResult = await store.get_price(assetId, '2024-01-15', 'adj_close');

      expect(closeResult!.price).toBe('185.50');
      expect(adjResult!.price).toBe('184.00');
    });
  });

  // -- FX rates -------------------------------------------------------------

  describe('fx rates', () => {
    it('put then get FX rate', async () => {
      const rate = makeFxRatePoint();
      await store.put_fx_rates([rate]);

      const result = await store.get_fx_rate('USD', 'EUR', '2024-01-15', 'close');

      expect(result).not.toBeNull();
      expect(result!.base).toBe('USD');
      expect(result!.quote).toBe('EUR');
      expect(result!.rate).toBe('0.9150');
      expect(result!.as_of_date).toBe('2024-01-15');
    });

    it('get_fx_rate returns null for non-existent', async () => {
      const result = await store.get_fx_rate('USD', 'EUR', '2024-01-15', 'close');
      expect(result).toBeNull();
    });

    it('FX rate lookup is case-insensitive', async () => {
      const rate = makeFxRatePoint({ base: 'usd', quote: 'eur' });
      await store.put_fx_rates([rate]);

      // Look up with uppercase
      const result = await store.get_fx_rate('USD', 'EUR', '2024-01-15', 'close');
      expect(result).not.toBeNull();
      expect(result!.rate).toBe('0.9150');

      // Look up with mixed case
      const result2 = await store.get_fx_rate('Usd', 'Eur', '2024-01-15', 'close');
      expect(result2).not.toBeNull();
      expect(result2!.rate).toBe('0.9150');
    });

    it('put_fx_rates normalizes base/quote to uppercase', async () => {
      const rate = makeFxRatePoint({ base: 'usd', quote: 'eur' });
      await store.put_fx_rates([rate]);

      const result = await store.get_fx_rate('USD', 'EUR', '2024-01-15', 'close');
      expect(result).not.toBeNull();
      expect(result!.base).toBe('USD');
      expect(result!.quote).toBe('EUR');
    });

    it('get_all_fx_rates returns all dates for a pair', async () => {
      const r1 = makeFxRatePoint({ as_of_date: '2024-01-15' });
      const r2 = makeFxRatePoint({ as_of_date: '2024-01-16' });
      const r3 = makeFxRatePoint({ as_of_date: '2024-01-17' });

      // Different pair to ensure filtering
      const rOther = makeFxRatePoint({
        base: 'GBP',
        quote: 'JPY',
        as_of_date: '2024-01-15',
      });

      await store.put_fx_rates([r1, r2, r3, rOther]);

      const results = await store.get_all_fx_rates('USD', 'EUR');
      expect(results).toHaveLength(3);

      const dates = results.map((r) => r.as_of_date).sort();
      expect(dates).toEqual(['2024-01-15', '2024-01-16', '2024-01-17']);
    });

    it('get_all_fx_rates is case-insensitive', async () => {
      const r1 = makeFxRatePoint({ base: 'usd', quote: 'eur', as_of_date: '2024-01-15' });
      await store.put_fx_rates([r1]);

      const results = await store.get_all_fx_rates('Usd', 'Eur');
      expect(results).toHaveLength(1);
    });

    it('put overwrites existing FX rate (same key)', async () => {
      const original = makeFxRatePoint({ rate: '0.9150' });
      await store.put_fx_rates([original]);

      const updated = makeFxRatePoint({ rate: '0.9200' });
      await store.put_fx_rates([updated]);

      const result = await store.get_fx_rate('USD', 'EUR', '2024-01-15', 'close');
      expect(result).not.toBeNull();
      expect(result!.rate).toBe('0.9200');
    });
  });

  // -- Asset entries --------------------------------------------------------

  describe('asset entries', () => {
    it('upsert then get asset entry', async () => {
      const asset = Asset.equity('AAPL', 'NASDAQ');
      const entry = AssetRegistryEntryFactory.new(asset);
      entry.provider_ids['yahoo'] = 'AAPL';
      entry.tz = 'America/New_York';

      await store.upsert_asset_entry(entry);

      const result = await store.get_asset_entry(entry.id);
      expect(result).not.toBeNull();
      expect(result!.id.asStr()).toBe(entry.id.asStr());
      expect(result!.asset).toEqual(asset);
      expect(result!.provider_ids).toEqual({ yahoo: 'AAPL' });
      expect(result!.tz).toBe('America/New_York');
    });

    it('get_asset_entry returns null for non-existent', async () => {
      const result = await store.get_asset_entry(AssetId.fromAsset(Asset.equity('AAPL')));
      expect(result).toBeNull();
    });

    it('upsert overwrites existing entry', async () => {
      const asset = Asset.equity('AAPL');
      const entry1 = AssetRegistryEntryFactory.new(asset);
      entry1.provider_ids['yahoo'] = 'AAPL';
      await store.upsert_asset_entry(entry1);

      const entry2 = AssetRegistryEntryFactory.new(asset);
      entry2.provider_ids['polygon'] = 'O:AAPL';
      entry2.tz = 'America/New_York';
      await store.upsert_asset_entry(entry2);

      const result = await store.get_asset_entry(entry2.id);
      expect(result).not.toBeNull();
      expect(result!.provider_ids).toEqual({ polygon: 'O:AAPL' });
      expect(result!.tz).toBe('America/New_York');
    });
  });
});
