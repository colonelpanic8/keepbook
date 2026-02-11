import { describe, it, expect } from 'vitest';
import { MemoryStorage } from '../storage/memory.js';
import { NullMarketDataStore, MemoryMarketDataStore } from '../market-data/store.js';
import { FixedClock } from '../clock.js';
import { FixedIdGenerator } from '../models/id-generator.js';
import { Id } from '../models/id.js';
import { Connection } from '../models/connection.js';
import { Account } from '../models/account.js';
import { BalanceSnapshot, AssetBalance } from '../models/balance.js';
import { Asset } from '../models/asset.js';
import { AssetId } from '../market-data/asset-id.js';
import type { ResolvedConfig } from '../config.js';
import type { PortfolioSnapshot, AssetSummary } from '../portfolio/models.js';
import { serializeSnapshot, portfolioSnapshot } from './portfolio.js';
import { formatChronoSerde } from './format.js';

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function makeIdGen(...ids: string[]): FixedIdGenerator {
  return new FixedIdGenerator(ids.map(s => Id.fromString(s)));
}

function makeClock(iso: string): FixedClock {
  return new FixedClock(new Date(iso));
}

function makeConfig(overrides?: Partial<ResolvedConfig>): ResolvedConfig {
  return {
    data_dir: '/tmp/test',
    reporting_currency: 'USD',
    refresh: {
      balance_staleness: 14 * 86400000,
      price_staleness: 86400000,
    },
    git: { auto_commit: false },
    ...overrides,
  };
}

/**
 * Set up storage with a connection, an account, and a balance snapshot.
 */
async function setupStorageWithBalance(
  clock: FixedClock,
  balanceTimestamp: string,
  balances: { asset: ReturnType<typeof Asset.currency>; amount: string }[],
): Promise<{ storage: MemoryStorage; accountId: Id; connectionId: Id }> {
  const storage = new MemoryStorage();
  const connIdGen = makeIdGen('conn-1');
  const conn = Connection.new(
    { name: 'Test Bank', synchronizer: 'manual' },
    connIdGen,
    clock,
  );
  await storage.saveConnection(conn);

  const acctIdGen = makeIdGen('acct-1');
  const acct = Account.newWithGenerator(
    acctIdGen,
    clock,
    'Checking',
    Id.fromString('conn-1'),
  );
  await storage.saveAccount(acct);

  const snapshot = BalanceSnapshot.new(
    new Date(balanceTimestamp),
    balances.map(b => AssetBalance.new(b.asset, b.amount)),
  );
  await storage.appendBalanceSnapshot(acct.id, snapshot);

  return {
    storage,
    accountId: acct.id,
    connectionId: Id.fromString('conn-1'),
  };
}

// ---------------------------------------------------------------------------
// serializeSnapshot
// ---------------------------------------------------------------------------

describe('serializeSnapshot', () => {
  it('converts a minimal snapshot (no by_asset/by_account)', () => {
    const snapshot: PortfolioSnapshot = {
      as_of_date: '2024-06-01',
      currency: 'USD',
      total_value: '0',
    };
    const result = serializeSnapshot(snapshot);
    expect(result).toEqual({
      as_of_date: '2024-06-01',
      currency: 'USD',
      total_value: '0',
    });
    // by_asset and by_account should be absent
    const json = JSON.stringify(result);
    expect(json).not.toContain('by_asset');
    expect(json).not.toContain('by_account');
  });

  it('includes by_asset and by_account when present (even if empty)', () => {
    const snapshot: PortfolioSnapshot = {
      as_of_date: '2024-06-01',
      currency: 'USD',
      total_value: '0',
      by_asset: [],
      by_account: [],
    };
    const result = serializeSnapshot(snapshot);
    expect(result).toEqual({
      as_of_date: '2024-06-01',
      currency: 'USD',
      total_value: '0',
      by_asset: [],
      by_account: [],
    });
  });

  it('formats price_timestamp with formatChronoSerde (Z suffix, no .000)', () => {
    const ts = new Date('2024-06-15T10:30:00Z');
    const snapshot: PortfolioSnapshot = {
      as_of_date: '2024-06-15',
      currency: 'USD',
      total_value: '1000',
      by_asset: [
        {
          asset: Asset.equity('AAPL'),
          total_amount: '10',
          amount_date: '2024-06-15',
          price: '150',
          price_date: '2024-06-15',
          price_timestamp: ts,
          value_in_base: '1500',
        },
      ],
    };
    const result = serializeSnapshot(snapshot) as Record<string, unknown>;
    const byAsset = result.by_asset as Record<string, unknown>[];
    expect(byAsset[0].price_timestamp).toBe('2024-06-15T10:30:00Z');
  });

  it('formats price_timestamp with subseconds correctly', () => {
    const ts = new Date('2024-06-15T10:30:00.456Z');
    const snapshot: PortfolioSnapshot = {
      as_of_date: '2024-06-15',
      currency: 'USD',
      total_value: '1000',
      by_asset: [
        {
          asset: Asset.equity('AAPL'),
          total_amount: '10',
          amount_date: '2024-06-15',
          price: '150',
          price_date: '2024-06-15',
          price_timestamp: ts,
          value_in_base: '1500',
        },
      ],
    };
    const result = serializeSnapshot(snapshot) as Record<string, unknown>;
    const byAsset = result.by_asset as Record<string, unknown>[];
    expect(byAsset[0].price_timestamp).toBe(
      '2024-06-15T10:30:00.456000000Z',
    );
  });

  it('omits undefined optional fields from asset summaries', () => {
    const summary: AssetSummary = {
      asset: Asset.currency('USD'),
      total_amount: '100',
      amount_date: '2024-06-15',
      // price, price_date, price_timestamp, fx_rate, fx_date, value_in_base, holdings all undefined
    };
    const snapshot: PortfolioSnapshot = {
      as_of_date: '2024-06-15',
      currency: 'USD',
      total_value: '100',
      by_asset: [summary],
    };
    const result = serializeSnapshot(snapshot);
    const json = JSON.stringify(result);
    expect(json).not.toContain('"price"');
    expect(json).not.toContain('"price_date"');
    expect(json).not.toContain('"price_timestamp"');
    expect(json).not.toContain('"fx_rate"');
    expect(json).not.toContain('"fx_date"');
    expect(json).not.toContain('"holdings"');
  });

  it('includes value_in_base when defined', () => {
    const summary: AssetSummary = {
      asset: Asset.currency('USD'),
      total_amount: '100',
      amount_date: '2024-06-15',
      value_in_base: '100',
    };
    const snapshot: PortfolioSnapshot = {
      as_of_date: '2024-06-15',
      currency: 'USD',
      total_value: '100',
      by_asset: [summary],
    };
    const result = serializeSnapshot(snapshot) as Record<string, unknown>;
    const byAsset = result.by_asset as Record<string, unknown>[];
    expect(byAsset[0].value_in_base).toBe('100');
  });

  it('omits value_in_base from account summaries when undefined', () => {
    const snapshot: PortfolioSnapshot = {
      as_of_date: '2024-06-15',
      currency: 'USD',
      total_value: '0',
      by_account: [
        {
          account_id: 'acct-1',
          account_name: 'Test',
          connection_name: 'Bank',
        },
      ],
    };
    const result = serializeSnapshot(snapshot);
    const json = JSON.stringify(result);
    expect(json).not.toContain('value_in_base');
  });

  it('serializes holdings detail', () => {
    const summary: AssetSummary = {
      asset: Asset.currency('USD'),
      total_amount: '200',
      amount_date: '2024-06-15',
      value_in_base: '200',
      holdings: [
        {
          account_id: 'acct-1',
          account_name: 'Checking',
          amount: '100',
          balance_date: '2024-06-15',
        },
        {
          account_id: 'acct-2',
          account_name: 'Savings',
          amount: '100',
          balance_date: '2024-06-14',
        },
      ],
    };
    const snapshot: PortfolioSnapshot = {
      as_of_date: '2024-06-15',
      currency: 'USD',
      total_value: '200',
      by_asset: [summary],
    };
    const result = serializeSnapshot(snapshot) as Record<string, unknown>;
    const byAsset = result.by_asset as Record<string, unknown>[];
    const holdings = byAsset[0].holdings as Record<string, unknown>[];
    expect(holdings).toHaveLength(2);
    expect(holdings[0]).toEqual({
      account_id: 'acct-1',
      account_name: 'Checking',
      amount: '100',
      balance_date: '2024-06-15',
    });
  });
});

// ---------------------------------------------------------------------------
// portfolioSnapshot - empty storage
// ---------------------------------------------------------------------------

describe('portfolioSnapshot', () => {
  it('returns empty snapshot for empty storage with grouping "both"', async () => {
    const storage = new MemoryStorage();
    const store = new NullMarketDataStore();
    const config = makeConfig();
    const clock = makeClock('2024-06-15T12:00:00Z');

    const result = await portfolioSnapshot(
      storage,
      store,
      config,
      {},
      clock,
    );

    expect(result).toEqual({
      as_of_date: '2024-06-15',
      currency: 'USD',
      total_value: '0',
      by_asset: [],
      by_account: [],
    });
  });

  // -------------------------------------------------------------------------
  // Single USD balance
  // -------------------------------------------------------------------------

  it('computes correct snapshot for single USD balance', async () => {
    const clock = makeClock('2024-06-15T12:00:00Z');
    const { storage } = await setupStorageWithBalance(
      clock,
      '2024-06-14T10:00:00Z',
      [{ asset: Asset.currency('USD'), amount: '100.50' }],
    );
    const store = new NullMarketDataStore();
    const config = makeConfig();

    const result = (await portfolioSnapshot(
      storage,
      store,
      config,
      {},
      clock,
    )) as Record<string, unknown>;

    expect(result.as_of_date).toBe('2024-06-15');
    expect(result.currency).toBe('USD');
    expect(result.total_value).toBe('100.5');

    const byAsset = result.by_asset as Record<string, unknown>[];
    expect(byAsset).toHaveLength(1);
    expect(byAsset[0].asset).toEqual({ type: 'currency', iso_code: 'USD' });
    expect(byAsset[0].total_amount).toBe('100.5');
    expect(byAsset[0].value_in_base).toBe('100.5');

    // Currency in same reporting currency should NOT have price/price_date/price_timestamp/fx_rate/fx_date
    const json = JSON.stringify(byAsset[0]);
    expect(json).not.toContain('"price"');
    expect(json).not.toContain('"price_date"');
    expect(json).not.toContain('"price_timestamp"');
    expect(json).not.toContain('"fx_rate"');
    expect(json).not.toContain('"fx_date"');
  });

  // -------------------------------------------------------------------------
  // Grouping by asset only
  // -------------------------------------------------------------------------

  it('groupBy "asset" omits by_account', async () => {
    const clock = makeClock('2024-06-15T12:00:00Z');
    const { storage } = await setupStorageWithBalance(
      clock,
      '2024-06-14T10:00:00Z',
      [{ asset: Asset.currency('USD'), amount: '50' }],
    );
    const store = new NullMarketDataStore();
    const config = makeConfig();

    const result = await portfolioSnapshot(
      storage,
      store,
      config,
      { groupBy: 'asset' },
      clock,
    );

    const obj = result as Record<string, unknown>;
    expect(obj.by_asset).toBeDefined();
    const json = JSON.stringify(result);
    expect(json).not.toContain('by_account');
  });

  // -------------------------------------------------------------------------
  // Grouping by account only
  // -------------------------------------------------------------------------

  it('groupBy "account" omits by_asset', async () => {
    const clock = makeClock('2024-06-15T12:00:00Z');
    const { storage } = await setupStorageWithBalance(
      clock,
      '2024-06-14T10:00:00Z',
      [{ asset: Asset.currency('USD'), amount: '50' }],
    );
    const store = new NullMarketDataStore();
    const config = makeConfig();

    const result = await portfolioSnapshot(
      storage,
      store,
      config,
      { groupBy: 'account' },
      clock,
    );

    const obj = result as Record<string, unknown>;
    expect(obj.by_account).toBeDefined();
    const json = JSON.stringify(result);
    expect(json).not.toContain('by_asset');
  });

  // -------------------------------------------------------------------------
  // Default currency from config
  // -------------------------------------------------------------------------

  it('uses config.reporting_currency as default currency', async () => {
    const storage = new MemoryStorage();
    const store = new NullMarketDataStore();
    const config = makeConfig({ reporting_currency: 'EUR' });
    const clock = makeClock('2024-06-15T12:00:00Z');

    const result = (await portfolioSnapshot(
      storage,
      store,
      config,
      {},
      clock,
    )) as Record<string, unknown>;

    expect(result.currency).toBe('EUR');
  });

  // -------------------------------------------------------------------------
  // Explicit currency overrides config
  // -------------------------------------------------------------------------

  it('explicit currency option overrides config', async () => {
    const storage = new MemoryStorage();
    const store = new NullMarketDataStore();
    const config = makeConfig({ reporting_currency: 'USD' });
    const clock = makeClock('2024-06-15T12:00:00Z');

    const result = (await portfolioSnapshot(
      storage,
      store,
      config,
      { currency: 'GBP' },
      clock,
    )) as Record<string, unknown>;

    expect(result.currency).toBe('GBP');
  });

  // -------------------------------------------------------------------------
  // Default date from clock
  // -------------------------------------------------------------------------

  it('uses clock.today() as default date', async () => {
    const storage = new MemoryStorage();
    const store = new NullMarketDataStore();
    const config = makeConfig();
    const clock = makeClock('2024-12-25T15:00:00Z');

    const result = (await portfolioSnapshot(
      storage,
      store,
      config,
      {},
      clock,
    )) as Record<string, unknown>;

    expect(result.as_of_date).toBe('2024-12-25');
  });

  // -------------------------------------------------------------------------
  // Explicit date overrides clock
  // -------------------------------------------------------------------------

  it('explicit date option overrides clock', async () => {
    const storage = new MemoryStorage();
    const store = new NullMarketDataStore();
    const config = makeConfig();
    const clock = makeClock('2024-06-15T12:00:00Z');

    const result = (await portfolioSnapshot(
      storage,
      store,
      config,
      { date: '2024-01-01' },
      clock,
    )) as Record<string, unknown>;

    expect(result.as_of_date).toBe('2024-01-01');
  });

  // -------------------------------------------------------------------------
  // price_timestamp formatted correctly
  // -------------------------------------------------------------------------

  it('formats price_timestamp with Z suffix via formatChronoSerde', async () => {
    const clock = makeClock('2024-06-15T12:00:00Z');

    // Set up an account with a crypto balance that requires a price lookup
    const storage = new MemoryStorage();
    const connIdGen = makeIdGen('conn-1');
    const conn = Connection.new(
      { name: 'Exchange', synchronizer: 'manual' },
      connIdGen,
      clock,
    );
    await storage.saveConnection(conn);

    const acctIdGen = makeIdGen('acct-1');
    const acct = Account.newWithGenerator(
      acctIdGen,
      clock,
      'Crypto',
      Id.fromString('conn-1'),
    );
    await storage.saveAccount(acct);

    const snapshot = BalanceSnapshot.new(
      new Date('2024-06-14T10:00:00Z'),
      [AssetBalance.new(Asset.crypto('BTC'), '1')],
    );
    await storage.appendBalanceSnapshot(acct.id, snapshot);

    // Set up market data store with a BTC price
    const mdStore = new MemoryMarketDataStore();
    const btcAsset = Asset.normalized(Asset.crypto('BTC'));
    const btcAssetId = AssetId.fromAsset(btcAsset);
    const priceTimestamp = new Date('2024-06-15T09:00:00Z');
    await mdStore.put_prices([
      {
        asset_id: btcAssetId,
        as_of_date: '2024-06-15',
        timestamp: priceTimestamp,
        price: '65000',
        quote_currency: 'USD',
        kind: 'close',
        source: 'test',
      },
    ]);

    const config = makeConfig();
    const result = (await portfolioSnapshot(
      storage,
      mdStore,
      config,
      {},
      clock,
    )) as Record<string, unknown>;

    const byAsset = result.by_asset as Record<string, unknown>[];
    const btcEntry = byAsset.find(
      (a: Record<string, unknown>) =>
        (a.asset as Record<string, unknown>).type === 'crypto',
    );
    expect(btcEntry).toBeDefined();
    expect(btcEntry!.price_timestamp).toBe('2024-06-15T09:00:00Z');
    expect(btcEntry!.price).toBe('65000');
    expect(btcEntry!.price_date).toBe('2024-06-15');
  });

  // -------------------------------------------------------------------------
  // Undefined fields truly absent from JSON.stringify
  // -------------------------------------------------------------------------

  it('undefined fields are absent from JSON.stringify output', async () => {
    const clock = makeClock('2024-06-15T12:00:00Z');
    const { storage } = await setupStorageWithBalance(
      clock,
      '2024-06-14T10:00:00Z',
      [{ asset: Asset.currency('USD'), amount: '100' }],
    );
    const store = new NullMarketDataStore();
    const config = makeConfig();

    const result = await portfolioSnapshot(
      storage,
      store,
      config,
      {},
      clock,
    );

    const json = JSON.stringify(result);
    const parsed = JSON.parse(json);

    // For a USD->USD balance: no price, no fx fields
    const usdAsset = parsed.by_asset[0];
    expect(usdAsset).toBeDefined();
    expect('price' in usdAsset).toBe(false);
    expect('price_date' in usdAsset).toBe(false);
    expect('price_timestamp' in usdAsset).toBe(false);
    expect('fx_rate' in usdAsset).toBe(false);
    expect('fx_date' in usdAsset).toBe(false);
    expect('holdings' in usdAsset).toBe(false);
  });

  // -------------------------------------------------------------------------
  // JSON round-trip preserves structure
  // -------------------------------------------------------------------------

  it('JSON round-trip preserves the serialized structure', async () => {
    const clock = makeClock('2024-06-15T12:00:00Z');
    const { storage } = await setupStorageWithBalance(
      clock,
      '2024-06-14T10:00:00Z',
      [{ asset: Asset.currency('USD'), amount: '250.75' }],
    );
    const store = new NullMarketDataStore();
    const config = makeConfig();

    const result = await portfolioSnapshot(
      storage,
      store,
      config,
      {},
      clock,
    );

    const json = JSON.stringify(result);
    const parsed = JSON.parse(json);

    expect(parsed.as_of_date).toBe('2024-06-15');
    expect(parsed.currency).toBe('USD');
    expect(parsed.total_value).toBe('250.75');
    expect(parsed.by_asset).toHaveLength(1);
    expect(parsed.by_asset[0].asset).toEqual({
      type: 'currency',
      iso_code: 'USD',
    });
    expect(parsed.by_asset[0].total_amount).toBe('250.75');
    expect(parsed.by_asset[0].value_in_base).toBe('250.75');
  });

  // -------------------------------------------------------------------------
  // Unknown groupBy defaults to "both"
  // -------------------------------------------------------------------------

  it('unknown groupBy defaults to "both"', async () => {
    const storage = new MemoryStorage();
    const store = new NullMarketDataStore();
    const config = makeConfig();
    const clock = makeClock('2024-06-15T12:00:00Z');

    const result = (await portfolioSnapshot(
      storage,
      store,
      config,
      { groupBy: 'invalid' },
      clock,
    )) as Record<string, unknown>;

    expect(result.by_asset).toBeDefined();
    expect(result.by_account).toBeDefined();
  });

  // -------------------------------------------------------------------------
  // Empty arrays are included (not omitted)
  // -------------------------------------------------------------------------

  it('empty by_asset and by_account arrays are included for grouping "both"', async () => {
    const storage = new MemoryStorage();
    const store = new NullMarketDataStore();
    const config = makeConfig();
    const clock = makeClock('2024-06-15T12:00:00Z');

    const result = await portfolioSnapshot(
      storage,
      store,
      config,
      { groupBy: 'both' },
      clock,
    );

    const json = JSON.stringify(result);
    const parsed = JSON.parse(json);
    expect(Array.isArray(parsed.by_asset)).toBe(true);
    expect(Array.isArray(parsed.by_account)).toBe(true);
    expect(parsed.by_asset).toEqual([]);
    expect(parsed.by_account).toEqual([]);
  });

  // -------------------------------------------------------------------------
  // Trailing zeros stripped from total_value
  // -------------------------------------------------------------------------

  it('strips trailing zeros from total_value', async () => {
    const clock = makeClock('2024-06-15T12:00:00Z');
    const { storage } = await setupStorageWithBalance(
      clock,
      '2024-06-14T10:00:00Z',
      [{ asset: Asset.currency('USD'), amount: '100.50' }],
    );
    const store = new NullMarketDataStore();
    const config = makeConfig();

    const result = (await portfolioSnapshot(
      storage,
      store,
      config,
      {},
      clock,
    )) as Record<string, unknown>;

    // "100.50" -> "100.5" (trailing zero stripped by Decimal normalize)
    expect(result.total_value).toBe('100.5');
  });

  // -------------------------------------------------------------------------
  // Account summary in output
  // -------------------------------------------------------------------------

  it('includes account summary with value_in_base for grouping "both"', async () => {
    const clock = makeClock('2024-06-15T12:00:00Z');
    const { storage } = await setupStorageWithBalance(
      clock,
      '2024-06-14T10:00:00Z',
      [{ asset: Asset.currency('USD'), amount: '500' }],
    );
    const store = new NullMarketDataStore();
    const config = makeConfig();

    const result = (await portfolioSnapshot(
      storage,
      store,
      config,
      {},
      clock,
    )) as Record<string, unknown>;

    const byAccount = result.by_account as Record<string, unknown>[];
    expect(byAccount).toHaveLength(1);
    expect(byAccount[0].account_id).toBe('acct-1');
    expect(byAccount[0].account_name).toBe('Checking');
    expect(byAccount[0].connection_name).toBe('Test Bank');
    expect(byAccount[0].value_in_base).toBe('500');
  });
});
