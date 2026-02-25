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
  let brokerageTxRequestBodies: unknown[];
  let bankingTxRequestBodies: unknown[];

  beforeEach(async () => {
    tmpDir = await fs.mkdtemp(path.join(os.tmpdir(), 'keepbook-schwab-test-'));
    brokerageTxRequestBodies = [];
    bankingTxRequestBodies = [];

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
                AccountNumberDisplayFull: '4400-33623420',
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

      if (
        req.method === 'POST' &&
        url === '/api/is.TransactionHistoryWeb/TransactionHistoryInterface/TransactionHistory/brokerage/transactions'
      ) {
        const chunks: Buffer[] = [];
        req.on('data', (chunk) => chunks.push(Buffer.from(chunk)));
        req.on('end', () => {
          const parsed = JSON.parse(Buffer.concat(chunks).toString() || '{}') as {
            bookmark?: unknown;
            selectedAccountId?: string;
          };
          brokerageTxRequestBodies.push(parsed);

          if (parsed.selectedAccountId === 'acct-1' && (parsed.bookmark ?? null) === null) {
            res.end(
              JSON.stringify({
                bookmark: {
                  fromKey: { primarySortCode: null, primarySortValue: '' },
                  fromExecutionDate: '2022-12-21T00:00:00',
                  fromPublTimeStamp: '2022-12-21 13:27:00.423163',
                  fromSecondarySortCode: '4',
                  fromSecondarySortValue: 'FG',
                  fromTertiarySortValue: '0.00000',
                },
                brokerageTransactions: [
                  {
                    transactionDate: '02/10/2026',
                    action: 'Buy',
                    symbol: 'AAPL',
                    description: 'APPLE INC',
                    shareQuantity: '2',
                    executionPrice: '$100.25',
                    feesAndCommission: '',
                    amount: '-$200.50',
                    sourceCode: '',
                    effectiveDate: '02/10/2026',
                    depositSequenceId: '0',
                    checkDate: '02/11/2026',
                    itemIssueId: 'row-1',
                    schwabOrderId: 'order-1',
                  },
                ],
              }),
            );
            return;
          }

          if (parsed.selectedAccountId === 'acct-1') {
            res.end(
              JSON.stringify({
                bookmark: null,
                brokerageTransactions: [
                  {
                    transactionDate: '01/13/2026 as of 12/31/2025',
                    action: 'Cash In Lieu',
                    symbol: 'FG',
                    description: 'F&G ANNUITIES & LIFE INC',
                    shareQuantity: '',
                    executionPrice: '',
                    feesAndCommission: '',
                    amount: '$9.05',
                    sourceCode: 'CIL',
                    effectiveDate: '12/31/2025',
                    depositSequenceId: '1',
                    checkDate: '01/13/2026',
                    itemIssueId: 'row-2',
                    schwabOrderId: '0',
                  },
                ],
              }),
            );
            return;
          }

          if (parsed.selectedAccountId === 'acct-2') {
            res.end(
              JSON.stringify({
                bookmark: null,
                brokerageTransactions: [
                  {
                    transactionDate: '02/05/2026',
                    action: 'Bill Pay',
                    symbol: '',
                    description: 'RENT PAYMENT',
                    shareQuantity: '',
                    executionPrice: '',
                    feesAndCommission: '',
                    amount: '-$3000.00',
                    sourceCode: 'BillPay',
                    effectiveDate: '02/05/2026',
                    depositSequenceId: '1',
                    checkDate: '02/05/2026',
                    itemIssueId: 'row-checking-1',
                    schwabOrderId: '0',
                  },
                ],
              }),
            );
            return;
          }

          res.end(
            JSON.stringify({
              bookmark: null,
              brokerageTransactions: [],
            }),
          );
        });
        return;
      }

      if (
        req.method === 'POST' &&
        url === '/api/is.TransactionHistoryWeb/TransactionHistoryInterface/TransactionHistory/banking/non-pledged-asset-line/transactions'
      ) {
        const chunks: Buffer[] = [];
        req.on('data', (chunk) => chunks.push(Buffer.from(chunk)));
        req.on('end', () => {
          const parsed = JSON.parse(Buffer.concat(chunks).toString() || '{}') as {
            selectedAccountId?: string;
            pageNumber?: string;
          };
          bankingTxRequestBodies.push(parsed);

          if (parsed.selectedAccountId === '440033623420') {
            res.end(
              JSON.stringify({
                postedTransactions: [
                  {
                    postingDate: '02/05/2026',
                    description: 'RENT PAYMENT',
                    type: 'ACH',
                    withdrawalAmount: '$3,000.00',
                    depositAmount: '',
                    runningBalance: '$50.50',
                    checkSequenceNumber: '0',
                  },
                ],
                pendingTransactions: [],
                pagingInformation: {
                  number: Number(parsed.pageNumber ?? '0'),
                  moreRecords: false,
                },
              }),
            );
            return;
          }

          res.end(
            JSON.stringify({
              postedTransactions: [],
              pendingTransactions: [],
              pagingInformation: {
                number: Number(parsed.pageNumber ?? '0'),
                moreRecords: false,
              },
            }),
          );
        });
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

    // Transactions are fetched for brokerage and checking.
    expect(result.transactions).toHaveLength(2);
    const brokerageTxns = result.transactions.find(([id]) => id.equals(brokerage!.id))?.[1] ?? [];
    const checkingTxns = result.transactions.find(([id]) => id.equals(checking!.id))?.[1] ?? [];
    expect(brokerageTxns).toHaveLength(2);
    expect(brokerageTxns[0].amount).toBe('-200.50');
    expect(brokerageTxns[1].amount).toBe('9.05');
    expect(checkingTxns).toHaveLength(1);
    expect(checkingTxns[0].description).toContain('RENT PAYMENT');
    expect(checkingTxns[0].amount).toBe('-3000.00');

    expect(brokerageTxRequestBodies).toHaveLength(2);
    expect((brokerageTxRequestBodies[0] as { timeFrame?: string }).timeFrame).toBe('All');
    expect((brokerageTxRequestBodies[0] as { selectedAccountId?: string }).selectedAccountId).toBe('acct-1');
    expect((brokerageTxRequestBodies[0] as { bookmark?: unknown }).bookmark).toBeNull();
    expect((brokerageTxRequestBodies[1] as { selectedAccountId?: string }).selectedAccountId).toBe('acct-1');
    expect((brokerageTxRequestBodies[1] as { bookmark?: unknown }).bookmark).not.toBeNull();

    expect(bankingTxRequestBodies).toHaveLength(1);
    expect((bankingTxRequestBodies[0] as { timeFrame?: string }).timeFrame).toBe('All');
    expect((bankingTxRequestBodies[0] as { selectedAccountId?: string }).selectedAccountId).toBe(
      '440033623420',
    );
    expect((bankingTxRequestBodies[0] as { pageNumber?: string }).pageNumber).toBe('0');
  });

  it('fails fast when session is missing', async () => {
    const storage = new MemoryStorage();
    const conn = Connection.new({ name: 'Schwab', synchronizer: 'schwab' });
    const cache = SessionCache.withPath(tmpDir);

    const syncer = new SchwabSynchronizer(conn.state.id, cache);
    await expect(syncer.sync(conn, storage)).rejects.toThrow(/No session found/);
  });
});
