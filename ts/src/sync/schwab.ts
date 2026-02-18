import { v4 as uuidv4 } from 'uuid';

import type { SessionData } from '../credentials/session.js';
import { SessionDataUtil } from '../credentials/session.js';
import { Id } from '../models/id.js';
import { Asset } from '../models/asset.js';
import {
  Transaction,
  type TransactionType,
  withId,
  withSynchronizerData,
  withTimestamp,
} from '../models/transaction.js';

export type AccountsResponse = {
  accounts: SchwabAccount[];
};

export type SchwabAccount = {
  AccountId: string;
  AccountNumberDisplay: string;
  AccountNumberDisplayFull: string;
  DefaultName: string;
  NickName: string;
  AccountType: string;
  IsBrokerage: boolean;
  IsBank: boolean;
  Balances?: AccountBalances;
};

export type AccountBalances = {
  Balance: number;
  DayChange: number;
  DayChangePct: number;
  Cash?: number;
  MarketValue?: number;
};

export type PositionsResponse = {
  security_groupings: SecurityGrouping[];
};

export type SecurityGrouping = {
  GroupName: string;
  Positions: Position[];
};

export type Position = {
  DefaultSymbol: string;
  Description: string;
  Quantity: number;
  Price: number;
  MarketValue: number;
  Cost: number;
  ProfitLoss: number;
  ProfitLossPercent: number;
  DayChange: number;
  PercentDayChange: number;
};

export class SchwabClient {
  static readonly API_BASE =
    'https://ausgateway.schwab.com/api/is.ClientSummaryExpWeb/V1/api';
  static readonly API_BASE_KEY = 'api_base';

  private readonly session: SessionData;
  private readonly apiBase: string;

  constructor(session: SessionData) {
    this.session = session;
    this.apiBase = (session.data?.[SchwabClient.API_BASE_KEY] ?? SchwabClient.API_BASE).toString();
  }

  private async request<T>(path: string): Promise<T> {
    const base = this.apiBase.replace(/\/+$/g, '');
    const url = `${base}${path}`;

    const token = this.session.token ?? null;
    if (token === null || token === undefined || token === '') {
      throw new Error('No bearer token in session');
    }

    const headers: Record<string, string> = {
      authorization: `Bearer ${token}`,
      'schwab-client-channel': 'IO',
      'schwab-client-correlid': uuidv4(),
      'schwab-env': 'PROD',
      'schwab-resource-version': '1',
      origin: 'https://client.schwab.com',
      referer: 'https://client.schwab.com/',
      accept: 'application/json',
    };

    const cookie = SessionDataUtil.cookieHeader(this.session);
    if (cookie !== '') {
      headers.cookie = cookie;
    }

    const resp = await fetch(url, { method: 'GET', headers });
    const body = await resp.text();
    if (!resp.ok) {
      throw new Error(`API request failed (${resp.status}): ${body}`);
    }

    try {
      return JSON.parse(body) as T;
    } catch (e: unknown) {
      throw new Error(`Failed to parse JSON response: ${(e as Error).message}`);
    }
  }

  async getAccounts(): Promise<AccountsResponse> {
    const raw = await this.request<{ Accounts: SchwabAccount[] }>('/Account?includeCustomGroups=true');
    return { accounts: raw.Accounts ?? [] };
  }

  async getPositions(): Promise<PositionsResponse> {
    // Schwab uses PascalCase keys; we keep that in types.
    // The response root shape contains `SecurityGroupings`.
    const raw = await this.request<{ SecurityGroupings: SecurityGrouping[] }>('/AggregatedPositions');
    return { security_groupings: raw.SecurityGroupings ?? [] };
  }
}

export function parseExportedSession(json: string): SessionData {
  const parsed = JSON.parse(json) as { token: string; cookies?: Record<string, string> };
  const rawToken = parsed.token ?? '';
  const token = rawToken.startsWith('Bearer ') ? rawToken.slice('Bearer '.length) : rawToken;
  return {
    token,
    cookies: parsed.cookies ?? {},
    captured_at: Math.floor(Date.now() / 1000),
    data: {},
  };
}

export type SchwabTransactionsImportResult = {
  transactions: TransactionType[];
  skipped: number;
};

function normalizeWs(s: string): string {
  return s.split(/\s+/g).filter(Boolean).join(' ');
}

function normalizeAmount(raw: string): string | null {
  let s = raw.trim();
  if (s === '') return null;

  let negative = false;
  if (s.startsWith('(') && s.endsWith(')') && s.length >= 2) {
    negative = true;
    s = s.slice(1, -1);
  }

  s = s.trim().replace(/\$/g, '').replace(/,/g, '');

  if (s.startsWith('-')) {
    negative = true;
    s = s.slice(1);
  } else if (s.startsWith('+')) {
    s = s.slice(1);
  }

  s = s.trim();
  if (s === '') return null;

  return negative ? `-${s}` : s;
}

function extractMmddyyyyDates(raw: string): Array<{ y: number; m: number; d: number }> {
  const out: Array<{ y: number; m: number; d: number }> = [];
  if (raw.length < 10) return out;

  for (let i = 0; i <= raw.length - 10; i += 1) {
    const sub = raw.slice(i, i + 10);
    if (!/^\d{2}\/\d{2}\/\d{4}$/.test(sub)) continue;
    const [mm, dd, yyyy] = sub.split('/');
    const m = Number(mm);
    const d = Number(dd);
    const y = Number(yyyy);
    if (!Number.isFinite(y) || !Number.isFinite(m) || !Number.isFinite(d)) continue;
    if (m < 1 || m > 12 || d < 1 || d > 31) continue;
    const last = out[out.length - 1] ?? null;
    if (last === null || last.y !== y || last.m !== m || last.d !== d) out.push({ y, m, d });
  }
  return out;
}

function isoDate(d: { y: number; m: number; d: number }): string {
  const mm = String(d.m).padStart(2, '0');
  const dd = String(d.d).padStart(2, '0');
  return `${d.y}-${mm}-${dd}`;
}

function getField(row: Record<string, unknown>, key: string): string | null {
  const v = row[key];
  if (v === null || v === undefined) return null;
  if (typeof v === 'string') return v;
  if (typeof v === 'number' || typeof v === 'boolean') return String(v);
  return JSON.stringify(v);
}

/**
 * Parse Schwab's "transaction history" JSON export into keepbook transactions.
 *
 * Notes:
 * - Only rows with a parseable `Amount` field are imported; others are skipped.
 * - Schwab exports often do not include a stable transaction id; IDs are generated deterministically
 *   from a fingerprint of the row contents plus a per-fingerprint occurrence counter.
 */
export function parseSchwabExportedTransactionsJson(
  accountId: Id,
  json: string,
): SchwabTransactionsImportResult {
  const parsed = JSON.parse(json) as unknown;
  if (!Array.isArray(parsed)) {
    throw new Error('Expected Schwab export to be a JSON array');
  }

  const transactions: TransactionType[] = [];
  let skipped = 0;
  const seenCounts = new Map<string, number>();

  for (const row of parsed) {
    if (row === null || typeof row !== 'object' || Array.isArray(row)) {
      skipped += 1;
      continue;
    }
    const obj = row as Record<string, unknown>;

    const dateRaw = getField(obj, 'Date') ?? '';
    const dates = extractMmddyyyyDates(dateRaw);
    const primary = dates[0] ?? null;
    if (primary === null) {
      skipped += 1;
      continue;
    }
    const asOf = dates[1] ?? null;

    const action = getField(obj, 'Action');
    const symbol = getField(obj, 'Symbol');
    const description = getField(obj, 'Description');
    const quantity = getField(obj, 'Quantity');
    const price = getField(obj, 'Price');
    const feesComm = getField(obj, 'Fees & Comm');
    const amountRaw = getField(obj, 'Amount');

    const amountNorm = amountRaw !== null ? normalizeAmount(amountRaw) : null;
    if (amountNorm === null) {
      skipped += 1;
      continue;
    }

    const parts: string[] = [];
    if (action !== null && normalizeWs(action) !== '') parts.push(normalizeWs(action));
    if (symbol !== null && normalizeWs(symbol) !== '') parts.push(normalizeWs(symbol));
    if (description !== null && normalizeWs(description) !== '') parts.push(normalizeWs(description));
    const desc = parts.length === 0 ? 'Schwab transaction' : parts.join(' ');

    const dateIso = isoDate(primary);
    const asOfIso = asOf !== null ? isoDate(asOf) : null;

    const fingerprint = [
      `date=${dateIso}`,
      `asof=${asOfIso ?? ''}`,
      `action=${action ? normalizeWs(action) : ''}`,
      `symbol=${symbol ? normalizeWs(symbol) : ''}`,
      `desc=${description ? normalizeWs(description) : ''}`,
      `qty=${quantity ? normalizeWs(quantity) : ''}`,
      `price=${price ? normalizeWs(price) : ''}`,
      `fees=${feesComm ? normalizeWs(feesComm) : ''}`,
      `amount=${amountNorm}`,
    ].join('|');

    const next = (seenCounts.get(fingerprint) ?? 0) + 1;
    seenCounts.set(fingerprint, next);

    const txId = Id.fromExternal(`schwab:export:${accountId.asStr()}:${fingerprint}:${next}`);
    const timestamp = new Date(Date.UTC(primary.y, primary.m - 1, primary.d, 0, 0, 0));

    const syncData = {
      source: 'schwab_export_json',
      date_raw: dateRaw,
      date: dateIso,
      as_of_date: asOfIso,
      action,
      symbol,
      description,
      quantity,
      price,
      fees_comm: feesComm,
      amount_raw: amountRaw,
      amount: amountNorm,
      fingerprint,
      occurrence: next,
    };

    let tx = Transaction.new(amountNorm, Asset.currency('USD'), desc);
    tx = withTimestamp(tx, timestamp);
    tx = withId(tx, txId);
    tx = withSynchronizerData(tx, syncData);
    transactions.push(tx);
  }

  return { transactions, skipped };
}
