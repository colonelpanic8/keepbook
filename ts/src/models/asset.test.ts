import { describe, it, expect } from 'vitest';
import { Asset, type CurrencyAsset, type EquityAsset, type CryptoAsset } from './asset.js';

describe('Asset', () => {
  describe('factory functions', () => {
    it('currency() creates a CurrencyAsset with trimmed iso_code', () => {
      const asset = Asset.currency('  USD  ');
      expect(asset).toEqual({ type: 'currency', iso_code: 'USD' });
    });

    it('equity() creates an EquityAsset with trimmed ticker and no exchange', () => {
      const asset = Asset.equity('  AAPL  ');
      expect(asset).toEqual({ type: 'equity', ticker: 'AAPL' });
      expect((asset as EquityAsset).exchange).toBeUndefined();
    });

    it('crypto() creates a CryptoAsset with trimmed symbol and no network', () => {
      const asset = Asset.crypto('  BTC  ');
      expect(asset).toEqual({ type: 'crypto', symbol: 'BTC' });
      expect((asset as CryptoAsset).network).toBeUndefined();
    });

    it('equity() with exchange creates an EquityAsset with exchange', () => {
      const asset = Asset.equity(' AAPL ', ' NASDAQ ');
      expect(asset).toEqual({ type: 'equity', ticker: 'AAPL', exchange: 'NASDAQ' });
    });

    it('crypto() with network creates a CryptoAsset with network', () => {
      const asset = Asset.crypto(' ETH ', ' ethereum ');
      expect(asset).toEqual({ type: 'crypto', symbol: 'ETH', network: 'ethereum' });
    });
  });

  describe('equals() - case-insensitive equality', () => {
    it('currencies are equal regardless of case', () => {
      expect(Asset.equals(Asset.currency('usd'), Asset.currency('USD'))).toBe(true);
    });

    it('currency numeric codes normalize to alpha codes', () => {
      expect(Asset.equals(Asset.currency('840'), Asset.currency('USD'))).toBe(true);
      expect(Asset.normalized(Asset.currency('840'))).toEqual({ type: 'currency', iso_code: 'USD' });
    });

    it('currencies with different iso_codes are not equal', () => {
      expect(Asset.equals(Asset.currency('USD'), Asset.currency('EUR'))).toBe(false);
    });

    it('equities are equal regardless of case', () => {
      expect(Asset.equals(Asset.equity('aapl'), Asset.equity('AAPL'))).toBe(true);
    });

    it('equities with same ticker but different exchanges are not equal', () => {
      expect(Asset.equals(Asset.equity('AAPL', 'NASDAQ'), Asset.equity('AAPL', 'NYSE'))).toBe(
        false,
      );
    });

    it('equities with same ticker, one with exchange one without, are not equal', () => {
      expect(Asset.equals(Asset.equity('AAPL', 'NASDAQ'), Asset.equity('AAPL'))).toBe(false);
    });

    it('equities with matching exchange are equal case-insensitively', () => {
      expect(Asset.equals(Asset.equity('aapl', 'nasdaq'), Asset.equity('AAPL', 'NASDAQ'))).toBe(
        true,
      );
    });

    it('cryptos are equal regardless of case', () => {
      expect(Asset.equals(Asset.crypto('btc'), Asset.crypto('BTC'))).toBe(true);
    });

    it('cryptos with same symbol but different networks are not equal', () => {
      expect(Asset.equals(Asset.crypto('USDC', 'ethereum'), Asset.crypto('USDC', 'solana'))).toBe(
        false,
      );
    });

    it('cryptos with same symbol, one with network one without, are not equal', () => {
      expect(Asset.equals(Asset.crypto('BTC', 'bitcoin'), Asset.crypto('BTC'))).toBe(false);
    });

    it('cryptos with matching network are equal case-insensitively', () => {
      expect(Asset.equals(Asset.crypto('eth', 'ETHEREUM'), Asset.crypto('ETH', 'ethereum'))).toBe(
        true,
      );
    });

    it('different asset types are never equal', () => {
      expect(Asset.equals(Asset.currency('USD'), Asset.equity('USD'))).toBe(false);

      expect(Asset.equals(Asset.currency('BTC'), Asset.crypto('BTC'))).toBe(false);

      expect(Asset.equals(Asset.equity('ETH'), Asset.crypto('ETH'))).toBe(false);
    });
  });

  describe('normalized()', () => {
    it('uppercases currency iso_code', () => {
      const asset = Asset.normalized({ type: 'currency', iso_code: 'usd' });
      expect(asset).toEqual({ type: 'currency', iso_code: 'USD' });
    });

    it('uppercases equity ticker and exchange', () => {
      const asset = Asset.normalized({ type: 'equity', ticker: 'aapl', exchange: 'nasdaq' });
      expect(asset).toEqual({ type: 'equity', ticker: 'AAPL', exchange: 'NASDAQ' });
    });

    it('uppercases crypto symbol and lowercases network', () => {
      const asset = Asset.normalized({ type: 'crypto', symbol: 'btc', network: 'ETHEREUM' });
      expect(asset).toEqual({ type: 'crypto', symbol: 'BTC', network: 'ethereum' });
    });

    it('drops empty optional exchange field', () => {
      const asset = Asset.normalized({ type: 'equity', ticker: 'aapl', exchange: '  ' });
      expect(asset).toEqual({ type: 'equity', ticker: 'AAPL' });
      expect((asset as EquityAsset).exchange).toBeUndefined();
    });

    it('drops empty optional network field', () => {
      const asset = Asset.normalized({ type: 'crypto', symbol: 'btc', network: '  ' });
      expect(asset).toEqual({ type: 'crypto', symbol: 'BTC' });
      expect((asset as CryptoAsset).network).toBeUndefined();
    });

    it('preserves undefined optional fields as undefined', () => {
      const equity = Asset.normalized({ type: 'equity', ticker: 'aapl' });
      expect((equity as EquityAsset).exchange).toBeUndefined();

      const crypto = Asset.normalized({ type: 'crypto', symbol: 'btc' });
      expect((crypto as CryptoAsset).network).toBeUndefined();
    });

    it('trims whitespace from all fields', () => {
      const asset = Asset.normalized({
        type: 'equity',
        ticker: '  aapl  ',
        exchange: '  nasdaq  ',
      });
      expect(asset).toEqual({ type: 'equity', ticker: 'AAPL', exchange: 'NASDAQ' });
    });
  });

  describe('hash()', () => {
    it('produces same key for case-different currencies', () => {
      expect(Asset.hash(Asset.currency('usd'))).toBe(Asset.hash(Asset.currency('USD')));
    });

    it('produces same key for case-different equities', () => {
      expect(Asset.hash(Asset.equity('aapl'))).toBe(Asset.hash(Asset.equity('AAPL')));
    });

    it('produces same key for case-different equities with exchange', () => {
      expect(Asset.hash(Asset.equity('aapl', 'nasdaq'))).toBe(
        Asset.hash(Asset.equity('AAPL', 'NASDAQ')),
      );
    });

    it('produces same key for case-different cryptos', () => {
      expect(Asset.hash(Asset.crypto('btc'))).toBe(Asset.hash(Asset.crypto('BTC')));
    });

    it('produces same key for case-different cryptos with network', () => {
      expect(Asset.hash(Asset.crypto('eth', 'ETHEREUM'))).toBe(
        Asset.hash(Asset.crypto('ETH', 'ethereum')),
      );
    });

    it('produces different keys for different asset types', () => {
      expect(Asset.hash(Asset.currency('USD'))).not.toBe(Asset.hash(Asset.equity('USD')));
      expect(Asset.hash(Asset.currency('BTC'))).not.toBe(Asset.hash(Asset.crypto('BTC')));
    });

    it('produces different keys for equities with vs without exchange', () => {
      expect(Asset.hash(Asset.equity('AAPL', 'NASDAQ'))).not.toBe(Asset.hash(Asset.equity('AAPL')));
    });
  });

  describe('JSON serialization', () => {
    it('currency serializes to match Rust serde format', () => {
      const asset = Asset.currency('USD');
      const json = JSON.stringify(asset);
      expect(JSON.parse(json)).toEqual({ type: 'currency', iso_code: 'USD' });
    });

    it('equity without exchange omits exchange field', () => {
      const asset = Asset.equity('AAPL');
      const json = JSON.stringify(asset);
      const parsed = JSON.parse(json);
      expect(parsed).toEqual({ type: 'equity', ticker: 'AAPL' });
      expect('exchange' in parsed).toBe(false);
    });

    it('equity with exchange includes exchange field', () => {
      const asset = Asset.equity('AAPL', 'NASDAQ');
      const json = JSON.stringify(asset);
      expect(JSON.parse(json)).toEqual({ type: 'equity', ticker: 'AAPL', exchange: 'NASDAQ' });
    });

    it('crypto without network omits network field', () => {
      const asset = Asset.crypto('BTC');
      const json = JSON.stringify(asset);
      const parsed = JSON.parse(json);
      expect(parsed).toEqual({ type: 'crypto', symbol: 'BTC' });
      expect('network' in parsed).toBe(false);
    });

    it('crypto with network includes network field', () => {
      const asset = Asset.crypto('ETH', 'ethereum');
      const json = JSON.stringify(asset);
      expect(JSON.parse(json)).toEqual({ type: 'crypto', symbol: 'ETH', network: 'ethereum' });
    });
  });

  describe('JSON deserialization', () => {
    it('parses currency from JSON', () => {
      const json = '{"type":"currency","iso_code":"USD"}';
      const asset = JSON.parse(json) as CurrencyAsset;
      expect(asset.type).toBe('currency');
      expect(asset.iso_code).toBe('USD');
    });

    it('parses equity from JSON', () => {
      const json = '{"type":"equity","ticker":"AAPL","exchange":"NASDAQ"}';
      const asset = JSON.parse(json) as EquityAsset;
      expect(asset.type).toBe('equity');
      expect(asset.ticker).toBe('AAPL');
      expect(asset.exchange).toBe('NASDAQ');
    });

    it('parses equity without exchange from JSON', () => {
      const json = '{"type":"equity","ticker":"AAPL"}';
      const asset = JSON.parse(json) as EquityAsset;
      expect(asset.type).toBe('equity');
      expect(asset.ticker).toBe('AAPL');
      expect(asset.exchange).toBeUndefined();
    });

    it('parses crypto from JSON', () => {
      const json = '{"type":"crypto","symbol":"BTC","network":"bitcoin"}';
      const asset = JSON.parse(json) as CryptoAsset;
      expect(asset.type).toBe('crypto');
      expect(asset.symbol).toBe('BTC');
      expect(asset.network).toBe('bitcoin');
    });

    it('parses crypto without network from JSON', () => {
      const json = '{"type":"crypto","symbol":"BTC"}';
      const asset = JSON.parse(json) as CryptoAsset;
      expect(asset.type).toBe('crypto');
      expect(asset.symbol).toBe('BTC');
      expect(asset.network).toBeUndefined();
    });

    it('round-trips through JSON correctly', () => {
      const assets = [
        Asset.currency('USD'),
        Asset.equity('AAPL', 'NASDAQ'),
        Asset.equity('GOOG'),
        Asset.crypto('BTC'),
        Asset.crypto('ETH', 'ethereum'),
      ];

      for (const original of assets) {
        const roundTripped = JSON.parse(JSON.stringify(original));
        expect(Asset.equals(roundTripped, original)).toBe(true);
      }
    });
  });
});
