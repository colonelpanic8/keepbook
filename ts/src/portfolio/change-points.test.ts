import { describe, it, expect } from 'vitest';
import {
  ChangePointCollector,
  dateToTimestamp,
  filterByGranularity,
  filterByDateRange,
  collectChangePoints,
  type ChangePoint,
} from './change-points.js';
import { Id } from '../models/id.js';
import { Asset } from '../models/asset.js';
import { AssetId } from '../market-data/asset-id.js';
import { MemoryStorage } from '../storage/memory.js';
import { MemoryMarketDataStore } from '../market-data/store.js';
import { Account } from '../models/account.js';
import { BalanceSnapshot, AssetBalance } from '../models/balance.js';
import { ConnectionState, type ConnectionConfig } from '../models/connection.js';

// ---------------------------------------------------------------------------
// ChangePointCollector
// ---------------------------------------------------------------------------

describe('ChangePointCollector', () => {
  it('tracks balance changes', () => {
    const collector = new ChangePointCollector();
    const ts = new Date('2024-06-15T10:00:00Z');
    const accountId = Id.fromString('acct-1');
    const asset = Asset.currency('USD');

    collector.addBalanceChange(ts, accountId, asset);

    expect(collector.length).toBe(1);
    expect(collector.isEmpty).toBe(false);

    const points = collector.intoChangePoints();
    expect(points).toHaveLength(1);
    expect(points[0].timestamp).toEqual(ts);
    expect(points[0].triggers).toHaveLength(1);
    expect(points[0].triggers[0]).toEqual({
      type: 'balance',
      account_id: accountId,
      asset,
    });
  });

  it('merges triggers at the same timestamp', () => {
    const collector = new ChangePointCollector();
    const ts = new Date('2024-06-15T10:00:00Z');
    const accountId = Id.fromString('acct-1');
    const usd = Asset.currency('USD');
    const btc = Asset.crypto('BTC');

    collector.addBalanceChange(ts, accountId, usd);
    collector.addBalanceChange(ts, accountId, btc);

    const points = collector.intoChangePoints();
    expect(points).toHaveLength(1);
    expect(points[0].triggers).toHaveLength(2);
    expect(points[0].triggers[0]).toEqual({
      type: 'balance',
      account_id: accountId,
      asset: usd,
    });
    expect(points[0].triggers[1]).toEqual({
      type: 'balance',
      account_id: accountId,
      asset: btc,
    });
  });

  it('sorts change points by timestamp', () => {
    const collector = new ChangePointCollector();
    const accountId = Id.fromString('acct-1');
    const usd = Asset.currency('USD');

    const ts3 = new Date('2024-06-17T10:00:00Z');
    const ts1 = new Date('2024-06-15T10:00:00Z');
    const ts2 = new Date('2024-06-16T10:00:00Z');

    // Add out of order
    collector.addBalanceChange(ts3, accountId, usd);
    collector.addBalanceChange(ts1, accountId, usd);
    collector.addBalanceChange(ts2, accountId, usd);

    const points = collector.intoChangePoints();
    expect(points).toHaveLength(3);
    expect(points[0].timestamp).toEqual(ts1);
    expect(points[1].timestamp).toEqual(ts2);
    expect(points[2].timestamp).toEqual(ts3);
  });

  it('tracks held assets from balance changes', () => {
    const collector = new ChangePointCollector();
    const accountId = Id.fromString('acct-1');
    const ts = new Date('2024-06-15T10:00:00Z');

    const usd = Asset.currency('USD');
    const aapl = Asset.equity('AAPL', 'NASDAQ');
    const btc = Asset.crypto('BTC');

    collector.addBalanceChange(ts, accountId, usd);
    collector.addBalanceChange(ts, accountId, aapl);
    collector.addBalanceChange(ts, accountId, btc);

    const held = collector.heldAssets();
    expect(held.size).toBe(3);

    // Check that the expected asset ids are present
    expect(held.has(AssetId.fromAsset(usd).asStr())).toBe(true);
    expect(held.has(AssetId.fromAsset(aapl).asStr())).toBe(true);
    expect(held.has(AssetId.fromAsset(btc).asStr())).toBe(true);
  });

  it('tracks price changes', () => {
    const collector = new ChangePointCollector();
    const ts = new Date('2024-06-15T10:00:00Z');
    const assetId = AssetId.fromAsset(Asset.equity('AAPL'));

    collector.addPriceChange(ts, assetId);

    const points = collector.intoChangePoints();
    expect(points).toHaveLength(1);
    expect(points[0].triggers[0]).toEqual({
      type: 'price',
      asset_id: assetId,
    });
  });

  it('tracks fx changes', () => {
    const collector = new ChangePointCollector();
    const ts = new Date('2024-06-15T10:00:00Z');

    collector.addFxChange(ts, 'EUR', 'USD');

    const points = collector.intoChangePoints();
    expect(points).toHaveLength(1);
    expect(points[0].triggers[0]).toEqual({
      type: 'fx_rate',
      base: 'EUR',
      quote: 'USD',
    });
  });

  it('isEmpty returns true for new collector', () => {
    const collector = new ChangePointCollector();
    expect(collector.isEmpty).toBe(true);
    expect(collector.length).toBe(0);
  });
});

// ---------------------------------------------------------------------------
// dateToTimestamp
// ---------------------------------------------------------------------------

describe('dateToTimestamp', () => {
  it('creates Date at end of day (23:59:59 UTC)', () => {
    const result = dateToTimestamp('2024-06-15');
    expect(result.getUTCFullYear()).toBe(2024);
    expect(result.getUTCMonth()).toBe(5); // 0-indexed
    expect(result.getUTCDate()).toBe(15);
    expect(result.getUTCHours()).toBe(23);
    expect(result.getUTCMinutes()).toBe(59);
    expect(result.getUTCSeconds()).toBe(59);
  });
});

// ---------------------------------------------------------------------------
// filterByGranularity
// ---------------------------------------------------------------------------

describe('filterByGranularity', () => {
  // Helper: create a simple change point
  function cp(dateStr: string): ChangePoint {
    return {
      timestamp: new Date(dateStr),
      triggers: [{ type: 'balance', account_id: Id.fromString('a'), asset: Asset.currency('USD') }],
    };
  }

  it('full granularity returns as-is', () => {
    const points = [
      cp('2024-06-15T10:00:00Z'),
      cp('2024-06-15T11:00:00Z'),
      cp('2024-06-15T12:00:00Z'),
    ];
    const result = filterByGranularity(points, 'full', 'first');
    expect(result).toHaveLength(3);
    expect(result).toEqual(points);
  });

  it('daily granularity with last strategy keeps last per day', () => {
    const points = [
      cp('2024-06-15T08:00:00Z'),
      cp('2024-06-15T16:00:00Z'),
      cp('2024-06-16T09:00:00Z'),
    ];
    const result = filterByGranularity(points, 'daily', 'last');
    expect(result).toHaveLength(2);
    // Last of June 15
    expect(result[0].timestamp).toEqual(new Date('2024-06-15T16:00:00Z'));
    // Only one on June 16
    expect(result[1].timestamp).toEqual(new Date('2024-06-16T09:00:00Z'));
  });

  it('daily granularity with first strategy keeps first per day', () => {
    const points = [
      cp('2024-06-15T08:00:00Z'),
      cp('2024-06-15T16:00:00Z'),
      cp('2024-06-16T09:00:00Z'),
    ];
    const result = filterByGranularity(points, 'daily', 'first');
    expect(result).toHaveLength(2);
    // First of June 15
    expect(result[0].timestamp).toEqual(new Date('2024-06-15T08:00:00Z'));
    // Only one on June 16
    expect(result[1].timestamp).toEqual(new Date('2024-06-16T09:00:00Z'));
  });

  it('hourly granularity buckets by hour', () => {
    const points = [
      cp('2024-06-15T10:05:00Z'),
      cp('2024-06-15T10:30:00Z'),
      cp('2024-06-15T11:15:00Z'),
    ];
    const result = filterByGranularity(points, 'hourly', 'first');
    expect(result).toHaveLength(2);
    expect(result[0].timestamp).toEqual(new Date('2024-06-15T10:05:00Z'));
    expect(result[1].timestamp).toEqual(new Date('2024-06-15T11:15:00Z'));
  });

  it('weekly granularity buckets by week', () => {
    const points = [
      cp('2024-06-10T10:00:00Z'), // Mon of week
      cp('2024-06-12T10:00:00Z'), // Wed same week
      cp('2024-06-17T10:00:00Z'), // Mon next week
    ];
    const result = filterByGranularity(points, 'weekly', 'last');
    expect(result).toHaveLength(2);
    // Last of first week
    expect(result[0].timestamp).toEqual(new Date('2024-06-12T10:00:00Z'));
    // Only one in second week
    expect(result[1].timestamp).toEqual(new Date('2024-06-17T10:00:00Z'));
  });

  it('monthly granularity buckets by year-month', () => {
    const points = [
      cp('2024-06-01T10:00:00Z'),
      cp('2024-06-15T10:00:00Z'),
      cp('2024-07-01T10:00:00Z'),
    ];
    const result = filterByGranularity(points, 'monthly', 'last');
    expect(result).toHaveLength(2);
    // Last of June
    expect(result[0].timestamp).toEqual(new Date('2024-06-15T10:00:00Z'));
    // Only one in July
    expect(result[1].timestamp).toEqual(new Date('2024-07-01T10:00:00Z'));
  });

  it('yearly granularity buckets by year', () => {
    const points = [
      cp('2024-03-15T10:00:00Z'),
      cp('2024-11-20T10:00:00Z'),
      cp('2025-01-05T10:00:00Z'),
    ];
    const result = filterByGranularity(points, 'yearly', 'first');
    expect(result).toHaveLength(2);
    // First of 2024
    expect(result[0].timestamp).toEqual(new Date('2024-03-15T10:00:00Z'));
    // First of 2025
    expect(result[1].timestamp).toEqual(new Date('2025-01-05T10:00:00Z'));
  });

  it('custom granularity with zero duration returns as-is', () => {
    const points = [cp('2024-06-15T10:00:00Z'), cp('2024-06-15T11:00:00Z')];
    const result = filterByGranularity(points, { custom_ms: 0 }, 'first');
    expect(result).toHaveLength(2);
    expect(result).toEqual(points);
  });

  it('custom granularity with negative duration returns as-is', () => {
    const points = [cp('2024-06-15T10:00:00Z'), cp('2024-06-15T11:00:00Z')];
    const result = filterByGranularity(points, { custom_ms: -100 }, 'first');
    expect(result).toHaveLength(2);
    expect(result).toEqual(points);
  });

  it('custom granularity with positive duration buckets correctly', () => {
    // 2 hour buckets = 7200000 ms
    const points = [
      cp('2024-06-15T10:00:00Z'),
      cp('2024-06-15T11:00:00Z'),
      cp('2024-06-15T12:00:00Z'),
      cp('2024-06-15T13:00:00Z'),
    ];
    const result = filterByGranularity(points, { custom_ms: 7200000 }, 'first');
    // 10:00 and 11:00 are in same 2h bucket, 12:00 and 13:00 in next
    expect(result).toHaveLength(2);
    expect(result[0].timestamp).toEqual(new Date('2024-06-15T10:00:00Z'));
    expect(result[1].timestamp).toEqual(new Date('2024-06-15T12:00:00Z'));
  });

  it('empty input returns empty', () => {
    const result = filterByGranularity([], 'daily', 'first');
    expect(result).toHaveLength(0);
  });
});

// ---------------------------------------------------------------------------
// filterByDateRange
// ---------------------------------------------------------------------------

describe('filterByDateRange', () => {
  function cp(dateStr: string): ChangePoint {
    return {
      timestamp: new Date(dateStr),
      triggers: [{ type: 'balance', account_id: Id.fromString('a'), asset: Asset.currency('USD') }],
    };
  }

  it('filters by start and end date', () => {
    const points = [
      cp('2024-06-14T10:00:00Z'),
      cp('2024-06-15T10:00:00Z'),
      cp('2024-06-16T10:00:00Z'),
      cp('2024-06-17T10:00:00Z'),
    ];
    const result = filterByDateRange(points, '2024-06-15', '2024-06-16');
    expect(result).toHaveLength(2);
    expect(result[0].timestamp).toEqual(new Date('2024-06-15T10:00:00Z'));
    expect(result[1].timestamp).toEqual(new Date('2024-06-16T10:00:00Z'));
  });

  it('filters with only start date', () => {
    const points = [
      cp('2024-06-14T10:00:00Z'),
      cp('2024-06-15T10:00:00Z'),
      cp('2024-06-16T10:00:00Z'),
    ];
    const result = filterByDateRange(points, '2024-06-15');
    expect(result).toHaveLength(2);
    expect(result[0].timestamp).toEqual(new Date('2024-06-15T10:00:00Z'));
    expect(result[1].timestamp).toEqual(new Date('2024-06-16T10:00:00Z'));
  });

  it('filters with only end date', () => {
    const points = [
      cp('2024-06-14T10:00:00Z'),
      cp('2024-06-15T10:00:00Z'),
      cp('2024-06-16T10:00:00Z'),
    ];
    const result = filterByDateRange(points, undefined, '2024-06-15');
    expect(result).toHaveLength(2);
    expect(result[0].timestamp).toEqual(new Date('2024-06-14T10:00:00Z'));
    expect(result[1].timestamp).toEqual(new Date('2024-06-15T10:00:00Z'));
  });

  it('no start or end returns all', () => {
    const points = [cp('2024-06-14T10:00:00Z'), cp('2024-06-15T10:00:00Z')];
    const result = filterByDateRange(points);
    expect(result).toHaveLength(2);
  });
});

// ---------------------------------------------------------------------------
// collectChangePoints
// ---------------------------------------------------------------------------

describe('collectChangePoints', () => {
  it('collects balance change points from storage', async () => {
    const storage = new MemoryStorage();
    const marketData = new MemoryMarketDataStore();

    // Set up a connection and account
    const connId = Id.fromString('conn-1');
    const conn = {
      config: { name: 'Test Connection', synchronizer: 'test' } as ConnectionConfig,
      state: ConnectionState.newWith(connId, new Date('2024-01-01T00:00:00Z')),
    };
    await storage.saveConnection(conn);

    const accountId = Id.fromString('acct-1');
    const account = Account.newWith(
      accountId,
      new Date('2024-01-01T00:00:00Z'),
      'Checking',
      connId,
    );
    await storage.saveAccount(account);

    // Add balance snapshots
    const snap1 = BalanceSnapshot.new(new Date('2024-06-15T10:00:00Z'), [
      AssetBalance.new(Asset.currency('USD'), '1000'),
    ]);
    const snap2 = BalanceSnapshot.new(new Date('2024-06-16T10:00:00Z'), [
      AssetBalance.new(Asset.currency('USD'), '1100'),
      AssetBalance.new(Asset.equity('AAPL'), '10'),
    ]);
    await storage.appendBalanceSnapshot(accountId, snap1);
    await storage.appendBalanceSnapshot(accountId, snap2);

    const points = await collectChangePoints(storage, marketData, {});
    // Should have change points for each snapshot
    expect(points.length).toBeGreaterThanOrEqual(2);

    // Verify sorted by timestamp
    for (let i = 1; i < points.length; i++) {
      expect(points[i].timestamp.getTime()).toBeGreaterThanOrEqual(
        points[i - 1].timestamp.getTime(),
      );
    }

    // First snapshot has 1 balance (USD), second has 2 (USD + AAPL)
    const firstPoint = points.find((p) => p.timestamp.getTime() === snap1.timestamp.getTime());
    expect(firstPoint).toBeDefined();
    expect(firstPoint!.triggers.some((t) => t.type === 'balance')).toBe(true);
  });

  it('collects with specific account ids', async () => {
    const storage = new MemoryStorage();
    const marketData = new MemoryMarketDataStore();

    const connId = Id.fromString('conn-1');
    const conn = {
      config: { name: 'Test Connection', synchronizer: 'test' } as ConnectionConfig,
      state: ConnectionState.newWith(connId, new Date('2024-01-01T00:00:00Z')),
    };
    await storage.saveConnection(conn);

    const acctId1 = Id.fromString('acct-1');
    const acctId2 = Id.fromString('acct-2');
    const acct1 = Account.newWith(acctId1, new Date('2024-01-01T00:00:00Z'), 'Checking', connId);
    const acct2 = Account.newWith(acctId2, new Date('2024-01-01T00:00:00Z'), 'Savings', connId);
    await storage.saveAccount(acct1);
    await storage.saveAccount(acct2);

    await storage.appendBalanceSnapshot(
      acctId1,
      BalanceSnapshot.new(new Date('2024-06-15T10:00:00Z'), [
        AssetBalance.new(Asset.currency('USD'), '1000'),
      ]),
    );
    await storage.appendBalanceSnapshot(
      acctId2,
      BalanceSnapshot.new(new Date('2024-06-16T10:00:00Z'), [
        AssetBalance.new(Asset.currency('EUR'), '500'),
      ]),
    );

    // Only request acct-1
    const points = await collectChangePoints(storage, marketData, {
      accountIds: [acctId1],
    });

    // Should only have the balance change from acct-1
    expect(points).toHaveLength(1);
    expect(points[0].timestamp).toEqual(new Date('2024-06-15T10:00:00Z'));
  });

  it('excludes accounts marked exclude_from_portfolio', async () => {
    const storage = new MemoryStorage();
    const marketData = new MemoryMarketDataStore();

    const connId = Id.fromString('conn-1');
    const conn = {
      config: { name: 'Test Connection', synchronizer: 'test' } as ConnectionConfig,
      state: ConnectionState.newWith(connId, new Date('2024-01-01T00:00:00Z')),
    };
    await storage.saveConnection(conn);

    const includedId = Id.fromString('acct-1');
    const excludedId = Id.fromString('acct-2');
    await storage.saveAccount(
      Account.newWith(includedId, new Date('2024-01-01T00:00:00Z'), 'Checking', connId),
    );
    await storage.saveAccount(
      Account.newWith(excludedId, new Date('2024-01-01T00:00:00Z'), 'Mortgage', connId),
    );
    await storage.saveAccountConfig(excludedId, { exclude_from_portfolio: true });

    await storage.appendBalanceSnapshot(
      includedId,
      BalanceSnapshot.new(new Date('2024-06-15T10:00:00Z'), [
        AssetBalance.new(Asset.currency('USD'), '1000'),
      ]),
    );
    await storage.appendBalanceSnapshot(
      excludedId,
      BalanceSnapshot.new(new Date('2024-06-16T10:00:00Z'), [
        AssetBalance.new(Asset.currency('USD'), '-500'),
      ]),
    );

    const points = await collectChangePoints(storage, marketData, {});
    expect(points).toHaveLength(1);
    expect(points[0].timestamp).toEqual(new Date('2024-06-15T10:00:00Z'));
  });

  it('includes price changes when includePrices is true', async () => {
    const storage = new MemoryStorage();
    const marketData = new MemoryMarketDataStore();

    const connId = Id.fromString('conn-1');
    const conn = {
      config: { name: 'Test Connection', synchronizer: 'test' } as ConnectionConfig,
      state: ConnectionState.newWith(connId, new Date('2024-01-01T00:00:00Z')),
    };
    await storage.saveConnection(conn);

    const accountId = Id.fromString('acct-1');
    const account = Account.newWith(
      accountId,
      new Date('2024-01-01T00:00:00Z'),
      'Brokerage',
      connId,
    );
    await storage.saveAccount(account);

    const aaplAsset = Asset.equity('AAPL');
    const aaplId = AssetId.fromAsset(aaplAsset);

    // Add a balance snapshot holding AAPL
    await storage.appendBalanceSnapshot(
      accountId,
      BalanceSnapshot.new(new Date('2024-06-15T10:00:00Z'), [AssetBalance.new(aaplAsset, '10')]),
    );

    // Add price data for AAPL
    await marketData.put_prices([
      {
        asset_id: aaplId,
        as_of_date: '2024-06-15',
        timestamp: new Date('2024-06-15T16:00:00Z'),
        price: '195.00',
        quote_currency: 'USD',
        kind: 'close',
        source: 'test',
      },
      {
        asset_id: aaplId,
        as_of_date: '2024-06-16',
        timestamp: new Date('2024-06-16T16:00:00Z'),
        price: '196.50',
        quote_currency: 'USD',
        kind: 'close',
        source: 'test',
      },
    ]);

    const points = await collectChangePoints(storage, marketData, {
      includePrices: true,
    });

    // Should have balance change + price changes
    const priceTriggers = points.flatMap((p) => p.triggers.filter((t) => t.type === 'price'));
    expect(priceTriggers.length).toBeGreaterThanOrEqual(1);
  });

  it('orders same-timestamp price triggers by asset id for deterministic output', async () => {
    const storage = new MemoryStorage();
    const marketData = new MemoryMarketDataStore();

    const connId = Id.fromString('conn-1');
    await storage.saveConnection({
      config: { name: 'Test Connection', synchronizer: 'test' } as ConnectionConfig,
      state: ConnectionState.newWith(connId, new Date('2024-01-01T00:00:00Z')),
    });

    const accountId = Id.fromString('acct-1');
    await storage.saveAccount(
      Account.newWith(accountId, new Date('2024-01-01T00:00:00Z'), 'Brokerage', connId),
    );

    const vxus = Asset.equity('VXUS');
    const googl = Asset.equity('GOOGL');
    const vxusId = AssetId.fromAsset(vxus);
    const googlId = AssetId.fromAsset(googl);

    // Intentionally add holdings in reverse lexical order to verify deterministic sorting.
    await storage.appendBalanceSnapshot(
      accountId,
      BalanceSnapshot.new(new Date('2024-06-15T10:00:00Z'), [
        AssetBalance.new(vxus, '1'),
        AssetBalance.new(googl, '1'),
      ]),
    );

    await marketData.put_prices([
      {
        asset_id: vxusId,
        as_of_date: '2024-06-15',
        timestamp: new Date('2024-06-15T16:00:00Z'),
        price: '60',
        quote_currency: 'USD',
        kind: 'close',
        source: 'test',
      },
      {
        asset_id: googlId,
        as_of_date: '2024-06-15',
        timestamp: new Date('2024-06-15T16:00:00Z'),
        price: '170',
        quote_currency: 'USD',
        kind: 'close',
        source: 'test',
      },
    ]);

    const points = await collectChangePoints(storage, marketData, { includePrices: true });
    const pricePoint = points.find(
      (p) =>
        p.timestamp.getTime() === new Date('2024-06-15T23:59:59Z').getTime() &&
        p.triggers.some((t) => t.type === 'price'),
    );

    expect(pricePoint).toBeDefined();
    const ids = pricePoint!.triggers
      .filter((t) => t.type === 'price')
      .map((t) => (t.type === 'price' ? t.asset_id.asStr() : ''));
    expect(ids).toEqual(['equity/GOOGL', 'equity/VXUS']);
  });
});
