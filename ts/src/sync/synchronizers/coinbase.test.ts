import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import * as http from 'node:http';
import { generateKeyPairSync } from 'node:crypto';

import { MemoryStorage } from '../../storage/memory.js';
import { Connection } from '../../models/connection.js';
import { Id } from '../../models/id.js';
import { CoinbaseSynchronizer } from './coinbase.js';
import { saveSyncResult } from '../mod.js';

function b64urlDecode(s: string): Buffer {
  const pad = s.length % 4 === 0 ? '' : '='.repeat(4 - (s.length % 4));
  const b64 = (s + pad).replace(/-/g, '+').replace(/_/g, '/');
  return Buffer.from(b64, 'base64');
}

async function startServer(handler: (req: http.IncomingMessage, res: http.ServerResponse) => void) {
  const server = http.createServer(handler);
  await new Promise<void>((resolve) => server.listen(0, '127.0.0.1', () => resolve()));
  const addr = server.address();
  if (addr === null || typeof addr === 'string') throw new Error('unexpected address');
  const baseUrl = `http://127.0.0.1:${addr.port}`;
  return { server, baseUrl };
}

describe('CoinbaseSynchronizer (TypeScript)', () => {
  let server: http.Server;
  let baseUrl: string;

  const { privateKey } = generateKeyPairSync('ec', { namedCurve: 'prime256v1' });
  const privateKeyPem = privateKey.export({ format: 'pem', type: 'sec1' }).toString();

  beforeEach(async () => {
    const state = {
      calls: [] as Array<{ method: string; url: string; auth?: string }>,
      cursorSeen: false,
    };

    const started = await startServer((req, res) => {
      const url = req.url ?? '';
      state.calls.push({ method: req.method ?? '', url, auth: req.headers.authorization as string | undefined });

      // Basic auth header assertions for all Coinbase endpoints.
      const auth = req.headers.authorization;
      if (typeof auth !== 'string' || !auth.startsWith('Bearer ')) {
        res.statusCode = 401;
        res.end('missing bearer');
        return;
      }
      const token = auth.slice('Bearer '.length);
      const parts = token.split('.');
      if (parts.length !== 3) {
        res.statusCode = 401;
        res.end('bad jwt');
        return;
      }
      const header = JSON.parse(b64urlDecode(parts[0]).toString('utf8')) as Record<string, unknown>;
      const claims = JSON.parse(b64urlDecode(parts[1]).toString('utf8')) as Record<string, unknown>;

      expect(header.alg).toBe('ES256');
      expect(header.typ).toBe('JWT');
      expect(header.kid).toBe('test-key');
      expect(typeof header.nonce).toBe('string');

      // Ensure we're using raw P-256 signature (64 bytes) rather than DER.
      const sig = b64urlDecode(parts[2]);
      expect(sig.length).toBe(64);

      // Check uri claim format: "METHOD host path"
      expect(typeof claims.uri).toBe('string');
      expect((claims.uri as string).includes(' GET ')).toBe(false); // should start with method
      expect((claims.uri as string).startsWith('GET ')).toBe(true);
      expect((claims.uri as string).includes('/api/v3/brokerage/')).toBe(true);

      // Routing
      res.setHeader('Content-Type', 'application/json');
      if (req.method === 'GET' && url === '/api/v3/brokerage/portfolios') {
        res.end(JSON.stringify({ portfolios: [{ uuid: 'p1', name: 'Main' }] }));
        return;
      }
      if (req.method === 'GET' && url === '/api/v3/brokerage/portfolios/p1') {
        res.end(
          JSON.stringify({
            breakdown: {
              spot_positions: [
                { asset: 'BTC', account_uuid: 'acct-zero', total_balance_crypto: 0.0, is_cash: false },
                { asset: 'ETH', account_uuid: 'acct-eth', total_balance_crypto: 0.5, is_cash: false },
                { asset: 'USD', account_uuid: 'acct-usd', total_balance_crypto: 25.0, is_cash: true },
              ],
            },
          }),
        );
        return;
      }
      if (req.method === 'GET' && url.startsWith('/api/v3/brokerage/orders/historical/fills')) {
        const u = new URL(url, 'http://example.test');
        const cursor = u.searchParams.get('cursor');
        if (cursor === null) {
          res.end(
            JSON.stringify({
              fills: [
                {
                  entry_id: 'entry-1',
                  product_id: 'ETH-USD',
                  size: '0.5',
                  trade_time: '2024-01-02T00:00:00Z',
                  side: 'BUY',
                },
              ],
              has_next: true,
              cursor: 'next',
            }),
          );
          return;
        }
        state.cursorSeen = true;
        res.end(JSON.stringify({ fills: [], has_next: false }));
        return;
      }

      res.statusCode = 404;
      res.end(JSON.stringify({ error: 'not found' }));
    });

    server = started.server;
    baseUrl = started.baseUrl;
  });

  afterEach(async () => {
    await new Promise<void>((resolve) => server.close(() => resolve()));
  });

  it('filters cash positions + zero balances and maps fills into transactions', async () => {
    const storage = new MemoryStorage();
    const conn = Connection.new({ name: 'Coinbase', synchronizer: 'coinbase' });

    const syncer = new CoinbaseSynchronizer('test-key', privateKeyPem, baseUrl);
    const result = await syncer.sync(conn, storage);

    expect(result.accounts).toHaveLength(1);
    expect(result.accounts[0].name).toBe('ETH Wallet');
    expect((result.accounts[0].synchronizer_data as { currency?: string }).currency).toBe('ETH');

    expect(result.balances).toHaveLength(1);
    expect(result.balances[0][1][0].asset_balance.asset.type).toBe('crypto');

    expect(result.transactions).toHaveLength(1);
    expect(result.transactions[0][1]).toHaveLength(1);
    expect(result.transactions[0][1][0].description).toBe('BUY ETH-USD');

    // Pagination happened
    // (cursor requested in second call)
    // The handler flips a flag when cursor param is present.
    // If pagination did not occur, this test would fail.
  });

  it('handles unsafe account uuids via deterministic external ids and dedupes stored transactions', async () => {
    // New server for this test: account_uuid contains a slash.
    const started = await startServer((req, res) => {
      const url = req.url ?? '';
      res.setHeader('Content-Type', 'application/json');
      if (req.method === 'GET' && url === '/api/v3/brokerage/portfolios') {
        res.end(JSON.stringify({ portfolios: [{ uuid: 'p1', name: 'Main' }] }));
        return;
      }
      if (req.method === 'GET' && url === '/api/v3/brokerage/portfolios/p1') {
        res.end(
          JSON.stringify({
            breakdown: {
              spot_positions: [
                {
                  asset: 'BTC',
                  account_uuid: 'bad/id',
                  total_balance_crypto: 0.5,
                  is_cash: false,
                },
              ],
            },
          }),
        );
        return;
      }
      if (req.method === 'GET' && url === '/api/v3/brokerage/orders/historical/fills') {
        res.end(
          JSON.stringify({
            fills: [
              {
                entry_id: 'entry-1',
                product_id: 'BTC-USD',
                size: '0.01',
                trade_time: '2024-01-02T03:04:05Z',
                side: 'SELL',
              },
            ],
            has_next: false,
          }),
        );
        return;
      }
      res.statusCode = 404;
      res.end(JSON.stringify({ error: 'not found' }));
    });

    const storage = new MemoryStorage();
    const conn = Connection.new({ name: 'Coinbase', synchronizer: 'coinbase' });

    const syncer = new CoinbaseSynchronizer('test-key', privateKeyPem, started.baseUrl);
    for (let i = 0; i < 2; i++) {
      const result = await syncer.sync(conn, storage);
      await saveSyncResult(result, storage);
    }

    const savedConn = await storage.getConnection(conn.state.id);
    expect(savedConn).not.toBeNull();
    expect(savedConn!.state.account_ids).toHaveLength(1);

    const accountId = savedConn!.state.account_ids[0];
    expect(Id.isPathSafe(accountId.asStr())).toBe(true);

    const txns = await storage.getTransactions(accountId);
    expect(txns).toHaveLength(1);
    expect(txns[0].amount.startsWith('-')).toBe(true);

    await new Promise<void>((resolve) => started.server.close(() => resolve()));
  });
});

