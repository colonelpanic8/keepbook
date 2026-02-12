import { v4 as uuidv4 } from 'uuid';

import type { SessionData } from '../credentials/session.js';
import { SessionDataUtil } from '../credentials/session.js';

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
