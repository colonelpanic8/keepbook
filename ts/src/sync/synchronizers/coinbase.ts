import { createPrivateKey, randomBytes, sign as cryptoSign } from 'node:crypto';

import type { Storage } from '../../storage/storage.js';
import { Id } from '../../models/id.js';
import { Asset } from '../../models/asset.js';
import { Account, type AccountType } from '../../models/account.js';
import { AssetBalance } from '../../models/balance.js';
import { Transaction, type TransactionType, withId, withSynchronizerData } from '../../models/transaction.js';
import type { ConnectionType } from '../../models/connection.js';
import type { CredentialStore } from '../../credentials/credential-store.js';
import type { SyncResult, SyncedAssetBalance } from '../mod.js';
import type { Synchronizer } from '../mod.js';

const CDP_API_BASE = 'https://api.coinbase.com';

type CoinbaseAccount = {
  uuid: string;
  name: string;
  currency: string;
  available_balance: { value: string; currency: string };
  type: string;
};

type CoinbaseFill = {
  entry_id?: string;
  trade_id?: string;
  order_id?: string;
  product_id: string;
  size: string;
  trade_time: string;
  side?: string;
};

function b64url(data: Buffer | string): string {
  const buf = typeof data === 'string' ? Buffer.from(data, 'utf8') : data;
  return buf
    .toString('base64')
    .replace(/\+/g, '-')
    .replace(/\//g, '_')
    .replace(/=+$/g, '');
}

function encodeUnreservedOnly(value: string): string {
  const bytes = Buffer.from(value, 'utf8');
  let out = '';
  for (const b of bytes) {
    // RFC 3986 unreserved: ALPHA / DIGIT / "-" / "." / "_" / "~"
    if (
      (b >= 0x41 && b <= 0x5a) ||
      (b >= 0x61 && b <= 0x7a) ||
      (b >= 0x30 && b <= 0x39) ||
      b === 0x2d ||
      b === 0x2e ||
      b === 0x5f ||
      b === 0x7e
    ) {
      out += String.fromCharCode(b);
    } else {
      out += '%' + b.toString(16).toUpperCase().padStart(2, '0');
    }
  }
  return out;
}

function baseAssetFromProductId(productId: string): string | null {
  const base = productId.split('-')[0];
  if (base === undefined) return null;
  const trimmed = base.trim();
  if (trimmed === '') return null;
  return trimmed;
}

function methodIsValid(method: string): boolean {
  return ['GET', 'POST', 'PUT', 'DELETE', 'PATCH', 'HEAD', 'OPTIONS'].includes(method);
}

export class CoinbaseSynchronizer implements Synchronizer {
  private readonly keyName: string;
  private readonly privateKeyPem: string;
  private readonly apiBase: string;

  constructor(keyName: string, privateKeyPem: string, apiBase?: string) {
    this.keyName = keyName;
    this.privateKeyPem = privateKeyPem;
    this.apiBase = apiBase ?? CDP_API_BASE;
  }

  static async fromCredentials(store: CredentialStore, apiBase?: string): Promise<CoinbaseSynchronizer> {
    const keyName =
      (await store.get('key-name')) ??
      (await store.get('key_name')) ??
      null;
    if (keyName === null) {
      throw new Error('Missing key-name in credentials');
    }

    const privateKey =
      (await store.get('private-key')) ??
      (await store.get('private_key')) ??
      null;
    if (privateKey === null) {
      throw new Error('Missing private-key in credentials');
    }

    return new CoinbaseSynchronizer(keyName, privateKey, apiBase);
  }

  name(): string {
    return 'coinbase';
  }

  private generateJwt(method: string, path: string): string {
    const now = Math.floor(Date.now() / 1000);
    const base = (this.apiBase ?? CDP_API_BASE)
      .replace(/\/+$/g, '')
      .replace(/^https:\/\//, '')
      .replace(/^http:\/\//, '');
    const uri = `${method} ${base}${path}`;

    const nonceHex = (() => {
      const hex = randomBytes(8).toString('hex');
      try {
        return BigInt('0x' + hex).toString(16);
      } catch {
        return hex.replace(/^0+/, '') || '0';
      }
    })();

    const header = {
      alg: 'ES256',
      typ: 'JWT',
      kid: this.keyName,
      nonce: nonceHex,
    } as const;

    const claims = {
      sub: this.keyName,
      iss: 'cdp',
      nbf: now,
      exp: now + 120,
      uri,
    } as const;

    const headerB64 = b64url(JSON.stringify(header));
    const claimsB64 = b64url(JSON.stringify(claims));
    const message = `${headerB64}.${claimsB64}`;

    // Coinbase expects JWS ES256 signature in raw (r||s) format, not DER.
    const keyObj = createPrivateKey(this.privateKeyPem);
    const sig = cryptoSign('sha256', Buffer.from(message, 'utf8'), {
      key: keyObj,
      dsaEncoding: 'ieee-p1363',
    });
    const sigB64 = b64url(sig);
    return `${message}.${sigB64}`;
  }

  private async request<T>(method: string, path: string): Promise<T> {
    const upper = method.toUpperCase();
    if (!methodIsValid(upper)) {
      throw new Error('Invalid HTTP method');
    }

    const jwt = this.generateJwt(upper, path);
    const base = this.apiBase.replace(/\/+$/g, '');
    const url = `${base}${path}`;

    const resp = await fetch(url, {
      method: upper,
      headers: {
        Authorization: `Bearer ${jwt}`,
        'Content-Type': 'application/json',
      },
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

  private async getAccounts(): Promise<CoinbaseAccount[]> {
    type PortfoliosResponse = { portfolios: Array<{ uuid: string; name?: string }> };
    type BreakdownResponse = {
      breakdown: {
        spot_positions?: Array<{
          asset: string;
          account_uuid: string;
          total_balance_crypto: number;
          is_cash?: boolean;
        }>;
      };
    };

    const accounts: CoinbaseAccount[] = [];
    const portfolios = await this.request<PortfoliosResponse>('GET', '/api/v3/brokerage/portfolios');
    for (const p of portfolios.portfolios ?? []) {
      const path = `/api/v3/brokerage/portfolios/${encodeUnreservedOnly(p.uuid)}`;
      const breakdown = await this.request<BreakdownResponse>('GET', path);
      const positions = breakdown.breakdown?.spot_positions ?? [];
      for (const pos of positions) {
        if (pos.is_cash === true) continue;
        accounts.push({
          uuid: pos.account_uuid,
          name: `${pos.asset} Wallet`,
          currency: pos.asset,
          available_balance: { value: pos.total_balance_crypto.toString(), currency: '' },
          type: 'ACCOUNT_TYPE_CRYPTO',
        });
      }
    }
    return accounts;
  }

  private async getFills(): Promise<CoinbaseFill[]> {
    type FillsResponse = { fills?: CoinbaseFill[]; has_next?: boolean; cursor?: string | null };

    const fills: CoinbaseFill[] = [];
    let cursor: string | null = null;
    for (;;) {
      const reqPath: string =
        cursor !== null
          ? `/api/v3/brokerage/orders/historical/fills?cursor=${encodeUnreservedOnly(cursor)}`
          : '/api/v3/brokerage/orders/historical/fills';

      const resp: FillsResponse = await this.request<FillsResponse>('GET', reqPath);
      fills.push(...(resp.fills ?? []));

      if (resp.has_next !== true) break;
      cursor = resp.cursor ?? null;
      if (cursor === null) {
        // Matches Rust behavior: has_next=true without cursor -> stop.
        break;
      }
    }
    return fills;
  }

  async sync(connection: ConnectionType, storage: Storage): Promise<SyncResult> {
    const coinbaseAccounts = await this.getAccounts();
    let fills: CoinbaseFill[] = [];
    try {
      fills = await this.getFills();
    } catch {
      fills = [];
    }

    const existingAccounts = await storage.listAccounts();
    const existingById = new Map<string, AccountType>();
    const existingIds = new Set<string>();
    for (const a of existingAccounts) {
      if (a.connection_id.equals(connection.state.id)) {
        existingById.set(a.id.asStr(), a);
        existingIds.add(a.id.asStr());
      }
    }

    const accountUuidByCurrency = new Map<string, string>();
    for (const a of coinbaseAccounts) {
      const currency = a.currency.trim().toUpperCase();
      if (!accountUuidByCurrency.has(currency)) {
        accountUuidByCurrency.set(currency, a.uuid);
      }
    }

    const fillsByAccountUuid = new Map<string, CoinbaseFill[]>();
    for (const f of fills) {
      const base = baseAssetFromProductId(f.product_id);
      if (base === null) continue;
      const accountUuid = accountUuidByCurrency.get(base.toUpperCase());
      if (accountUuid === undefined) continue;
      const arr = fillsByAccountUuid.get(accountUuid) ?? [];
      arr.push(f);
      fillsByAccountUuid.set(accountUuid, arr);
    }

    const accounts: AccountType[] = [];
    const balances: Array<[Id, SyncedAssetBalance[]]> = [];
    const transactions: Array<[Id, TransactionType[]]> = [];

    for (const cb of coinbaseAccounts) {
      let accountId: Id;
      try {
        accountId = Id.fromStringChecked(cb.uuid);
      } catch {
        accountId = Id.fromExternal(`coinbase:${cb.uuid}`);
      }

      const balanceAmount = (() => {
        const n = Number.parseFloat(cb.available_balance.value);
        return Number.isFinite(n) ? n : 0;
      })();

      const existing = existingIds.has(accountId.asStr());
      const cbTxns = fillsByAccountUuid.get(cb.uuid) ?? [];
      fillsByAccountUuid.delete(cb.uuid);

      if (balanceAmount === 0 && !existing && cbTxns.length === 0) {
        continue;
      }

      const createdAt = existingById.get(accountId.asStr())?.created_at ?? new Date();

      const acct: AccountType = {
        ...Account.newWith(accountId, createdAt, cb.name, connection.state.id),
        tags: ['coinbase', cb.type],
        synchronizer_data: { currency: cb.currency },
      };

      const asset = Asset.crypto(cb.currency);
      const assetBalance = AssetBalance.new(asset, cb.available_balance.value);

      const accountTransactions: TransactionType[] = cbTxns
        .map((tx): TransactionType | null => {
          const ts = new Date(tx.trade_time);
          if (Number.isNaN(ts.getTime())) return null;

          const side = (tx.side ?? '').trim().toUpperCase();
          const sideLabel = side === '' ? 'FILL' : side;

          let amount = tx.size;
          if (sideLabel === 'SELL' && !amount.startsWith('-')) {
            amount = `-${amount}`;
          }

          const entryId =
            tx.entry_id ?? tx.trade_id ?? tx.order_id ?? `${tx.product_id}:${tx.trade_time}:${tx.side ?? ''}`;
          const txId = Id.fromExternal(`coinbase:fill:${entryId}`);
          const description = `${sideLabel} ${tx.product_id}`;

          let t = Transaction.new(amount, Asset.crypto(cb.currency), description);
          t = withId(t, txId);
          t = withSynchronizerData(t, {
            coinbase_entry_id: entryId,
            trade_id: tx.trade_id ?? null,
            order_id: tx.order_id ?? null,
            product_id: tx.product_id,
            side: sideLabel,
          });
          return { ...t, timestamp: ts, timestamp_raw: tx.trade_time };
        })
        .filter((t): t is TransactionType => t !== null);

      accounts.push(acct);
      balances.push([accountId, [{ asset_balance: assetBalance }]]);
      transactions.push([accountId, accountTransactions]);
    }

    const now = new Date();
    const updatedConnection: ConnectionType = {
      config: connection.config,
      state: {
        ...connection.state,
        account_ids: accounts.map((a) => a.id),
        last_sync: {
          at: now,
          at_raw: now.toISOString(),
          status: 'success',
        },
        status: 'active',
      },
    };

    return {
      connection: updatedConnection,
      accounts,
      balances,
      transactions,
    };
  }
}
