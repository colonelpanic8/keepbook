import { describe, it, expect } from 'vitest';
import { Id } from './id.js';
import { FixedIdGenerator } from './id-generator.js';
import { FixedClock } from '../clock.js';
import { Account, AccountConfig, type BalanceBackfillPolicy, type AccountType } from './account.js';

describe('Account', () => {
  const fixedId = Id.fromString('test-account-id');
  const fixedConnectionId = Id.fromString('test-connection-id');
  const fixedDate = new Date('2024-01-15T10:30:00.000Z');

  describe('newWith', () => {
    it('creates an account with explicit id and timestamp', () => {
      const account = Account.newWith(fixedId, fixedDate, 'Checking', fixedConnectionId);

      expect(account.id.equals(fixedId)).toBe(true);
      expect(account.name).toBe('Checking');
      expect(account.connection_id.equals(fixedConnectionId)).toBe(true);
      expect(account.created_at.getTime()).toBe(fixedDate.getTime());
      expect(account.active).toBe(true);
      expect(account.tags).toEqual([]);
      expect(account.synchronizer_data).toBeNull();
    });
  });

  describe('newWithGenerator', () => {
    it('uses injected id generator and clock', () => {
      const ids = new FixedIdGenerator([fixedId]);
      const clock = new FixedClock(fixedDate);

      const account = Account.newWithGenerator(ids, clock, 'Savings', fixedConnectionId);

      expect(account.id.equals(fixedId)).toBe(true);
      expect(account.name).toBe('Savings');
      expect(account.connection_id.equals(fixedConnectionId)).toBe(true);
      expect(account.created_at.getTime()).toBe(fixedDate.getTime());
      expect(account.active).toBe(true);
      expect(account.tags).toEqual([]);
      expect(account.synchronizer_data).toBeNull();
    });
  });

  describe('new', () => {
    it('creates an account with auto-generated id and current time', () => {
      const account = Account.new('Brokerage', fixedConnectionId);

      // Should have a valid id (UUID format)
      expect(account.id.asStr()).toMatch(
        /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/,
      );
      expect(account.name).toBe('Brokerage');
      expect(account.connection_id.equals(fixedConnectionId)).toBe(true);
      expect(account.active).toBe(true);
      expect(account.tags).toEqual([]);
      expect(account.synchronizer_data).toBeNull();
      // created_at should be recent (within last second)
      expect(Date.now() - account.created_at.getTime()).toBeLessThan(1000);
    });
  });

  describe('JSON serialization', () => {
    it('serializes with snake_case fields', () => {
      const account = Account.newWith(fixedId, fixedDate, 'Checking', fixedConnectionId);
      const json = Account.toJSON(account);

      expect(json.id).toBe('test-account-id');
      expect(json.name).toBe('Checking');
      expect(json.connection_id).toBe('test-connection-id');
      expect(json.tags).toEqual([]);
      expect(json.created_at).toBe('2024-01-15T10:30:00.000Z');
      expect(json.active).toBe(true);
    });

    it('omits synchronizer_data when null', () => {
      const account = Account.newWith(fixedId, fixedDate, 'Checking', fixedConnectionId);
      const json = Account.toJSON(account);

      expect('synchronizer_data' in json).toBe(false);
    });

    it('includes synchronizer_data when non-null', () => {
      const account = Account.newWith(fixedId, fixedDate, 'Checking', fixedConnectionId);
      const withData: AccountType = { ...account, synchronizer_data: { key: 'value' } };
      const json = Account.toJSON(withData);

      expect(json.synchronizer_data).toEqual({ key: 'value' });
    });

    it('round-trips through JSON', () => {
      const account = Account.newWith(fixedId, fixedDate, 'Checking', fixedConnectionId);
      const withTags: AccountType = { ...account, tags: ['bank', 'primary'] };
      const json = Account.toJSON(withTags);
      const parsed = Account.fromJSON(json);

      expect(parsed.id.equals(fixedId)).toBe(true);
      expect(parsed.name).toBe('Checking');
      expect(parsed.connection_id.equals(fixedConnectionId)).toBe(true);
      expect(parsed.created_at.getTime()).toBe(fixedDate.getTime());
      expect(parsed.active).toBe(true);
      expect(parsed.tags).toEqual(['bank', 'primary']);
      expect(parsed.synchronizer_data).toBeNull();
    });
  });
});

describe('BalanceBackfillPolicy', () => {
  it('has the expected string literal values', () => {
    const policies: BalanceBackfillPolicy[] = ['none', 'zero', 'carry_earliest'];
    expect(policies).toHaveLength(3);
  });
});

describe('AccountConfig', () => {
  it('can be created with optional fields', () => {
    const config: AccountConfig = {};
    expect(config.balance_staleness).toBeUndefined();
    expect(config.balance_backfill).toBeUndefined();
  });

  it('can be created with all fields', () => {
    const config: AccountConfig = {
      balance_staleness: 86400000, // 1 day in ms
      balance_backfill: 'zero',
    };
    expect(config.balance_staleness).toBe(86400000);
    expect(config.balance_backfill).toBe('zero');
  });
});
