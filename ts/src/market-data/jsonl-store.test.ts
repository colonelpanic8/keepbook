import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { mkdtemp, rm, readFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { Asset } from '../models/asset.js';
import { AssetId } from './asset-id.js';
import type { PricePoint, FxRatePoint } from './models.js';
import { AssetRegistryEntryFactory } from './models.js';
import { JsonlMarketDataStore, sanitizeCode } from './jsonl-store.js';

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
// sanitizeCode
// ---------------------------------------------------------------------------

describe('sanitizeCode', () => {
  it('uppercases alphabetic characters', () => {
    expect(sanitizeCode('usd')).toBe('USD');
  });

  it('keeps alphanumeric characters', () => {
    expect(sanitizeCode('ABC123')).toBe('ABC123');
  });

  it('replaces non-alphanumeric characters with underscore', () => {
    expect(sanitizeCode('US.D')).toBe('US_D');
    expect(sanitizeCode('a-b')).toBe('A_B');
  });

  it('trims whitespace', () => {
    expect(sanitizeCode('  usd  ')).toBe('USD');
  });

  it('handles empty string after trim', () => {
    expect(sanitizeCode('')).toBe('');
  });
});

// ---------------------------------------------------------------------------
// JsonlMarketDataStore
// ---------------------------------------------------------------------------

describe('JsonlMarketDataStore', () => {
  let tmpDir: string;
  let store: JsonlMarketDataStore;

  beforeEach(async () => {
    tmpDir = await mkdtemp(join(tmpdir(), 'jsonl-store-test-'));
    store = new JsonlMarketDataStore(tmpDir);
  });

  afterEach(async () => {
    await rm(tmpDir, { recursive: true, force: true });
  });

  // -- Prices ---------------------------------------------------------------

  describe('prices', () => {
    it('put_prices then get_price roundtrip', async () => {
      const price = makePricePoint();
      await store.put_prices([price]);

      const result = await store.get_price(price.asset_id, '2024-01-15', 'close');

      expect(result).not.toBeNull();
      expect(result!.asset_id.asStr()).toBe(price.asset_id.asStr());
      expect(result!.price).toBe('185.50');
      expect(result!.as_of_date).toBe('2024-01-15');
      expect(result!.kind).toBe('close');
      expect(result!.quote_currency).toBe('USD');
      expect(result!.source).toBe('yahoo');
      expect(result!.timestamp.getTime()).toBe(price.timestamp.getTime());
    });

    it('get_price returns null for missing asset', async () => {
      const result = await store.get_price(
        AssetId.fromAsset(Asset.equity('AAPL')),
        '2024-01-15',
        'close',
      );
      expect(result).toBeNull();
    });

    it('get_price selects latest by timestamp when multiple entries for same date/kind', async () => {
      const assetId = AssetId.fromAsset(Asset.equity('AAPL'));
      const earlier = makePricePoint({
        asset_id: assetId,
        timestamp: new Date('2024-01-15T10:00:00Z'),
        price: '180.00',
      });
      const later = makePricePoint({
        asset_id: assetId,
        timestamp: new Date('2024-01-15T21:00:00Z'),
        price: '185.50',
      });

      await store.put_prices([earlier, later]);

      const result = await store.get_price(assetId, '2024-01-15', 'close');
      expect(result).not.toBeNull();
      expect(result!.price).toBe('185.50');
      expect(result!.timestamp.getTime()).toBe(later.timestamp.getTime());
    });

    it('get_price filters by kind', async () => {
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
      expect(closeResult!.price).toBe('185.50');

      const adjResult = await store.get_price(assetId, '2024-01-15', 'adj_close');
      expect(adjResult!.price).toBe('184.00');
    });

    it('get_all_prices across multiple years', async () => {
      const assetId = AssetId.fromAsset(Asset.equity('AAPL'));
      const p2023 = makePricePoint({
        asset_id: assetId,
        as_of_date: '2023-12-29',
        timestamp: new Date('2023-12-29T21:00:00Z'),
        price: '192.00',
      });
      const p2024a = makePricePoint({
        asset_id: assetId,
        as_of_date: '2024-01-15',
        timestamp: new Date('2024-01-15T21:00:00Z'),
        price: '185.50',
      });
      const p2024b = makePricePoint({
        asset_id: assetId,
        as_of_date: '2024-06-15',
        timestamp: new Date('2024-06-15T21:00:00Z'),
        price: '210.00',
      });

      await store.put_prices([p2023, p2024a, p2024b]);

      const all = await store.get_all_prices(assetId);
      expect(all).toHaveLength(3);

      // Should be sorted by timestamp
      expect(all[0].as_of_date).toBe('2023-12-29');
      expect(all[1].as_of_date).toBe('2024-01-15');
      expect(all[2].as_of_date).toBe('2024-06-15');
    });

    it('get_all_prices returns empty for missing asset', async () => {
      const result = await store.get_all_prices(AssetId.fromAsset(Asset.equity('MISSING')));
      expect(result).toEqual([]);
    });

    it('put_prices groups by asset and year correctly', async () => {
      const aaplId = AssetId.fromAsset(Asset.equity('AAPL'));
      const msftId = AssetId.fromAsset(Asset.equity('MSFT'));

      const prices = [
        makePricePoint({ asset_id: aaplId, as_of_date: '2024-01-15' }),
        makePricePoint({ asset_id: msftId, as_of_date: '2024-01-15', price: '400.00' }),
        makePricePoint({
          asset_id: aaplId,
          as_of_date: '2023-12-01',
          timestamp: new Date('2023-12-01T21:00:00Z'),
          price: '195.00',
        }),
      ];

      await store.put_prices(prices);

      const aaplAll = await store.get_all_prices(aaplId);
      expect(aaplAll).toHaveLength(2);

      const msftAll = await store.get_all_prices(msftId);
      expect(msftAll).toHaveLength(1);
      expect(msftAll[0].price).toBe('400.00');
    });

    it('put_prices creates correct directory structure', async () => {
      const assetId = AssetId.fromAsset(Asset.equity('AAPL'));
      await store.put_prices([makePricePoint({ asset_id: assetId })]);

      const filePath = join(tmpDir, 'prices', assetId.asStr(), '2024.jsonl');
      const content = await readFile(filePath, 'utf-8');
      expect(content.trim()).not.toBe('');
      const parsed = JSON.parse(content.trim());
      expect(parsed.asset_id).toBe(assetId.asStr());
    });
  });

  // -- FX rates -------------------------------------------------------------

  describe('fx rates', () => {
    it('put_fx_rates then get_fx_rate roundtrip', async () => {
      const rate = makeFxRatePoint();
      await store.put_fx_rates([rate]);

      const result = await store.get_fx_rate('USD', 'EUR', '2024-01-15', 'close');

      expect(result).not.toBeNull();
      expect(result!.base).toBe('USD');
      expect(result!.quote).toBe('EUR');
      expect(result!.rate).toBe('0.9150');
      expect(result!.as_of_date).toBe('2024-01-15');
      expect(result!.kind).toBe('close');
      expect(result!.source).toBe('ecb');
      expect(result!.timestamp.getTime()).toBe(rate.timestamp.getTime());
    });

    it('get_fx_rate returns null for missing pair', async () => {
      const result = await store.get_fx_rate('USD', 'EUR', '2024-01-15', 'close');
      expect(result).toBeNull();
    });

    it('get_fx_rate selects latest by timestamp', async () => {
      const earlier = makeFxRatePoint({
        timestamp: new Date('2024-01-15T10:00:00Z'),
        rate: '0.9100',
      });
      const later = makeFxRatePoint({
        timestamp: new Date('2024-01-15T18:00:00Z'),
        rate: '0.9150',
      });

      await store.put_fx_rates([earlier, later]);

      const result = await store.get_fx_rate('USD', 'EUR', '2024-01-15', 'close');
      expect(result).not.toBeNull();
      expect(result!.rate).toBe('0.9150');
    });

    it('get_all_fx_rates across multiple years', async () => {
      const r2023 = makeFxRatePoint({
        as_of_date: '2023-12-29',
        timestamp: new Date('2023-12-29T18:00:00Z'),
        rate: '0.9050',
      });
      const r2024 = makeFxRatePoint({
        as_of_date: '2024-01-15',
        timestamp: new Date('2024-01-15T18:00:00Z'),
        rate: '0.9150',
      });

      await store.put_fx_rates([r2023, r2024]);

      const all = await store.get_all_fx_rates('USD', 'EUR');
      expect(all).toHaveLength(2);

      // Should be sorted by timestamp
      expect(all[0].as_of_date).toBe('2023-12-29');
      expect(all[1].as_of_date).toBe('2024-01-15');
    });

    it('get_all_fx_rates returns empty for missing pair', async () => {
      const result = await store.get_all_fx_rates('USD', 'EUR');
      expect(result).toEqual([]);
    });

    it('fx directory uses sanitized code', async () => {
      const rate = makeFxRatePoint({ base: 'usd', quote: 'eur' });
      await store.put_fx_rates([rate]);

      const filePath = join(tmpDir, 'fx', 'USD-EUR', '2024.jsonl');
      const content = await readFile(filePath, 'utf-8');
      expect(content.trim()).not.toBe('');
    });
  });

  // -- Asset entries --------------------------------------------------------

  describe('asset entries', () => {
    it('upsert_asset_entry then get_asset_entry roundtrip', async () => {
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

    it('get_asset_entry returns latest entry (last in file)', async () => {
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

    it('upsert writes to assets/index.jsonl', async () => {
      const entry = AssetRegistryEntryFactory.new(Asset.equity('AAPL'));
      await store.upsert_asset_entry(entry);

      const filePath = join(tmpDir, 'assets', 'index.jsonl');
      const content = await readFile(filePath, 'utf-8');
      const lines = content.trim().split('\n');
      expect(lines).toHaveLength(1);
      const parsed = JSON.parse(lines[0]);
      expect(parsed.id).toBe(entry.id.asStr());
    });

    it('get_asset_entry distinguishes different asset ids', async () => {
      const aaplEntry = AssetRegistryEntryFactory.new(Asset.equity('AAPL'));
      aaplEntry.provider_ids['yahoo'] = 'AAPL';

      const msftEntry = AssetRegistryEntryFactory.new(Asset.equity('MSFT'));
      msftEntry.provider_ids['yahoo'] = 'MSFT';

      await store.upsert_asset_entry(aaplEntry);
      await store.upsert_asset_entry(msftEntry);

      const aaplResult = await store.get_asset_entry(aaplEntry.id);
      expect(aaplResult).not.toBeNull();
      expect(aaplResult!.provider_ids).toEqual({ yahoo: 'AAPL' });

      const msftResult = await store.get_asset_entry(msftEntry.id);
      expect(msftResult).not.toBeNull();
      expect(msftResult!.provider_ids).toEqual({ yahoo: 'MSFT' });
    });
  });
});
