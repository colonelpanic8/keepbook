import { describe, it, expect } from 'vitest';

import { Id } from '../models/id.js';
import { parseExportedSession, parseSchwabExportedTransactionsJson } from './schwab.js';

describe('schwab session parsing', () => {
  it('strips Bearer prefix from exported token', () => {
    const session = parseExportedSession(JSON.stringify({ token: 'Bearer test-token', cookies: {} }));
    expect(session.token).toBe('test-token');
  });
});

describe('schwab transaction export parsing', () => {
  it('parses JSON export rows and generates deterministic ids', () => {
    const json = JSON.stringify([
      {
        Date: '10/11/2024',
        Action: 'Exchange or Exercise',
        Symbol: 'XPOA',
        Description: 'XPOA CBOE PUT NOV 24 7.5',
        Quantity: '2',
        Price: '0',
        'Fees & Comm': '0',
        Amount: '0',
      },
      {
        Date: '05/20/2024 as of 05/17/2024',
        Action: 'Dividend',
        Symbol: 'VTI',
        Description: 'VANGUARD TOTAL STOCK MARKET ETF',
        Amount: '$1.23',
      },
    ]);

    const accountId = Id.fromString('acct-1');
    const first = parseSchwabExportedTransactionsJson(accountId, json);
    expect(first.skipped).toBe(0);
    expect(first.transactions).toHaveLength(2);
    expect(first.transactions[1].amount).toBe('1.23');

    const second = parseSchwabExportedTransactionsJson(accountId, json);
    expect(first.transactions[0].id.asStr()).toBe(second.transactions[0].id.asStr());
    expect(first.transactions[1].id.asStr()).toBe(second.transactions[1].id.asStr());
  });
});
