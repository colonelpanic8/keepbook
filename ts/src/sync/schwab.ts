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

export type TransactionHistoryTimeFrame = 'Last6Months' | 'All';

export type TransactionHistoryBrokerageAccount = {
  id: string;
  nickName?: string;
};

type TransactionHistoryBookmark = {
  fromKey: {
    primarySortCode: string | null;
    primarySortValue: string;
  };
  fromExecutionDate: string;
  fromPublTimeStamp: string;
  fromSecondarySortCode: string;
  fromSecondarySortValue: string;
  fromTertiarySortValue: string;
};

type BrokerageTransactionsRequest = {
  accountNickname: string;
  exportType: 'Csv';
  includeOptionsInSearch: boolean;
  isSpsLinkedUkAccount: boolean;
  selectedAccountId: string;
  selectedTransactionTypes: string[];
  sortColumn: 'Date';
  sortDirection: 'Descending';
  symbol: string;
  timeFrame: TransactionHistoryTimeFrame;
  bookmark: TransactionHistoryBookmark | null;
  shouldPaginate: boolean;
};

type BrokerageTransactionsResponse = {
  brokerageTransactions?: BrokerageTransaction[];
  bookmark?: TransactionHistoryBookmark | null;
};

type TransactionHistoryInitResponse = {
  accountSelectorData?: {
    brokerageAccountList?: {
      brokerageAccounts?: TransactionHistoryBrokerageAccount[];
    };
  };
};

export type BrokerageTransaction = {
  transactionDate: string;
  action?: string;
  symbol?: string;
  description?: string;
  shareQuantity?: string;
  executionPrice?: string;
  feesAndCommission?: string;
  amount?: string;
  sourceCode?: string;
  effectiveDate?: string;
  depositSequenceId?: string;
  checkDate?: string;
  itemIssueId?: string;
  schwabOrderId?: string;
};

const TRANSACTION_HISTORY_MAX_PAGES = 20;
const TRANSACTION_HISTORY_INIT_PATH =
  '/api/is.TransactionHistoryWeb/TransactionHistoryInterface/TransactionHistory/init';
const TRANSACTION_HISTORY_PATH =
  '/api/is.TransactionHistoryWeb/TransactionHistoryInterface/TransactionHistory/brokerage/transactions';
const TRANSACTION_TYPES = [
  'Adjustments',
  'AtmActivity',
  'BillPay',
  'CorporateActions',
  'Checks',
  'Deposits',
  'DividendsAndCapitalGains',
  'ElectronicTransfers',
  'Fees',
  'Interest',
  'Misc',
  'SecurityTransfers',
  'Taxes',
  'Trades',
  'VisaDebitCard',
  'Withdrawals',
] as const;

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

  private apiOrigin(): string {
    const base = this.apiBase.replace(/\/+$/g, '');
    const idx = base.indexOf('/api/');
    return idx >= 0 ? base.slice(0, idx) : base;
  }

  private resolveUrl(path: string): string {
    if (path.startsWith('http://') || path.startsWith('https://')) return path;
    if (path.startsWith('/api/')) return `${this.apiOrigin()}${path}`;
    const base = this.apiBase.replace(/\/+$/g, '');
    return `${base}${path}`;
  }

  private async request<T>(
    path: string,
    init?: {
      method?: 'GET' | 'POST';
      body?: unknown;
    },
  ): Promise<T> {
    const url = this.resolveUrl(path);

    const token = this.session.token ?? null;
    if (token === null || token === undefined || token === '') {
      throw new Error('No bearer token in session');
    }

    const headers: Record<string, string> = {
      authorization: `Bearer ${token}`,
      'schwab-client-channel': 'IO',
      'schwab-client-correlid': uuidv4(),
      'schwab-env': 'PROD',
      'schwab-environment': 'PROD',
      'schwab-client-appid': 'AD00008376',
      'schwab-resource-version': '1',
      origin: 'https://client.schwab.com',
      referer: 'https://client.schwab.com/',
      accept: 'application/json',
    };

    const cookie = SessionDataUtil.cookieHeader(this.session);
    if (cookie !== '') {
      headers.cookie = cookie;
    }

    const method = init?.method ?? 'GET';
    const hasBody = init?.body !== undefined;
    if (hasBody) headers['content-type'] = 'application/json';

    const resp = await fetch(url, {
      method,
      headers,
      body: hasBody ? JSON.stringify(init?.body) : undefined,
    });
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
    const raw = await this.request<{ SecurityGroupings: SecurityGrouping[] }>('/AggregatedPositions');
    return { security_groupings: raw.SecurityGroupings ?? [] };
  }

  async getTransactionHistoryBrokerageAccounts(): Promise<TransactionHistoryBrokerageAccount[]> {
    const raw = await this.request<TransactionHistoryInitResponse>(TRANSACTION_HISTORY_INIT_PATH);
    return raw.accountSelectorData?.brokerageAccountList?.brokerageAccounts ?? [];
  }

  async getBrokerageTransactions(
    accountId: string,
    accountNickname: string,
    timeFrame: TransactionHistoryTimeFrame,
  ): Promise<BrokerageTransaction[]> {
    const rows: BrokerageTransaction[] = [];
    let bookmark: TransactionHistoryBookmark | null = null;

    for (let page = 0; page < TRANSACTION_HISTORY_MAX_PAGES; page += 1) {
      const req: BrokerageTransactionsRequest = {
        accountNickname,
        exportType: 'Csv',
        includeOptionsInSearch: false,
        isSpsLinkedUkAccount: false,
        selectedAccountId: accountId,
        selectedTransactionTypes: [...TRANSACTION_TYPES],
        sortColumn: 'Date',
        sortDirection: 'Descending',
        symbol: '',
        timeFrame,
        bookmark,
        shouldPaginate: true,
      };

      const resp = await this.request<BrokerageTransactionsResponse>(TRANSACTION_HISTORY_PATH, {
        method: 'POST',
        body: req,
      });

      rows.push(...(resp.brokerageTransactions ?? []));
      bookmark = resp.bookmark ?? null;
      if (bookmark === null) return rows;
    }

    throw new Error(
      `Schwab transaction pagination exceeded ${String(TRANSACTION_HISTORY_MAX_PAGES)} pages`,
    );
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

type ParsedTransactionRecord = {
  dateRaw: string;
  action: string | null;
  symbol: string | null;
  description: string | null;
  quantity: string | null;
  price: string | null;
  feesComm: string | null;
  amountRaw: string | null;
  fingerprintExtras: Array<[string, string | null]>;
  syncDataExtras: Record<string, unknown>;
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

function valueOrNull(v: string | null | undefined): string | null {
  if (v === null || v === undefined) return null;
  const trimmed = v.trim();
  return trimmed === '' ? null : trimmed;
}

function parseTransactionRecords(
  accountId: Id,
  records: ParsedTransactionRecord[],
  source: string,
  idPrefix: string,
): SchwabTransactionsImportResult {
  const transactions: TransactionType[] = [];
  let skipped = 0;
  const seenCounts = new Map<string, number>();

  for (const record of records) {
    const dates = extractMmddyyyyDates(record.dateRaw);
    const primary = dates[0] ?? null;
    if (primary === null) {
      skipped += 1;
      continue;
    }
    const asOf = dates[1] ?? null;

    const amountNorm = record.amountRaw !== null ? normalizeAmount(record.amountRaw) : null;
    if (amountNorm === null) {
      skipped += 1;
      continue;
    }

    const parts: string[] = [];
    if (record.action !== null && normalizeWs(record.action) !== '') parts.push(normalizeWs(record.action));
    if (record.symbol !== null && normalizeWs(record.symbol) !== '') parts.push(normalizeWs(record.symbol));
    if (record.description !== null && normalizeWs(record.description) !== '') {
      parts.push(normalizeWs(record.description));
    }
    const desc = parts.length === 0 ? 'Schwab transaction' : parts.join(' ');

    const dateIso = isoDate(primary);
    const asOfIso = asOf !== null ? isoDate(asOf) : null;

    const fingerprintParts = [
      `date=${dateIso}`,
      `asof=${asOfIso ?? ''}`,
      `action=${record.action ? normalizeWs(record.action) : ''}`,
      `symbol=${record.symbol ? normalizeWs(record.symbol) : ''}`,
      `desc=${record.description ? normalizeWs(record.description) : ''}`,
      `qty=${record.quantity ? normalizeWs(record.quantity) : ''}`,
      `price=${record.price ? normalizeWs(record.price) : ''}`,
      `fees=${record.feesComm ? normalizeWs(record.feesComm) : ''}`,
      `amount=${amountNorm}`,
    ];

    for (const [k, v] of record.fingerprintExtras) {
      fingerprintParts.push(`${k}=${v ? normalizeWs(v) : ''}`);
    }

    const fingerprint = fingerprintParts.join('|');
    const next = (seenCounts.get(fingerprint) ?? 0) + 1;
    seenCounts.set(fingerprint, next);

    const txId = Id.fromExternal(`${idPrefix}:${accountId.asStr()}:${fingerprint}:${String(next)}`);
    const timestamp = new Date(Date.UTC(primary.y, primary.m - 1, primary.d, 0, 0, 0));

    const syncData: Record<string, unknown> = {
      source,
      date_raw: record.dateRaw,
      date: dateIso,
      as_of_date: asOfIso,
      action: record.action,
      symbol: record.symbol,
      description: record.description,
      quantity: record.quantity,
      price: record.price,
      fees_comm: record.feesComm,
      amount_raw: record.amountRaw,
      amount: amountNorm,
      fingerprint,
      occurrence: next,
      ...record.syncDataExtras,
    };

    let tx = Transaction.new(amountNorm, Asset.currency('USD'), desc);
    tx = withTimestamp(tx, timestamp);
    tx = withId(tx, txId);
    tx = withSynchronizerData(tx, syncData);
    transactions.push(tx);
  }

  return { transactions, skipped };
}

/**
 * Parse Schwab's "transaction history" JSON export into keepbook transactions.
 */
export function parseSchwabExportedTransactionsJson(
  accountId: Id,
  json: string,
): SchwabTransactionsImportResult {
  const parsed = JSON.parse(json) as unknown;
  if (!Array.isArray(parsed)) {
    throw new Error('Expected Schwab export to be a JSON array');
  }

  const records: ParsedTransactionRecord[] = [];
  for (const row of parsed) {
    if (row === null || typeof row !== 'object' || Array.isArray(row)) {
      records.push({
        dateRaw: '',
        action: null,
        symbol: null,
        description: null,
        quantity: null,
        price: null,
        feesComm: null,
        amountRaw: null,
        fingerprintExtras: [],
        syncDataExtras: {},
      });
      continue;
    }

    const obj = row as Record<string, unknown>;
    records.push({
      dateRaw: getField(obj, 'Date') ?? '',
      action: getField(obj, 'Action'),
      symbol: getField(obj, 'Symbol'),
      description: getField(obj, 'Description'),
      quantity: getField(obj, 'Quantity'),
      price: getField(obj, 'Price'),
      feesComm: getField(obj, 'Fees & Comm'),
      amountRaw: getField(obj, 'Amount'),
      fingerprintExtras: [],
      syncDataExtras: {},
    });
  }

  return parseTransactionRecords(accountId, records, 'schwab_export_json', 'schwab:export');
}

/**
 * Parse Schwab brokerage transaction-history API rows into keepbook transactions.
 */
export function parseSchwabBrokerageTransactions(
  accountId: Id,
  rows: BrokerageTransaction[],
): SchwabTransactionsImportResult {
  const records: ParsedTransactionRecord[] = rows.map((row) => {
    const sourceCode = valueOrNull(row.sourceCode);
    const effectiveDate = valueOrNull(row.effectiveDate);
    const depositSequenceId = valueOrNull(row.depositSequenceId);
    const checkDate = valueOrNull(row.checkDate);
    const itemIssueId = valueOrNull(row.itemIssueId);
    const schwabOrderId = valueOrNull(row.schwabOrderId);

    return {
      dateRaw: row.transactionDate,
      action: valueOrNull(row.action),
      symbol: valueOrNull(row.symbol),
      description: valueOrNull(row.description),
      quantity: valueOrNull(row.shareQuantity),
      price: valueOrNull(row.executionPrice),
      feesComm: valueOrNull(row.feesAndCommission),
      amountRaw: valueOrNull(row.amount),
      fingerprintExtras: [
        ['source_code', sourceCode],
        ['effective_date', effectiveDate],
        ['deposit_sequence_id', depositSequenceId],
        ['check_date', checkDate],
        ['item_issue_id', itemIssueId],
        ['schwab_order_id', schwabOrderId],
      ],
      syncDataExtras: {
        source_code: sourceCode,
        effective_date: effectiveDate,
        deposit_sequence_id: depositSequenceId,
        check_date: checkDate,
        item_issue_id: itemIssueId,
        schwab_order_id: schwabOrderId,
      },
    };
  });

  return parseTransactionRecords(
    accountId,
    records,
    'schwab_transaction_history_api',
    'schwab:history',
  );
}
