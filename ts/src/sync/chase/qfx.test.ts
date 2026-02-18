import { describe, it, expect } from 'vitest';

import { parseOfxDateTime, parseQfxStatement } from './qfx.js';

describe('QFX/OFX parser (TypeScript)', () => {
  it('parses a minimal bank statement', () => {
    const qfx = `OFXHEADER:100
DATA:OFXSGML
VERSION:102
SECURITY:NONE
ENCODING:USASCII
CHARSET:1252
COMPRESSION:NONE
OLDFILEUID:NONE
NEWFILEUID:NONE

<OFX>
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
<BANKID>021000021
<ACCTID>123456789
<ACCTTYPE>CHECKING
</BANKACCTFROM>
<BANKTRANLIST>
<DTSTART>20260101000000[-5:EST]
<DTEND>20260201000000[-5:EST]
<STMTTRN>
<TRNTYPE>DEBIT
<DTPOSTED>20260205120000[-5:EST]
<TRNAMT>-12.3400
<FITID>202602050001
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

    const stmt = parseQfxStatement(qfx);
    expect(stmt.kind).toBe('bank');
    expect(stmt.currency).toBe('USD');
    expect(stmt.account_id).toBe('123456789');
    expect(stmt.account_type).toBe('CHECKING');
    expect(stmt.ledger_balance?.toFixed()).toBe('1000');
    expect(stmt.ledger_balance_as_of?.toISOString()).toBe('2026-02-06T17:00:00.000Z');
    expect(stmt.transactions).toHaveLength(1);
    const t = stmt.transactions[0];
    expect(t.fitid).toBe('202602050001');
    expect(t.amount.toFixed()).toBe('-12.34');
    expect(t.posted_at.toISOString()).toBe('2026-02-05T17:00:00.000Z');
    expect(t.name).toBe('COFFEE SHOP');
    expect(t.memo).toBe('LATTE');
    expect(t.trn_type).toBe('DEBIT');
  });

  it('parses date-only OFX datetimes', () => {
    expect(parseOfxDateTime('20260205').toISOString()).toBe('2026-02-05T00:00:00.000Z');
  });
});

