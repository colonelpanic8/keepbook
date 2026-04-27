/**
 * Portfolio service.
 *
 * Port of the Rust `portfolio::service` module. Calculates portfolio snapshots
 * by aggregating balances from storage, valuing each asset using the
 * MarketDataService for prices and FX rates, and producing summaries.
 */

import { Decimal } from '../decimal.js';

import { Asset, type AssetType } from '../models/asset.js';
import type { AccountType } from '../models/account.js';
import type { BalanceSnapshotType } from '../models/balance.js';
import type { ConnectionType } from '../models/connection.js';
import { type Clock, SystemClock } from '../clock.js';
import type { Id } from '../models/id.js';
import type { Storage } from '../storage/storage.js';
import { MarketDataService } from '../market-data/service.js';
import { AssetId } from '../market-data/asset-id.js';
import { decStr, decStrRounded } from '../format/decimal.js';

import type {
  PortfolioQuery,
  PortfolioSnapshot,
  AssetSummary,
  AccountSummary,
  AccountHolding,
  EquityValuationAdjustment,
  PortfolioValuationScenario,
} from './models.js';

// ---------------------------------------------------------------------------
// Internal types
// ---------------------------------------------------------------------------

/** Valuation result for a single unit of an asset in the target currency. */
interface AssetValuation {
  /** Value of one unit in target currency. Undefined if price data unavailable. */
  value: Decimal | undefined;
  price: string | undefined;
  price_date: string | undefined;
  price_timestamp: Date | undefined;
  fx_rate: string | undefined;
  fx_date: string | undefined;
}

/** A single asset holding from a snapshot. */
interface HoldingEntry {
  account_id: Id;
  asset: AssetType;
  amount: string;
  cost_basis: string | undefined;
  timestamp: Date;
}

/** Aggregated data for a single asset across all accounts. */
interface AssetAggregate {
  total_amount: Decimal;
  amount_with_cost_basis: Decimal;
  total_cost_basis: Decimal | undefined;
  latest_balance_date: string; // "YYYY-MM-DD"
  holdings: HoldingEntry[];
}

interface GainsTotals {
  total_cost_basis: Decimal | undefined;
  total_unrealized_gain: Decimal | undefined;
  prospective_capital_gains_tax: Decimal | undefined;
}

interface ResolvedValuationScenario {
  multiplier: Decimal;
  output: PortfolioValuationScenario;
}

/** Context loaded from storage for portfolio calculation. */
interface CalculationContext {
  account_map: Map<string, AccountType>;
  connection_map: Map<string, ConnectionType>;
  filtered_snapshots: Array<[Id, BalanceSnapshotType]>;
  zero_accounts: Id[];
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** Extract "YYYY-MM-DD" from a Date (UTC). */
function dateToString(d: Date): string {
  return d.toISOString().slice(0, 10);
}

// ---------------------------------------------------------------------------
// PortfolioService
// ---------------------------------------------------------------------------

export class PortfolioService {
  private readonly storage: Storage;
  private readonly marketData: MarketDataService;
  private readonly clock: Clock;

  constructor(storage: Storage, marketData: MarketDataService, clock?: Clock) {
    this.storage = storage;
    this.marketData = marketData;
    this.clock = clock ?? new SystemClock();
  }

  async calculate(query: PortfolioQuery): Promise<PortfolioSnapshot> {
    // 1. Load accounts, connections, and filtered balances
    const ctx = await this.loadCalculationContext(query.as_of_date, query.account_ids ?? []);

    // 2. Aggregate balances by normalized asset
    const byAssetAgg = this.aggregateByAsset(ctx.filtered_snapshots);

    // 3. Fetch valuations for all unique assets
    const priceCache = await this.fetchAssetValuations(
      byAssetAgg,
      query.currency,
      query.as_of_date,
    );

    const valuationScenario = this.resolveEquityValuationScenario(
      byAssetAgg,
      priceCache,
      query.equity_valuation_adjustment,
      query.currency_decimals,
    );
    const effectivePriceCache =
      valuationScenario === undefined
        ? new Map(priceCache)
        : this.applyEquityValuationMultiplier(priceCache, valuationScenario.multiplier);

    // 4. Build asset summaries and total value
    const {
      summaries: assetSummaries,
      totalValue,
      gainsTotals,
    } = this.buildAssetSummaries(
      byAssetAgg,
      effectivePriceCache,
      ctx.account_map,
      query.include_detail,
      query.currency_decimals,
      query.capital_gains_tax_rate,
    );

    // 5. Build account summaries
    const accountSummaries = this.buildAccountSummaries(
      ctx.filtered_snapshots,
      ctx.zero_accounts,
      effectivePriceCache,
      ctx.account_map,
      ctx.connection_map,
      query.currency_decimals,
    );

    // 6. Sort for consistent output
    accountSummaries.sort((a, b) => a.account_name.localeCompare(b.account_name));
    assetSummaries.sort((a, b) => {
      const aId = AssetId.fromAsset(a.asset).asStr();
      const bId = AssetId.fromAsset(b.asset).asStr();
      return aId < bId ? -1 : aId > bId ? 1 : 0;
    });

    // 7. Build snapshot based on grouping
    const byAsset =
      query.grouping === 'asset' || query.grouping === 'both' ? assetSummaries : undefined;
    const byAccount =
      query.grouping === 'account' || query.grouping === 'both' ? accountSummaries : undefined;

    const snapshot: PortfolioSnapshot = {
      as_of_date: query.as_of_date,
      currency: query.currency,
      total_value: decStrRounded(totalValue, query.currency_decimals),
      by_asset: byAsset,
      by_account: byAccount,
    };
    if (gainsTotals.total_cost_basis !== undefined) {
      snapshot.total_cost_basis = decStrRounded(
        gainsTotals.total_cost_basis,
        query.currency_decimals,
      );
    }
    if (gainsTotals.total_unrealized_gain !== undefined) {
      snapshot.total_unrealized_gain = decStrRounded(
        gainsTotals.total_unrealized_gain,
        query.currency_decimals,
      );
    }
    if (gainsTotals.prospective_capital_gains_tax !== undefined) {
      snapshot.prospective_capital_gains_tax = decStrRounded(
        gainsTotals.prospective_capital_gains_tax,
        query.currency_decimals,
      );
    }
    if (valuationScenario !== undefined) {
      snapshot.valuation_scenario = valuationScenario.output;
    }
    return snapshot;
  }

  // -----------------------------------------------------------------------
  // Private: load calculation context
  // -----------------------------------------------------------------------

  private async loadCalculationContext(
    asOfDate: string,
    accountIds: Id[],
  ): Promise<CalculationContext> {
    const accounts = await this.storage.listAccounts();
    const connections = await this.storage.listConnections();

    const accountMap = new Map<string, AccountType>();
    for (const a of accounts) {
      accountMap.set(a.id.asStr(), a);
    }

    const connectionMap = new Map<string, ConnectionType>();
    for (const c of connections) {
      connectionMap.set(c.state.id.asStr(), c);
    }

    const asOfEnd = new Date(asOfDate + 'T23:59:59Z');
    const filteredSnapshots: Array<[Id, BalanceSnapshotType]> = [];
    const zeroAccounts: Id[] = [];
    const scopedAccountIds = new Set(accountIds.map((id) => id.asStr()));

    for (const account of accountMap.values()) {
      if (scopedAccountIds.size > 0 && !scopedAccountIds.has(account.id.asStr())) {
        continue;
      }

      const accountConfig = this.storage.getAccountConfig(account.id);
      if (accountConfig?.exclude_from_portfolio === true) {
        continue;
      }

      const policy = accountConfig?.balance_backfill ?? 'none';
      const snapshots = await this.storage.getBalanceSnapshots(account.id);

      if (snapshots.length === 0) {
        // No snapshots at all - check backfill policy
        if (policy === 'zero') {
          zeroAccounts.push(account.id);
        }
        continue;
      }

      // Find latest snapshot before or on as_of_date (end-of-day)
      let latestBefore: BalanceSnapshotType | undefined;
      for (const s of snapshots) {
        if (s.timestamp.getTime() <= asOfEnd.getTime()) {
          if (
            latestBefore === undefined ||
            s.timestamp.getTime() > latestBefore.timestamp.getTime()
          ) {
            latestBefore = s;
          }
        }
      }

      if (latestBefore !== undefined) {
        filteredSnapshots.push([account.id, latestBefore]);
        continue;
      }

      // No snapshot before as_of_date - check backfill policy
      switch (policy) {
        case 'carry_earliest': {
          // Use the earliest snapshot
          let earliest: BalanceSnapshotType | undefined;
          for (const s of snapshots) {
            if (earliest === undefined || s.timestamp.getTime() < earliest.timestamp.getTime()) {
              earliest = s;
            }
          }
          if (earliest !== undefined) {
            filteredSnapshots.push([account.id, earliest]);
          }
          break;
        }
        case 'zero':
          zeroAccounts.push(account.id);
          break;
        case 'none':
        default:
          // Skip this account
          break;
      }
    }

    return {
      account_map: accountMap,
      connection_map: connectionMap,
      filtered_snapshots: filteredSnapshots,
      zero_accounts: zeroAccounts,
    };
  }

  // -----------------------------------------------------------------------
  // Private: aggregate balances by asset
  // -----------------------------------------------------------------------

  private aggregateByAsset(
    snapshots: Array<[Id, BalanceSnapshotType]>,
  ): Map<string, { asset: AssetType; agg: AssetAggregate }> {
    const byAsset = new Map<string, { asset: AssetType; agg: AssetAggregate }>();

    for (const [accountId, snapshot] of snapshots) {
      for (const assetBalance of snapshot.balances) {
        const normalizedAsset = Asset.normalized(assetBalance.asset);
        const key = Asset.hash(normalizedAsset);
        const amount = new Decimal(assetBalance.amount);
        const balanceDate = dateToString(snapshot.timestamp);

        const existing = byAsset.get(key);
        if (existing !== undefined) {
          existing.agg.total_amount = existing.agg.total_amount.plus(amount);
          if (assetBalance.cost_basis !== undefined) {
            const costBasis = new Decimal(assetBalance.cost_basis);
            existing.agg.amount_with_cost_basis = existing.agg.amount_with_cost_basis.plus(amount);
            existing.agg.total_cost_basis = (existing.agg.total_cost_basis ?? new Decimal(0)).plus(
              costBasis,
            );
          }
          if (balanceDate > existing.agg.latest_balance_date) {
            existing.agg.latest_balance_date = balanceDate;
          }
          existing.agg.holdings.push({
            account_id: accountId,
            asset: normalizedAsset,
            amount: assetBalance.amount,
            cost_basis: assetBalance.cost_basis,
            timestamp: snapshot.timestamp,
          });
        } else {
          const costBasis =
            assetBalance.cost_basis === undefined
              ? undefined
              : new Decimal(assetBalance.cost_basis);
          byAsset.set(key, {
            asset: normalizedAsset,
            agg: {
              total_amount: amount,
              amount_with_cost_basis:
                costBasis === undefined ? new Decimal(0) : new Decimal(amount),
              total_cost_basis: costBasis,
              latest_balance_date: balanceDate,
              holdings: [
                {
                  account_id: accountId,
                  asset: normalizedAsset,
                  amount: assetBalance.amount,
                  cost_basis: assetBalance.cost_basis,
                  timestamp: snapshot.timestamp,
                },
              ],
            },
          });
        }
      }
    }

    return byAsset;
  }

  // -----------------------------------------------------------------------
  // Private: fetch asset valuations
  // -----------------------------------------------------------------------

  private async fetchAssetValuations(
    byAsset: Map<string, { asset: AssetType; agg: AssetAggregate }>,
    targetCurrency: string,
    asOfDate: string,
  ): Promise<Map<string, AssetValuation>> {
    const cache = new Map<string, AssetValuation>();

    for (const [key, { asset }] of byAsset) {
      const valuation = await this.valueAsset(asset, new Decimal(1), targetCurrency, asOfDate);
      cache.set(key, valuation);
    }

    return cache;
  }

  private assetValue(agg: AssetAggregate, valuation: AssetValuation): Decimal | undefined {
    return valuation.value === undefined ? undefined : valuation.value.times(agg.total_amount);
  }

  private equityValueTotals(
    byAsset: Map<string, { asset: AssetType; agg: AssetAggregate }>,
    priceCache: Map<string, AssetValuation>,
  ): { equityValue: Decimal; nonEquityValue: Decimal } {
    let equityValue = new Decimal(0);
    let nonEquityValue = new Decimal(0);

    for (const [key, { asset, agg }] of byAsset) {
      const valuation = priceCache.get(key);
      if (valuation === undefined) {
        throw new Error(`Missing valuation for asset ${AssetId.fromAsset(asset).asStr()}`);
      }

      const value = this.assetValue(agg, valuation);
      if (value === undefined) continue;

      if (asset.type === 'equity') {
        equityValue = equityValue.plus(value);
      } else {
        nonEquityValue = nonEquityValue.plus(value);
      }
    }

    return { equityValue, nonEquityValue };
  }

  private resolveEquityValuationScenario(
    byAsset: Map<string, { asset: AssetType; agg: AssetAggregate }>,
    priceCache: Map<string, AssetValuation>,
    adjustment: EquityValuationAdjustment | undefined,
    currencyDecimals: number | undefined,
  ): ResolvedValuationScenario | undefined {
    if (adjustment === undefined) return undefined;

    const { equityValue: equityValueBefore, nonEquityValue } = this.equityValueTotals(
      byAsset,
      priceCache,
    );
    if (equityValueBefore.lte(0)) {
      throw new Error('Equity valuation scenario requires positive priced equity holdings');
    }

    let multiplier: Decimal;
    let targetPreTaxTotalValue: Decimal | undefined;
    if (adjustment.type === 'percent_change') {
      multiplier = new Decimal(1).plus(adjustment.percent.div(100));
    } else {
      targetPreTaxTotalValue = adjustment.amount;
      const requiredEquityValue = targetPreTaxTotalValue.minus(nonEquityValue);
      if (requiredEquityValue.lt(0)) {
        throw new Error(
          `Target pre-tax total value ${targetPreTaxTotalValue.toString()} is below non-equity portfolio value ${nonEquityValue.toString()}`,
        );
      }
      multiplier = requiredEquityValue.div(equityValueBefore);
    }

    if (multiplier.lt(0)) {
      throw new Error('Equity valuation scenario would produce negative equity prices');
    }

    const equityValueAfter = equityValueBefore.times(multiplier);
    const preTaxTotalValue = nonEquityValue.plus(equityValueAfter);
    const equityChangePercent = multiplier.minus(1).times(100);

    return {
      multiplier,
      output: {
        equity_multiplier: decStr(multiplier),
        equity_change_percent: decStr(equityChangePercent),
        pre_tax_total_value: decStrRounded(preTaxTotalValue, currencyDecimals),
        equity_value_before: decStrRounded(equityValueBefore, currencyDecimals),
        equity_value_after: decStrRounded(equityValueAfter, currencyDecimals),
        target_pre_tax_total_value:
          targetPreTaxTotalValue === undefined
            ? undefined
            : decStrRounded(targetPreTaxTotalValue, currencyDecimals),
      },
    };
  }

  private applyEquityValuationMultiplier(
    priceCache: Map<string, AssetValuation>,
    multiplier: Decimal,
  ): Map<string, AssetValuation> {
    const adjusted = new Map<string, AssetValuation>();
    for (const [key, valuation] of priceCache) {
      const next = { ...valuation };
      const asset = key;
      if (asset.startsWith('equity:') || asset.startsWith('equity/')) {
        next.value = next.value?.times(multiplier);
        if (next.price !== undefined) {
          next.price = decStr(new Decimal(next.price).times(multiplier));
        }
      }
      adjusted.set(key, next);
    }
    return adjusted;
  }

  // -----------------------------------------------------------------------
  // Private: value an asset
  // -----------------------------------------------------------------------

  private async valueAsset(
    asset: AssetType,
    amount: Decimal,
    targetCurrency: string,
    asOfDate: string,
  ): Promise<AssetValuation> {
    switch (asset.type) {
      case 'currency': {
        const isoCode = asset.iso_code;
        if (isoCode.toUpperCase() === targetCurrency.toUpperCase()) {
          // Same currency, no conversion
          return {
            value: amount,
            price: undefined,
            price_date: undefined,
            price_timestamp: undefined,
            fx_rate: undefined,
            fx_date: undefined,
          };
        }

        // Need FX conversion
        try {
          const rate = await this.marketData.fxClose(isoCode, targetCurrency, asOfDate);
          const fxRate = new Decimal(rate.rate);
          return {
            value: amount.times(fxRate),
            price: undefined,
            price_date: undefined,
            price_timestamp: undefined,
            fx_rate: decStr(fxRate),
            fx_date: rate.as_of_date,
          };
        } catch {
          // No FX rate available
          return {
            value: undefined,
            price: undefined,
            price_date: undefined,
            price_timestamp: undefined,
            fx_rate: undefined,
            fx_date: undefined,
          };
        }
      }

      case 'equity':
      case 'crypto': {
        const today = this.clock.today();
        let pricePoint;
        try {
          if (asOfDate === today) {
            pricePoint = await this.marketData.priceLatest(asset, asOfDate);
          } else {
            pricePoint =
              (await this.marketData.valuationPriceFromStore(asset, asOfDate, true)) ??
              (await this.marketData.priceClose(asset, asOfDate));
          }
        } catch {
          // No price available
          return {
            value: undefined,
            price: undefined,
            price_date: undefined,
            price_timestamp: undefined,
            fx_rate: undefined,
            fx_date: undefined,
          };
        }

        const price = new Decimal(pricePoint.price);
        const valueInQuote = amount.times(price);

        // Check if quote currency matches target
        if (pricePoint.quote_currency.toUpperCase() === targetCurrency.toUpperCase()) {
          return {
            value: valueInQuote,
            price: decStr(price),
            price_date: pricePoint.as_of_date,
            price_timestamp: pricePoint.timestamp,
            fx_rate: undefined,
            fx_date: undefined,
          };
        }

        // Need FX conversion from quote currency to target
        try {
          const rate = await this.marketData.fxClose(
            pricePoint.quote_currency,
            targetCurrency,
            asOfDate,
          );
          const fxRate = new Decimal(rate.rate);
          return {
            value: valueInQuote.times(fxRate),
            price: decStr(price),
            price_date: pricePoint.as_of_date,
            price_timestamp: pricePoint.timestamp,
            fx_rate: decStr(fxRate),
            fx_date: rate.as_of_date,
          };
        } catch {
          // Have price but no FX rate
          return {
            value: undefined,
            price: decStr(price),
            price_date: pricePoint.as_of_date,
            price_timestamp: pricePoint.timestamp,
            fx_rate: undefined,
            fx_date: undefined,
          };
        }
      }
    }
  }

  // -----------------------------------------------------------------------
  // Private: build asset summaries
  // -----------------------------------------------------------------------

  private buildAssetSummaries(
    byAsset: Map<string, { asset: AssetType; agg: AssetAggregate }>,
    priceCache: Map<string, AssetValuation>,
    accountMap: Map<string, AccountType>,
    includeDetail: boolean,
    currencyDecimals: number | undefined,
    capitalGainsTaxRate: Decimal | undefined,
  ): { summaries: AssetSummary[]; totalValue: Decimal; gainsTotals: GainsTotals } {
    const summaries: AssetSummary[] = [];
    let totalValue = new Decimal(0);
    const gainsTotals: GainsTotals = {
      total_cost_basis: undefined,
      total_unrealized_gain: undefined,
      prospective_capital_gains_tax: undefined,
    };

    for (const [key, { asset, agg }] of byAsset) {
      const valuation = priceCache.get(key);
      if (valuation === undefined) {
        throw new Error(`Missing valuation for asset ${AssetId.fromAsset(asset).asStr()}`);
      }

      const assetValue =
        valuation.value !== undefined ? valuation.value.times(agg.total_amount) : undefined;

      if (assetValue !== undefined) {
        totalValue = totalValue.plus(assetValue);
      }

      const costBasis = agg.total_cost_basis;
      const unrealizedGain =
        valuation.value !== undefined && costBasis !== undefined
          ? valuation.value.times(agg.amount_with_cost_basis).minus(costBasis)
          : undefined;
      const prospectiveTax =
        unrealizedGain !== undefined && unrealizedGain.gt(0) && capitalGainsTaxRate !== undefined
          ? unrealizedGain.times(capitalGainsTaxRate)
          : undefined;

      if (costBasis !== undefined) {
        gainsTotals.total_cost_basis = (gainsTotals.total_cost_basis ?? new Decimal(0)).plus(
          costBasis,
        );
      }
      if (unrealizedGain !== undefined) {
        gainsTotals.total_unrealized_gain = (
          gainsTotals.total_unrealized_gain ?? new Decimal(0)
        ).plus(unrealizedGain);
      }
      if (prospectiveTax !== undefined) {
        gainsTotals.prospective_capital_gains_tax = (
          gainsTotals.prospective_capital_gains_tax ?? new Decimal(0)
        ).plus(prospectiveTax);
      }

      const holdings = includeDetail
        ? this.buildHoldingsDetail(agg.holdings, accountMap, valuation.value, currencyDecimals)
        : undefined;

      const summary: AssetSummary = {
        asset,
        total_amount: decStr(agg.total_amount),
        amount_date: agg.latest_balance_date,
        value_in_base:
          assetValue !== undefined ? decStrRounded(assetValue, currencyDecimals) : undefined,
        holdings,
      };

      if (valuation.price !== undefined) {
        summary.price = valuation.price;
      }
      if (valuation.price_date !== undefined) {
        summary.price_date = valuation.price_date;
      }
      if (valuation.price_timestamp !== undefined) {
        summary.price_timestamp = valuation.price_timestamp;
      }
      if (valuation.fx_rate !== undefined) {
        summary.fx_rate = valuation.fx_rate;
      }
      if (valuation.fx_date !== undefined) {
        summary.fx_date = valuation.fx_date;
      }
      if (costBasis !== undefined) {
        summary.cost_basis = decStrRounded(costBasis, currencyDecimals);
      }
      if (unrealizedGain !== undefined) {
        summary.unrealized_gain = decStrRounded(unrealizedGain, currencyDecimals);
      }
      if (prospectiveTax !== undefined) {
        summary.prospective_capital_gains_tax = decStrRounded(prospectiveTax, currencyDecimals);
      }

      summaries.push(summary);
    }

    return { summaries, totalValue, gainsTotals };
  }

  private buildHoldingsDetail(
    holdings: HoldingEntry[],
    accountMap: Map<string, AccountType>,
    unitValue: Decimal | undefined,
    currencyDecimals: number | undefined,
  ): AccountHolding[] {
    return holdings.map((h) => {
      const account = accountMap.get(h.account_id.asStr());
      const accountName = account?.name ?? '';
      const amount = new Decimal(h.amount);
      const out: AccountHolding = {
        account_id: h.account_id.asStr(),
        account_name: accountName,
        amount: decStr(amount),
        balance_date: dateToString(h.timestamp),
      };
      if (h.cost_basis !== undefined) {
        const costBasis = new Decimal(h.cost_basis);
        out.cost_basis = decStrRounded(costBasis, currencyDecimals);
        if (unitValue !== undefined) {
          out.unrealized_gain = decStrRounded(
            unitValue.times(amount).minus(costBasis),
            currencyDecimals,
          );
        }
      }
      return out;
    });
  }

  // -----------------------------------------------------------------------
  // Private: build account summaries
  // -----------------------------------------------------------------------

  private buildAccountSummaries(
    snapshots: Array<[Id, BalanceSnapshotType]>,
    zeroAccounts: Id[],
    priceCache: Map<string, AssetValuation>,
    accountMap: Map<string, AccountType>,
    connectionMap: Map<string, ConnectionType>,
    currencyDecimals: number | undefined,
  ): AccountSummary[] {
    // Track (sum, hasMissing) per account
    const byAccount = new Map<string, { sum: Decimal; hasMissing: boolean }>();

    for (const [accountId, snapshot] of snapshots) {
      for (const assetBalance of snapshot.balances) {
        const normalizedAsset = Asset.normalized(assetBalance.asset);
        const assetKey = Asset.hash(normalizedAsset);
        const amount = new Decimal(assetBalance.amount);

        const valuation = priceCache.get(assetKey);
        if (valuation === undefined) {
          throw new Error(
            `Missing valuation for asset ${AssetId.fromAsset(normalizedAsset).asStr()}`,
          );
        }

        const accountKey = accountId.asStr();
        let entry = byAccount.get(accountKey);
        if (entry === undefined) {
          entry = { sum: new Decimal(0), hasMissing: false };
          byAccount.set(accountKey, entry);
        }

        if (valuation.value !== undefined) {
          entry.sum = entry.sum.plus(valuation.value.times(amount));
        } else {
          entry.hasMissing = true;
        }
      }
    }

    const summaries: AccountSummary[] = [];

    for (const [accountKey, { sum, hasMissing }] of byAccount) {
      const account = accountMap.get(accountKey);
      if (account === undefined) continue;

      const connection = connectionMap.get(account.connection_id.asStr());
      if (connection === undefined) continue;

      summaries.push({
        account_id: accountKey,
        account_name: account.name,
        connection_name: connection.config.name,
        value_in_base: hasMissing ? undefined : decStrRounded(sum, currencyDecimals),
      });
    }

    // Add zero accounts
    for (const accountId of zeroAccounts) {
      const accountKey = accountId.asStr();
      // Skip if already present
      if (summaries.some((s) => s.account_id === accountKey)) {
        continue;
      }

      const account = accountMap.get(accountKey);
      if (account === undefined) continue;

      const connection = connectionMap.get(account.connection_id.asStr());
      if (connection === undefined) continue;

      summaries.push({
        account_id: accountKey,
        account_name: account.name,
        connection_name: connection.config.name,
        value_in_base: decStrRounded(new Decimal(0), currencyDecimals),
      });
    }

    return summaries;
  }
}
