import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import * as http from 'node:http';
import * as os from 'node:os';
import * as path from 'node:path';
import * as fs from 'node:fs/promises';

import { MemoryStorage } from '../../storage/memory.js';
import { Connection } from '../../models/connection.js';
import { SessionCache } from '../../credentials/session.js';
import { SchwabSynchronizer } from './schwab.js';

async function startServer(handler: (req: http.IncomingMessage, res: http.ServerResponse) => void) {
  const server = http.createServer(handler);
  await new Promise<void>((resolve) => server.listen(0, '127.0.0.1', () => resolve()));
  const addr = server.address();
  if (addr === null || typeof addr === 'string') throw new Error('unexpected address');
  const baseUrl = `http://127.0.0.1:${addr.port}`;
  return { server, baseUrl };
}

describe('SchwabSynchronizer (TypeScript)', () => {
  let server: http.Server;
  let baseUrl: string;
  let tmpDir: string;

  beforeEach(async () => {
    tmpDir = await fs.mkdtemp(path.join(os.tmpdir(), 'keepbook-schwab-test-'));

    const started = await startServer((req, res) => {
      const url = req.url ?? '';
      res.setHeader('Content-Type', 'application/json');

      if (req.headers.authorization !== 'Bearer test-token') {
        res.statusCode = 401;
        res.end(JSON.stringify({ error: 'missing bearer' }));
        return;
      }

      if (req.method === 'GET' && url === '/Account?includeCustomGroups=true') {
        res.end(
          JSON.stringify({
            Accounts: [
              {
                AccountId: 'acct-1',
                AccountNumberDisplay: '...1234',
                AccountNumberDisplayFull: '0000...1234',
                DefaultName: 'Brokerage',
                NickName: '',
                AccountType: 'Brokerage',
                IsBrokerage: true,
                IsBank: false,
                Balances: {
                  Balance: 123.0,
                  DayChange: 0.0,
                  DayChangePct: 0.0,
                  Cash: 10.0,
                  MarketValue: 113.0,
                },
              },
              {
                AccountId: 'acct-2',
                AccountNumberDisplay: '...9999',
                AccountNumberDisplayFull: '0000...9999',
                DefaultName: 'Checking',
                NickName: 'My Checking',
                AccountType: 'Bank',
                IsBrokerage: false,
                IsBank: true,
                Balances: {
                  Balance: 50.5,
                  DayChange: 0.0,
                  DayChangePct: 0.0,
                },
              },
            ],
          }),
        );
        return;
      }

      if (req.method === 'GET' && url === '/AggregatedPositions') {
        res.end(
          JSON.stringify({
            SecurityGroupings: [
              {
                GroupName: 'Equities',
                Positions: [
                  {
                    DefaultSymbol: 'CASH',
                    Description: 'Cash',
                    Quantity: 1,
                    Price: 1,
                    MarketValue: 1,
                    Cost: 1,
                    ProfitLoss: 0,
                    ProfitLossPercent: 0,
                    DayChange: 0,
                    PercentDayChange: 0,
                  },
                  {
                    DefaultSymbol: 'AAPL',
                    Description: 'Apple',
                    Quantity: 2,
                    Price: 100.25,
                    MarketValue: 200.5,
                    Cost: 150,
                    ProfitLoss: 50.5,
                    ProfitLossPercent: 0.336,
                    DayChange: 1,
                    PercentDayChange: 0.01,
                  },
                ],
              },
            ],
          }),
        );
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
    await fs.rm(tmpDir, { recursive: true, force: true });
  });

  it('syncs accounts + balances using session cache and Schwab internal endpoints', async () => {
    const storage = new MemoryStorage();
    const conn = Connection.new({ name: 'Schwab', synchronizer: 'schwab' });

    const cache = SessionCache.withPath(tmpDir);
    cache.set(conn.state.id.asStr(), {
      token: 'test-token',
      cookies: { a: 'b' },
      captured_at: Math.floor(Date.now() / 1000),
      data: { api_base: baseUrl },
    });

    const syncer = new SchwabSynchronizer(conn.state.id, cache);
    const result = await syncer.sync(conn, storage);

    expect(result.accounts).toHaveLength(2);
    const brokerage = result.accounts.find((a) => a.name === 'Brokerage');
    const checking = result.accounts.find((a) => a.name === 'My Checking');
    expect(brokerage).toBeDefined();
    expect(checking).toBeDefined();

    // Brokerage: AAPL position (CASH filtered out) + cash balance from balances.Cash
    const brokerageBalances = result.balances.find(([id]) => id.equals(brokerage!.id))?.[1] ?? [];
    expect(brokerageBalances.some((b) => (b.asset_balance.asset as { type: string }).type === 'equity')).toBe(true);
    expect(brokerageBalances.some((b) => (b.asset_balance.asset as { type: string }).type === 'currency')).toBe(true);

    // Bank: total balance as USD
    const checkingBalances = result.balances.find(([id]) => id.equals(checking!.id))?.[1] ?? [];
    expect(checkingBalances).toHaveLength(1);
    expect((checkingBalances[0].asset_balance.asset as { type: string }).type).toBe('currency');
  });

  it('fails fast when session is missing', async () => {
    const storage = new MemoryStorage();
    const conn = Connection.new({ name: 'Schwab', synchronizer: 'schwab' });
    const cache = SessionCache.withPath(tmpDir);

    const syncer = new SchwabSynchronizer(conn.state.id, cache);
    await expect(syncer.sync(conn, storage)).rejects.toThrow(/No session found/);
  });
});

