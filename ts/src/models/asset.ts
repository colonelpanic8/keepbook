/**
 * Asset types representing financial instruments.
 *
 * Mirrors the Rust `Asset` enum with a discriminated union on the `type` field.
 * Uses snake_case field names to match Rust serde serialization format.
 */

// ---------------------------------------------------------------------------
// Variant interfaces
// ---------------------------------------------------------------------------

export interface CurrencyAsset {
  readonly type: 'currency';
  readonly iso_code: string;
}

export interface EquityAsset {
  readonly type: 'equity';
  readonly ticker: string;
  readonly exchange?: string;
}

export interface CryptoAsset {
  readonly type: 'crypto';
  readonly symbol: string;
  readonly network?: string;
}

export type AssetType = CurrencyAsset | EquityAsset | CryptoAsset;

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/** Uppercase a trimmed string. */
function normalizeUpper(s: string): string {
  return s.trim().toUpperCase();
}

function normalizeCurrencyCode(s: string): string {
  const trimmed = s.trim();
  // Some sources provide ISO 4217 numeric codes (e.g. "840" for USD).
  // Normalize those into alpha codes where we can.
  if (trimmed === '840') return 'USD';
  return trimmed.toUpperCase();
}

/** Uppercase an optional string; returns undefined if empty/whitespace-only. */
function normalizeOptUpper(s: string | undefined): string | undefined {
  if (s === undefined) return undefined;
  const trimmed = s.trim();
  if (trimmed === '') return undefined;
  return trimmed.toUpperCase();
}

/** Lowercase an optional string; returns undefined if empty/whitespace-only. */
function normalizeOptLower(s: string | undefined): string | undefined {
  if (s === undefined) return undefined;
  const trimmed = s.trim();
  if (trimmed === '') return undefined;
  return trimmed.toLowerCase();
}

// ---------------------------------------------------------------------------
// Asset namespace (factory functions + operations)
// ---------------------------------------------------------------------------

export const Asset = {
  /**
   * Create a currency asset.
   * Trims whitespace from iso_code.
   */
  currency(iso_code: string): CurrencyAsset {
    return { type: 'currency', iso_code: iso_code.trim() };
  },

  /**
   * Create an equity asset.
   * Trims whitespace from ticker and optional exchange.
   */
  equity(ticker: string, exchange?: string): EquityAsset {
    const trimmedExchange = exchange !== undefined ? exchange.trim() : undefined;
    const result: EquityAsset = { type: 'equity', ticker: ticker.trim() };
    if (trimmedExchange !== undefined && trimmedExchange !== '') {
      return { ...result, exchange: trimmedExchange };
    }
    return result;
  },

  /**
   * Create a crypto asset.
   * Trims whitespace from symbol and optional network.
   */
  crypto(symbol: string, network?: string): CryptoAsset {
    const trimmedNetwork = network !== undefined ? network.trim() : undefined;
    const result: CryptoAsset = { type: 'crypto', symbol: symbol.trim() };
    if (trimmedNetwork !== undefined && trimmedNetwork !== '') {
      return { ...result, network: trimmedNetwork };
    }
    return result;
  },

  /**
   * Return a normalized copy of an asset.
   *
   * - Currency: iso_code is uppercased and trimmed.
   * - Equity: ticker and exchange are uppercased and trimmed.
   *   Empty/whitespace-only exchange becomes undefined.
   * - Crypto: symbol is uppercased and trimmed.
   *   Network is lowercased and trimmed. Empty/whitespace-only network becomes undefined.
   */
  normalized(asset: AssetType): AssetType {
    switch (asset.type) {
      case 'currency':
        return { type: 'currency', iso_code: normalizeCurrencyCode(asset.iso_code) };
      case 'equity': {
        const exchange = normalizeOptUpper(asset.exchange);
        const result: EquityAsset = { type: 'equity', ticker: normalizeUpper(asset.ticker) };
        if (exchange !== undefined) {
          return { ...result, exchange };
        }
        return result;
      }
      case 'crypto': {
        const network = normalizeOptLower(asset.network);
        const result: CryptoAsset = { type: 'crypto', symbol: normalizeUpper(asset.symbol) };
        if (network !== undefined) {
          return { ...result, network };
        }
        return result;
      }
    }
  },

  /**
   * Case-insensitive equality comparison.
   * Compares normalized forms of both assets.
   */
  equals(a: AssetType, b: AssetType): boolean {
    if (a.type !== b.type) return false;

    const na = Asset.normalized(a);
    const nb = Asset.normalized(b);

    switch (na.type) {
      case 'currency':
        return na.iso_code === (nb as CurrencyAsset).iso_code;
      case 'equity': {
        const nbe = nb as EquityAsset;
        return na.ticker === nbe.ticker && na.exchange === nbe.exchange;
      }
      case 'crypto': {
        const nbc = nb as CryptoAsset;
        return na.symbol === nbc.symbol && na.network === nbc.network;
      }
    }
  },

  /**
   * Produce a string hash key suitable for use as a Map key.
   * Two assets that are equals() will produce the same hash.
   */
  hash(asset: AssetType): string {
    const n = Asset.normalized(asset);
    switch (n.type) {
      case 'currency':
        return `currency:${n.iso_code}`;
      case 'equity':
        return `equity:${n.ticker}:${n.exchange ?? ''}`;
      case 'crypto':
        return `crypto:${n.symbol}:${n.network ?? ''}`;
    }
  },
} as const;
