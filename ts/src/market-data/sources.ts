/**
 * Market data source interfaces and routers.
 *
 * Port of the Rust `market_data::sources` and `market_data::provider` modules.
 *
 * - Source interfaces define the contract for external data providers.
 * - Router classes hold a list of sources and try them in order, returning
 *   the first non-null result.
 */

import type { AssetType } from '../models/asset.js';
import type { AssetId } from './asset-id.js';
import type { PricePoint, FxRatePoint } from './models.js';

// ---------------------------------------------------------------------------
// Generic provider interface
// ---------------------------------------------------------------------------

/** Generic market data source that can fetch prices and FX rates. */
export interface MarketDataSource {
  fetchPrice(asset: AssetType, assetId: AssetId, date: string): Promise<PricePoint | null>;

  fetchFxRate(base: string, quote: string, date: string): Promise<FxRatePoint | null>;

  name(): string;
}

// ---------------------------------------------------------------------------
// Asset-specific source interfaces
// ---------------------------------------------------------------------------

/** Source for equity (stock) price data. */
export interface EquityPriceSource {
  fetchClose(asset: AssetType, assetId: AssetId, date: string): Promise<PricePoint | null>;

  fetchQuote(asset: AssetType, assetId: AssetId): Promise<PricePoint | null>;

  name(): string;
}

/** Source for cryptocurrency price data. */
export interface CryptoPriceSource {
  fetchClose(asset: AssetType, assetId: AssetId, date: string): Promise<PricePoint | null>;

  fetchQuote(asset: AssetType, assetId: AssetId): Promise<PricePoint | null>;

  name(): string;
}

/** Source for foreign exchange rate data. */
export interface FxRateSource {
  fetchClose(base: string, quote: string, date: string): Promise<FxRatePoint | null>;

  name(): string;
}

// ---------------------------------------------------------------------------
// Routers
// ---------------------------------------------------------------------------

/**
 * Router that tries multiple equity price sources in order.
 * Returns the first non-null result, or null if all sources fail.
 */
export class EquityPriceRouter {
  readonly #sources: EquityPriceSource[];

  constructor(sources: EquityPriceSource[]) {
    this.#sources = sources;
  }

  async fetchClose(asset: AssetType, assetId: AssetId, date: string): Promise<PricePoint | null> {
    for (const source of this.#sources) {
      const result = await source.fetchClose(asset, assetId, date);
      if (result !== null) {
        return result;
      }
    }
    return null;
  }

  async fetchQuote(asset: AssetType, assetId: AssetId): Promise<PricePoint | null> {
    for (const source of this.#sources) {
      const result = await source.fetchQuote(asset, assetId);
      if (result !== null) {
        return result;
      }
    }
    return null;
  }
}

/**
 * Router that tries multiple crypto price sources in order.
 * Returns the first non-null result, or null if all sources fail.
 */
export class CryptoPriceRouter {
  readonly #sources: CryptoPriceSource[];

  constructor(sources: CryptoPriceSource[]) {
    this.#sources = sources;
  }

  async fetchClose(asset: AssetType, assetId: AssetId, date: string): Promise<PricePoint | null> {
    for (const source of this.#sources) {
      const result = await source.fetchClose(asset, assetId, date);
      if (result !== null) {
        return result;
      }
    }
    return null;
  }

  async fetchQuote(asset: AssetType, assetId: AssetId): Promise<PricePoint | null> {
    for (const source of this.#sources) {
      const result = await source.fetchQuote(asset, assetId);
      if (result !== null) {
        return result;
      }
    }
    return null;
  }
}

/**
 * Router that tries multiple FX rate sources in order.
 * Returns the first non-null result, or null if all sources fail.
 */
export class FxRateRouter {
  readonly #sources: FxRateSource[];

  constructor(sources: FxRateSource[]) {
    this.#sources = sources;
  }

  async fetchClose(base: string, quote: string, date: string): Promise<FxRatePoint | null> {
    for (const source of this.#sources) {
      const result = await source.fetchClose(base, quote, date);
      if (result !== null) {
        return result;
      }
    }
    return null;
  }
}
