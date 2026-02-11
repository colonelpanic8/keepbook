/**
 * AssetId: a human-readable, path-safe identifier for an Asset.
 *
 * Port of the Rust `AssetId` struct. The inner string is built from the
 * asset's normalized form with each segment sanitized so it is safe to use
 * as a file-system path component or map key.
 */

import { Asset, AssetType } from '../models/asset.js';

// ---------------------------------------------------------------------------
// Segment helpers
// ---------------------------------------------------------------------------

/**
 * Sanitize a single path segment:
 * - Trim whitespace
 * - Replace '/', '\\', '\0' with '-'
 * - If empty, ".", or ".." after sanitization, return "_"
 */
export function sanitizeSegment(value: string): string {
  const sanitized = value
    .trim()
    .split('')
    .map((c) => (c === '/' || c === '\\' || c === '\0' ? '-' : c))
    .join('');

  if (sanitized === '' || sanitized === '.' || sanitized === '..') {
    return '_';
  }
  return sanitized;
}

/** Sanitize and uppercase a segment. */
export function normalizeUpperSegment(value: string): string {
  return sanitizeSegment(value).toUpperCase();
}

/** Sanitize and lowercase a segment. */
export function normalizeLowerSegment(value: string): string {
  return sanitizeSegment(value).toLowerCase();
}

// ---------------------------------------------------------------------------
// AssetId class
// ---------------------------------------------------------------------------

export class AssetId {
  readonly #value: string;

  private constructor(value: string) {
    this.#value = value;
  }

  /**
   * Build an `AssetId` from an `Asset`.
   *
   * The asset is first normalized (uppercased fields, empty optionals
   * become undefined) and then each segment is sanitized to be path-safe.
   */
  static fromAsset(asset: AssetType): AssetId {
    const normalized = Asset.normalized(asset);

    let id: string;
    switch (normalized.type) {
      case 'currency':
        id = `currency/${normalizeUpperSegment(normalized.iso_code)}`;
        break;
      case 'equity':
        if (normalized.exchange !== undefined) {
          id = `equity/${normalizeUpperSegment(normalized.ticker)}/${normalizeUpperSegment(normalized.exchange)}`;
        } else {
          id = `equity/${normalizeUpperSegment(normalized.ticker)}`;
        }
        break;
      case 'crypto':
        if (normalized.network !== undefined) {
          id = `crypto/${normalizeUpperSegment(normalized.symbol)}/${normalizeLowerSegment(normalized.network)}`;
        } else {
          id = `crypto/${normalizeUpperSegment(normalized.symbol)}`;
        }
        break;
    }

    return new AssetId(id);
  }

  /**
   * Create an `AssetId` from a raw string (e.g. for deserialization).
   * No normalization or sanitization is applied.
   */
  static fromString(value: string): AssetId {
    return new AssetId(value);
  }

  /** Return the inner id string. */
  asStr(): string {
    return this.#value;
  }

  /** String coercion (`String(id)`, template literals). */
  toString(): string {
    return this.#value;
  }

  /**
   * JSON serialization support.
   * `JSON.stringify(assetId)` will produce a plain JSON string.
   */
  toJSON(): string {
    return this.#value;
  }

  /** Value equality based on inner string. */
  equals(other: AssetId): boolean {
    return this.#value === other.#value;
  }
}
