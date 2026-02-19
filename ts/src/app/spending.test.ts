import { describe, it, expect } from 'vitest';
import { MemoryStorage } from '../storage/memory.js';
import { MemoryMarketDataStore, NullMarketDataStore } from '../market-data/store.js';
import { FixedClock } from '../clock.js';
import { FixedIdGenerator } from '../models/id-generator.js';
import { Id } from '../models/id.js';
import { Account } from '../models/account.js';
import { Asset } from '../models/asset.js';
import { Transaction } from '../models/transaction.js';
import { AssetId } from '../market-data/asset-id.js';
import type { ResolvedConfig } from '../config.js';
import { spendingReport } from './spending.js';

function makeConfig(overrides?: Partial<ResolvedConfig>): ResolvedConfig {
  return {
    data_dir: '/tmp/test',
    reporting_currency: 'USD',
    display: {},
    refresh: {
      balance_staleness: 14 * 86400000,
      price_staleness: 86400000,
    },
    tray: { history_points: 8, spending_windows_days: [7, 30, 90] },
    git: { auto_commit: false, auto_push: false, merge_master_before_command: false },
    ...overrides,
  };
}

describe('spendingReport', () => {
  it('buckets by timezone-local date', async () => {
    const storage = new MemoryStorage();
    const cfg = makeConfig();

    const acctId = Id.fromString('acct-1');
    const connId = Id.fromString('conn-1');
    await storage.saveAccount(Account.newWith(acctId, new Date('2026-01-01T00:00:00Z'), 'Checking', connId));

    // 2026-02-01T02:30Z is 2026-01-31 in America/New_York (winter).
    const clock = new FixedClock(new Date('2026-02-01T02:30:00Z'));
    const ids = new FixedIdGenerator([Id.fromString('tx-1')]);
    const tx = Transaction.newWithGenerator(ids, clock, '-10', Asset.currency('USD'), 'Test');
    await storage.appendTransactions(acctId, [tx]);

    const out = await spendingReport(storage, new NullMarketDataStore(), cfg, {
      period: 'daily',
      start: '2026-01-30',
      end: '2026-02-02',
      tz: 'America/New_York',
      account: 'acct-1',
      status: 'posted',
      direction: 'outflow',
      group_by: 'none',
      lookback_days: 7,
    });

    expect(out.periods).toHaveLength(1);
    expect(out.periods[0].start_date).toBe('2026-01-31');
    expect(out.periods[0].total).toBe('10');
  });

  it('converts FX and equity prices', async () => {
    const storage = new MemoryStorage();
    const cfg = makeConfig();

    const acctId = Id.fromString('acct-1');
    const connId = Id.fromString('conn-1');
    await storage.saveAccount(Account.newWith(acctId, new Date('2026-01-01T00:00:00Z'), 'Checking', connId));

    const clock = new FixedClock(new Date('2026-02-05T12:00:00Z'));
    const ids = new FixedIdGenerator([Id.fromString('tx-eur'), Id.fromString('tx-eq')]);
    const txEur = Transaction.newWithGenerator(ids, clock, '-10', Asset.currency('EUR'), 'EUR debit');
    const txEq = Transaction.newWithGenerator(ids, clock, '-2', Asset.equity('AAPL'), 'Buy AAPL shares');
    await storage.appendTransactions(acctId, [txEur, txEq]);

    const store = new MemoryMarketDataStore();
    await store.put_fx_rates([
      {
        base: 'EUR',
        quote: 'USD',
        as_of_date: '2026-02-05',
        timestamp: new Date('2026-02-05T12:00:00Z'),
        rate: '1.2',
        kind: 'close',
        source: 'test',
      },
    ]);
    await store.put_prices([
      {
        asset_id: AssetId.fromAsset(Asset.equity('AAPL')),
        as_of_date: '2026-02-05',
        timestamp: new Date('2026-02-05T12:00:00Z'),
        price: '50',
        quote_currency: 'USD',
        kind: 'close',
        source: 'test',
      },
    ]);

    const out = await spendingReport(storage, store, cfg, {
      period: 'monthly',
      start: '2026-02-01',
      end: '2026-02-28',
      tz: 'UTC',
      account: 'acct-1',
      status: 'posted',
      direction: 'outflow',
      group_by: 'none',
      lookback_days: 7,
      include_noncurrency: true,
    });

    expect(out.total).toBe('112');
    expect(out.transaction_count).toBe(2);
  });
});
