/**
 * Integration tests + JSON compatibility verification.
 *
 * End-to-end tests that create known storage state via the mutation commands
 * and verify exact JSON output from the list/portfolio commands. This is the
 * final verification that the TypeScript CLI app layer produces output
 * identical to the Rust CLI.
 */

import { describe, it, expect } from 'vitest';
import { readFileSync } from 'node:fs';
import { MemoryStorage } from '../storage/memory.js';
import { NullMarketDataStore } from '../market-data/store.js';
import { FixedClock } from '../clock.js';
import { FixedIdGenerator } from '../models/id-generator.js';
import { Id } from '../models/id.js';
import { addConnection, addAccount, removeConnection, setBalance } from './mutations.js';
import { listConnections, listAccounts, listBalances, listAll } from './list.js';
import { portfolioSnapshot, portfolioHistory, portfolioChangePoints } from './portfolio.js';
import { configOutput } from './config.js';
import type { ResolvedConfig } from '../config.js';

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function makeIdGen(...ids: string[]): FixedIdGenerator {
  return new FixedIdGenerator(ids.map((s) => Id.fromString(s)));
}

function makeClock(iso: string): FixedClock {
  return new FixedClock(new Date(iso));
}

function makeConfig(overrides?: Partial<ResolvedConfig>): ResolvedConfig {
  return {
    data_dir: '/tmp/test',
    reporting_currency: 'USD',
    display: {},
    refresh: {
      balance_staleness: 14 * 86400000,
      price_staleness: 86400000,
    },
    git: { auto_commit: false, auto_push: false, merge_master_before_command: false },
    ...overrides,
  };
}

function readContractFixture(name: string): unknown {
  // Repo root is 3 levels up from ts/src/app/.
  const url = new URL(`../../../contracts/${name}.json`, import.meta.url);
  return JSON.parse(readFileSync(url, 'utf8')) as unknown;
}

type SetBalanceResultShape = {
  success: boolean;
  balance: {
    amount: string;
    asset: Record<string, unknown>;
    timestamp: string;
  };
};

type SnapshotAssetRow = {
  asset: {
    type: string;
    iso_code?: string;
  };
  total_amount: string;
  value_in_base?: string;
};

type SnapshotAccountRow = {
  account_name: string;
  connection_name: string;
  value_in_base?: string;
};

type SnapshotResultShape = {
  total_value: string;
  currency?: string;
  by_asset?: SnapshotAssetRow[];
  by_account?: SnapshotAccountRow[];
};

// ===========================================================================
// 1. Full workflow test
// ===========================================================================

describe('Integration: full workflow', () => {
  it('add connection, account, balance, then list and portfolio commands', async () => {
    const storage = new MemoryStorage();
    const marketDataStore = new NullMarketDataStore();
    const config = makeConfig();
    const clock = makeClock('2024-06-15T12:00:00Z');

    // --- Step 1: Add a connection ---
    const connResult = await addConnection(storage, 'My Bank', makeIdGen('conn-1'), clock);
    expect(connResult).toEqual({
      success: true,
      connection: {
        id: 'conn-1',
        name: 'My Bank',
        synchronizer: 'manual',
      },
    });

    // --- Step 2: Add an account ---
    const acctResult = await addAccount(
      storage,
      'conn-1',
      'Checking',
      [],
      makeIdGen('acct-1'),
      clock,
    );
    expect(acctResult).toEqual({
      success: true,
      account: {
        id: 'acct-1',
        name: 'Checking',
        connection_id: 'conn-1',
      },
    });

    // --- Step 3: Set balance ---
    const balanceClock = makeClock('2024-06-15T10:00:00Z');
    const balResult = await setBalance(storage, 'acct-1', 'USD', '1500.50', balanceClock);
    const balanceResult = balResult as SetBalanceResultShape;
    expect(balanceResult.success).toBe(true);
    expect(balanceResult.balance.amount).toBe('1500.5');
    expect(balanceResult.balance.asset).toEqual({ type: 'currency', iso_code: 'USD' });

    // --- Step 4: List connections ---
    const connections = await listConnections(storage);
    expect(connections).toHaveLength(1);
    expect(connections[0].account_count).toBe(1);
    expect(connections[0].name).toBe('My Bank');
    expect(connections[0].synchronizer).toBe('manual');
    expect(connections[0].status).toBe('active');

    // --- Step 5: List accounts ---
    const accounts = await listAccounts(storage);
    expect(accounts).toHaveLength(1);
    expect(accounts[0].id).toBe('acct-1');
    expect(accounts[0].name).toBe('Checking');
    expect(accounts[0].connection_id).toBe('conn-1');

    // --- Step 6: List balances ---
    const balances = await listBalances(storage, config);
    expect(balances).toHaveLength(1);
    expect(balances[0].account_id).toBe('acct-1');
    expect(balances[0].amount).toBe('1500.5');
    expect(balances[0].asset).toEqual({ type: 'currency', iso_code: 'USD' });
    // value_in_reporting_currency matches the amount because it is the same currency
    expect(balances[0].value_in_reporting_currency).toBe('1500.5');
    expect(balances[0].reporting_currency).toBe('USD');

    // --- Step 7: List all ---
    const all = await listAll(storage, config);
    expect(all.connections).toHaveLength(1);
    expect(all.accounts).toHaveLength(1);
    expect(all.balances).toHaveLength(1);
    expect(all.price_sources).toEqual([]);

    // --- Step 8: Portfolio snapshot ---
    const snap = (await portfolioSnapshot(storage, marketDataStore, config, {}, clock)) as Record<
      string,
      unknown
    >;
    expect(snap.total_value).toBe('1500.5');
    expect(snap.currency).toBe('USD');
    expect(snap.as_of_date).toBe('2024-06-15');
    const byAsset = snap.by_asset as Record<string, unknown>[];
    expect(byAsset).toHaveLength(1);
    expect(byAsset[0].asset).toEqual({ type: 'currency', iso_code: 'USD' });
    expect(byAsset[0].total_amount).toBe('1500.5');
    expect(byAsset[0].value_in_base).toBe('1500.5');
  });
});

// ===========================================================================
// 2. JSON snapshot tests
// ===========================================================================

describe('Integration: JSON snapshot tests', () => {
  it('listConnections JSON matches expected shape', async () => {
    const storage = new MemoryStorage();
    const clock = makeClock('2024-01-15T10:00:00Z');
    await addConnection(storage, 'Test Bank', makeIdGen('conn-1'), clock);
    await addAccount(storage, 'conn-1', 'Savings', [], makeIdGen('acct-1'), clock);

    const result = await listConnections(storage);
    const json = JSON.stringify(result, null, 2);
    const parsed = JSON.parse(json);

    expect(parsed[0]).toEqual({
      id: 'conn-1',
      name: 'Test Bank',
      synchronizer: 'manual',
      status: 'active',
      account_count: 1,
      last_sync: null,
    });
  });

  it('listAccounts JSON matches expected shape', async () => {
    const storage = new MemoryStorage();
    const clock = makeClock('2024-01-15T10:00:00Z');
    await addConnection(storage, 'Test Bank', makeIdGen('conn-1'), clock);
    await addAccount(storage, 'conn-1', 'Savings', ['tag1'], makeIdGen('acct-1'), clock);

    const result = await listAccounts(storage);
    const json = JSON.stringify(result, null, 2);
    const parsed = JSON.parse(json);

    expect(parsed[0]).toEqual({
      id: 'acct-1',
      name: 'Savings',
      connection_id: 'conn-1',
      tags: ['tag1'],
      active: true,
    });
  });

  it('listBalances JSON matches expected shape with value_in_reporting_currency', async () => {
    const storage = new MemoryStorage();
    const clock = makeClock('2024-01-15T10:00:00Z');
    const config = makeConfig();
    await addConnection(storage, 'Test Bank', makeIdGen('conn-1'), clock);
    await addAccount(storage, 'conn-1', 'Savings', [], makeIdGen('acct-1'), clock);
    await setBalance(storage, 'acct-1', 'USD', '100.5', clock);

    const result = await listBalances(storage, config);
    const json = JSON.stringify(result, null, 2);
    const parsed = JSON.parse(json);

    expect(parsed[0].account_id).toBe('acct-1');
    expect(parsed[0].asset).toEqual({ type: 'currency', iso_code: 'USD' });
    expect(parsed[0].amount).toBe('100.5');
    expect(parsed[0].value_in_reporting_currency).toBe('100.5');
    expect(parsed[0].reporting_currency).toBe('USD');
    expect(parsed[0].timestamp).toBe('2024-01-15T10:00:00+00:00');
  });

  it('portfolioSnapshot JSON matches expected shape', async () => {
    const storage = new MemoryStorage();
    const marketDataStore = new NullMarketDataStore();
    const config = makeConfig();
    const clock = makeClock('2024-06-15T12:00:00Z');

    await addConnection(storage, 'Bank', makeIdGen('conn-1'), clock);
    await addAccount(storage, 'conn-1', 'Checking', [], makeIdGen('acct-1'), clock);
    await setBalance(storage, 'acct-1', 'USD', '250.75', makeClock('2024-06-14T10:00:00Z'));

    const result = await portfolioSnapshot(storage, marketDataStore, config, {}, clock);
    const json = JSON.stringify(result, null, 2);
    const parsed = JSON.parse(json);

    expect(parsed).toEqual({
      as_of_date: '2024-06-15',
      currency: 'USD',
      total_value: '250.75',
      by_asset: [
        {
          asset: { type: 'currency', iso_code: 'USD' },
          total_amount: '250.75',
          amount_date: '2024-06-14',
          value_in_base: '250.75',
        },
      ],
      by_account: [
        {
          account_id: 'acct-1',
          account_name: 'Checking',
          connection_name: 'Bank',
          value_in_base: '250.75',
        },
      ],
    });
  });

  it('configOutput JSON matches expected shape', () => {
    const config = makeConfig();
    const result = configOutput('/home/user/.config/keepbook/config.toml', config);
    const json = JSON.stringify(result, null, 2);
    const parsed = JSON.parse(json);

    expect(parsed).toEqual({
      config_file: '/home/user/.config/keepbook/config.toml',
      data_directory: '/tmp/test',
      git: {
        auto_commit: false,
        auto_push: false,
        merge_master_before_command: false,
      },
    });
  });
});

// ===========================================================================
// 3. Specific compatibility checks
// ===========================================================================

describe('Integration: JSON compatibility checks', () => {
  // -------------------------------------------------------------------------
  // Asset serialization format
  // -------------------------------------------------------------------------

  it('list asset serialization uses snake_case with Rust key order', async () => {
    const storage = new MemoryStorage();
    const clock = makeClock('2024-01-15T10:00:00Z');
    const config = makeConfig();
    await addConnection(storage, 'Bank', makeIdGen('conn-1'), clock);
    await addAccount(storage, 'conn-1', 'Checking', [], makeIdGen('acct-1'), clock);
    await setBalance(storage, 'acct-1', 'USD', '100', clock);

    const balances = await listBalances(storage, config);
    const json = JSON.stringify(balances[0].asset);
    expect(json).toBe('{"iso_code":"USD","type":"currency"}');
    // NOT camelCase
    expect(json).not.toContain('isoCode');
  });

  // -------------------------------------------------------------------------
  // Timestamp formats
  // -------------------------------------------------------------------------

  it('mutation timestamp uses formatRfc3339: +00:00 suffix', async () => {
    const storage = new MemoryStorage();
    const clock = makeClock('2024-01-15T10:00:00Z');
    await addConnection(storage, 'Bank', makeIdGen('conn-1'), clock);
    await addAccount(storage, 'conn-1', 'Checking', [], makeIdGen('acct-1'), clock);
    const result = (await setBalance(
      storage,
      'acct-1',
      'USD',
      '100',
      clock,
    )) as SetBalanceResultShape;
    expect(result.balance.timestamp).toBe('2024-01-15T10:00:00+00:00');
  });

  it('listBalances timestamp uses formatRfc3339: +00:00 suffix', async () => {
    const storage = new MemoryStorage();
    const clock = makeClock('2024-01-15T10:00:00Z');
    const config = makeConfig();
    await addConnection(storage, 'Bank', makeIdGen('conn-1'), clock);
    await addAccount(storage, 'conn-1', 'Checking', [], makeIdGen('acct-1'), clock);
    await setBalance(storage, 'acct-1', 'USD', '100', clock);

    const balances = await listBalances(storage, config);
    expect(balances[0].timestamp).toBe('2024-01-15T10:00:00+00:00');
    // NOT Z suffix
    expect(balances[0].timestamp).not.toMatch(/Z$/);
  });

  it('change point timestamps use formatChronoSerde: Z suffix', async () => {
    const storage = new MemoryStorage();
    const marketDataStore = new NullMarketDataStore();
    const config = makeConfig();
    const clock = makeClock('2024-06-15T12:00:00Z');

    await addConnection(storage, 'Bank', makeIdGen('conn-1'), clock);
    await addAccount(storage, 'conn-1', 'Checking', [], makeIdGen('acct-1'), clock);
    await setBalance(storage, 'acct-1', 'USD', '100', makeClock('2024-06-14T10:00:00Z'));

    const result = await portfolioChangePoints(storage, marketDataStore, config, {}, clock);
    expect(result.points).toHaveLength(1);
    expect(result.points[0].timestamp).toBe('2024-06-14T10:00:00Z');
    // Z suffix, NOT +00:00
    expect(result.points[0].timestamp).not.toContain('+00:00');
  });

  // -------------------------------------------------------------------------
  // Decimal string format
  // -------------------------------------------------------------------------

  it('decimal strings strip trailing zeros: "100.5" not "100.50"', async () => {
    const storage = new MemoryStorage();
    const clock = makeClock('2024-01-15T10:00:00Z');
    await addConnection(storage, 'Bank', makeIdGen('conn-1'), clock);
    await addAccount(storage, 'conn-1', 'Checking', [], makeIdGen('acct-1'), clock);

    const result = (await setBalance(
      storage,
      'acct-1',
      'USD',
      '100.50',
      clock,
    )) as SetBalanceResultShape;
    expect(result.balance.amount).toBe('100.5');
  });

  it('decimal "0" not "0.00"', async () => {
    const storage = new MemoryStorage();
    const clock = makeClock('2024-01-15T10:00:00Z');
    await addConnection(storage, 'Bank', makeIdGen('conn-1'), clock);
    await addAccount(storage, 'conn-1', 'Checking', [], makeIdGen('acct-1'), clock);

    const result = (await setBalance(
      storage,
      'acct-1',
      'USD',
      '0.00',
      clock,
    )) as SetBalanceResultShape;
    expect(result.balance.amount).toBe('0');
  });

  it('portfolio total_value strips trailing zeros', async () => {
    const storage = new MemoryStorage();
    const marketDataStore = new NullMarketDataStore();
    const config = makeConfig();
    const clock = makeClock('2024-06-15T12:00:00Z');

    await addConnection(storage, 'Bank', makeIdGen('conn-1'), clock);
    await addAccount(storage, 'conn-1', 'Checking', [], makeIdGen('acct-1'), clock);
    await setBalance(storage, 'acct-1', 'USD', '100.50', makeClock('2024-06-14T10:00:00Z'));

    const snap = (await portfolioSnapshot(
      storage,
      marketDataStore,
      config,
      {},
      clock,
    )) as SnapshotResultShape;
    expect(snap.total_value).toBe('100.5');
  });

  // -------------------------------------------------------------------------
  // value_in_reporting_currency: null (present, not omitted)
  // -------------------------------------------------------------------------

  it('value_in_reporting_currency is null (present) when currency does not match', async () => {
    const storage = new MemoryStorage();
    const clock = makeClock('2024-01-15T10:00:00Z');
    const config = makeConfig();
    await addConnection(storage, 'Bank', makeIdGen('conn-1'), clock);
    await addAccount(storage, 'conn-1', 'Euro Acct', [], makeIdGen('acct-1'), clock);
    await setBalance(storage, 'acct-1', 'EUR', '500', clock);

    const balances = await listBalances(storage, config);
    const json = JSON.stringify(balances[0]);
    const parsed = JSON.parse(json);

    // Field must be present
    expect('value_in_reporting_currency' in parsed).toBe(true);
    // And its value is null
    expect(parsed.value_in_reporting_currency).toBeNull();
  });

  // -------------------------------------------------------------------------
  // by_asset omitted when grouping = "account"
  // -------------------------------------------------------------------------

  it('by_asset omitted from JSON when groupBy = "account"', async () => {
    const storage = new MemoryStorage();
    const marketDataStore = new NullMarketDataStore();
    const config = makeConfig();
    const clock = makeClock('2024-06-15T12:00:00Z');

    await addConnection(storage, 'Bank', makeIdGen('conn-1'), clock);
    await addAccount(storage, 'conn-1', 'Checking', [], makeIdGen('acct-1'), clock);
    await setBalance(storage, 'acct-1', 'USD', '100', makeClock('2024-06-14T10:00:00Z'));

    const snap = await portfolioSnapshot(
      storage,
      marketDataStore,
      config,
      { groupBy: 'account' },
      clock,
    );
    const json = JSON.stringify(snap);
    const parsed = JSON.parse(json);

    // by_asset should NOT be present at all
    expect('by_asset' in parsed).toBe(false);
    // by_account SHOULD be present
    expect('by_account' in parsed).toBe(true);
  });

  // -------------------------------------------------------------------------
  // by_account omitted when grouping = "asset"
  // -------------------------------------------------------------------------

  it('by_account omitted from JSON when groupBy = "asset"', async () => {
    const storage = new MemoryStorage();
    const marketDataStore = new NullMarketDataStore();
    const config = makeConfig();
    const clock = makeClock('2024-06-15T12:00:00Z');

    const snap = await portfolioSnapshot(
      storage,
      marketDataStore,
      config,
      { groupBy: 'asset' },
      clock,
    );
    const json = JSON.stringify(snap);
    const parsed = JSON.parse(json);

    expect('by_asset' in parsed).toBe(true);
    expect('by_account' in parsed).toBe(false);
  });

  // -------------------------------------------------------------------------
  // change_triggers omitted when triggers list is empty
  // -------------------------------------------------------------------------

  it('change_triggers omitted from history point JSON when triggers list would be empty', () => {
    // HistoryPoint with undefined change_triggers => field absent from JSON
    const point = {
      timestamp: '2024-06-14T10:00:00+00:00',
      date: '2024-06-14',
      total_value: '100',
      change_triggers: undefined,
    };
    const json = JSON.stringify(point);
    expect(json).not.toContain('change_triggers');
  });

  // -------------------------------------------------------------------------
  // start_date: null and end_date: null present in JSON (not omitted)
  // -------------------------------------------------------------------------

  it('start_date and end_date are null (present) in history JSON', async () => {
    const storage = new MemoryStorage();
    const marketDataStore = new NullMarketDataStore();
    const config = makeConfig();
    const clock = makeClock('2024-06-15T12:00:00Z');

    const result = await portfolioHistory(storage, marketDataStore, config, {}, clock);
    const json = JSON.stringify(result);
    const parsed = JSON.parse(json);

    expect('start_date' in parsed).toBe(true);
    expect('end_date' in parsed).toBe(true);
    expect(parsed.start_date).toBeNull();
    expect(parsed.end_date).toBeNull();
  });

  it('start_date and end_date are null (present) in change-points JSON', async () => {
    const storage = new MemoryStorage();
    const marketDataStore = new NullMarketDataStore();
    const config = makeConfig();
    const clock = makeClock('2024-06-15T12:00:00Z');

    const result = await portfolioChangePoints(storage, marketDataStore, config, {}, clock);
    const json = JSON.stringify(result);
    const parsed = JSON.parse(json);

    expect('start_date' in parsed).toBe(true);
    expect('end_date' in parsed).toBe(true);
    expect(parsed.start_date).toBeNull();
    expect(parsed.end_date).toBeNull();
  });

  // -------------------------------------------------------------------------
  // summary field omitted from history when fewer than 2 points
  // -------------------------------------------------------------------------

  it('summary omitted from history JSON when fewer than 2 points', async () => {
    const storage = new MemoryStorage();
    const marketDataStore = new NullMarketDataStore();
    const config = makeConfig();
    const clock = makeClock('2024-06-15T12:00:00Z');

    // Empty storage -> 0 points
    const result = await portfolioHistory(storage, marketDataStore, config, {}, clock);
    const json = JSON.stringify(result);
    const parsed = JSON.parse(json);

    expect('summary' in parsed).toBe(false);
  });

  it('summary omitted from history JSON with exactly 1 point', async () => {
    const storage = new MemoryStorage();
    const marketDataStore = new NullMarketDataStore();
    const config = makeConfig();
    const clock = makeClock('2024-06-15T12:00:00Z');

    await addConnection(storage, 'Bank', makeIdGen('conn-1'), clock);
    await addAccount(storage, 'conn-1', 'Checking', [], makeIdGen('acct-1'), clock);
    await setBalance(storage, 'acct-1', 'USD', '100', makeClock('2024-06-14T10:00:00Z'));

    const result = await portfolioHistory(storage, marketDataStore, config, {}, clock);
    expect(result.points).toHaveLength(1);

    const json = JSON.stringify(result);
    const parsed = JSON.parse(json);
    expect('summary' in parsed).toBe(false);
  });

  it('summary present in history JSON with 2+ points', async () => {
    const storage = new MemoryStorage();
    const marketDataStore = new NullMarketDataStore();
    const config = makeConfig();
    const clock = makeClock('2024-06-15T12:00:00Z');

    await addConnection(storage, 'Bank', makeIdGen('conn-1'), clock);
    await addAccount(storage, 'conn-1', 'Checking', [], makeIdGen('acct-1'), clock);
    await setBalance(storage, 'acct-1', 'USD', '100', makeClock('2024-06-13T10:00:00Z'));
    await setBalance(storage, 'acct-1', 'USD', '200', makeClock('2024-06-14T10:00:00Z'));

    const result = await portfolioHistory(storage, marketDataStore, config, {}, clock);
    expect(result.points).toHaveLength(2);

    const json = JSON.stringify(result);
    const parsed = JSON.parse(json);
    expect('summary' in parsed).toBe(true);
    expect(parsed.summary.initial_value).toBe('100');
    expect(parsed.summary.final_value).toBe('200');
  });
});

// ===========================================================================
// 4. Multi-asset portfolio test
// ===========================================================================

describe('Integration: multi-asset portfolio', () => {
  it('USD and EUR balances: EUR shows undefined value_in_base (no FX rate)', async () => {
    const storage = new MemoryStorage();
    const marketDataStore = new NullMarketDataStore();
    const config = makeConfig();
    const clock = makeClock('2024-06-15T12:00:00Z');

    // Connection 1 with USD account
    await addConnection(storage, 'US Bank', makeIdGen('conn-1'), clock);
    await addAccount(storage, 'conn-1', 'Checking USD', [], makeIdGen('acct-1'), clock);
    await setBalance(storage, 'acct-1', 'USD', '1000', makeClock('2024-06-14T10:00:00Z'));

    // Connection 2 with EUR account
    await addConnection(storage, 'EU Bank', makeIdGen('conn-2'), clock);
    await addAccount(storage, 'conn-2', 'Checking EUR', [], makeIdGen('acct-2'), clock);
    await setBalance(storage, 'acct-2', 'EUR', '500', makeClock('2024-06-14T10:00:00Z'));

    const snap = (await portfolioSnapshot(
      storage,
      marketDataStore,
      config,
      {},
      clock,
    )) as SnapshotResultShape;

    // Total value should be 1000 (USD only, since EUR has no FX rate)
    expect(snap.total_value).toBe('1000');
    expect(snap.currency).toBe('USD');

    const byAsset = snap.by_asset ?? [];
    expect(byAsset).toHaveLength(2);

    // Find USD and EUR entries
    const usdEntry = byAsset.find((a) => a.asset.type === 'currency' && a.asset.iso_code === 'USD');
    const eurEntry = byAsset.find((a) => a.asset.type === 'currency' && a.asset.iso_code === 'EUR');

    expect(usdEntry).toBeDefined();
    expect(eurEntry).toBeDefined();
    if (usdEntry === undefined || eurEntry === undefined) {
      throw new Error('Expected both USD and EUR rows');
    }
    expect(usdEntry.total_amount).toBe('1000');
    expect(usdEntry.value_in_base).toBe('1000');
    expect(eurEntry.total_amount).toBe('500');

    // EUR has no FX rate, so value_in_base should be absent (undefined -> omitted from JSON)
    const eurJson = JSON.stringify(eurEntry);
    const eurParsed = JSON.parse(eurJson);
    expect('value_in_base' in eurParsed).toBe(false);

    // Verify by_account shows correct account names
    const byAccount = snap.by_account ?? [];
    expect(byAccount).toHaveLength(2);

    const acctNames = byAccount.map((a) => a.account_name).sort();
    expect(acctNames).toEqual(['Checking EUR', 'Checking USD']);

    // USD account has value_in_base, EUR account does not
    const usdAcct = byAccount.find((a) => a.account_name === 'Checking USD');
    const eurAcct = byAccount.find((a) => a.account_name === 'Checking EUR');
    if (usdAcct === undefined || eurAcct === undefined) {
      throw new Error('Expected both USD and EUR account rows');
    }

    expect(usdAcct.value_in_base).toBe('1000');
    expect(usdAcct.connection_name).toBe('US Bank');

    // EUR account has no FX rate -> value_in_base omitted
    const eurAcctJson = JSON.stringify(eurAcct);
    const eurAcctParsed = JSON.parse(eurAcctJson);
    expect('value_in_base' in eurAcctParsed).toBe(false);
    expect(eurAcct.connection_name).toBe('EU Bank');
  });
});

// ===========================================================================
// 5. Mutation error responses
// ===========================================================================

describe('Integration: mutation error responses', () => {
  it('add connection with duplicate name returns {success: false, error: ...}', async () => {
    const storage = new MemoryStorage();
    const clock = makeClock('2024-06-15T12:00:00Z');

    await addConnection(storage, 'My Bank', makeIdGen('conn-1'), clock);
    const result = await addConnection(storage, 'My Bank', makeIdGen('conn-2'), clock);

    const obj = result as Record<string, unknown>;
    expect(obj.success).toBe(false);
    expect(typeof obj.error).toBe('string');
    expect(obj.error).toContain('My Bank');
  });

  it('remove non-existent connection returns {success: false, error: ...}', async () => {
    const storage = new MemoryStorage();

    const result = await removeConnection(storage, 'nonexistent-id');

    const obj = result as Record<string, unknown>;
    expect(obj.success).toBe(false);
    expect(typeof obj.error).toBe('string');
  });

  it('set balance with invalid amount returns {success: false, error: ...}', async () => {
    const storage = new MemoryStorage();
    const clock = makeClock('2024-06-15T12:00:00Z');

    await addConnection(storage, 'Bank', makeIdGen('conn-1'), clock);
    await addAccount(storage, 'conn-1', 'Checking', [], makeIdGen('acct-1'), clock);

    const result = await setBalance(storage, 'acct-1', 'USD', 'not-a-number');

    const obj = result as Record<string, unknown>;
    expect(obj.success).toBe(false);
    expect(typeof obj.error).toBe('string');
    expect(obj.error).toContain('not-a-number');
  });

  it('set balance on non-existent account returns {success: false, error: ...}', async () => {
    const storage = new MemoryStorage();

    const result = await setBalance(storage, 'nonexistent', 'USD', '100');

    const obj = result as Record<string, unknown>;
    expect(obj.success).toBe(false);
    expect(typeof obj.error).toBe('string');
    expect(obj.error).toContain('nonexistent');
  });

  it('add account to non-existent connection returns {success: false, error: ...}', async () => {
    const storage = new MemoryStorage();
    const clock = makeClock('2024-06-15T12:00:00Z');

    const result = await addAccount(
      storage,
      'nonexistent',
      'Checking',
      [],
      makeIdGen('acct-1'),
      clock,
    );

    const obj = result as Record<string, unknown>;
    expect(obj.success).toBe(false);
    expect(typeof obj.error).toBe('string');
  });

  it('all error responses serialize correctly to JSON', async () => {
    const storage = new MemoryStorage();
    const clock = makeClock('2024-06-15T12:00:00Z');

    await addConnection(storage, 'Bank', makeIdGen('conn-1'), clock);

    // Duplicate connection
    const r1 = await addConnection(storage, 'Bank', makeIdGen('conn-2'), clock);
    const j1 = JSON.parse(JSON.stringify(r1));
    expect(j1.success).toBe(false);
    expect(typeof j1.error).toBe('string');

    // Non-existent connection removal
    const r2 = await removeConnection(storage, 'nope');
    const j2 = JSON.parse(JSON.stringify(r2));
    expect(j2.success).toBe(false);
    expect(typeof j2.error).toBe('string');

    // Non-existent account balance
    const r3 = await setBalance(storage, 'nope', 'USD', '100');
    const j3 = JSON.parse(JSON.stringify(r3));
    expect(j3.success).toBe(false);
    expect(typeof j3.error).toBe('string');
  });
});

// ===========================================================================
// 6. Change points output
// ===========================================================================

describe('Integration: change points output', () => {
  it('balance change point has correct trigger serialization', async () => {
    const storage = new MemoryStorage();
    const marketDataStore = new NullMarketDataStore();
    const config = makeConfig();
    const clock = makeClock('2024-06-15T12:00:00Z');

    await addConnection(storage, 'Bank', makeIdGen('conn-1'), clock);
    await addAccount(storage, 'conn-1', 'Checking', [], makeIdGen('acct-1'), clock);
    await setBalance(storage, 'acct-1', 'USD', '100', makeClock('2024-06-14T10:00:00Z'));

    const result = await portfolioChangePoints(storage, marketDataStore, config, {}, clock);

    expect(result.points).toHaveLength(1);

    // Verify timestamp uses Z suffix (formatChronoSerde)
    expect(result.points[0].timestamp).toBe('2024-06-14T10:00:00Z');
    expect(result.points[0].timestamp).toMatch(/Z$/);
    expect(result.points[0].timestamp).not.toContain('+00:00');

    // Verify trigger serialization
    expect(result.points[0].triggers).toHaveLength(1);
    expect(result.points[0].triggers[0]).toEqual({
      type: 'balance',
      account_id: 'acct-1',
      asset: { type: 'currency', iso_code: 'USD' },
    });

    // Verify full JSON round-trip
    const json = JSON.stringify(result, null, 2);
    const parsed = JSON.parse(json);

    // start_date and end_date are null (present)
    expect('start_date' in parsed).toBe(true);
    expect('end_date' in parsed).toBe(true);
    expect(parsed.start_date).toBeNull();
    expect(parsed.end_date).toBeNull();

    // Trigger asset uses snake_case
    const triggerJson = JSON.stringify(parsed.points[0].triggers[0].asset);
    expect(triggerJson).toBe('{"type":"currency","iso_code":"USD"}');
  });

  it('change points with date range filtering', async () => {
    const storage = new MemoryStorage();
    const marketDataStore = new NullMarketDataStore();
    const config = makeConfig();
    const clock = makeClock('2024-06-20T12:00:00Z');

    await addConnection(storage, 'Bank', makeIdGen('conn-1'), clock);
    await addAccount(storage, 'conn-1', 'Checking', [], makeIdGen('acct-1'), clock);

    // Add multiple balance snapshots at different times
    await setBalance(storage, 'acct-1', 'USD', '100', makeClock('2024-06-10T10:00:00Z'));
    await setBalance(storage, 'acct-1', 'USD', '200', makeClock('2024-06-12T10:00:00Z'));
    await setBalance(storage, 'acct-1', 'USD', '300', makeClock('2024-06-15T10:00:00Z'));

    const result = await portfolioChangePoints(
      storage,
      marketDataStore,
      config,
      { start: '2024-06-11', end: '2024-06-14' },
      clock,
    );

    // Only the 2024-06-12 point should be included
    expect(result.points).toHaveLength(1);
    expect(result.points[0].timestamp).toBe('2024-06-12T10:00:00Z');

    expect(result.start_date).toBe('2024-06-11');
    expect(result.end_date).toBe('2024-06-14');
  });

  it('history change_triggers present when there are triggers', async () => {
    const storage = new MemoryStorage();
    const marketDataStore = new NullMarketDataStore();
    const config = makeConfig();
    const clock = makeClock('2024-06-15T12:00:00Z');

    await addConnection(storage, 'Bank', makeIdGen('conn-1'), clock);
    await addAccount(storage, 'conn-1', 'Checking', [], makeIdGen('acct-1'), clock);
    await setBalance(storage, 'acct-1', 'USD', '100', makeClock('2024-06-14T10:00:00Z'));

    const result = await portfolioHistory(storage, marketDataStore, config, {}, clock);

    expect(result.points).toHaveLength(1);
    expect(result.points[0].change_triggers).toBeDefined();
    expect(result.points[0].change_triggers!).toContain(
      'balance:acct-1:{"type":"currency","iso_code":"USD"}',
    );

    // Verify the trigger string format in JSON
    const json = JSON.stringify(result);
    const parsed = JSON.parse(json);
    expect(parsed.points[0].change_triggers[0]).toBe(
      'balance:acct-1:{"type":"currency","iso_code":"USD"}',
    );
  });
});

// ===========================================================================
// Additional integration: full JSON round-trip tests
// ===========================================================================

describe('Integration: full JSON round-trip verification', () => {
  it('listAll JSON includes all expected top-level keys', async () => {
    const storage = new MemoryStorage();
    const clock = makeClock('2024-06-15T12:00:00Z');
    const config = makeConfig();

    await addConnection(storage, 'Bank', makeIdGen('conn-1'), clock);
    await addAccount(storage, 'conn-1', 'Checking', ['primary'], makeIdGen('acct-1'), clock);
    await setBalance(storage, 'acct-1', 'USD', '500', clock);

    const result = await listAll(storage, config);
    const json = JSON.stringify(result, null, 2);
    const parsed = JSON.parse(json);

    expect(Object.keys(parsed).sort()).toEqual([
      'accounts',
      'balances',
      'connections',
      'price_sources',
    ]);

    // Verify nested shapes
    expect(parsed.connections[0].last_sync).toBeNull();
    expect(parsed.accounts[0].tags).toEqual(['primary']);
    expect(parsed.accounts[0].active).toBe(true);
    expect(parsed.balances[0].value_in_reporting_currency).toBe('500');
    expect(parsed.price_sources).toEqual([]);
  });

  it('portfolio history with 2 points round-trips through JSON correctly', async () => {
    const storage = new MemoryStorage();
    const marketDataStore = new NullMarketDataStore();
    const config = makeConfig();
    const clock = makeClock('2024-06-15T12:00:00Z');

    await addConnection(storage, 'Bank', makeIdGen('conn-1'), clock);
    await addAccount(storage, 'conn-1', 'Checking', [], makeIdGen('acct-1'), clock);
    await setBalance(storage, 'acct-1', 'USD', '100', makeClock('2024-06-13T10:00:00Z'));
    await setBalance(storage, 'acct-1', 'USD', '150', makeClock('2024-06-14T10:00:00Z'));

    const result = await portfolioHistory(storage, marketDataStore, config, {}, clock);

    const json = JSON.stringify(result, null, 2);
    const parsed = JSON.parse(json);

    // Structural verification
    expect(parsed.currency).toBe('USD');
    expect(parsed.start_date).toBeNull();
    expect(parsed.end_date).toBeNull();
    expect(parsed.granularity).toBe('none');
    expect(parsed.points).toHaveLength(2);

    // Points
    expect(parsed.points[0].timestamp).toBe('2024-06-13T10:00:00+00:00');
    expect(parsed.points[0].date).toBe('2024-06-13');
    expect(parsed.points[0].total_value).toBe('100');
    expect(parsed.points[1].timestamp).toBe('2024-06-14T10:00:00+00:00');
    expect(parsed.points[1].date).toBe('2024-06-14');
    expect(parsed.points[1].total_value).toBe('150');

    // Summary
    expect(parsed.summary).toBeDefined();
    expect(parsed.summary.initial_value).toBe('100');
    expect(parsed.summary.final_value).toBe('150');
    expect(parsed.summary.absolute_change).toBe('50');
    expect(parsed.summary.percentage_change).toBe('50.00');
  });

  it('portfolio snapshot with empty storage shows zero total and empty arrays', async () => {
    const storage = new MemoryStorage();
    const marketDataStore = new NullMarketDataStore();
    const config = makeConfig();
    const clock = makeClock('2024-06-15T12:00:00Z');

    const result = await portfolioSnapshot(storage, marketDataStore, config, {}, clock);
    const parsed = JSON.parse(JSON.stringify(result));
    expect(parsed).toEqual(readContractFixture('portfolio_snapshot_empty'));
  });

  it('multiple mutations build up state correctly', async () => {
    const storage = new MemoryStorage();
    const clock = makeClock('2024-06-15T12:00:00Z');
    const config = makeConfig();

    // Add two connections with accounts
    await addConnection(storage, 'Bank A', makeIdGen('conn-a'), clock);
    await addConnection(storage, 'Bank B', makeIdGen('conn-b'), clock);
    await addAccount(storage, 'conn-a', 'Savings', [], makeIdGen('acct-a'), clock);
    await addAccount(storage, 'conn-b', 'Checking', [], makeIdGen('acct-b'), clock);

    // Set balances
    await setBalance(storage, 'acct-a', 'USD', '1000', clock);
    await setBalance(storage, 'acct-b', 'USD', '2000', clock);

    // Verify list commands reflect all mutations
    const connections = await listConnections(storage);
    expect(connections).toHaveLength(2);
    expect(connections.every((c) => c.account_count === 1)).toBe(true);

    const accounts = await listAccounts(storage);
    expect(accounts).toHaveLength(2);

    const balances = await listBalances(storage, config);
    expect(balances).toHaveLength(2);
    const amounts = balances.map((b) => b.amount).sort();
    expect(amounts).toEqual(['1000', '2000']);

    // Portfolio total should be sum
    const marketDataStore = new NullMarketDataStore();
    const snap = (await portfolioSnapshot(
      storage,
      marketDataStore,
      config,
      {},
      clock,
    )) as SnapshotResultShape;
    expect(snap.total_value).toBe('3000');
  });

  it('remove connection cleans up properly', async () => {
    const storage = new MemoryStorage();
    const clock = makeClock('2024-06-15T12:00:00Z');

    await addConnection(storage, 'Bank', makeIdGen('conn-1'), clock);
    await addAccount(storage, 'conn-1', 'Checking', [], makeIdGen('acct-1'), clock);
    await setBalance(storage, 'acct-1', 'USD', '500', clock);

    // Before removal
    expect(await listConnections(storage)).toHaveLength(1);
    expect(await listAccounts(storage)).toHaveLength(1);

    // Remove
    const result = await removeConnection(storage, 'conn-1');
    expect((result as { success: boolean }).success).toBe(true);

    // After removal
    expect(await listConnections(storage)).toHaveLength(0);
    expect(await listAccounts(storage)).toHaveLength(0);
  });
});
