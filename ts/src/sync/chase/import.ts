import { Account } from '../../models/account.js';
import type { AccountType } from '../../models/account.js';
import { Asset } from '../../models/asset.js';
import { AssetBalance } from '../../models/balance.js';
import { ConnectionState, type ConnectionType } from '../../models/connection.js';
import { Id } from '../../models/id.js';
import {
  Transaction,
  withId as withTxId,
  withSynchronizerData as withTxSyncData,
  withTimestamp as withTxTimestamp,
} from '../../models/transaction.js';
import type { TransactionType } from '../../models/transaction.js';
import { decStr } from '../../format/decimal.js';
import type { Storage } from '../../storage/storage.js';
import type { SyncResult, SyncedAssetBalance } from '../mod.js';
import { SyncedAssetBalanceFactory } from '../mod.js';
import { parseQfxStatement, type QfxStatement } from './qfx.js';

function formatDescription(name: string | undefined, memo: string | undefined): string {
  const n = (name ?? '').trim();
  const m = (memo ?? '').trim();

  if (n !== '' && m !== '') {
    const nUp = n.toUpperCase();
    const mUp = m.toUpperCase();
    if (nUp === mUp || n.includes(m)) return n;
    return `${n} - ${m}`;
  }
  if (n !== '') return n;
  if (m !== '') return m;
  return 'Chase transaction';
}

function digitsSuffix4(s: string): string | null {
  const digits = s.replace(/[^0-9]/g, '');
  if (digits.length < 4) return null;
  return digits.slice(digits.length - 4);
}

export async function importQfxStatements(
  connection: ConnectionType,
  storage: Storage,
  statements: QfxStatement[],
): Promise<SyncResult> {
  const byAcct = new Map<string, QfxStatement[]>();
  for (const stmt of statements) {
    const arr = byAcct.get(stmt.account_id);
    if (arr) arr.push(stmt);
    else byAcct.set(stmt.account_id, [stmt]);
  }

  const accounts: AccountType[] = [];
  const balances: Array<[Id, SyncedAssetBalance[]]> = [];
  const transactions: Array<[Id, TransactionType[]]> = [];

  for (const [acctid, stmts] of byAcct.entries()) {
    const accountId = Id.fromExternal(`chase:${connection.state.id.asStr()}:${acctid}`);
    const existing = await storage.getAccount(accountId);
    const createdAt = existing?.created_at ?? new Date();

    const suffix = digitsSuffix4(acctid);
    const name = suffix ? `Chase (${suffix})` : 'Chase';

    const first = stmts[0];
    const tags = ['chase', first.kind === 'credit_card' ? 'credit_card' : 'bank'];
    if (first.account_type) tags.push(first.account_type.toLowerCase());

    const account = {
      ...Account.newWith(accountId, createdAt, name, connection.state.id),
      tags,
      synchronizer_data: {
        acctid,
        account_type: first.account_type ?? null,
        currency: first.currency ?? null,
      },
    };
    accounts.push(account);

    // Ledger balance: choose newest as_of if present.
    let bestBal: { asOf: number; amt: string; currency: string } | null = null;
    for (const stmt of stmts) {
      if (!stmt.ledger_balance || !stmt.ledger_balance_as_of) continue;
      const asOfMs = stmt.ledger_balance_as_of.getTime();
      if (bestBal === null || asOfMs > bestBal.asOf) {
        bestBal = {
          asOf: asOfMs,
          amt: decStr(stmt.ledger_balance),
          currency: stmt.currency ?? 'USD',
        };
      }
    }

    if (bestBal) {
      balances.push([
        accountId,
        [
          SyncedAssetBalanceFactory.new(
            AssetBalance.new(Asset.currency(bestBal.currency), bestBal.amt),
          ),
        ],
      ]);
    } else {
      balances.push([accountId, []]);
    }

    const txns: TransactionType[] = [];
    for (const stmt of stmts) {
      const currency = stmt.currency ?? 'USD';
      for (const t of stmt.transactions) {
        const txId = Id.fromExternal(
          `chase:${connection.state.id.asStr()}:${acctid}:${t.fitid}`,
        );
        const desc = formatDescription(t.name, t.memo);
        let tx = Transaction.new(decStr(t.amount), Asset.currency(currency), desc);
        tx = withTxTimestamp(tx, t.posted_at);
        tx = withTxId(tx, txId);
        tx = withTxSyncData(tx, {
          fitid: t.fitid,
          trntype: t.trn_type ?? null,
          name: t.name ?? null,
          memo: t.memo ?? null,
          checknum: t.check_num ?? null,
          refnum: t.ref_num ?? null,
          acctid,
        });
        txns.push(tx);
      }
    }
    transactions.push([accountId, txns]);
  }

  const now = new Date();
  const updatedConn: ConnectionType = {
    config: connection.config,
    state: {
      ...ConnectionState.newWith(connection.state.id, connection.state.created_at),
      status: 'active',
      created_at: connection.state.created_at,
      account_ids: accounts.map((a) => a.id),
      last_sync: { at: now, at_raw: now.toISOString(), status: 'success' },
      synchronizer_data: connection.state.synchronizer_data ?? null,
    },
  };

  return {
    connection: updatedConn,
    accounts,
    balances,
    transactions,
  };
}

export function parseAndImportQfx(
  connection: ConnectionType,
  storage: Storage,
  qfxContents: string[],
): Promise<SyncResult> {
  const statements = qfxContents.map(parseQfxStatement);
  return importQfxStatements(connection, storage, statements);
}
