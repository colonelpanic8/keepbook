/**
 * JSONL-based file-system implementation of MarketDataStore.
 *
 * Port of the Rust `market_data::jsonl_store` module. Stores data as JSONL
 * files organized by directory:
 *
 *   base_path/
 *     prices/
 *       {asset_id}/      e.g. "equity/AAPL" or "crypto/BTC"
 *         {year}.jsonl   e.g. "2024.jsonl"
 *     fx/
 *       {BASE}-{QUOTE}/  e.g. "USD-EUR"
 *         {year}.jsonl
 *     assets/
 *       index.jsonl      one AssetRegistryEntry per line
 */

import { mkdir, readdir, readFile, appendFile } from 'node:fs/promises';
import { join, dirname } from 'node:path';
import { AssetId } from './asset-id.js';
import type { MarketDataStore } from './store.js';
import type {
  PriceKind,
  FxRateKind,
  PricePoint,
  FxRatePoint,
  AssetRegistryEntry,
} from './models.js';
import {
  pricePointToJSON,
  pricePointFromJSON,
  fxRatePointToJSON,
  fxRatePointFromJSON,
  assetRegistryEntryToJSON,
  assetRegistryEntryFromJSON,
} from './models.js';
import type { PricePointJSON, FxRatePointJSON, AssetRegistryEntryJSON } from './models.js';

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/**
 * Sanitize a currency/FX code for use as a directory name.
 * Trims whitespace, replaces non-alphanumeric characters with '_',
 * and uppercases the result.
 */
export function sanitizeCode(value: string): string {
  return value
    .trim()
    .split('')
    .map((c) => (/[a-zA-Z0-9]/.test(c) ? c : '_'))
    .join('')
    .toUpperCase();
}

/** Extract the four-digit year from a "YYYY-MM-DD" date string. */
function yearFromDate(date: string): number {
  return parseInt(date.slice(0, 4), 10);
}

/**
 * Read a JSONL file and deserialize each line.
 * Returns an empty array if the file does not exist (ENOENT).
 */
async function readJsonl<TJson, T>(path: string, deserialize: (json: TJson) => T): Promise<T[]> {
  let content: string;
  try {
    content = await readFile(path, 'utf-8');
  } catch (err: unknown) {
    if ((err as NodeJS.ErrnoException).code === 'ENOENT') {
      return [];
    }
    throw err;
  }

  const items: T[] = [];
  for (const line of content.split('\n')) {
    const trimmed = line.trim();
    if (trimmed === '') {
      continue;
    }
    const json: TJson = JSON.parse(trimmed);
    items.push(deserialize(json));
  }
  return items;
}

/**
 * Append items to a JSONL file, creating parent directories as needed.
 */
async function appendJsonl<TJson, T>(
  path: string,
  items: T[],
  serialize: (item: T) => TJson,
): Promise<void> {
  if (items.length === 0) {
    return;
  }

  await mkdir(dirname(path), { recursive: true });

  const lines = items.map((item) => JSON.stringify(serialize(item)) + '\n').join('');
  await appendFile(path, lines);
}

// ---------------------------------------------------------------------------
// JsonlMarketDataStore
// ---------------------------------------------------------------------------

export class JsonlMarketDataStore implements MarketDataStore {
  private readonly basePath: string;

  constructor(basePath: string) {
    this.basePath = basePath;
  }

  // -- Path helpers ---------------------------------------------------------

  private assetsIndexFile(): string {
    return join(this.basePath, 'assets', 'index.jsonl');
  }

  private pricesDir(assetId: AssetId): string {
    return join(this.basePath, 'prices', assetId.asStr());
  }

  private fxDir(base: string, quote: string): string {
    const pair = `${sanitizeCode(base)}-${sanitizeCode(quote)}`;
    return join(this.basePath, 'fx', pair);
  }

  private priceFile(assetId: AssetId, date: string): string {
    return join(this.pricesDir(assetId), `${yearFromDate(date)}.jsonl`);
  }

  private fxFile(base: string, quote: string, date: string): string {
    return join(this.fxDir(base, quote), `${yearFromDate(date)}.jsonl`);
  }

  // -- Prices ---------------------------------------------------------------

  async get_price(assetId: AssetId, date: string, kind: PriceKind): Promise<PricePoint | null> {
    const path = this.priceFile(assetId, date);
    const prices = await readJsonl<PricePointJSON, PricePoint>(path, pricePointFromJSON);
    return this.selectLatestPrice(prices, date, kind);
  }

  async get_all_prices(assetId: AssetId): Promise<PricePoint[]> {
    const dir = this.pricesDir(assetId);

    let entries: string[];
    try {
      entries = await readdir(dir);
    } catch (err: unknown) {
      if ((err as NodeJS.ErrnoException).code === 'ENOENT') {
        return [];
      }
      throw err;
    }

    const allPrices: PricePoint[] = [];
    for (const entry of entries) {
      if (entry.endsWith('.jsonl')) {
        const filePath = join(dir, entry);
        const prices = await readJsonl<PricePointJSON, PricePoint>(filePath, pricePointFromJSON);
        allPrices.push(...prices);
      }
    }

    // Sort by timestamp for consistent ordering
    allPrices.sort((a, b) => a.timestamp.getTime() - b.timestamp.getTime());
    return allPrices;
  }

  async put_prices(prices: PricePoint[]): Promise<void> {
    if (prices.length === 0) {
      return;
    }

    // Group by (assetId, year)
    const grouped = new Map<string, PricePoint[]>();
    for (const price of prices) {
      const key = `${price.asset_id.asStr()}|${yearFromDate(price.as_of_date)}`;
      const group = grouped.get(key);
      if (group) {
        group.push(price);
      } else {
        grouped.set(key, [price]);
      }
    }

    for (const [key, items] of grouped) {
      const [assetIdStr, yearStr] = key.split('|');
      const assetId = AssetId.fromString(assetIdStr);
      const date = `${yearStr}-01-01`;
      const path = this.priceFile(assetId, date);
      await appendJsonl<PricePointJSON, PricePoint>(path, items, pricePointToJSON);
    }
  }

  // -- FX rates -------------------------------------------------------------

  async get_fx_rate(
    base: string,
    quote: string,
    date: string,
    kind: FxRateKind,
  ): Promise<FxRatePoint | null> {
    const path = this.fxFile(base, quote, date);
    const rates = await readJsonl<FxRatePointJSON, FxRatePoint>(path, fxRatePointFromJSON);
    return this.selectLatestFx(rates, date, kind);
  }

  async get_all_fx_rates(base: string, quote: string): Promise<FxRatePoint[]> {
    const dir = this.fxDir(base, quote);

    let entries: string[];
    try {
      entries = await readdir(dir);
    } catch (err: unknown) {
      if ((err as NodeJS.ErrnoException).code === 'ENOENT') {
        return [];
      }
      throw err;
    }

    const allRates: FxRatePoint[] = [];
    for (const entry of entries) {
      if (entry.endsWith('.jsonl')) {
        const filePath = join(dir, entry);
        const rates = await readJsonl<FxRatePointJSON, FxRatePoint>(filePath, fxRatePointFromJSON);
        allRates.push(...rates);
      }
    }

    // Sort by timestamp for consistent ordering
    allRates.sort((a, b) => a.timestamp.getTime() - b.timestamp.getTime());
    return allRates;
  }

  async put_fx_rates(rates: FxRatePoint[]): Promise<void> {
    if (rates.length === 0) {
      return;
    }

    // Group by (base, quote, year)
    const grouped = new Map<string, FxRatePoint[]>();
    for (const rate of rates) {
      const key = `${rate.base}|${rate.quote}|${yearFromDate(rate.as_of_date)}`;
      const group = grouped.get(key);
      if (group) {
        group.push(rate);
      } else {
        grouped.set(key, [rate]);
      }
    }

    for (const [key, items] of grouped) {
      const [base, quote, yearStr] = key.split('|');
      const date = `${yearStr}-01-01`;
      const path = this.fxFile(base, quote, date);
      await appendJsonl<FxRatePointJSON, FxRatePoint>(path, items, fxRatePointToJSON);
    }
  }

  // -- Asset entries --------------------------------------------------------

  async get_asset_entry(assetId: AssetId): Promise<AssetRegistryEntry | null> {
    const path = this.assetsIndexFile();
    const entries = await readJsonl<AssetRegistryEntryJSON, AssetRegistryEntry>(
      path,
      assetRegistryEntryFromJSON,
    );

    // Reverse search: the last matching entry wins (upsert semantics)
    for (let i = entries.length - 1; i >= 0; i--) {
      if (entries[i].id.equals(assetId)) {
        return entries[i];
      }
    }
    return null;
  }

  async upsert_asset_entry(entry: AssetRegistryEntry): Promise<void> {
    const path = this.assetsIndexFile();
    await appendJsonl<AssetRegistryEntryJSON, AssetRegistryEntry>(
      path,
      [entry],
      assetRegistryEntryToJSON,
    );
  }

  // -- Private selection helpers --------------------------------------------

  private selectLatestPrice(
    prices: PricePoint[],
    date: string,
    kind: PriceKind,
  ): PricePoint | null {
    const filtered = prices.filter((p) => p.as_of_date === date && p.kind === kind);
    if (filtered.length === 0) {
      return null;
    }
    return filtered.reduce((latest, p) =>
      p.timestamp.getTime() > latest.timestamp.getTime() ? p : latest,
    );
  }

  private selectLatestFx(rates: FxRatePoint[], date: string, kind: FxRateKind): FxRatePoint | null {
    const filtered = rates.filter((r) => r.as_of_date === date && r.kind === kind);
    if (filtered.length === 0) {
      return null;
    }
    return filtered.reduce((latest, r) =>
      r.timestamp.getTime() > latest.timestamp.getTime() ? r : latest,
    );
  }
}
