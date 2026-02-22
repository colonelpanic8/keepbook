/**
 * Market data service for price and FX rate lookups.
 *
 * Port of the Rust `MarketDataService`. Provides lookback-aware price and FX
 * rate retrieval from the underlying MarketDataStore, with optional external
 * fetching through routers and a generic provider.
 */

import { type AssetType, Asset } from '../models/asset.js';
import { type Clock, SystemClock } from '../clock.js';
import { AssetId } from './asset-id.js';
import type { PricePoint, FxRatePoint } from './models.js';
import { AssetRegistryEntryFactory } from './models.js';
import type { MarketDataStore } from './store.js';
import type { MarketDataSource } from './sources.js';
import type { EquityPriceRouter, CryptoPriceRouter, FxRateRouter } from './sources.js';

// ---------------------------------------------------------------------------
// Date helpers
// ---------------------------------------------------------------------------

/**
 * Subtract a number of days from a "YYYY-MM-DD" string, returning "YYYY-MM-DD".
 */
function subtractDays(dateStr: string, days: number): string {
  const d = new Date(dateStr + 'T00:00:00Z');
  d.setUTCDate(d.getUTCDate() - days);
  return d.toISOString().slice(0, 10);
}

function selectLatestPriceOnOrBefore(prices: PricePoint[], date: string): PricePoint | null {
  const candidates = prices.filter((p) => p.kind === 'close' && p.as_of_date <= date);
  if (candidates.length === 0) {
    return null;
  }
  candidates.sort((a, b) => {
    if (a.as_of_date !== b.as_of_date) {
      return a.as_of_date.localeCompare(b.as_of_date);
    }
    return a.timestamp.getTime() - b.timestamp.getTime();
  });
  return candidates[candidates.length - 1];
}

function selectLatestFxRateOnOrBefore(rates: FxRatePoint[], date: string): FxRatePoint | null {
  const candidates = rates.filter((r) => r.kind === 'close' && r.as_of_date <= date);
  if (candidates.length === 0) {
    return null;
  }
  candidates.sort((a, b) => {
    if (a.as_of_date !== b.as_of_date) {
      return a.as_of_date.localeCompare(b.as_of_date);
    }
    return a.timestamp.getTime() - b.timestamp.getTime();
  });
  return candidates[candidates.length - 1];
}

// ---------------------------------------------------------------------------
// MarketDataService
// ---------------------------------------------------------------------------

export class MarketDataService {
  private readonly store: MarketDataStore;
  private readonly provider: MarketDataSource | null;
  // Null means unbounded store lookups (latest close <= query date).
  private storeLookbackDays_: number | null = null;
  // Bounds external fetch attempts when exact-date close is unavailable.
  private fetchLookbackDays_: number = 7;
  private quoteStaleness_: number = 300_000; // 5 minutes in ms
  private clock_: Clock = new SystemClock();
  private equityRouter_: EquityPriceRouter | null = null;
  private cryptoRouter_: CryptoPriceRouter | null = null;
  private fxRouter_: FxRateRouter | null = null;

  constructor(store: MarketDataStore, provider?: MarketDataSource | null) {
    this.store = store;
    this.provider = provider ?? null;
  }

  // -- Builder methods ------------------------------------------------------

  withEquityRouter(router: EquityPriceRouter): this {
    this.equityRouter_ = router;
    return this;
  }

  withCryptoRouter(router: CryptoPriceRouter): this {
    this.cryptoRouter_ = router;
    return this;
  }

  withFxRouter(router: FxRateRouter): this {
    this.fxRouter_ = router;
    return this;
  }

  withLookbackDays(days: number): this {
    this.storeLookbackDays_ = days;
    this.fetchLookbackDays_ = days;
    return this;
  }

  withQuoteStaleness(ms: number): this {
    this.quoteStaleness_ = ms;
    return this;
  }

  withClock(clock: Clock): this {
    this.clock_ = clock;
    return this;
  }

  // -- Store-only lookups ---------------------------------------------------

  /**
   * Get price from store only, no external fetching.
   * Returns the latest close on or before the query date.
   *
   * If `storeLookbackDays_` is set, lookup is bounded to that range.
   * Otherwise (default), lookup is unbounded.
   */
  async priceFromStore(asset: AssetType, date: string): Promise<PricePoint | null> {
    const normalized = Asset.normalized(asset);
    const assetId = AssetId.fromAsset(normalized);

    if (this.storeLookbackDays_ !== null) {
      for (let offset = 0; offset <= this.storeLookbackDays_; offset++) {
        const targetDate = subtractDays(date, offset);
        const price = await this.store.get_price(assetId, targetDate, 'close');
        if (price !== null) {
          return price;
        }
      }
      return null;
    }

    const all = await this.store.get_all_prices(assetId);
    return selectLatestPriceOnOrBefore(all, date);
  }

  /**
   * Get a valuation price from store only, no external fetching.
   *
   * - First tries close prices via `priceFromStore`
   * - If none found and `allowQuoteFallback` is true, tries same-day quote
   */
  async valuationPriceFromStore(
    asset: AssetType,
    date: string,
    allowQuoteFallback: boolean,
  ): Promise<PricePoint | null> {
    const close = await this.priceFromStore(asset, date);
    if (close !== null) {
      return close;
    }

    if (!allowQuoteFallback) {
      return null;
    }

    const normalized = Asset.normalized(asset);
    const assetId = AssetId.fromAsset(normalized);
    return this.store.get_price(assetId, date, 'quote');
  }

  /**
   * Get FX rate from store only, no external fetching.
   * Returns the latest close on or before the query date.
   *
   * If `storeLookbackDays_` is set, lookup is bounded to that range.
   * Otherwise (default), lookup is unbounded.
   */
  async fxFromStore(base: string, quote: string, date: string): Promise<FxRatePoint | null> {
    const baseNorm = base.trim().toUpperCase();
    const quoteNorm = quote.trim().toUpperCase();

    if (this.storeLookbackDays_ !== null) {
      for (let offset = 0; offset <= this.storeLookbackDays_; offset++) {
        const targetDate = subtractDays(date, offset);
        const rate = await this.store.get_fx_rate(baseNorm, quoteNorm, targetDate, 'close');
        if (rate !== null) {
          return rate;
        }
      }
      return null;
    }

    const all = await this.store.get_all_fx_rates(baseNorm, quoteNorm);
    return selectLatestFxRateOnOrBefore(all, date);
  }

  // -- Price lookups with external fetching ---------------------------------

  /**
   * Get a close price for an asset on or before the given date.
   * Tries store first (with lookback), then fetches from routers/provider.
   * Throws if no price is found.
   */
  async priceClose(asset: AssetType, date: string): Promise<PricePoint> {
    // Try store first
    const cached = await this.priceFromStore(asset, date);
    if (cached !== null) {
      return cached;
    }

    // Try fetching from external sources with bounded lookback
    for (let offset = 0; offset <= this.fetchLookbackDays_; offset++) {
      const targetDate = subtractDays(date, offset);
      const fetched = await this.fetchClosePrice(asset, targetDate);
      if (fetched !== null) {
        await this.storePrice(fetched);
        return fetched;
      }
    }

    const assetId = AssetId.fromAsset(Asset.normalized(asset));
    throw new Error(`No close price found for asset ${assetId.asStr()} on or before ${date}`);
  }

  /**
   * Like priceClose, but returns [PricePoint, boolean] where the boolean
   * indicates whether the price was freshly fetched from an external source.
   */
  async priceCloseForce(asset: AssetType, date: string): Promise<[PricePoint, boolean]> {
    // Try fetching from external sources first, with bounded lookback.
    for (let offset = 0; offset <= this.fetchLookbackDays_; offset++) {
      const targetDate = subtractDays(date, offset);
      const fetched = await this.fetchClosePrice(asset, targetDate);
      if (fetched !== null) {
        await this.storePrice(fetched);
        return [fetched, true];
      }
    }

    // Fall back to store
    const cached = await this.priceFromStore(asset, date);
    if (cached !== null) {
      return [cached, false];
    }

    const assetId = AssetId.fromAsset(Asset.normalized(asset));
    throw new Error(`No close price found for asset ${assetId.asStr()} on or before ${date}`);
  }

  /**
   * Get the latest available price for an asset.
   * Checks for a fresh cached quote, tries live quote, then falls back to close.
   * Throws if no price is found.
   */
  async priceLatest(asset: AssetType, date: string): Promise<PricePoint> {
    const [result] = await this.priceLatestWithStatus(asset, date);
    return result;
  }

  /**
   * Like priceLatest, but returns [PricePoint, boolean] where the boolean
   * indicates whether the price was freshly fetched.
   */
  async priceLatestWithStatus(asset: AssetType, date: string): Promise<[PricePoint, boolean]> {
    return this.priceLatestInternal(asset, date, false);
  }

  /**
   * Like priceLatestWithStatus, but always fetches a new quote
   * (ignores cached quote freshness).
   */
  async priceLatestForce(asset: AssetType, date: string): Promise<[PricePoint, boolean]> {
    return this.priceLatestInternal(asset, date, true);
  }

  // -- FX lookups -----------------------------------------------------------

  /**
   * Get the FX close rate for a currency pair on or before the given date.
   * Returns identity rate (1) if base === quote (case-insensitive).
   * Tries store with lookback, then external sources.
   * Throws if no rate is found.
   */
  async fxClose(base: string, quote: string, date: string): Promise<FxRatePoint> {
    const baseNorm = base.trim().toUpperCase();
    const quoteNorm = quote.trim().toUpperCase();

    // Identity case
    if (baseNorm === quoteNorm) {
      return this.makeIdentityFxRate(baseNorm, quoteNorm, date);
    }

    // Try store with lookback
    const cached = await this.fxFromStore(baseNorm, quoteNorm, date);
    if (cached !== null) {
      return cached;
    }

    // Try external sources with bounded lookback
    for (let offset = 0; offset <= this.fetchLookbackDays_; offset++) {
      const targetDate = subtractDays(date, offset);
      const fetched = await this.fetchFxRate(baseNorm, quoteNorm, targetDate);
      if (fetched !== null) {
        await this.store.put_fx_rates([fetched]);
        return fetched;
      }
    }

    throw new Error(`No close FX rate found for ${baseNorm}->${quoteNorm} on or before ${date}`);
  }

  /**
   * Like fxClose, but returns [FxRatePoint, boolean] where the boolean
   * indicates whether the rate was freshly fetched.
   */
  async fxCloseForce(base: string, quote: string, date: string): Promise<[FxRatePoint, boolean]> {
    const baseNorm = base.trim().toUpperCase();
    const quoteNorm = quote.trim().toUpperCase();

    // Identity case
    if (baseNorm === quoteNorm) {
      return [this.makeIdentityFxRate(baseNorm, quoteNorm, date), false];
    }

    // Try external sources first, with bounded lookback.
    for (let offset = 0; offset <= this.fetchLookbackDays_; offset++) {
      const targetDate = subtractDays(date, offset);
      const fetched = await this.fetchFxRate(baseNorm, quoteNorm, targetDate);
      if (fetched !== null) {
        await this.store.put_fx_rates([fetched]);
        return [fetched, true];
      }
    }

    // Fall back to store
    const cached = await this.fxFromStore(baseNorm, quoteNorm, date);
    if (cached !== null) {
      return [cached, false];
    }

    throw new Error(`No close FX rate found for ${baseNorm}->${quoteNorm} on or before ${date}`);
  }

  // -- Direct storage -------------------------------------------------------

  /**
   * Register an asset in the registry if not already present.
   */
  async registerAsset(asset: AssetType): Promise<void> {
    const normalized = Asset.normalized(asset);
    const entry = AssetRegistryEntryFactory.new(normalized);
    const existing = await this.store.get_asset_entry(entry.id);
    if (existing === null) {
      await this.store.upsert_asset_entry(entry);
    }
  }

  /**
   * Store a price point idempotently.
   * Skips if an existing price for the same asset/date/kind has a
   * newer-or-equal timestamp.
   */
  async storePrice(price: PricePoint): Promise<void> {
    const existing = await this.store.get_price(price.asset_id, price.as_of_date, price.kind);

    if (existing !== null && existing.timestamp >= price.timestamp) {
      // Existing is newer or equal, skip
      return;
    }

    await this.store.put_prices([price]);
  }

  // -- Private helpers ------------------------------------------------------

  /**
   * Internal helper for priceLatest variants.
   * @param force If true, always fetch a new quote (ignore cached freshness).
   */
  private async priceLatestInternal(
    asset: AssetType,
    date: string,
    force: boolean,
  ): Promise<[PricePoint, boolean]> {
    const normalized = Asset.normalized(asset);
    const assetId = AssetId.fromAsset(normalized);

    // Step 1: Check cached quote freshness (unless force)
    if (!force) {
      const cachedQuote = await this.store.get_price(assetId, date, 'quote');
      if (cachedQuote !== null && this.isQuoteFresh(cachedQuote)) {
        return [cachedQuote, false];
      }
    }

    // Step 2: Try to fetch a live quote from sources
    const liveQuote = await this.fetchQuotePrice(normalized, assetId);
    if (liveQuote !== null) {
      await this.storePrice(liveQuote);
      return [liveQuote, true];
    }

    // Step 3: Fall back to close price (with lookback)
    const closePrice = await this.priceFromStore(asset, date);
    if (closePrice !== null) {
      return [closePrice, false];
    }

    throw new Error(`No price found for asset ${assetId.asStr()} on or before ${date}`);
  }

  /** Check if a cached quote is still within the staleness threshold. */
  private isQuoteFresh(quote: PricePoint): boolean {
    const now = this.clock_.now();
    const age = now.getTime() - quote.timestamp.getTime();
    return age < this.quoteStaleness_;
  }

  /** Create an identity FX rate (base == quote, rate == 1). */
  private makeIdentityFxRate(base: string, quote: string, date: string): FxRatePoint {
    return {
      base,
      quote,
      as_of_date: date,
      timestamp: this.clock_.now(),
      rate: '1',
      kind: 'close',
      source: 'identity',
    };
  }

  /**
   * Try to fetch a close price from routers (based on asset type),
   * then from the generic provider.
   */
  private async fetchClosePrice(asset: AssetType, date: string): Promise<PricePoint | null> {
    const normalized = Asset.normalized(asset);
    const assetId = AssetId.fromAsset(normalized);

    // Try type-specific router
    let result: PricePoint | null = null;
    switch (normalized.type) {
      case 'equity':
        if (this.equityRouter_ !== null) {
          result = await this.equityRouter_.fetchClose(normalized, assetId, date);
        }
        break;
      case 'crypto':
        if (this.cryptoRouter_ !== null) {
          result = await this.cryptoRouter_.fetchClose(normalized, assetId, date);
        }
        break;
      case 'currency':
        // No specific router for currency prices
        break;
    }

    if (result !== null) {
      return result;
    }

    // Try generic provider
    if (this.provider !== null) {
      return this.provider.fetchPrice(normalized, assetId, date);
    }

    return null;
  }

  /**
   * Try to fetch a live quote from the appropriate router.
   */
  private async fetchQuotePrice(asset: AssetType, assetId: AssetId): Promise<PricePoint | null> {
    switch (asset.type) {
      case 'equity':
        if (this.equityRouter_ !== null) {
          return this.equityRouter_.fetchQuote(asset, assetId);
        }
        break;
      case 'crypto':
        if (this.cryptoRouter_ !== null) {
          return this.cryptoRouter_.fetchQuote(asset, assetId);
        }
        break;
      case 'currency':
        break;
    }
    return null;
  }

  /**
   * Try to fetch an FX rate from the FX router, then from the generic provider.
   */
  private async fetchFxRate(
    base: string,
    quote: string,
    date: string,
  ): Promise<FxRatePoint | null> {
    if (this.fxRouter_ !== null) {
      const result = await this.fxRouter_.fetchClose(base, quote, date);
      if (result !== null) {
        return result;
      }
    }

    if (this.provider !== null) {
      return this.provider.fetchFxRate(base, quote, date);
    }

    return null;
  }
}
