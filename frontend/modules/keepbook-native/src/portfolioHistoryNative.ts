/**
 * Native-safe portfolio history function.
 *
 * This is a standalone reimplementation of the `portfolioHistory` function from
 * `ts/src/app/portfolio.ts` that avoids importing modules with `node:` dependencies
 * (JsonlMarketDataStore, git.ts). It uses only the clean building blocks.
 */

import type { Storage } from '@keepbook/storage/storage';
import type { MarketDataStore } from '@keepbook/market-data/store';
import { MarketDataService } from '@keepbook/market-data/service';
import { AssetId } from '@keepbook/market-data/asset-id';
import { PortfolioService } from '@keepbook/portfolio/service';
import { Decimal } from '@keepbook/decimal';
import {
  collectChangePoints,
  filterByDateRange,
  filterByGranularity,
  type ChangePoint,
  type ChangeTrigger,
} from '@keepbook/portfolio/change-points';
import {
  formatRfc3339,
  formatRfc3339FromEpochNanos,
  parseGranularity,
  formatDateYMD,
  decStr,
  decStrRounded,
} from '@keepbook/app/format';
import type { ResolvedConfig } from '@keepbook/config';
import type { AssetSummary } from '@keepbook/portfolio/models';
import type {
  HistoryOutput,
  HistoryPoint,
  HistorySummary,
} from '@keepbook/app/types';
import { type Clock, SystemClock } from '@keepbook/clock';

// ---------------------------------------------------------------------------
// Options
// ---------------------------------------------------------------------------

export interface PortfolioHistoryOptions {
  currency?: string;
  start?: string;
  end?: string;
  granularity?: string;
  includePrices?: boolean;
}

// ---------------------------------------------------------------------------
// Helpers (copied from ts/src/app/portfolio.ts to avoid node: imports)
// ---------------------------------------------------------------------------

function formatTrigger(trigger: ChangeTrigger): string {
  switch (trigger.type) {
    case 'balance':
      return `balance:${trigger.account_id}:${JSON.stringify(trigger.asset)}`;
    case 'price':
      return `price:${trigger.asset_id.asStr()}`;
    case 'fx_rate':
      return `fx:${trigger.base}/${trigger.quote}`;
  }
}

function computeHistoryTotalValueWithCarryForward(
  byAsset: AssetSummary[],
  carryForwardUnitValues: Map<string, Decimal>,
  currencyDecimals: number | undefined,
): string | undefined {
  try {
    let totalValue = new Decimal(0);

    for (const summary of byAsset) {
      const assetId = AssetId.fromAsset(summary.asset).asStr();
      const totalAmount = new Decimal(summary.total_amount);
      let assetValue: Decimal;

      if (summary.value_in_base !== undefined) {
        assetValue = new Decimal(summary.value_in_base);
        if (!totalAmount.isZero()) {
          carryForwardUnitValues.set(assetId, assetValue.div(totalAmount));
        }
      } else if (totalAmount.isZero()) {
        assetValue = new Decimal(0);
      } else {
        const unitValue = carryForwardUnitValues.get(assetId);
        assetValue = unitValue !== undefined ? unitValue.times(totalAmount) : new Decimal(0);
      }

      totalValue = totalValue.plus(assetValue);
    }

    return decStrRounded(totalValue, currencyDecimals);
  } catch {
    return undefined;
  }
}

// ---------------------------------------------------------------------------
// Main function
// ---------------------------------------------------------------------------

/**
 * Execute the portfolio history command (native-safe version).
 *
 * This mirrors the logic of `portfolioHistory` in `ts/src/app/portfolio.ts`
 * but avoids importing `JsonlMarketDataStore` and `git.ts`.
 */
export async function portfolioHistoryNative(
  storage: Storage,
  marketDataStore: MarketDataStore,
  config: ResolvedConfig,
  options: PortfolioHistoryOptions,
  clock?: Clock,
): Promise<HistoryOutput> {
  const effectiveClock = clock ?? new SystemClock();
  const currency = options.currency ?? config.reporting_currency;
  const granularity = parseGranularity(options.granularity ?? 'none');

  const marketDataService = new MarketDataService(marketDataStore);

  // Collect change points
  const allPoints = await collectChangePoints(storage, marketDataStore, {
    includePrices: options.includePrices ?? true,
  });

  // Filter by date range
  const dateFiltered = filterByDateRange(allPoints, options.start, options.end);

  // Filter by granularity
  const points = filterByGranularity(dateFiltered, granularity, 'last');

  // Calculate portfolio value at each change point
  const historyPoints: HistoryPoint[] = [];
  let previousTotalValue: Decimal | undefined;
  const carryForwardUnitValues = new Map<string, Decimal>();
  for (const point of points) {
    const portfolioService = new PortfolioService(storage, marketDataService, effectiveClock);

    const snapshot = await portfolioService.calculate({
      as_of_date: formatDateYMD(point.timestamp),
      currency,
      currency_decimals: config.display.currency_decimals,
      grouping: 'both',
      include_detail: false,
    });

    const totalValue =
      snapshot.by_asset !== undefined
        ? computeHistoryTotalValueWithCarryForward(
            snapshot.by_asset,
            carryForwardUnitValues,
            config.display.currency_decimals,
          ) ?? snapshot.total_value
        : snapshot.total_value;
    const currentTotalValue = new Decimal(totalValue);

    // Format triggers
    const triggers = point.triggers.map(formatTrigger);

    let percentageChangeFromPrevious: string | null = null;
    if (previousTotalValue !== undefined) {
      if (previousTotalValue.isZero()) {
        percentageChangeFromPrevious = 'N/A';
      } else {
        percentageChangeFromPrevious = currentTotalValue
          .minus(previousTotalValue)
          .div(previousTotalValue)
          .times(100)
          .toDecimalPlaces(2)
          .toFixed(2);
      }
    }

    const historyPoint: HistoryPoint = {
      timestamp:
        point.timestamp_nanos !== undefined
          ? formatRfc3339FromEpochNanos(point.timestamp_nanos)
          : formatRfc3339(point.timestamp),
      date: formatDateYMD(point.timestamp),
      total_value: totalValue,
      percentage_change_from_previous: percentageChangeFromPrevious,
      change_triggers: triggers.length > 0 ? triggers : undefined,
    };

    historyPoints.push(historyPoint);
    previousTotalValue = currentTotalValue;
  }

  // Calculate summary if 2+ points
  let summary: HistorySummary | undefined;
  if (historyPoints.length >= 2) {
    const initialValue = new Decimal(historyPoints[0].total_value);
    const finalValue = new Decimal(historyPoints[historyPoints.length - 1].total_value);
    const absoluteChange = finalValue.minus(initialValue);

    let percentageChange: string;
    if (initialValue.isZero()) {
      percentageChange = 'N/A';
    } else {
      const pct = finalValue.minus(initialValue).div(initialValue).times(100);
      percentageChange = pct.toDecimalPlaces(2).toFixed(2);
    }

    summary = {
      initial_value: decStr(initialValue),
      final_value: decStr(finalValue),
      absolute_change: decStr(absoluteChange),
      percentage_change: percentageChange,
    };
  }

  return {
    currency,
    start_date: options.start ?? null,
    end_date: options.end ?? null,
    granularity: options.granularity ?? 'none',
    points: historyPoints,
    summary,
  };
}
