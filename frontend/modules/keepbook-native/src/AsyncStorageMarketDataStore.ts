/**
 * AsyncStorage-backed implementation of the keepbook MarketDataStore interface.
 *
 * Reads price and FX rate data from AsyncStorage keys like:
 *   keepbook.file.{dataDir}.data/prices/{asset_id}/{year}.jsonl
 *   keepbook.file.{dataDir}.data/fx/{pair}/{year}.jsonl
 *
 * This is a READ-ONLY adapter. All write methods are no-ops.
 */

import AsyncStorage from '@react-native-async-storage/async-storage';
import type { MarketDataStore } from '@keepbook/market-data/store';
import type {
  PriceKind,
  FxRateKind,
  PricePoint,
  FxRatePoint,
  AssetRegistryEntry,
  PricePointJSON,
  FxRatePointJSON,
} from '@keepbook/market-data/models';
import { pricePointFromJSON, fxRatePointFromJSON } from '@keepbook/market-data/models';
import { AssetId } from '@keepbook/market-data/asset-id';

// ---------------------------------------------------------------------------
// AsyncStorageMarketDataStore
// ---------------------------------------------------------------------------

export class AsyncStorageMarketDataStore implements MarketDataStore {
  private dataDir: string;
  private manifestCache: string[] | null = null;

  constructor(dataDir: string) {
    this.dataDir = dataDir;
  }

  /** Invalidate cached manifest. */
  clearCache(): void {
    this.manifestCache = null;
  }

  private fileKey(relativePath: string): string {
    return `keepbook.file.${this.dataDir}.${relativePath}`;
  }

  private manifestKey(): string {
    return `keepbook.manifest.${this.dataDir}`;
  }

  private async getManifest(): Promise<string[]> {
    if (this.manifestCache) return this.manifestCache;
    const raw = await AsyncStorage.getItem(this.manifestKey());
    this.manifestCache = raw ? JSON.parse(raw) : [];
    return this.manifestCache!;
  }

  private async readFile(relativePath: string): Promise<string | null> {
    return AsyncStorage.getItem(this.fileKey(relativePath));
  }

  private parseJsonl<T>(content: string): T[] {
    return content
      .split('\n')
      .filter((line) => line.trim())
      .map((line) => JSON.parse(line) as T);
  }

  // -----------------------------------------------------------------------
  // Prices
  // -----------------------------------------------------------------------

  async get_price(assetId: AssetId, date: string, kind: PriceKind): Promise<PricePoint | null> {
    const all = await this.get_all_prices(assetId);
    const matches = all.filter((p) => p.as_of_date === date && p.kind === kind);
    if (matches.length === 0) return null;

    // Return latest by timestamp
    let latest = matches[0];
    for (let i = 1; i < matches.length; i++) {
      if (matches[i].timestamp.getTime() > latest.timestamp.getTime()) {
        latest = matches[i];
      }
    }
    return latest;
  }

  async get_all_prices(assetId: AssetId): Promise<PricePoint[]> {
    const manifest = await this.getManifest();
    const prefix = `data/prices/${assetId.asStr()}/`;
    const files = manifest.filter(
      (p) => p.startsWith(prefix) && p.endsWith('.jsonl'),
    );

    const results: PricePoint[] = [];
    for (const file of files) {
      const raw = await this.readFile(file);
      if (!raw) continue;

      const jsonItems = this.parseJsonl<PricePointJSON>(raw);
      for (const json of jsonItems) {
        results.push(pricePointFromJSON(json));
      }
    }

    return results;
  }

  async put_prices(_prices: PricePoint[]): Promise<void> {
    // No-op for read-only store
  }

  // -----------------------------------------------------------------------
  // FX Rates
  // -----------------------------------------------------------------------

  async get_fx_rate(
    base: string,
    quote: string,
    date: string,
    kind: FxRateKind,
  ): Promise<FxRatePoint | null> {
    const all = await this.get_all_fx_rates(base, quote);
    const matches = all.filter((r) => r.as_of_date === date && r.kind === kind);
    if (matches.length === 0) return null;

    // Return latest by timestamp
    let latest = matches[0];
    for (let i = 1; i < matches.length; i++) {
      if (matches[i].timestamp.getTime() > latest.timestamp.getTime()) {
        latest = matches[i];
      }
    }
    return latest;
  }

  async get_all_fx_rates(base: string, quote: string): Promise<FxRatePoint[]> {
    const manifest = await this.getManifest();
    const pair = `${base.toUpperCase()}-${quote.toUpperCase()}`;
    const prefix = `data/fx/${pair}/`;
    const files = manifest.filter(
      (p) => p.startsWith(prefix) && p.endsWith('.jsonl'),
    );

    const results: FxRatePoint[] = [];
    for (const file of files) {
      const raw = await this.readFile(file);
      if (!raw) continue;

      const jsonItems = this.parseJsonl<FxRatePointJSON>(raw);
      for (const json of jsonItems) {
        results.push(fxRatePointFromJSON(json));
      }
    }

    return results;
  }

  async put_fx_rates(_rates: FxRatePoint[]): Promise<void> {
    // No-op for read-only store
  }

  // -----------------------------------------------------------------------
  // Asset Registry
  // -----------------------------------------------------------------------

  async get_asset_entry(_assetId: AssetId): Promise<AssetRegistryEntry | null> {
    return null;
  }

  async upsert_asset_entry(_entry: AssetRegistryEntry): Promise<void> {
    // No-op for read-only store
  }
}
