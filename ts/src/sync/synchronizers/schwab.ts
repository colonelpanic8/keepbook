import type { Storage } from '../../storage/storage.js';
import { Id } from '../../models/id.js';
import { Account, type AccountType } from '../../models/account.js';
import { Asset } from '../../models/asset.js';
import { AssetBalance } from '../../models/balance.js';
import type { ConnectionType } from '../../models/connection.js';
import { AssetId } from '../../market-data/asset-id.js';
import type { PricePoint } from '../../market-data/models.js';
import { SessionCache, type SessionData } from '../../credentials/session.js';
import {
  parseSchwabBankingTransactions,
  parseSchwabBrokerageTransactions,
  type BankingTransaction,
  SchwabClient,
  type BrokerageTransaction,
  type Position,
  type TransactionHistoryTimeFrame,
} from '../schwab.js';
import {
  DefaultSyncOptions,
  type SyncOptions,
  type SyncResult,
  type SyncedAssetBalance,
  type Synchronizer,
  type TransactionSyncMode,
} from '../mod.js';

function todayUtc(): string {
  return new Date().toISOString().slice(0, 10);
}

function transactionTimeFrame(mode: TransactionSyncMode, existingCount: number): TransactionHistoryTimeFrame {
  if (mode === 'full') return 'All';
  return existingCount === 0 ? 'All' : 'Last6Months';
}

function bankingSelectedAccountId(accountNumberDisplayFull: string, fallbackAccountId: string): string {
  const digits = accountNumberDisplayFull.replace(/\D/g, '');
  return digits === '' ? fallbackAccountId : digits;
}

export class SchwabSynchronizer implements Synchronizer {
  private readonly connectionId: Id;
  private readonly sessionCache: SessionCache;

  constructor(connectionId: Id, sessionCache?: SessionCache) {
    this.connectionId = connectionId;
    this.sessionCache = sessionCache ?? SessionCache.new();
  }

  name(): string {
    return 'schwab';
  }

  private sessionKey(): string {
    return this.connectionId.asStr();
  }

  private getSession(): SessionData | null {
    return this.sessionCache.get(this.sessionKey());
  }

  async sync(connection: ConnectionType, storage: Storage): Promise<SyncResult> {
    return this.syncWithOptions(connection, storage, DefaultSyncOptions);
  }

  async syncWithOptions(
    connection: ConnectionType,
    storage: Storage,
    options: SyncOptions,
  ): Promise<SyncResult> {
    const session = this.getSession();
    if (session === null) {
      throw new Error('No session found. Run login first.');
    }

    const existing = await storage.listAccounts();
    const existingById = new Map<string, AccountType>();
    for (const a of existing) {
      if (a.connection_id.equals(connection.state.id)) {
        existingById.set(a.id.asStr(), a);
      }
    }

    const client = new SchwabClient(session);
    const accountsResp = await client.getAccounts();
    const positionsResp = await client.getPositions();
    const historyAccounts = await client
      .getTransactionHistoryBrokerageAccounts()
      .catch((_err) => [] as Array<{ id: string; nickName?: string }>);
    const allPositions: Position[] = positionsResp.security_groupings.flatMap((g) => g.Positions ?? []);
    const historyAccountIdsByName = new Map<string, string>();
    for (const acct of historyAccounts) {
      const id = acct.id.trim();
      if (id === '') continue;
      const key = (acct.nickName ?? '').trim().toLowerCase();
      if (key !== '') historyAccountIdsByName.set(key, id);
    }
    const loneHistoryAccountId =
      historyAccounts.length === 1 && historyAccounts[0].id.trim() !== ''
        ? historyAccounts[0].id.trim()
        : null;

    const now = new Date();
    const asOfDate = todayUtc();

    const accounts: AccountType[] = [];
    const balances: Array<[Id, SyncedAssetBalance[]]> = [];
    const transactions: Array<[Id, ReturnType<typeof parseSchwabBrokerageTransactions>['transactions']]> = [];

    for (const schwabAccount of accountsResp.accounts) {
      const accountId = Id.fromExternal(schwabAccount.AccountId);
      const createdAt = existingById.get(accountId.asStr())?.created_at ?? now;

      const name = schwabAccount.NickName.trim() === '' ? schwabAccount.DefaultName : schwabAccount.NickName;
      const account: AccountType = {
        ...Account.newWith(accountId, createdAt, name, connection.state.id),
        tags: ['schwab', schwabAccount.AccountType.toLowerCase()],
        synchronizer_data: { account_number: schwabAccount.AccountNumberDisplayFull },
      };

      const accountBalances: SyncedAssetBalance[] = [];

      if (schwabAccount.IsBrokerage) {
        for (const position of allPositions) {
          if (position.DefaultSymbol === 'CASH') continue;

          const asset = Asset.equity(position.DefaultSymbol);
          const assetBalance = AssetBalance.new(asset, position.Quantity.toString());

          const price: PricePoint = {
            asset_id: AssetId.fromAsset(asset),
            as_of_date: asOfDate,
            timestamp: now,
            price: position.Price.toString(),
            quote_currency: 'USD',
            kind: 'close',
            source: 'schwab',
          };

          accountBalances.push({ asset_balance: assetBalance, price });
        }

        const cash = schwabAccount.Balances?.Cash;
        if (cash !== undefined) {
          accountBalances.push({
            asset_balance: AssetBalance.new(Asset.currency('USD'), cash.toString()),
          });
        }
      } else {
        const bal = schwabAccount.Balances?.Balance;
        if (bal !== undefined) {
          accountBalances.push({
            asset_balance: AssetBalance.new(Asset.currency('USD'), bal.toString()),
          });
        }
      }

      const existingTxns = await storage.getTransactions(accountId);
      const timeFrame = transactionTimeFrame(options.transactions, existingTxns.length);
      if (schwabAccount.IsBrokerage) {
        const txAccountId =
          historyAccountIdsByName.get(name.trim().toLowerCase()) ??
          loneHistoryAccountId ??
          schwabAccount.AccountId;

        let historyRows: BrokerageTransaction[] = [];
        try {
          historyRows = await client.getBrokerageTransactions(txAccountId, name, timeFrame);
        } catch (err) {
          throw new Error(
            `Failed to fetch Schwab transactions for account ${schwabAccount.AccountId} (transaction-history id ${txAccountId}): ${String(err)}`,
          );
        }

        if (historyRows.length > 0) {
          try {
            const parsed = parseSchwabBrokerageTransactions(accountId, historyRows);
            if (parsed.transactions.length > 0) {
              transactions.push([accountId, parsed.transactions]);
            }
          } catch (err) {
            throw new Error(
              `Failed to parse Schwab transactions for account ${schwabAccount.AccountId}: ${String(err)}`,
            );
          }
        }
      } else {
        const bankTxAccountId = bankingSelectedAccountId(
          schwabAccount.AccountNumberDisplayFull,
          schwabAccount.AccountId,
        );
        const bankNickname = schwabAccount.DefaultName.trim() === '' ? name : schwabAccount.DefaultName;

        let historyRows: BankingTransaction[] = [];
        try {
          historyRows = await client.getBankingTransactions(bankTxAccountId, bankNickname, timeFrame);
        } catch (err) {
          if (timeFrame === 'All') {
            console.warn(
              `Schwab: banking transaction-history does not support timeFrame=All for account ${schwabAccount.AccountId} (banking id ${bankTxAccountId}), retrying Last6Months: ${String(err)}`,
            );
            try {
              historyRows = await client.getBankingTransactions(
                bankTxAccountId,
                bankNickname,
                'Last6Months',
              );
            } catch (retryErr) {
              console.warn(
                `Schwab: transaction-history fetch unavailable for non-brokerage account ${schwabAccount.AccountId} (banking id ${bankTxAccountId}): ${String(retryErr)}`,
              );
            }
          } else {
            console.warn(
              `Schwab: transaction-history fetch unavailable for non-brokerage account ${schwabAccount.AccountId} (banking id ${bankTxAccountId}): ${String(err)}`,
            );
          }
        }

        if (historyRows.length > 0) {
          try {
            const parsed = parseSchwabBankingTransactions(accountId, historyRows);
            if (parsed.transactions.length > 0) {
              transactions.push([accountId, parsed.transactions]);
            }
          } catch (err) {
            console.warn(
              `Schwab: failed to parse banking transaction-history rows for account ${schwabAccount.AccountId}: ${String(err)}`,
            );
          }
        }
      }

      accounts.push(account);
      balances.push([accountId, accountBalances]);
    }

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
