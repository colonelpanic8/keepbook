/**
 * Market data model types.
 *
 * Port of the Rust `market_data::models` module. Defines the core data
 * structures for price points, FX rate points, and the asset registry.
 */

import { AssetType } from '../models/asset.js';
import { AssetId } from './asset-id.js';

// ---------------------------------------------------------------------------
// Kind enums (string literal unions)
// ---------------------------------------------------------------------------

/** The type of price observation. */
export type PriceKind = 'close' | 'adj_close' | 'quote';

/** The type of FX rate observation. */
export type FxRateKind = 'close';

// ---------------------------------------------------------------------------
// PricePoint
// ---------------------------------------------------------------------------

/** A single price observation for an asset on a given date. */
export interface PricePoint {
  readonly asset_id: AssetId;
  /** Date string in "YYYY-MM-DD" format. */
  readonly as_of_date: string;
  readonly timestamp: Date;
  /** Decimal price as a string to preserve precision. */
  readonly price: string;
  readonly quote_currency: string;
  readonly kind: PriceKind;
  readonly source: string;
}

// ---------------------------------------------------------------------------
// FxRatePoint
// ---------------------------------------------------------------------------

/** A single FX rate observation for a currency pair on a given date. */
export interface FxRatePoint {
  readonly base: string;
  readonly quote: string;
  /** Date string in "YYYY-MM-DD" format. */
  readonly as_of_date: string;
  readonly timestamp: Date;
  /** Decimal rate as a string to preserve precision. */
  readonly rate: string;
  readonly kind: FxRateKind;
  readonly source: string;
}

// ---------------------------------------------------------------------------
// AssetRegistryEntry
// ---------------------------------------------------------------------------

/** Registry entry that links an AssetId to its Asset and provider metadata. */
export interface AssetRegistryEntry {
  readonly id: AssetId;
  readonly asset: AssetType;
  provider_ids: Record<string, string>;
  tz?: string;
}

/** Factory functions for AssetRegistryEntry. */
export const AssetRegistryEntryFactory = {
  /**
   * Create a new AssetRegistryEntry, auto-generating the id from the asset.
   */
  new(asset: AssetType): AssetRegistryEntry {
    return {
      id: AssetId.fromAsset(asset),
      asset,
      provider_ids: {},
    };
  },
} as const;

// ---------------------------------------------------------------------------
// JSON serialization helpers
// ---------------------------------------------------------------------------

/** Plain-object shape of a serialized PricePoint. */
export interface PricePointJSON {
  asset_id: string;
  as_of_date: string;
  timestamp: string;
  price: string;
  quote_currency: string;
  kind: PriceKind;
  source: string;
}

/** Serialize a PricePoint to a plain JSON-safe object. */
export function pricePointToJSON(p: PricePoint): PricePointJSON {
  return {
    asset_id: p.asset_id.toJSON(),
    as_of_date: p.as_of_date,
    timestamp: p.timestamp.toISOString(),
    price: p.price,
    quote_currency: p.quote_currency,
    kind: p.kind,
    source: p.source,
  };
}

/** Deserialize a PricePoint from a plain JSON object. */
export function pricePointFromJSON(json: PricePointJSON): PricePoint {
  return {
    asset_id: AssetId.fromString(json.asset_id),
    as_of_date: json.as_of_date,
    timestamp: new Date(json.timestamp),
    price: json.price,
    quote_currency: json.quote_currency,
    kind: json.kind,
    source: json.source,
  };
}

/** Plain-object shape of a serialized FxRatePoint. */
export interface FxRatePointJSON {
  base: string;
  quote: string;
  as_of_date: string;
  timestamp: string;
  rate: string;
  kind: FxRateKind;
  source: string;
}

/** Serialize an FxRatePoint to a plain JSON-safe object. */
export function fxRatePointToJSON(p: FxRatePoint): FxRatePointJSON {
  return {
    base: p.base,
    quote: p.quote,
    as_of_date: p.as_of_date,
    timestamp: p.timestamp.toISOString(),
    rate: p.rate,
    kind: p.kind,
    source: p.source,
  };
}

/** Deserialize an FxRatePoint from a plain JSON object. */
export function fxRatePointFromJSON(json: FxRatePointJSON): FxRatePoint {
  return {
    base: json.base,
    quote: json.quote,
    as_of_date: json.as_of_date,
    timestamp: new Date(json.timestamp),
    rate: json.rate,
    kind: json.kind,
    source: json.source,
  };
}

/** Plain-object shape of a serialized AssetRegistryEntry. */
export interface AssetRegistryEntryJSON {
  id: string;
  asset: AssetType;
  provider_ids: Record<string, string>;
  tz?: string;
}

/** Serialize an AssetRegistryEntry to a plain JSON-safe object. */
export function assetRegistryEntryToJSON(entry: AssetRegistryEntry): AssetRegistryEntryJSON {
  return {
    id: entry.id.toJSON(),
    asset: entry.asset,
    provider_ids: { ...entry.provider_ids },
    ...(entry.tz !== undefined ? { tz: entry.tz } : {}),
  };
}

/** Deserialize an AssetRegistryEntry from a plain JSON object. */
export function assetRegistryEntryFromJSON(json: AssetRegistryEntryJSON): AssetRegistryEntry {
  return {
    id: AssetId.fromString(json.id),
    asset: json.asset,
    provider_ids: { ...json.provider_ids },
    ...(json.tz !== undefined ? { tz: json.tz } : {}),
  };
}
