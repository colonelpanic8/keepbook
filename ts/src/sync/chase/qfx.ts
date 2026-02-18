/**
 * Minimal QFX/OFX (OFX 1.x SGML) parser for Chase downloads.
 *
 * This intentionally parses only a small subset:
 * - statement kind (bank vs credit card)
 * - currency, account id/type
 * - ledger balance (optional)
 * - STMTTRN list with FITID/DTPOSTED/TRNAMT/NAME/MEMO and a few extras
 */

import { Decimal } from '../../decimal.js';

export type StatementKind = 'bank' | 'credit_card';

export interface QfxTransaction {
  readonly fitid: string;
  readonly posted_at: Date;
  readonly amount: Decimal;
  readonly trn_type?: string;
  readonly name?: string;
  readonly memo?: string;
  readonly check_num?: string;
  readonly ref_num?: string;
}

export interface QfxStatement {
  readonly kind: StatementKind;
  readonly currency?: string;
  readonly account_id: string;
  readonly account_type?: string;
  readonly ledger_balance?: Decimal;
  readonly ledger_balance_as_of?: Date;
  readonly transactions: QfxTransaction[];
}

export function parseQfxStatement(content: string): QfxStatement {
  const ofx = extractOfxBody(content);
  const ofxU = ofx.toUpperCase();

  const kind: StatementKind =
    ofxU.includes('<CCSTMTTRNRS>') || ofxU.includes('<CREDITCARDMSGSRSV1>') || ofxU.includes('<CCSTMTRS>')
      ? 'credit_card'
      : 'bank';
  const acctContainer = kind === 'credit_card' ? 'CCACCTFROM' : 'BANKACCTFROM';

  const currency = extractTagValue(ofx, 'CURDEF')?.trim();

  const acctBlock = extractBlock(ofx, acctContainer);
  if (acctBlock === null) throw new Error(`Missing <${acctContainer}> block`);
  const accountId = extractTagValue(acctBlock, 'ACCTID')?.trim();
  if (!accountId) throw new Error(`Missing <ACCTID> within <${acctContainer}>`);
  const accountType = extractTagValue(acctBlock, 'ACCTTYPE')?.trim();

  let ledgerBalance: Decimal | undefined;
  let ledgerBalanceAsOf: Date | undefined;
  const ledger = extractBlock(ofx, 'LEDGERBAL');
  if (ledger !== null) {
    const balAmt = extractTagValue(ledger, 'BALAMT')?.trim();
    if (balAmt) ledgerBalance = new Decimal(balAmt);
    const asOf = extractTagValue(ledger, 'DTASOF')?.trim();
    if (asOf) ledgerBalanceAsOf = parseOfxDateTime(asOf);
  }

  const transactions: QfxTransaction[] = [];
  for (const trn of extractBlocks(ofx, 'STMTTRN')) {
    const fitid = extractTagValue(trn, 'FITID')?.trim();
    if (!fitid) throw new Error('Missing <FITID> in <STMTTRN>');

    const postedRaw = extractTagValue(trn, 'DTPOSTED')?.trim();
    if (!postedRaw) throw new Error(`Missing <DTPOSTED> for FITID=${fitid}`);
    const postedAt = parseOfxDateTime(postedRaw);

    const amtRaw = extractTagValue(trn, 'TRNAMT')?.trim();
    if (!amtRaw) throw new Error(`Missing <TRNAMT> for FITID=${fitid}`);
    const amount = new Decimal(amtRaw);

    const trnType = extractTagValue(trn, 'TRNTYPE')?.trim();
    const name = extractTagValue(trn, 'NAME')?.trim();
    const memo = extractTagValue(trn, 'MEMO')?.trim();
    const checkNum = extractTagValue(trn, 'CHECKNUM')?.trim();
    const refNum = extractTagValue(trn, 'REFNUM')?.trim();

    transactions.push({
      fitid,
      posted_at: postedAt,
      amount,
      trn_type: trnType && trnType !== '' ? trnType : undefined,
      name: name && name !== '' ? name : undefined,
      memo: memo && memo !== '' ? memo : undefined,
      check_num: checkNum && checkNum !== '' ? checkNum : undefined,
      ref_num: refNum && refNum !== '' ? refNum : undefined,
    });
  }

  return {
    kind,
    currency: currency && currency !== '' ? currency : undefined,
    account_id: accountId,
    account_type: accountType && accountType !== '' ? accountType : undefined,
    ledger_balance: ledgerBalance,
    ledger_balance_as_of: ledgerBalanceAsOf,
    transactions,
  };
}

function extractOfxBody(content: string): string {
  const u = content.toUpperCase();
  const idx = u.indexOf('<OFX>');
  return idx === -1 ? content : content.slice(idx);
}

function extractBlocks(content: string, tag: string): string[] {
  const tagU = tag.toUpperCase();
  const u = content.toUpperCase();
  const open = `<${tagU}>`;
  const close = `</${tagU}>`;

  const blocks: string[] = [];
  let searchFrom = 0;
  for (;;) {
    const startRel = u.indexOf(open, searchFrom);
    if (startRel === -1) break;
    const start = startRel + open.length;

    let end = u.indexOf(close, start);
    if (end === -1) {
      const nextOpen = u.indexOf(open, start);
      end = nextOpen === -1 ? content.length : nextOpen;
    }

    blocks.push(content.slice(start, end));
    searchFrom = Math.min(end, content.length);
  }
  return blocks;
}

function extractBlock(content: string, tag: string): string | null {
  const blocks = extractBlocks(content, tag);
  return blocks.length > 0 ? blocks[0] : null;
}

function extractTagValue(content: string, tag: string): string | null {
  const tagU = tag.toUpperCase();
  const u = content.toUpperCase();
  const needle = `<${tagU}>`;
  const start = u.indexOf(needle);
  if (start === -1) return null;
  const after = start + needle.length;
  const rest = content.slice(after);
  const end = rest.indexOf('<');
  const value = (end === -1 ? rest : rest.slice(0, end)).trim();
  return value === '' ? null : value;
}

export function parseOfxDateTime(raw: string): Date {
  // OFX 1.x datetime often looks like:
  //   YYYYMMDD
  //   YYYYMMDDHHMMSS
  //   YYYYMMDDHHMMSS.XXX
  //   YYYYMMDDHHMMSS[-5:EST]
  // We parse the leading digits, then apply the bracketed hour offset if present.
  let digits = '';
  for (let i = 0; i < raw.length; i++) {
    const ch = raw[i];
    if (ch >= '0' && ch <= '9') digits += ch;
    else break;
  }
  if (digits.length < 8) throw new Error(`OFX datetime has fewer than 8 leading digits: ${raw}`);

  const year = Number.parseInt(digits.slice(0, 4), 10);
  const month = Number.parseInt(digits.slice(4, 6), 10);
  const day = Number.parseInt(digits.slice(6, 8), 10);

  let hour = 0;
  let minute = 0;
  let second = 0;
  if (digits.length >= 14) {
    hour = Number.parseInt(digits.slice(8, 10), 10);
    minute = Number.parseInt(digits.slice(10, 12), 10);
    second = Number.parseInt(digits.slice(12, 14), 10);
  }

  // Interpret the parsed components as "local" time in the offset zone (if present).
  let utcMs = Date.UTC(year, month - 1, day, hour, minute, second);

  const bi = raw.indexOf('[');
  if (bi !== -1) {
    const end = raw.indexOf(']', bi + 1);
    if (end !== -1) {
      const inner = raw.slice(bi + 1, end);
      const offPart = inner.split(':')[0]?.trim() ?? '';
      if (offPart !== '') {
        const sign = offPart.startsWith('-') ? -1 : 1;
        const offDigits = offPart.replace(/[^0-9]/g, '');
        if (offDigits !== '') {
          let hh = 0;
          let mm = 0;
          if (offDigits.length <= 2) {
            hh = Number.parseInt(offDigits, 10);
          } else if (offDigits.length === 4) {
            hh = Number.parseInt(offDigits.slice(0, 2), 10);
            mm = Number.parseInt(offDigits.slice(2, 4), 10);
          }
          const offsetSecs = sign * (hh * 3600 + mm * 60);
          utcMs -= offsetSecs * 1000;
        }
      }
    }
  }

  return new Date(utcMs);
}

