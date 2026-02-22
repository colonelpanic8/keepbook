import { describe, it, expect, beforeEach } from 'vitest';
import { Asset, AssetType } from '../models/asset.js';
import { AssetId } from './asset-id.js';
import type { PricePoint, FxRatePoint } from './models.js';
import { MemoryMarketDataStore } from './store.js';
import { FixedClock } from '../clock.js';
import {
  type MarketDataSource,
  type EquityPriceSource,
  type CryptoPriceSource,
  type FxRateSource,
  EquityPriceRouter,
  CryptoPriceRouter,
  FxRateRouter,
} from './sources.js';
import { MarketDataService } from './service.js';

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

const AAPL = Asset.equity('AAPL');
const AAPL_ID = AssetId.fromAsset(AAPL);
const BTC = Asset.crypto('BTC');
const BTC_ID = AssetId.fromAsset(BTC);

function makePrice(overrides: Partial<PricePoint> = {}): PricePoint {
  return {
    asset_id: AAPL_ID,
    as_of_date: '2024-01-15',
    timestamp: new Date('2024-01-15T21:00:00Z'),
    price: '185.50',
    quote_currency: 'USD',
    kind: 'close',
    source: 'test',
    ...overrides,
  };
}

function makeFxRate(overrides: Partial<FxRatePoint> = {}): FxRatePoint {
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

/** A stub equity source that returns a canned response or null. */
class StubEquitySource implements EquityPriceSource {
  readonly #name: string;
  closeResult: PricePoint | null;
  quoteResult: PricePoint | null;

  constructor(
    name: string,
    closeResult: PricePoint | null = null,
    quoteResult: PricePoint | null = null,
  ) {
    this.#name = name;
    this.closeResult = closeResult;
    this.quoteResult = quoteResult;
  }

  async fetchClose(
    _asset: AssetType,
    _assetId: AssetId,
    _date: string,
  ): Promise<PricePoint | null> {
    return this.closeResult;
  }

  async fetchQuote(_asset: AssetType, _assetId: AssetId): Promise<PricePoint | null> {
    return this.quoteResult;
  }

  name(): string {
    return this.#name;
  }
}

/** A stub crypto source that returns a canned response or null. */
class StubCryptoSource implements CryptoPriceSource {
  readonly #name: string;
  closeResult: PricePoint | null;
  quoteResult: PricePoint | null;

  constructor(
    name: string,
    closeResult: PricePoint | null = null,
    quoteResult: PricePoint | null = null,
  ) {
    this.#name = name;
    this.closeResult = closeResult;
    this.quoteResult = quoteResult;
  }

  async fetchClose(
    _asset: AssetType,
    _assetId: AssetId,
    _date: string,
  ): Promise<PricePoint | null> {
    return this.closeResult;
  }

  async fetchQuote(_asset: AssetType, _assetId: AssetId): Promise<PricePoint | null> {
    return this.quoteResult;
  }

  name(): string {
    return this.#name;
  }
}

/** A stub FX source that returns a canned response or null. */
class StubFxSource implements FxRateSource {
  readonly #name: string;
  closeResult: FxRatePoint | null;

  constructor(name: string, closeResult: FxRatePoint | null = null) {
    this.#name = name;
    this.closeResult = closeResult;
  }

  async fetchClose(_base: string, _quote: string, _date: string): Promise<FxRatePoint | null> {
    return this.closeResult;
  }

  name(): string {
    return this.#name;
  }
}

/** A stub MarketDataSource (generic provider). */
class StubProvider implements MarketDataSource {
  readonly #name: string;
  priceResult: PricePoint | null;
  fxResult: FxRatePoint | null;

  constructor(
    name: string,
    priceResult: PricePoint | null = null,
    fxResult: FxRatePoint | null = null,
  ) {
    this.#name = name;
    this.priceResult = priceResult;
    this.fxResult = fxResult;
  }

  async fetchPrice(
    _asset: AssetType,
    _assetId: AssetId,
    _date: string,
  ): Promise<PricePoint | null> {
    return this.priceResult;
  }

  async fetchFxRate(_base: string, _quote: string, _date: string): Promise<FxRatePoint | null> {
    return this.fxResult;
  }

  name(): string {
    return this.#name;
  }
}

// ---------------------------------------------------------------------------
// Router tests
// ---------------------------------------------------------------------------

describe('EquityPriceRouter', () => {
  it('returns result from first source that responds', async () => {
    const price = makePrice({ source: 'source-b' });
    const sourceA = new StubEquitySource('a', null);
    const sourceB = new StubEquitySource('b', price);
    const router = new EquityPriceRouter([sourceA, sourceB]);

    const result = await router.fetchClose(AAPL, AAPL_ID, '2024-01-15');
    expect(result).not.toBeNull();
    expect(result!.source).toBe('source-b');
  });

  it('returns null when no source responds', async () => {
    const sourceA = new StubEquitySource('a', null);
    const router = new EquityPriceRouter([sourceA]);

    const result = await router.fetchClose(AAPL, AAPL_ID, '2024-01-15');
    expect(result).toBeNull();
  });

  it('fetchQuote tries sources in order', async () => {
    const quote = makePrice({ kind: 'quote', source: 'source-a' });
    const sourceA = new StubEquitySource('a', null, quote);
    const sourceB = new StubEquitySource('b', null, null);
    const router = new EquityPriceRouter([sourceA, sourceB]);

    const result = await router.fetchQuote(AAPL, AAPL_ID);
    expect(result).not.toBeNull();
    expect(result!.source).toBe('source-a');
  });
});

describe('CryptoPriceRouter', () => {
  it('returns result from first source that responds', async () => {
    const price = makePrice({ asset_id: BTC_ID, source: 'crypto-src' });
    const source = new StubCryptoSource('crypto', price);
    const router = new CryptoPriceRouter([source]);

    const result = await router.fetchClose(BTC, BTC_ID, '2024-01-15');
    expect(result).not.toBeNull();
    expect(result!.source).toBe('crypto-src');
  });
});

describe('FxRateRouter', () => {
  it('returns result from first source that responds', async () => {
    const rate = makeFxRate({ source: 'fx-src' });
    const source = new StubFxSource('fx', rate);
    const router = new FxRateRouter([source]);

    const result = await router.fetchClose('USD', 'EUR', '2024-01-15');
    expect(result).not.toBeNull();
    expect(result!.source).toBe('fx-src');
  });

  it('returns null when all sources fail', async () => {
    const sourceA = new StubFxSource('a', null);
    const sourceB = new StubFxSource('b', null);
    const router = new FxRateRouter([sourceA, sourceB]);

    const result = await router.fetchClose('USD', 'EUR', '2024-01-15');
    expect(result).toBeNull();
  });
});

// ---------------------------------------------------------------------------
// MarketDataService tests
// ---------------------------------------------------------------------------

describe('MarketDataService', () => {
  let store: MemoryMarketDataStore;
  let clock: FixedClock;
  let service: MarketDataService;

  beforeEach(() => {
    store = new MemoryMarketDataStore();
    clock = new FixedClock(new Date('2024-01-15T22:00:00Z'));
  });

  // -- priceFromStore -------------------------------------------------------

  describe('priceFromStore', () => {
    it('returns price for exact date', async () => {
      const price = makePrice({ as_of_date: '2024-01-15' });
      await store.put_prices([price]);

      service = new MarketDataService(store);
      const result = await service.priceFromStore(AAPL, '2024-01-15');

      expect(result).not.toBeNull();
      expect(result!.price).toBe('185.50');
      expect(result!.as_of_date).toBe('2024-01-15');
    });

    it('returns price from lookback', async () => {
      // Store a price 3 days before the query date
      const price = makePrice({ as_of_date: '2024-01-12' });
      await store.put_prices([price]);

      service = new MarketDataService(store);
      const result = await service.priceFromStore(AAPL, '2024-01-15');

      expect(result).not.toBeNull();
      expect(result!.price).toBe('185.50');
      expect(result!.as_of_date).toBe('2024-01-12');
    });

    it('returns older cached price by default (unbounded store lookup)', async () => {
      // Store a price 10 days before the query date.
      const price = makePrice({ as_of_date: '2024-01-05' });
      await store.put_prices([price]);

      service = new MarketDataService(store);
      const result = await service.priceFromStore(AAPL, '2024-01-15');

      expect(result).not.toBeNull();
      expect(result!.as_of_date).toBe('2024-01-05');
    });
  });

  // -- fxClose --------------------------------------------------------------

  describe('fxClose', () => {
    it('returns identity rate for same currency', async () => {
      service = new MarketDataService(store);
      const result = await service.fxClose('USD', 'USD', '2024-01-15');

      expect(result.rate).toBe('1');
      expect(result.base).toBe('USD');
      expect(result.quote).toBe('USD');
      expect(result.source).toBe('identity');
    });

    it('normalizes case for identity check', async () => {
      service = new MarketDataService(store);
      const result = await service.fxClose('usd', 'USD', '2024-01-15');

      expect(result.rate).toBe('1');
      expect(result.source).toBe('identity');
    });

    it('uses lookback to find FX rate', async () => {
      // Store rate 2 days before query date
      const rate = makeFxRate({ as_of_date: '2024-01-13' });
      await store.put_fx_rates([rate]);

      service = new MarketDataService(store);
      const result = await service.fxClose('USD', 'EUR', '2024-01-15');

      expect(result.rate).toBe('0.9150');
      expect(result.as_of_date).toBe('2024-01-13');
    });

    it('throws when not found after lookback', async () => {
      service = new MarketDataService(store);

      await expect(service.fxClose('USD', 'EUR', '2024-01-15')).rejects.toThrow(
        /No close FX rate found/i,
      );
    });
  });

  // -- fxCloseForce ---------------------------------------------------------

  describe('fxCloseForce', () => {
    it('returns identity with fetched=false', async () => {
      service = new MarketDataService(store);
      const [result, fetched] = await service.fxCloseForce('USD', 'USD', '2024-01-15');

      expect(result.rate).toBe('1');
      expect(fetched).toBe(false);
    });
  });

  // -- priceClose -----------------------------------------------------------

  describe('priceClose', () => {
    it('returns cached price when no sources configured', async () => {
      const price = makePrice({ as_of_date: '2024-01-15' });
      await store.put_prices([price]);

      service = new MarketDataService(store);
      const result = await service.priceClose(AAPL, '2024-01-15');

      expect(result.price).toBe('185.50');
    });

    it('throws when not found after lookback and no sources', async () => {
      service = new MarketDataService(store);

      await expect(service.priceClose(AAPL, '2024-01-15')).rejects.toThrow(/No close price found/i);
    });

    it('fetches from equity router when not in store', async () => {
      const fetchedPrice = makePrice({
        as_of_date: '2024-01-15',
        price: '186.00',
        source: 'yahoo',
      });
      const equitySource = new StubEquitySource('yahoo', fetchedPrice);
      const equityRouter = new EquityPriceRouter([equitySource]);

      service = new MarketDataService(store);
      service.withEquityRouter(equityRouter).withClock(clock);

      const result = await service.priceClose(AAPL, '2024-01-15');

      expect(result.price).toBe('186.00');

      // Verify it was stored
      const stored = await store.get_price(AAPL_ID, '2024-01-15', 'close');
      expect(stored).not.toBeNull();
      expect(stored!.price).toBe('186.00');
    });

    it('fetches from crypto router for crypto assets', async () => {
      const fetchedPrice = makePrice({
        asset_id: BTC_ID,
        as_of_date: '2024-01-15',
        price: '42000.00',
        source: 'coingecko',
      });
      const cryptoSource = new StubCryptoSource('coingecko', fetchedPrice);
      const cryptoRouter = new CryptoPriceRouter([cryptoSource]);

      service = new MarketDataService(store);
      service.withCryptoRouter(cryptoRouter).withClock(clock);

      const result = await service.priceClose(BTC, '2024-01-15');

      expect(result.price).toBe('42000.00');
    });

    it('fetches from generic provider when routers do not have result', async () => {
      const fetchedPrice = makePrice({
        as_of_date: '2024-01-15',
        price: '187.00',
        source: 'generic',
      });
      const provider = new StubProvider('generic', fetchedPrice);

      service = new MarketDataService(store, provider);
      service.withClock(clock);

      const result = await service.priceClose(AAPL, '2024-01-15');

      expect(result.price).toBe('187.00');
    });
  });

  // -- priceCloseForce ------------------------------------------------------

  describe('priceCloseForce', () => {
    it('returns fetched=false when price comes from store', async () => {
      const price = makePrice({ as_of_date: '2024-01-15' });
      await store.put_prices([price]);

      service = new MarketDataService(store);
      const [result, fetched] = await service.priceCloseForce(AAPL, '2024-01-15');

      expect(result.price).toBe('185.50');
      expect(fetched).toBe(false);
    });

    it('returns fetched=true when price comes from source', async () => {
      const fetchedPrice = makePrice({
        as_of_date: '2024-01-15',
        price: '186.00',
        source: 'yahoo',
      });
      const equitySource = new StubEquitySource('yahoo', fetchedPrice);
      const equityRouter = new EquityPriceRouter([equitySource]);

      service = new MarketDataService(store);
      service.withEquityRouter(equityRouter).withClock(clock);

      const [result, fetched] = await service.priceCloseForce(AAPL, '2024-01-15');

      expect(result.price).toBe('186.00');
      expect(fetched).toBe(true);
    });
  });

  // -- priceLatestWithStatus ------------------------------------------------

  describe('priceLatestWithStatus', () => {
    it('returns fresh cached quote without fetching', async () => {
      // Store a quote that is "fresh" (within staleness threshold)
      const quotePrice = makePrice({
        as_of_date: '2024-01-15',
        kind: 'quote',
        price: '186.25',
        // timestamp very recent relative to clock
        timestamp: new Date('2024-01-15T21:59:00Z'),
      });
      await store.put_prices([quotePrice]);

      service = new MarketDataService(store);
      // Set staleness to 5 minutes (300000ms)
      service.withQuoteStaleness(300_000).withClock(clock);

      const [result, fetched] = await service.priceLatestWithStatus(AAPL, '2024-01-15');

      expect(result.price).toBe('186.25');
      expect(result.kind).toBe('quote');
      expect(fetched).toBe(false);
    });

    it('fetches new quote when cached quote is stale', async () => {
      // Store a stale quote (old timestamp)
      const staleQuote = makePrice({
        as_of_date: '2024-01-15',
        kind: 'quote',
        price: '185.00',
        timestamp: new Date('2024-01-15T10:00:00Z'), // 12 hours old
      });
      await store.put_prices([staleQuote]);

      // Set up source to return fresh quote
      const freshQuote = makePrice({
        as_of_date: '2024-01-15',
        kind: 'quote',
        price: '186.50',
        timestamp: new Date('2024-01-15T22:00:00Z'),
        source: 'yahoo',
      });
      const equitySource = new StubEquitySource('yahoo', null, freshQuote);
      const equityRouter = new EquityPriceRouter([equitySource]);

      service = new MarketDataService(store);
      // 5 minutes staleness
      service.withEquityRouter(equityRouter).withQuoteStaleness(300_000).withClock(clock);

      const [result, fetched] = await service.priceLatestWithStatus(AAPL, '2024-01-15');

      expect(result.price).toBe('186.50');
      expect(fetched).toBe(true);
    });

    it('falls back to close price when no quote available', async () => {
      const closePrice = makePrice({
        as_of_date: '2024-01-15',
        kind: 'close',
        price: '185.50',
      });
      await store.put_prices([closePrice]);

      service = new MarketDataService(store);
      service.withClock(clock);

      const [result, fetched] = await service.priceLatestWithStatus(AAPL, '2024-01-15');

      expect(result.price).toBe('185.50');
      expect(result.kind).toBe('close');
      expect(fetched).toBe(false);
    });
  });

  // -- priceLatest ----------------------------------------------------------

  describe('priceLatest', () => {
    it('returns the latest price', async () => {
      const closePrice = makePrice({
        as_of_date: '2024-01-15',
        kind: 'close',
        price: '185.50',
      });
      await store.put_prices([closePrice]);

      service = new MarketDataService(store);
      service.withClock(clock);

      const result = await service.priceLatest(AAPL, '2024-01-15');
      expect(result.price).toBe('185.50');
    });
  });

  // -- priceLatestForce -----------------------------------------------------

  describe('priceLatestForce', () => {
    it('ignores fresh cached quote and fetches new one', async () => {
      // Store a "fresh" quote
      const freshCachedQuote = makePrice({
        as_of_date: '2024-01-15',
        kind: 'quote',
        price: '185.00',
        timestamp: new Date('2024-01-15T21:59:00Z'),
      });
      await store.put_prices([freshCachedQuote]);

      // Source returns a newer quote
      const newQuote = makePrice({
        as_of_date: '2024-01-15',
        kind: 'quote',
        price: '186.75',
        timestamp: new Date('2024-01-15T22:00:00Z'),
        source: 'yahoo',
      });
      const equitySource = new StubEquitySource('yahoo', null, newQuote);
      const equityRouter = new EquityPriceRouter([equitySource]);

      service = new MarketDataService(store);
      service.withEquityRouter(equityRouter).withQuoteStaleness(300_000).withClock(clock);

      const [result, fetched] = await service.priceLatestForce(AAPL, '2024-01-15');

      expect(result.price).toBe('186.75');
      expect(fetched).toBe(true);
    });

    it('falls back to close when forced fetch returns no quote', async () => {
      const closePrice = makePrice({
        as_of_date: '2024-01-15',
        kind: 'close',
        price: '185.50',
      });
      await store.put_prices([closePrice]);

      // Equity source returns no quote
      const equitySource = new StubEquitySource('yahoo', null, null);
      const equityRouter = new EquityPriceRouter([equitySource]);

      service = new MarketDataService(store);
      service.withEquityRouter(equityRouter).withClock(clock);

      const [result, fetched] = await service.priceLatestForce(AAPL, '2024-01-15');

      // Falls back to close price
      expect(result.price).toBe('185.50');
      expect(fetched).toBe(false);
    });
  });

  // -- storePrice -----------------------------------------------------------

  describe('storePrice', () => {
    it('is idempotent - skips if existing price has newer-or-equal timestamp', async () => {
      const newer = makePrice({
        as_of_date: '2024-01-15',
        price: '186.00',
        timestamp: new Date('2024-01-15T22:00:00Z'),
      });
      await store.put_prices([newer]);

      service = new MarketDataService(store);

      // Try to store an older price
      const older = makePrice({
        as_of_date: '2024-01-15',
        price: '185.00',
        timestamp: new Date('2024-01-15T20:00:00Z'),
      });
      await service.storePrice(older);

      // The newer price should still be there
      const result = await store.get_price(AAPL_ID, '2024-01-15', 'close');
      expect(result).not.toBeNull();
      expect(result!.price).toBe('186.00');
    });

    it('overwrites when new price has newer timestamp', async () => {
      const older = makePrice({
        as_of_date: '2024-01-15',
        price: '185.00',
        timestamp: new Date('2024-01-15T20:00:00Z'),
      });
      await store.put_prices([older]);

      service = new MarketDataService(store);

      const newer = makePrice({
        as_of_date: '2024-01-15',
        price: '186.00',
        timestamp: new Date('2024-01-15T22:00:00Z'),
      });
      await service.storePrice(newer);

      const result = await store.get_price(AAPL_ID, '2024-01-15', 'close');
      expect(result).not.toBeNull();
      expect(result!.price).toBe('186.00');
    });
  });

  // -- registerAsset --------------------------------------------------------

  describe('registerAsset', () => {
    it('creates entry if not exists', async () => {
      service = new MarketDataService(store);

      await service.registerAsset(AAPL);

      const entry = await store.get_asset_entry(AAPL_ID);
      expect(entry).not.toBeNull();
      expect(entry!.asset).toEqual(AAPL);
    });

    it('does not overwrite existing entry', async () => {
      service = new MarketDataService(store);

      // First registration
      await service.registerAsset(AAPL);
      const entry = await store.get_asset_entry(AAPL_ID);
      entry!.provider_ids['yahoo'] = 'AAPL';
      await store.upsert_asset_entry(entry!);

      // Second registration should not wipe out provider_ids
      await service.registerAsset(AAPL);
      const entry2 = await store.get_asset_entry(AAPL_ID);
      expect(entry2).not.toBeNull();
      expect(entry2!.provider_ids['yahoo']).toBe('AAPL');
    });
  });

  // -- fxFromStore ----------------------------------------------------------

  describe('fxFromStore', () => {
    it('returns rate for exact date', async () => {
      const rate = makeFxRate({ as_of_date: '2024-01-15' });
      await store.put_fx_rates([rate]);

      service = new MarketDataService(store);
      const result = await service.fxFromStore('USD', 'EUR', '2024-01-15');

      expect(result).not.toBeNull();
      expect(result!.rate).toBe('0.9150');
    });

    it('returns rate from lookback', async () => {
      const rate = makeFxRate({ as_of_date: '2024-01-12' });
      await store.put_fx_rates([rate]);

      service = new MarketDataService(store);
      const result = await service.fxFromStore('USD', 'EUR', '2024-01-15');

      expect(result).not.toBeNull();
      expect(result!.rate).toBe('0.9150');
    });

    it('returns older cached FX rate by default (unbounded store lookup)', async () => {
      const rate = makeFxRate({ as_of_date: '2024-01-05' });
      await store.put_fx_rates([rate]);

      service = new MarketDataService(store);
      const result = await service.fxFromStore('USD', 'EUR', '2024-01-15');

      expect(result).not.toBeNull();
      expect(result!.as_of_date).toBe('2024-01-05');
    });
  });

  // -- withLookbackDays -----------------------------------------------------

  describe('withLookbackDays', () => {
    it('bounds store lookup when explicitly configured', async () => {
      const price = makePrice({ as_of_date: '2024-01-05' });
      await store.put_prices([price]);

      service = new MarketDataService(store);
      service.withLookbackDays(7);

      const result = await service.priceFromStore(AAPL, '2024-01-15');
      expect(result).toBeNull();
    });
  });
});
