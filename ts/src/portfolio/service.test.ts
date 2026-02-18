import { describe, it, expect, beforeEach } from 'vitest';

import { Asset, type AssetType } from '../models/asset.js';
import { Id } from '../models/id.js';
import { Account, type AccountType } from '../models/account.js';
import { AssetBalance, BalanceSnapshot } from '../models/balance.js';
import { Connection, type ConnectionType } from '../models/connection.js';
import { MemoryStorage } from '../storage/memory.js';
import { MemoryMarketDataStore } from '../market-data/store.js';
import { AssetId } from '../market-data/asset-id.js';
import type { PricePoint, FxRatePoint } from '../market-data/models.js';
import { MarketDataService } from '../market-data/service.js';
import { FixedClock } from '../clock.js';

import { PortfolioService } from './service.js';
import type { PortfolioQuery } from './models.js';

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

/** Create a connection and save it to storage. */
async function createConnection(storage: MemoryStorage, name: string): Promise<ConnectionType> {
  const conn = Connection.new({
    name,
    synchronizer: 'manual',
  });
  await storage.saveConnection(conn);
  return conn;
}

/** Create an account, save it to storage, and add it to the connection's account_ids. */
async function createAccount(
  storage: MemoryStorage,
  name: string,
  connection: ConnectionType,
): Promise<AccountType> {
  const account = Account.newWith(
    Id.new(),
    new Date('2026-01-01T00:00:00Z'),
    name,
    connection.state.id,
  );
  await storage.saveAccount(account);
  return account;
}

/** Add a balance snapshot to an account. */
async function addSnapshot(
  storage: MemoryStorage,
  accountId: Id,
  timestamp: Date,
  balances: Array<{ asset: AssetType; amount: string }>,
): Promise<void> {
  const assetBalances = balances.map((b) => AssetBalance.new(b.asset, b.amount));
  const snapshot = BalanceSnapshot.new(timestamp, assetBalances);
  await storage.appendBalanceSnapshot(accountId, snapshot);
}

/** Create a PricePoint for the store. */
function makePrice(
  asset: AssetType,
  date: string,
  price: string,
  quoteCurrency: string = 'USD',
): PricePoint {
  return {
    asset_id: AssetId.fromAsset(asset),
    as_of_date: date,
    timestamp: new Date(date + 'T21:00:00Z'),
    price,
    quote_currency: quoteCurrency,
    kind: 'close',
    source: 'test',
  };
}

/** Create an FxRatePoint for the store. */
function makeFxRate(base: string, quote: string, date: string, rate: string): FxRatePoint {
  return {
    base,
    quote,
    as_of_date: date,
    timestamp: new Date(date + 'T18:00:00Z'),
    rate,
    kind: 'close',
    source: 'test',
  };
}

/** Build a standard PortfolioQuery. */
function makeQuery(overrides: Partial<PortfolioQuery> = {}): PortfolioQuery {
  return {
    as_of_date: '2026-02-02',
    currency: 'USD',
    grouping: 'both',
    include_detail: false,
    ...overrides,
  };
}

// ---------------------------------------------------------------------------
// Shared fixtures
// ---------------------------------------------------------------------------

let storage: MemoryStorage;
let mdStore: MemoryMarketDataStore;

beforeEach(() => {
  storage = new MemoryStorage();
  mdStore = new MemoryMarketDataStore();
});

function buildService(clock?: FixedClock): PortfolioService {
  const mdService = new MarketDataService(mdStore);
  if (clock !== undefined) {
    mdService.withClock(clock);
  }
  return new PortfolioService(storage, mdService, clock);
}

// ---------------------------------------------------------------------------
// 1. Single currency holding (USD->USD, no price lookup needed)
// ---------------------------------------------------------------------------

describe('PortfolioService', () => {
  it('values a single USD currency holding without price lookup', async () => {
    const conn = await createConnection(storage, 'Test Bank');
    const account = await createAccount(storage, 'Checking', conn);
    await addSnapshot(storage, account.id, new Date('2026-02-01T12:00:00Z'), [
      { asset: Asset.currency('USD'), amount: '1000.00' },
    ]);

    const service = buildService();
    const result = await service.calculate(makeQuery({ currency: 'USD', grouping: 'both' }));

    // Decimal.normalize() strips trailing zeros: "1000.00" -> "1000"
    expect(result.total_value).toBe('1000');
    expect(result.currency).toBe('USD');
    expect(result.as_of_date).toBe('2026-02-02');

    // Asset summary should have no price/fx fields since same currency
    const assetSummary = result.by_asset![0];
    expect(assetSummary.price).toBeUndefined();
    expect(assetSummary.fx_rate).toBeUndefined();
    expect(assetSummary.value_in_base).toBe('1000');
  });

  // ---------------------------------------------------------------------------
  // 2. Equity with price (10 shares AAPL at $200 = $2000)
  // ---------------------------------------------------------------------------

  it('values an equity holding using close price', async () => {
    const conn = await createConnection(storage, 'Broker');
    const account = await createAccount(storage, 'Brokerage', conn);
    await addSnapshot(storage, account.id, new Date('2026-02-01T12:00:00Z'), [
      { asset: Asset.equity('AAPL'), amount: '10' },
    ]);

    // Store AAPL price at $200
    await mdStore.put_prices([makePrice(Asset.equity('AAPL'), '2026-02-02', '200')]);

    const service = buildService();
    const result = await service.calculate(makeQuery({ currency: 'USD', grouping: 'asset' }));

    // 10 * 200 = 2000
    expect(result.total_value).toBe('2000');

    const assetSummary = result.by_asset![0];
    expect(assetSummary.total_amount).toBe('10');
    expect(assetSummary.price).toBe('200');
    expect(assetSummary.value_in_base).toBe('2000');
    // Same quote currency as target, no FX needed
    expect(assetSummary.fx_rate).toBeUndefined();
  });

  // ---------------------------------------------------------------------------
  // 3. Equity with FX conversion (EUR target, needs USD->EUR rate)
  // ---------------------------------------------------------------------------

  it('converts equity value using FX rate when target differs from quote currency', async () => {
    const conn = await createConnection(storage, 'Broker');
    const account = await createAccount(storage, 'Brokerage', conn);
    await addSnapshot(storage, account.id, new Date('2026-02-01T12:00:00Z'), [
      { asset: Asset.equity('AAPL'), amount: '10' },
    ]);

    // AAPL at $200 (quoted in USD)
    await mdStore.put_prices([makePrice(Asset.equity('AAPL'), '2026-02-02', '200', 'USD')]);
    // USD->EUR at 0.91
    await mdStore.put_fx_rates([makeFxRate('USD', 'EUR', '2026-02-02', '0.91')]);

    const service = buildService();
    const result = await service.calculate(makeQuery({ currency: 'EUR', grouping: 'asset' }));

    // 10 * 200 * 0.91 = 1820
    expect(result.total_value).toBe('1820');
    expect(result.currency).toBe('EUR');

    const assetSummary = result.by_asset![0];
    expect(assetSummary.total_amount).toBe('10');
    expect(assetSummary.price).toBe('200');
    expect(assetSummary.fx_rate).toBe('0.91');
    expect(assetSummary.value_in_base).toBe('1820');
  });

  // ---------------------------------------------------------------------------
  // 4. Multiple accounts, same asset are aggregated
  // ---------------------------------------------------------------------------

  it('aggregates the same asset across multiple accounts', async () => {
    const conn = await createConnection(storage, 'Bank');
    const checking = await createAccount(storage, 'Checking', conn);
    const savings = await createAccount(storage, 'Savings', conn);

    await addSnapshot(storage, checking.id, new Date('2026-02-01T12:00:00Z'), [
      { asset: Asset.currency('USD'), amount: '1000' },
    ]);
    await addSnapshot(storage, savings.id, new Date('2026-02-01T14:00:00Z'), [
      { asset: Asset.currency('USD'), amount: '2000' },
    ]);

    const service = buildService();
    const result = await service.calculate(makeQuery({ currency: 'USD', grouping: 'asset' }));

    expect(result.total_value).toBe('3000');

    const byAsset = result.by_asset!;
    expect(byAsset).toHaveLength(1);
    expect(byAsset[0].total_amount).toBe('3000');
  });

  // ---------------------------------------------------------------------------
  // 5. Case-insensitive asset merging (USD and usd merged)
  // ---------------------------------------------------------------------------

  it('merges assets case-insensitively', async () => {
    const conn = await createConnection(storage, 'Bank');
    const account1 = await createAccount(storage, 'Account1', conn);
    const account2 = await createAccount(storage, 'Account2', conn);

    await addSnapshot(storage, account1.id, new Date('2026-02-01T12:00:00Z'), [
      { asset: Asset.currency('USD'), amount: '1000' },
    ]);
    await addSnapshot(storage, account2.id, new Date('2026-02-01T14:00:00Z'), [
      { asset: Asset.currency(' usd '), amount: '2000' },
    ]);

    const service = buildService();
    const result = await service.calculate(makeQuery({ currency: 'USD', grouping: 'asset' }));

    const byAsset = result.by_asset!;
    expect(byAsset).toHaveLength(1);
    expect(byAsset[0].total_amount).toBe('3000');
    // Normalized asset should be uppercase
    expect(byAsset[0].asset).toEqual({ type: 'currency', iso_code: 'USD' });
  });

  // ---------------------------------------------------------------------------
  // 6. Uses latest snapshot before as_of_date (ignores future snapshots)
  // ---------------------------------------------------------------------------

  it('uses latest snapshot before as_of_date and ignores future ones', async () => {
    const conn = await createConnection(storage, 'Test Bank');
    const account = await createAccount(storage, 'Checking', conn);

    // Older snapshot (before as_of_date)
    await addSnapshot(storage, account.id, new Date('2026-02-01T12:00:00Z'), [
      { asset: Asset.currency('USD'), amount: '1000' },
    ]);
    // Newer snapshot (after as_of_date of 2026-02-02)
    await addSnapshot(storage, account.id, new Date('2026-02-03T12:00:00Z'), [
      { asset: Asset.currency('USD'), amount: '2000' },
    ]);

    const service = buildService();
    const result = await service.calculate(
      makeQuery({ as_of_date: '2026-02-02', currency: 'USD', grouping: 'both' }),
    );

    // Should use the older snapshot ($1000), not the future one ($2000)
    expect(result.total_value).toBe('1000');
  });

  // ---------------------------------------------------------------------------
  // 7. Zero backfill policy
  // ---------------------------------------------------------------------------

  it('applies zero backfill policy for accounts with only future snapshots', async () => {
    const conn = await createConnection(storage, 'Test Bank');
    const account = await createAccount(storage, 'Checking', conn);

    // Configure zero backfill
    await storage.saveAccountConfig(account.id, {
      balance_backfill: 'zero',
    });

    // Only a future snapshot
    await addSnapshot(storage, account.id, new Date('2026-02-03T12:00:00Z'), [
      { asset: Asset.currency('USD'), amount: '1000' },
    ]);

    const service = buildService();
    const result = await service.calculate(
      makeQuery({
        as_of_date: '2026-02-01',
        currency: 'USD',
        grouping: 'account',
      }),
    );

    expect(result.total_value).toBe('0');

    const byAccount = result.by_account!;
    expect(byAccount).toHaveLength(1);
    expect(byAccount[0].value_in_base).toBe('0');
  });

  // ---------------------------------------------------------------------------
  // 8. CarryEarliest backfill policy
  // ---------------------------------------------------------------------------

  it('applies carry_earliest backfill policy using earliest future snapshot', async () => {
    const conn = await createConnection(storage, 'Test Bank');
    const account = await createAccount(storage, 'Checking', conn);

    // Configure carry_earliest backfill
    await storage.saveAccountConfig(account.id, {
      balance_backfill: 'carry_earliest',
    });

    // Only a future snapshot
    await addSnapshot(storage, account.id, new Date('2026-02-03T12:00:00Z'), [
      { asset: Asset.currency('USD'), amount: '1000' },
    ]);

    const service = buildService();
    const result = await service.calculate(
      makeQuery({
        as_of_date: '2026-02-01',
        currency: 'USD',
        grouping: 'both',
      }),
    );

    // Should carry the earliest snapshot back
    expect(result.total_value).toBe('1000');
  });

  // ---------------------------------------------------------------------------
  // 9. Detail mode includes holdings per account
  // ---------------------------------------------------------------------------

  it('includes per-account holdings detail when include_detail is true', async () => {
    const conn = await createConnection(storage, 'Bank');
    const checking = await createAccount(storage, 'Checking', conn);
    const savings = await createAccount(storage, 'Savings', conn);

    await addSnapshot(storage, checking.id, new Date('2026-02-01T12:00:00Z'), [
      { asset: Asset.currency('USD'), amount: '1000' },
    ]);
    await addSnapshot(storage, savings.id, new Date('2026-02-01T14:00:00Z'), [
      { asset: Asset.currency('USD'), amount: '2000' },
    ]);

    const service = buildService();
    const result = await service.calculate(
      makeQuery({
        currency: 'USD',
        grouping: 'asset',
        include_detail: true,
      }),
    );

    expect(result.total_value).toBe('3000');

    const byAsset = result.by_asset!;
    expect(byAsset).toHaveLength(1);

    const holdings = byAsset[0].holdings!;
    expect(holdings).toHaveLength(2);

    // Find holdings by name
    const checkingHolding = holdings.find((h) => h.account_name === 'Checking');
    const savingsHolding = holdings.find((h) => h.account_name === 'Savings');
    expect(checkingHolding).toBeDefined();
    expect(savingsHolding).toBeDefined();
    expect(checkingHolding!.amount).toBe('1000');
    expect(savingsHolding!.amount).toBe('2000');
  });

  // ---------------------------------------------------------------------------
  // 10. Grouping: asset only, account only, both
  // ---------------------------------------------------------------------------

  describe('grouping modes', () => {
    async function setupGroupingData(): Promise<void> {
      const conn = await createConnection(storage, 'Test Bank');
      const account = await createAccount(storage, 'Checking', conn);
      await addSnapshot(storage, account.id, new Date('2026-02-01T12:00:00Z'), [
        { asset: Asset.currency('USD'), amount: '1000' },
      ]);
    }

    it('grouping=asset includes by_asset but not by_account', async () => {
      await setupGroupingData();
      const service = buildService();
      const result = await service.calculate(makeQuery({ grouping: 'asset' }));

      expect(result.by_asset).toBeDefined();
      expect(result.by_account).toBeUndefined();
    });

    it('grouping=account includes by_account but not by_asset', async () => {
      await setupGroupingData();
      const service = buildService();
      const result = await service.calculate(makeQuery({ grouping: 'account' }));

      expect(result.by_account).toBeDefined();
      expect(result.by_asset).toBeUndefined();
    });

    it('grouping=both includes by_asset and by_account', async () => {
      await setupGroupingData();
      const service = buildService();
      const result = await service.calculate(makeQuery({ grouping: 'both' }));

      expect(result.by_asset).toBeDefined();
      expect(result.by_account).toBeDefined();
    });
  });

  // ---------------------------------------------------------------------------
  // Additional edge cases
  // ---------------------------------------------------------------------------

  it('handles currency FX conversion (non-equity)', async () => {
    const conn = await createConnection(storage, 'EU Bank');
    const account = await createAccount(storage, 'Euro Account', conn);
    await addSnapshot(storage, account.id, new Date('2026-02-01T12:00:00Z'), [
      { asset: Asset.currency('EUR'), amount: '500' },
    ]);

    // EUR->USD at 1.10
    await mdStore.put_fx_rates([makeFxRate('EUR', 'USD', '2026-02-02', '1.10')]);

    const service = buildService();
    const result = await service.calculate(makeQuery({ currency: 'USD', grouping: 'asset' }));

    // 500 EUR * 1.10 = 550 USD
    expect(result.total_value).toBe('550');

    const assetSummary = result.by_asset![0];
    expect(assetSummary.fx_rate).toBe('1.1');
    expect(assetSummary.value_in_base).toBe('550');
    // Currency assets don't have a price
    expect(assetSummary.price).toBeUndefined();
  });

  it('zero backfill applies to accounts with no snapshots at all', async () => {
    const conn = await createConnection(storage, 'Test Bank');
    const account = await createAccount(storage, 'Empty', conn);

    // Configure zero backfill but add no snapshots
    await storage.saveAccountConfig(account.id, {
      balance_backfill: 'zero',
    });

    const service = buildService();
    const result = await service.calculate(
      makeQuery({
        as_of_date: '2026-02-01',
        currency: 'USD',
        grouping: 'account',
      }),
    );

    expect(result.total_value).toBe('0');
    const byAccount = result.by_account!;
    expect(byAccount).toHaveLength(1);
    expect(byAccount[0].value_in_base).toBe('0');
  });

  it('excludes accounts marked exclude_from_portfolio', async () => {
    const conn = await createConnection(storage, 'Bank');
    const checking = await createAccount(storage, 'Checking', conn);
    const mortgage = await createAccount(storage, 'Mortgage', conn);

    await storage.saveAccountConfig(mortgage.id, {
      exclude_from_portfolio: true,
    });

    await addSnapshot(storage, checking.id, new Date('2026-02-01T12:00:00Z'), [
      { asset: Asset.currency('USD'), amount: '1000' },
    ]);
    await addSnapshot(storage, mortgage.id, new Date('2026-02-01T12:00:00Z'), [
      { asset: Asset.currency('USD'), amount: '-500' },
    ]);

    const service = buildService();
    const result = await service.calculate(makeQuery({ currency: 'USD', grouping: 'both' }));

    expect(result.total_value).toBe('1000');
    expect(result.by_account).toHaveLength(1);
    expect(result.by_account![0].account_name).toBe('Checking');
  });

  it('sorts by_asset by AssetId string', async () => {
    const conn = await createConnection(storage, 'Multi');
    const account = await createAccount(storage, 'Multi-asset', conn);

    await addSnapshot(storage, account.id, new Date('2026-02-01T12:00:00Z'), [
      { asset: Asset.currency('USD'), amount: '100' },
      { asset: Asset.currency('EUR'), amount: '200' },
    ]);

    // EUR->USD conversion
    await mdStore.put_fx_rates([makeFxRate('EUR', 'USD', '2026-02-02', '1.10')]);

    const service = buildService();
    const result = await service.calculate(makeQuery({ currency: 'USD', grouping: 'asset' }));

    const byAsset = result.by_asset!;
    expect(byAsset).toHaveLength(2);
    // currency/EUR < currency/USD (alphabetical)
    expect(byAsset[0].asset).toEqual({ type: 'currency', iso_code: 'EUR' });
    expect(byAsset[1].asset).toEqual({ type: 'currency', iso_code: 'USD' });
  });

  it('sorts by_account by account name', async () => {
    const conn = await createConnection(storage, 'Bank');
    const savings = await createAccount(storage, 'Savings', conn);
    const checking = await createAccount(storage, 'Checking', conn);

    await addSnapshot(storage, savings.id, new Date('2026-02-01T12:00:00Z'), [
      { asset: Asset.currency('USD'), amount: '2000' },
    ]);
    await addSnapshot(storage, checking.id, new Date('2026-02-01T14:00:00Z'), [
      { asset: Asset.currency('USD'), amount: '1000' },
    ]);

    const service = buildService();
    const result = await service.calculate(makeQuery({ currency: 'USD', grouping: 'account' }));

    const byAccount = result.by_account!;
    expect(byAccount).toHaveLength(2);
    // Checking < Savings (alphabetical)
    expect(byAccount[0].account_name).toBe('Checking');
    expect(byAccount[1].account_name).toBe('Savings');
  });

  it('handles missing price gracefully with undefined value_in_base', async () => {
    const conn = await createConnection(storage, 'Broker');
    const account = await createAccount(storage, 'Brokerage', conn);
    await addSnapshot(storage, account.id, new Date('2026-02-01T12:00:00Z'), [
      { asset: Asset.equity('UNKNOWN'), amount: '10' },
    ]);

    // No price stored for UNKNOWN

    const service = buildService();
    const result = await service.calculate(makeQuery({ currency: 'USD', grouping: 'both' }));

    // Total value should be 0 when price is unavailable
    expect(result.total_value).toBe('0');

    const assetSummary = result.by_asset![0];
    expect(assetSummary.value_in_base).toBeUndefined();
    expect(assetSummary.price).toBeUndefined();

    // Account summary should have undefined value when price is missing
    const acctSummary = result.by_account![0];
    expect(acctSummary.value_in_base).toBeUndefined();
  });
});
