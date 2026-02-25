import { describe, it, expect } from 'vitest';
import { MemoryStorage } from '../storage/memory.js';
import { MemoryMarketDataStore, NullMarketDataStore } from '../market-data/store.js';
import { FixedClock } from '../clock.js';
import { FixedIdGenerator } from '../models/id-generator.js';
import { Id } from '../models/id.js';
import { Connection } from '../models/connection.js';
import { Account } from '../models/account.js';
import { Asset } from '../models/asset.js';
import { Transaction, withStandardizedMetadata } from '../models/transaction.js';
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
    spending: { ignore_accounts: [], ignore_connections: [], ignore_tags: [] },
    ignore: { transaction_rules: [] },
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

  it('uses category precedence annotation > standardized metadata > uncategorized', async () => {
    const storage = new MemoryStorage();
    const cfg = makeConfig();

    const acctId = Id.fromString('acct-1');
    const connId = Id.fromString('conn-1');
    await storage.saveAccount(Account.newWith(acctId, new Date('2026-01-01T00:00:00Z'), 'Checking', connId));

    const clock = new FixedClock(new Date('2026-02-05T12:00:00Z'));
    const ids = new FixedIdGenerator([Id.fromString('tx-meta'), Id.fromString('tx-ann')]);
    const txMeta = withStandardizedMetadata(
      Transaction.newWithGenerator(ids, clock, '-10', Asset.currency('USD'), 'Fallback to metadata category'),
      { merchant_category_label: 'Groceries' },
    );
    const txAnn = withStandardizedMetadata(
      Transaction.newWithGenerator(ids, clock, '-20', Asset.currency('USD'), 'Annotation category wins'),
      { merchant_category_label: 'Shopping' },
    );
    await storage.appendTransactions(acctId, [txMeta, txAnn]);
    await storage.appendTransactionAnnotationPatches(acctId, [
      {
        transaction_id: txAnn.id,
        timestamp: clock.now(),
        category: 'Dining',
      },
    ]);

    const out = await spendingReport(storage, new NullMarketDataStore(), cfg, {
      period: 'monthly',
      start: '2026-02-01',
      end: '2026-02-28',
      tz: 'UTC',
      account: 'acct-1',
      status: 'posted',
      direction: 'outflow',
      group_by: 'category',
      lookback_days: 7,
    });

    expect(out.total).toBe('30');
    expect(out.transaction_count).toBe(2);
    expect(out.periods).toHaveLength(1);
    expect(out.periods[0].breakdown).toEqual([
      { key: 'Dining', total: '20', transaction_count: 1 },
      { key: 'Groceries', total: '10', transaction_count: 1 },
    ]);
  });

  it('ignores accounts by configured spending ignore tags for portfolio scope', async () => {
    const storage = new MemoryStorage();
    const cfg = makeConfig({
      spending: { ignore_accounts: [], ignore_connections: [], ignore_tags: ['brokerage'] },
    });

    const connId = Id.fromString('conn-1');
    const cardAcct = Account.newWith(Id.fromString('acct-card'), new Date('2026-01-01T00:00:00Z'), 'Card', connId);
    const brokerageAcct = {
      ...Account.newWith(
        Id.fromString('acct-brokerage'),
        new Date('2026-01-01T00:00:00Z'),
        'Individual',
        connId,
      ),
      tags: ['brokerage'],
    };
    await storage.saveAccount(cardAcct);
    await storage.saveAccount(brokerageAcct);

    const clock = new FixedClock(new Date('2026-02-05T12:00:00Z'));
    const ids = new FixedIdGenerator([Id.fromString('tx-card'), Id.fromString('tx-brokerage')]);
    const txCard = Transaction.newWithGenerator(ids, clock, '-10', Asset.currency('USD'), 'Card spend');
    const txBrokerage = Transaction.newWithGenerator(
      ids,
      clock,
      '-2000',
      Asset.currency('USD'),
      'Brokerage transfer',
    );
    await storage.appendTransactions(cardAcct.id, [txCard]);
    await storage.appendTransactions(brokerageAcct.id, [txBrokerage]);

    const out = await spendingReport(storage, new NullMarketDataStore(), cfg, {
      period: 'monthly',
      start: '2026-02-01',
      end: '2026-02-28',
      tz: 'UTC',
      status: 'posted',
      direction: 'outflow',
      group_by: 'none',
      lookback_days: 7,
    });

    expect(out.total).toBe('10');
    expect(out.transaction_count).toBe(1);
  });

  it('applies global ignore regex rules to spending rows', async () => {
    const storage = new MemoryStorage();
    const cfg = makeConfig({
      ignore: {
        transaction_rules: [
          {
            account_name: '(?i)^Investor Checking$',
            synchronizer: '(?i)^schwab$',
            description: '(?i)credit\\s+crd\\s+(?:e?pay|autopay)',
          },
        ],
      },
    });

    const conn = Connection.new(
      { name: 'Schwab', synchronizer: 'schwab' },
      new FixedIdGenerator([Id.fromString('conn-1')]),
      new FixedClock(new Date('2026-01-01T00:00:00Z')),
    );
    await storage.saveConnection(conn);

    const acctId = Id.fromString('acct-1');
    await storage.saveAccount(
      Account.newWith(acctId, new Date('2026-01-01T00:00:00Z'), 'Investor Checking', Id.fromString('conn-1')),
    );

    const clock = new FixedClock(new Date('2026-02-05T12:00:00Z'));
    const ids = new FixedIdGenerator([Id.fromString('tx-cc'), Id.fromString('tx-rent')]);
    const txCc = Transaction.newWithGenerator(
      ids,
      clock,
      '-120',
      Asset.currency('USD'),
      'ACH CHASE CREDIT CRD EPAY',
    );
    const txRent = Transaction.newWithGenerator(
      ids,
      clock,
      '-2000',
      Asset.currency('USD'),
      'BALLAST WEB PMTS',
    );
    await storage.appendTransactions(acctId, [txCc, txRent]);

    const out = await spendingReport(storage, new NullMarketDataStore(), cfg, {
      period: 'monthly',
      start: '2026-02-01',
      end: '2026-02-28',
      tz: 'UTC',
      account: 'acct-1',
      status: 'posted',
      direction: 'outflow',
      group_by: 'none',
      lookback_days: 7,
    });

    expect(out.total).toBe('2000');
    expect(out.transaction_count).toBe(1);
  });

  it('ignores internal transfer hints in spending rows', async () => {
    const storage = new MemoryStorage();
    const cfg = makeConfig();

    const acctId = Id.fromString('acct-1');
    const connId = Id.fromString('conn-1');
    await storage.saveAccount(
      Account.newWith(acctId, new Date('2026-01-01T00:00:00Z'), 'Sapphire Reserve (6395)', connId),
    );

    const clock = new FixedClock(new Date('2026-02-18T12:00:00Z'));
    const ids = new FixedIdGenerator([Id.fromString('tx-pay'), Id.fromString('tx-food')]);
    const txPayment = withStandardizedMetadata(
      Transaction.newWithGenerator(ids, clock, '-4450.62', Asset.currency('USD'), 'Payment Thank You - Web'),
      { transaction_kind: 'payment', is_internal_transfer_hint: true },
    );
    const txFood = Transaction.newWithGenerator(ids, clock, '-25', Asset.currency('USD'), 'Bay Padel LLC');
    await storage.appendTransactions(acctId, [txPayment, txFood]);

    const out = await spendingReport(storage, new NullMarketDataStore(), cfg, {
      period: 'monthly',
      start: '2026-02-01',
      end: '2026-02-28',
      tz: 'UTC',
      account: 'acct-1',
      status: 'posted',
      direction: 'outflow',
      group_by: 'none',
      lookback_days: 7,
    });

    expect(out.total).toBe('25');
    expect(out.transaction_count).toBe(1);
  });
});
