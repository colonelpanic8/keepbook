import { describe, it, expect } from 'vitest';

import { MemoryStorage } from '../../storage/memory.js';
import { Connection } from '../../models/connection.js';
import { FixedIdGenerator } from '../../models/id-generator.js';
import { Id } from '../../models/id.js';
import { FixedClock } from '../../clock.js';

import { parseAndImportQfx } from './import.js';

describe('Chase QFX import (TypeScript)', () => {
  it('imports accounts, balances, and transactions deterministically', async () => {
    const ids = new FixedIdGenerator([Id.fromString('conn-1')]);
    const clock = new FixedClock(new Date('2026-02-10T00:00:00Z'));
    const conn = Connection.new({ name: 'Chase Bank', synchronizer: 'chase' }, ids, clock);

    const storage = new MemoryStorage();
    await storage.saveConnection(conn);

    const qfx = `<OFX>
<BANKMSGSRSV1>
<STMTTRNRS>
<TRNUID>1
<STATUS>
<CODE>0
<SEVERITY>INFO
</STATUS>
<STMTRS>
<CURDEF>USD
<BANKACCTFROM>
<ACCTID>123456789
<ACCTTYPE>CHECKING
</BANKACCTFROM>
<BANKTRANLIST>
<STMTTRN>
<TRNTYPE>DEBIT
<DTPOSTED>20260205120000[-5:EST]
<TRNAMT>-12.3400
<FITID>abc-1
<NAME>COFFEE SHOP
<MEMO>LATTE
</STMTTRN>
</BANKTRANLIST>
<LEDGERBAL>
<BALAMT>1000.00
<DTASOF>20260206120000[-5:EST]
</LEDGERBAL>
</STMTRS>
</STMTTRNRS>
</BANKMSGSRSV1>
</OFX>`;

    const result = await parseAndImportQfx(conn, storage, [qfx]);

    expect(result.accounts).toHaveLength(1);
    const acct = result.accounts[0];
    expect(acct.name).toBe('Chase (6789)');
    expect(acct.tags).toContain('chase');
    expect(acct.tags).toContain('bank');
    expect(acct.tags).toContain('checking');

    const expectedAccountId = Id.fromExternal('chase:conn-1:123456789').asStr();
    expect(acct.id.asStr()).toBe(expectedAccountId);

    expect(result.balances).toHaveLength(1);
    const [balAcctId, sab] = result.balances[0];
    expect(balAcctId.asStr()).toBe(expectedAccountId);
    expect(sab).toHaveLength(1);
    expect(sab[0].asset_balance.amount).toBe('1000');
    expect(sab[0].asset_balance.asset).toEqual({ type: 'currency', iso_code: 'USD' });

    expect(result.transactions).toHaveLength(1);
    const [txAcctId, txns] = result.transactions[0];
    expect(txAcctId.asStr()).toBe(expectedAccountId);
    expect(txns).toHaveLength(1);
    expect(txns[0].amount).toBe('-12.34');
    expect(txns[0].description).toBe('COFFEE SHOP - LATTE');
    expect(txns[0].timestamp.toISOString()).toBe('2026-02-05T17:00:00.000Z');

    expect(result.connection.state.account_ids.map((id) => id.asStr())).toEqual([expectedAccountId]);
    expect(result.connection.state.last_sync?.status).toBe('success');
  });
});
