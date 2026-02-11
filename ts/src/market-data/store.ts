/**
 * Market data store interface and implementations.
 *
 * Port of the Rust `market_data::store` module. Defines the async store
 * trait (interface) and provides NullMarketDataStore and MemoryMarketDataStore
 * implementations.
 */

import { AssetId } from './asset-id.js';
import type {
  PriceKind,
  FxRateKind,
  PricePoint,
  FxRatePoint,
  AssetRegistryEntry,
} from './models.js';

// ---------------------------------------------------------------------------
// MarketDataStore interface
// ---------------------------------------------------------------------------

/** Async store for market data: prices, FX rates, and asset registry entries. */
export interface MarketDataStore {
  get_price(assetId: AssetId, date: string, kind: PriceKind): Promise<PricePoint | null>;

  get_all_prices(assetId: AssetId): Promise<PricePoint[]>;

  put_prices(prices: PricePoint[]): Promise<void>;

  get_fx_rate(
    base: string,
    quote: string,
    date: string,
    kind: FxRateKind,
  ): Promise<FxRatePoint | null>;

  get_all_fx_rates(base: string, quote: string): Promise<FxRatePoint[]>;

  put_fx_rates(rates: FxRatePoint[]): Promise<void>;

  get_asset_entry(assetId: AssetId): Promise<AssetRegistryEntry | null>;

  upsert_asset_entry(entry: AssetRegistryEntry): Promise<void>;
}

// ---------------------------------------------------------------------------
// NullMarketDataStore
// ---------------------------------------------------------------------------

/** A no-op store: all getters return null/empty, all puts are no-ops. */
export class NullMarketDataStore implements MarketDataStore {
  async get_price(_assetId: AssetId, _date: string, _kind: PriceKind): Promise<PricePoint | null> {
    return null;
  }

  async get_all_prices(_assetId: AssetId): Promise<PricePoint[]> {
    return [];
  }

  async put_prices(_prices: PricePoint[]): Promise<void> {}

  async get_fx_rate(
    _base: string,
    _quote: string,
    _date: string,
    _kind: FxRateKind,
  ): Promise<FxRatePoint | null> {
    return null;
  }

  async get_all_fx_rates(_base: string, _quote: string): Promise<FxRatePoint[]> {
    return [];
  }

  async put_fx_rates(_rates: FxRatePoint[]): Promise<void> {}

  async get_asset_entry(_assetId: AssetId): Promise<AssetRegistryEntry | null> {
    return null;
  }

  async upsert_asset_entry(_entry: AssetRegistryEntry): Promise<void> {}
}

// ---------------------------------------------------------------------------
// MemoryMarketDataStore
// ---------------------------------------------------------------------------

/**
 * In-memory store backed by Maps.
 *
 * Key schemes:
 * - Prices: `${assetId}|${date}|${kind}`
 * - FX rates: `${BASE}|${QUOTE}|${date}|${kind}` (base/quote normalized to uppercase)
 * - Asset entries: AssetId string
 */
export class MemoryMarketDataStore implements MarketDataStore {
  readonly #prices = new Map<string, PricePoint>();
  readonly #fxRates = new Map<string, FxRatePoint>();
  readonly #assetEntries = new Map<string, AssetRegistryEntry>();

  // -- Prices ---------------------------------------------------------------

  async get_price(assetId: AssetId, date: string, kind: PriceKind): Promise<PricePoint | null> {
    const key = priceKey(assetId.asStr(), date, kind);
    return this.#prices.get(key) ?? null;
  }

  async get_all_prices(assetId: AssetId): Promise<PricePoint[]> {
    const prefix = assetId.asStr();
    const results: PricePoint[] = [];
    for (const [key, point] of this.#prices) {
      if (key.startsWith(prefix + '|')) {
        results.push(point);
      }
    }
    return results;
  }

  async put_prices(prices: PricePoint[]): Promise<void> {
    for (const p of prices) {
      const key = priceKey(p.asset_id.asStr(), p.as_of_date, p.kind);
      this.#prices.set(key, p);
    }
  }

  // -- FX rates -------------------------------------------------------------

  async get_fx_rate(
    base: string,
    quote: string,
    date: string,
    kind: FxRateKind,
  ): Promise<FxRatePoint | null> {
    const key = fxKey(base.toUpperCase(), quote.toUpperCase(), date, kind);
    return this.#fxRates.get(key) ?? null;
  }

  async get_all_fx_rates(base: string, quote: string): Promise<FxRatePoint[]> {
    const prefix = `${base.toUpperCase()}|${quote.toUpperCase()}|`;
    const results: FxRatePoint[] = [];
    for (const [key, point] of this.#fxRates) {
      if (key.startsWith(prefix)) {
        results.push(point);
      }
    }
    return results;
  }

  async put_fx_rates(rates: FxRatePoint[]): Promise<void> {
    for (const r of rates) {
      const normalizedBase = r.base.toUpperCase();
      const normalizedQuote = r.quote.toUpperCase();
      const key = fxKey(normalizedBase, normalizedQuote, r.as_of_date, r.kind);
      // Store with normalized base/quote
      const normalized: FxRatePoint = {
        ...r,
        base: normalizedBase,
        quote: normalizedQuote,
      };
      this.#fxRates.set(key, normalized);
    }
  }

  // -- Asset entries --------------------------------------------------------

  async get_asset_entry(assetId: AssetId): Promise<AssetRegistryEntry | null> {
    return this.#assetEntries.get(assetId.asStr()) ?? null;
  }

  async upsert_asset_entry(entry: AssetRegistryEntry): Promise<void> {
    this.#assetEntries.set(entry.id.asStr(), entry);
  }
}

// ---------------------------------------------------------------------------
// Key helpers
// ---------------------------------------------------------------------------

function priceKey(assetIdStr: string, date: string, kind: PriceKind): string {
  return `${assetIdStr}|${date}|${kind}`;
}

function fxKey(base: string, quote: string, date: string, kind: FxRateKind): string {
  return `${base}|${quote}|${date}|${kind}`;
}
