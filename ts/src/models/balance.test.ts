import { describe, it, expect } from 'vitest';
import { Asset } from './asset.js';
import { FixedClock } from '../clock.js';
import { AssetBalance, BalanceSnapshot } from './balance.js';

describe('AssetBalance', () => {
  describe('new', () => {
    it('creates an asset balance', () => {
      const usd = Asset.currency('USD');
      const balance = AssetBalance.new(usd, '1500.00');

      expect(Asset.equals(balance.asset, usd)).toBe(true);
      expect(balance.amount).toBe('1500.00');
    });

    it('preserves the exact amount string', () => {
      const btc = Asset.crypto('BTC');
      const balance = AssetBalance.new(btc, '0.00145000');

      expect(balance.amount).toBe('0.00145000');
    });
  });

  describe('JSON serialization', () => {
    it('serializes correctly', () => {
      const balance = AssetBalance.new(Asset.currency('EUR'), '250.50');
      const json = AssetBalance.toJSON(balance);

      expect(json.asset).toEqual({ type: 'currency', iso_code: 'EUR' });
      expect(json.amount).toBe('250.50');
    });

    it('round-trips through JSON', () => {
      const original = AssetBalance.new(Asset.equity('AAPL', 'NASDAQ'), '100');
      const json = AssetBalance.toJSON(original);
      const parsed = AssetBalance.fromJSON(json);

      expect(Asset.equals(parsed.asset, original.asset)).toBe(true);
      expect(parsed.amount).toBe('100');
    });
  });
});

describe('BalanceSnapshot', () => {
  const fixedDate = new Date('2024-06-15T12:00:00.000Z');
  const usd = Asset.currency('USD');
  const btc = Asset.crypto('BTC');

  describe('new', () => {
    it('creates a snapshot with given timestamp and balances', () => {
      const balances = [AssetBalance.new(usd, '5000.00'), AssetBalance.new(btc, '0.5')];
      const snapshot = BalanceSnapshot.new(fixedDate, balances);

      expect(snapshot.timestamp.getTime()).toBe(fixedDate.getTime());
      expect(snapshot.balances).toHaveLength(2);
      expect(snapshot.balances[0].amount).toBe('5000.00');
      expect(snapshot.balances[1].amount).toBe('0.5');
    });

    it('creates an empty snapshot', () => {
      const snapshot = BalanceSnapshot.new(fixedDate, []);

      expect(snapshot.timestamp.getTime()).toBe(fixedDate.getTime());
      expect(snapshot.balances).toEqual([]);
    });
  });

  describe('now', () => {
    it('creates a snapshot with current time', () => {
      const balances = [AssetBalance.new(usd, '100.00')];
      const snapshot = BalanceSnapshot.now(balances);

      expect(Date.now() - snapshot.timestamp.getTime()).toBeLessThan(1000);
      expect(snapshot.balances).toHaveLength(1);
    });
  });

  describe('nowWith', () => {
    it('uses injected clock for timestamp', () => {
      const clock = new FixedClock(fixedDate);
      const balances = [AssetBalance.new(usd, '999.99')];
      const snapshot = BalanceSnapshot.nowWith(clock, balances);

      expect(snapshot.timestamp.getTime()).toBe(fixedDate.getTime());
      expect(snapshot.balances).toHaveLength(1);
      expect(snapshot.balances[0].amount).toBe('999.99');
    });
  });

  describe('JSON serialization', () => {
    it('serializes correctly', () => {
      const balances = [AssetBalance.new(usd, '5000.00'), AssetBalance.new(btc, '0.5')];
      const snapshot = BalanceSnapshot.new(fixedDate, balances);
      const json = BalanceSnapshot.toJSON(snapshot);

      expect(json.timestamp).toBe('2024-06-15T12:00:00.000Z');
      expect(json.balances).toHaveLength(2);
      expect(json.balances[0].amount).toBe('5000.00');
      expect(json.balances[1].asset).toEqual({ type: 'crypto', symbol: 'BTC' });
    });

    it('round-trips through JSON', () => {
      const balances = [AssetBalance.new(usd, '5000.00'), AssetBalance.new(btc, '0.5')];
      const original = BalanceSnapshot.new(fixedDate, balances);
      const json = BalanceSnapshot.toJSON(original);
      const parsed = BalanceSnapshot.fromJSON(json);

      expect(parsed.timestamp.getTime()).toBe(fixedDate.getTime());
      expect(parsed.balances).toHaveLength(2);
      expect(parsed.balances[0].amount).toBe('5000.00');
      expect(Asset.equals(parsed.balances[1].asset, btc)).toBe(true);
    });
  });
});
