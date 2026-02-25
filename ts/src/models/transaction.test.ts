import { describe, it, expect } from 'vitest';
import { Id } from './id.js';
import { FixedIdGenerator } from './id-generator.js';
import { FixedClock } from '../clock.js';
import { Asset } from './asset.js';
import {
  Transaction,
  withTimestamp,
  withStatus,
  withId,
  withSynchronizerData,
  withStandardizedMetadata,
  type TransactionStatus,
} from './transaction.js';

describe('Transaction', () => {
  const fixedId = Id.fromString('test-tx-id');
  const fixedDate = new Date('2024-03-20T14:00:00.000Z');
  const usd = Asset.currency('USD');

  describe('newWithGenerator', () => {
    it('creates a transaction with injected deps', () => {
      const ids = new FixedIdGenerator([fixedId]);
      const clock = new FixedClock(fixedDate);

      const tx = Transaction.newWithGenerator(ids, clock, '100.50', usd, 'Paycheck');

      expect(tx.id.equals(fixedId)).toBe(true);
      expect(tx.timestamp.getTime()).toBe(fixedDate.getTime());
      expect(tx.amount).toBe('100.50');
      expect(Asset.equals(tx.asset, usd)).toBe(true);
      expect(tx.description).toBe('Paycheck');
      expect(tx.status).toBe('posted');
      expect(tx.synchronizer_data).toBeNull();
    });
  });

  describe('new', () => {
    it('creates a transaction with auto-generated id and current time', () => {
      const tx = Transaction.new('50.00', usd, 'Groceries');

      expect(tx.id.asStr()).toMatch(
        /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/,
      );
      expect(tx.amount).toBe('50.00');
      expect(Asset.equals(tx.asset, usd)).toBe(true);
      expect(tx.description).toBe('Groceries');
      expect(tx.status).toBe('posted');
      expect(tx.synchronizer_data).toBeNull();
      expect(Date.now() - tx.timestamp.getTime()).toBeLessThan(1000);
    });
  });

  describe('builder functions', () => {
    const ids = new FixedIdGenerator([fixedId]);
    const clock = new FixedClock(fixedDate);
    const baseTx = Transaction.newWithGenerator(ids, clock, '100.00', usd, 'Test');

    it('withTimestamp creates a modified copy', () => {
      const newDate = new Date('2024-06-01T00:00:00.000Z');
      const modified = withTimestamp(baseTx, newDate);

      expect(modified.timestamp.getTime()).toBe(newDate.getTime());
      // Original is unchanged
      expect(baseTx.timestamp.getTime()).toBe(fixedDate.getTime());
      // Other fields preserved
      expect(modified.id.equals(fixedId)).toBe(true);
      expect(modified.amount).toBe('100.00');
    });

    it('withStatus creates a modified copy', () => {
      const modified = withStatus(baseTx, 'pending');

      expect(modified.status).toBe('pending');
      expect(baseTx.status).toBe('posted');
      expect(modified.id.equals(fixedId)).toBe(true);
    });

    it('withId creates a modified copy', () => {
      const newId = Id.fromString('new-id');
      const modified = withId(baseTx, newId);

      expect(modified.id.equals(newId)).toBe(true);
      expect(baseTx.id.equals(fixedId)).toBe(true);
    });

    it('withSynchronizerData creates a modified copy', () => {
      const data = { plaid_id: 'abc123' };
      const modified = withSynchronizerData(baseTx, data);

      expect(modified.synchronizer_data).toEqual({ plaid_id: 'abc123' });
      expect(baseTx.synchronizer_data).toBeNull();
    });

    it('withSynchronizerData derives standardized metadata from chase fields', () => {
      const modified = withSynchronizerData(baseTx, {
        merchant_dba_name: 'Coffee Shop',
        merchant_category_code: '5814',
        merchant_category_name: 'Fast Food',
        etu_standard_transaction_type_group_name: 'Purchases',
        enriched_merchant_names: ['Blue Bottle Coffee'],
      });

      expect(modified.standardized_metadata).toEqual({
        merchant_name: 'Blue Bottle Coffee',
        merchant_category_code: '5814',
        merchant_category_label: 'Fast Food',
        transaction_kind: 'purchase',
        is_internal_transfer_hint: false,
      });
    });

    it('withStandardizedMetadata sets explicit metadata', () => {
      const modified = withStandardizedMetadata(baseTx, {
        merchant_name: 'Test Merchant',
        transaction_kind: 'payment',
      });
      expect(modified.standardized_metadata).toEqual({
        merchant_name: 'Test Merchant',
        transaction_kind: 'payment',
      });
    });
  });

  describe('JSON serialization', () => {
    it('serializes with snake_case fields', () => {
      const ids = new FixedIdGenerator([fixedId]);
      const clock = new FixedClock(fixedDate);
      const tx = Transaction.newWithGenerator(ids, clock, '42.00', usd, 'Coffee');
      const json = Transaction.toJSON(tx);

      expect(json.id).toBe('test-tx-id');
      expect(json.timestamp).toBe('2024-03-20T14:00:00.000Z');
      expect(json.amount).toBe('42.00');
      expect(json.asset).toEqual({ type: 'currency', iso_code: 'USD' });
      expect(json.description).toBe('Coffee');
      expect(json.status).toBe('posted');
    });

    it('omits synchronizer_data when null', () => {
      const ids = new FixedIdGenerator([fixedId]);
      const clock = new FixedClock(fixedDate);
      const tx = Transaction.newWithGenerator(ids, clock, '42.00', usd, 'Coffee');
      const json = Transaction.toJSON(tx);

      expect('synchronizer_data' in json).toBe(false);
    });

    it('includes synchronizer_data when non-null', () => {
      const ids = new FixedIdGenerator([fixedId]);
      const clock = new FixedClock(fixedDate);
      const tx = Transaction.newWithGenerator(ids, clock, '42.00', usd, 'Coffee');
      const withData = withSynchronizerData(tx, { key: 'val' });
      const json = Transaction.toJSON(withData);

      expect(json.synchronizer_data).toEqual({ key: 'val' });
    });

    it('backfills standardized metadata from synchronizer_data when missing', () => {
      const parsed = Transaction.fromJSON({
        id: 'test-tx-id',
        timestamp: '2024-03-20T14:00:00.000Z',
        amount: '42.00',
        asset: { type: 'currency', iso_code: 'USD' },
        description: 'Coffee',
        status: 'posted',
        synchronizer_data: {
          merchant_dba_name: 'Coffee Shop',
          merchant_category_code: '5814',
        },
      });

      expect(parsed.standardized_metadata).toEqual({
        merchant_name: 'Coffee Shop',
        merchant_category_code: '5814',
      });
    });

    it('round-trips through JSON', () => {
      const ids = new FixedIdGenerator([fixedId]);
      const clock = new FixedClock(fixedDate);
      const tx = Transaction.newWithGenerator(ids, clock, '42.00', usd, 'Coffee');
      const modified = withStatus(tx, 'reversed');
      const json = Transaction.toJSON(modified);
      const parsed = Transaction.fromJSON(json);

      expect(parsed.id.equals(fixedId)).toBe(true);
      expect(parsed.timestamp.getTime()).toBe(fixedDate.getTime());
      expect(parsed.amount).toBe('42.00');
      expect(Asset.equals(parsed.asset, usd)).toBe(true);
      expect(parsed.description).toBe('Coffee');
      expect(parsed.status).toBe('reversed');
      expect(parsed.synchronizer_data).toBeNull();
    });
  });
});

describe('TransactionStatus', () => {
  it('has the expected string literal values', () => {
    const statuses: TransactionStatus[] = ['pending', 'posted', 'reversed', 'canceled', 'failed'];
    expect(statuses).toHaveLength(5);
  });
});
