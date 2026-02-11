import { describe, it, expect } from 'vitest';
import { Id } from './models/id.js';
import type { ConnectionType, ConnectionStateType } from './models/connection.js';
import type { AccountConfig } from './models/account.js';
import type { RefreshConfig } from './config.js';
import type { PricePoint } from './market-data/models.js';
import { AssetId } from './market-data/asset-id.js';
import {
  StalenessCheck,
  resolveBalanceStaleness,
  checkBalanceStalenessAt,
  checkPriceStalenessAt,
} from './staleness.js';

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

const MS_PER_HOUR = 60 * 60 * 1000;

function makeConnection(opts?: { lastSyncAt?: Date; balanceStaleness?: number }): ConnectionType {
  const state: ConnectionStateType = {
    id: Id.fromString('conn-1'),
    status: 'active',
    created_at: new Date('2026-01-01T00:00:00Z'),
    account_ids: [],
    synchronizer_data: null,
    ...(opts?.lastSyncAt !== undefined
      ? { last_sync: { at: opts.lastSyncAt, status: 'success' as const } }
      : {}),
  };
  return {
    config: {
      name: 'Test',
      synchronizer: 'manual',
      ...(opts?.balanceStaleness !== undefined ? { balance_staleness: opts.balanceStaleness } : {}),
    },
    state,
  };
}

function makePrice(timestamp: Date): PricePoint {
  return {
    asset_id: AssetId.fromString('equity/AAPL'),
    as_of_date: '2026-02-01',
    timestamp,
    price: '150.00',
    quote_currency: 'USD',
    kind: 'close',
    source: 'test',
  };
}

const globalConfig: RefreshConfig = {
  balance_staleness: 24 * MS_PER_HOUR,
  price_staleness: 12 * MS_PER_HOUR,
};

// ---------------------------------------------------------------------------
// checkBalanceStalenessAt
// ---------------------------------------------------------------------------

describe('checkBalanceStalenessAt', () => {
  it('returns stale when sync is old (48h sync, 24h threshold)', () => {
    const now = new Date('2026-02-10T00:00:00Z');
    const lastSync = new Date('2026-02-08T00:00:00Z'); // 48h ago
    const threshold = 24 * MS_PER_HOUR;
    const conn = makeConnection({ lastSyncAt: lastSync });

    const result = checkBalanceStalenessAt(conn, threshold, now);

    expect(result.is_stale).toBe(true);
    expect(result.age).toBe(48 * MS_PER_HOUR);
    expect(result.threshold).toBe(threshold);
  });

  it('returns fresh when sync is recent (12h sync, 24h threshold)', () => {
    const now = new Date('2026-02-10T00:00:00Z');
    const lastSync = new Date('2026-02-09T12:00:00Z'); // 12h ago
    const threshold = 24 * MS_PER_HOUR;
    const conn = makeConnection({ lastSyncAt: lastSync });

    const result = checkBalanceStalenessAt(conn, threshold, now);

    expect(result.is_stale).toBe(false);
    expect(result.age).toBe(12 * MS_PER_HOUR);
    expect(result.threshold).toBe(threshold);
  });

  it('returns stale (missing) when never synced', () => {
    const now = new Date('2026-02-10T00:00:00Z');
    const threshold = 24 * MS_PER_HOUR;
    const conn = makeConnection(); // no lastSyncAt

    const result = checkBalanceStalenessAt(conn, threshold, now);

    expect(result.is_stale).toBe(true);
    expect(result.age).toBeNull();
    expect(result.threshold).toBe(threshold);
  });

  it('returns fresh when sync is in the future (age clamped to 0)', () => {
    const now = new Date('2026-02-10T00:00:00Z');
    const lastSync = new Date('2026-02-11T00:00:00Z'); // 1 day in the future
    const threshold = 24 * MS_PER_HOUR;
    const conn = makeConnection({ lastSyncAt: lastSync });

    const result = checkBalanceStalenessAt(conn, threshold, now);

    expect(result.is_stale).toBe(false);
    expect(result.age).toBe(0);
    expect(result.threshold).toBe(threshold);
  });

  it('returns stale when age equals threshold (>= comparison)', () => {
    const now = new Date('2026-02-10T00:00:00Z');
    const lastSync = new Date('2026-02-09T00:00:00Z'); // exactly 24h ago
    const threshold = 24 * MS_PER_HOUR;
    const conn = makeConnection({ lastSyncAt: lastSync });

    const result = checkBalanceStalenessAt(conn, threshold, now);

    expect(result.is_stale).toBe(true);
    expect(result.age).toBe(24 * MS_PER_HOUR);
    expect(result.threshold).toBe(threshold);
  });
});

// ---------------------------------------------------------------------------
// checkPriceStalenessAt
// ---------------------------------------------------------------------------

describe('checkPriceStalenessAt', () => {
  it('returns stale when price is old', () => {
    const now = new Date('2026-02-10T00:00:00Z');
    const price = makePrice(new Date('2026-02-08T00:00:00Z')); // 48h ago
    const threshold = 24 * MS_PER_HOUR;

    const result = checkPriceStalenessAt(price, threshold, now);

    expect(result.is_stale).toBe(true);
    expect(result.age).toBe(48 * MS_PER_HOUR);
    expect(result.threshold).toBe(threshold);
  });

  it('returns fresh when price is recent', () => {
    const now = new Date('2026-02-10T00:00:00Z');
    const price = makePrice(new Date('2026-02-09T12:00:00Z')); // 12h ago
    const threshold = 24 * MS_PER_HOUR;

    const result = checkPriceStalenessAt(price, threshold, now);

    expect(result.is_stale).toBe(false);
    expect(result.age).toBe(12 * MS_PER_HOUR);
    expect(result.threshold).toBe(threshold);
  });

  it('returns stale (missing) when no price provided', () => {
    const now = new Date('2026-02-10T00:00:00Z');
    const threshold = 24 * MS_PER_HOUR;

    const result = checkPriceStalenessAt(null, threshold, now);

    expect(result.is_stale).toBe(true);
    expect(result.age).toBeNull();
    expect(result.threshold).toBe(threshold);
  });

  it('returns fresh when price timestamp is in the future (age clamped to 0)', () => {
    const now = new Date('2026-02-10T00:00:00Z');
    const price = makePrice(new Date('2026-02-11T00:00:00Z')); // 1 day in future
    const threshold = 24 * MS_PER_HOUR;

    const result = checkPriceStalenessAt(price, threshold, now);

    expect(result.is_stale).toBe(false);
    expect(result.age).toBe(0);
    expect(result.threshold).toBe(threshold);
  });

  it('returns stale when age equals threshold (>= comparison)', () => {
    const now = new Date('2026-02-10T00:00:00Z');
    const price = makePrice(new Date('2026-02-09T00:00:00Z')); // exactly 24h ago
    const threshold = 24 * MS_PER_HOUR;

    const result = checkPriceStalenessAt(price, threshold, now);

    expect(result.is_stale).toBe(true);
    expect(result.age).toBe(24 * MS_PER_HOUR);
    expect(result.threshold).toBe(threshold);
  });
});

// ---------------------------------------------------------------------------
// StalenessCheck factories
// ---------------------------------------------------------------------------

describe('StalenessCheck factories', () => {
  it('stale() creates a stale check with given age and threshold', () => {
    const check = StalenessCheck.stale(1000, 500);
    expect(check.is_stale).toBe(true);
    expect(check.age).toBe(1000);
    expect(check.threshold).toBe(500);
  });

  it('fresh() creates a fresh check with given age and threshold', () => {
    const check = StalenessCheck.fresh(100, 500);
    expect(check.is_stale).toBe(false);
    expect(check.age).toBe(100);
    expect(check.threshold).toBe(500);
  });

  it('missing() creates a stale check with null age', () => {
    const check = StalenessCheck.missing(500);
    expect(check.is_stale).toBe(true);
    expect(check.age).toBeNull();
    expect(check.threshold).toBe(500);
  });
});

// ---------------------------------------------------------------------------
// resolveBalanceStaleness
// ---------------------------------------------------------------------------

describe('resolveBalanceStaleness', () => {
  it('uses account config override when present', () => {
    const accountConfig: AccountConfig = { balance_staleness: 6 * MS_PER_HOUR };
    const conn = makeConnection({ balanceStaleness: 12 * MS_PER_HOUR });

    const result = resolveBalanceStaleness(accountConfig, conn, globalConfig);

    expect(result).toBe(6 * MS_PER_HOUR);
  });

  it('falls back to connection config when account config has no override', () => {
    const accountConfig: AccountConfig = {}; // no balance_staleness
    const conn = makeConnection({ balanceStaleness: 12 * MS_PER_HOUR });

    const result = resolveBalanceStaleness(accountConfig, conn, globalConfig);

    expect(result).toBe(12 * MS_PER_HOUR);
  });

  it('falls back to connection config when account config is null', () => {
    const conn = makeConnection({ balanceStaleness: 12 * MS_PER_HOUR });

    const result = resolveBalanceStaleness(null, conn, globalConfig);

    expect(result).toBe(12 * MS_PER_HOUR);
  });

  it('falls back to global config when neither account nor connection has override', () => {
    const accountConfig: AccountConfig = {};
    const conn = makeConnection(); // no balanceStaleness

    const result = resolveBalanceStaleness(accountConfig, conn, globalConfig);

    expect(result).toBe(24 * MS_PER_HOUR);
  });

  it('falls back to global config when account config is null and connection has no override', () => {
    const conn = makeConnection();

    const result = resolveBalanceStaleness(null, conn, globalConfig);

    expect(result).toBe(24 * MS_PER_HOUR);
  });
});
