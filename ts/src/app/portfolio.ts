/**
 * Portfolio snapshot and history commands.
 *
 * Provides `serializeSnapshot` (convert library types to plain JSON-serializable
 * objects), `portfolioSnapshot` (the top-level snapshot command handler), and
 * `portfolioHistory` (the top-level history command handler).
 */

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import type { Storage } from '../storage/storage.js';
import type { MarketDataStore } from '../market-data/store.js';
import { MarketDataService } from '../market-data/service.js';
import { JsonlMarketDataStore } from '../market-data/jsonl-store.js';
import { AssetId } from '../market-data/asset-id.js';
import type { PricePoint, FxRatePoint } from '../market-data/models.js';
import { PortfolioService } from '../portfolio/service.js';
import { Asset, type AssetType } from '../models/asset.js';
import { Id } from '../models/id.js';
import type { AccountType } from '../models/account.js';
import { findAccount, findConnection } from '../storage/lookup.js';
import type {
  PortfolioSnapshot,
  AssetSummary,
  AccountSummary,
  AccountHolding,
  Grouping,
  EquityValuationAdjustment,
} from '../portfolio/models.js';
import { type Clock, SystemClock } from '../clock.js';
import {
  formatChronoSerde,
  formatChronoSerdeFromEpochNanos,
  parseGranularity,
  formatDateYMD,
  formatRfc3339,
  formatRfc3339FromEpochNanos,
  decStr,
  decStrRounded,
} from './format.js';
import type { ResolvedConfig } from '../config.js';
import {
  collectChangePoints,
  filterByDateRange,
  filterByGranularity,
  type ChangePoint,
  type ChangeTrigger,
} from '../portfolio/change-points.js';
import type {
  HistoryOutput,
  HistoryPoint,
  HistorySummary,
  SerializedChangePoint,
  SerializedChangeTrigger,
  ChangePointsOutput,
  PriceHistoryOutput,
  PriceHistoryScopeOutput,
  PriceHistoryStats,
  PriceHistoryFailure,
  TaxImpactOutput,
  TaxImpactPoint,
  TaxImpactGraphOutput,
} from './types.js';
import { Decimal } from '../decimal.js';
import { tryAutoCommit } from '../git.js';

// ---------------------------------------------------------------------------
// Serialization
// ---------------------------------------------------------------------------

/**
 * Serialize an `AccountHolding` to a plain object.
 * All fields are already JSON-safe strings.
 */
function serializeHolding(h: AccountHolding): object {
  const out: Record<string, unknown> = {
    account_id: h.account_id,
    account_name: h.account_name,
    amount: h.amount,
    balance_date: h.balance_date,
  };
  if (h.cost_basis !== undefined) out.cost_basis = h.cost_basis;
  if (h.unrealized_gain !== undefined) out.unrealized_gain = h.unrealized_gain;
  return out;
}

/**
 * Serialize an `AssetSummary` to a plain JSON-serializable object.
 *
 * - `price_timestamp` (Date) is formatted via `formatChronoSerde`.
 * - `undefined` fields are omitted (matches Rust `skip_serializing_if`).
 */
function serializeAssetSummary(s: AssetSummary): object {
  const out: Record<string, unknown> = {
    asset: s.asset,
    total_amount: s.total_amount,
    amount_date: s.amount_date,
  };

  if (s.price !== undefined) out.price = s.price;
  if (s.price_date !== undefined) out.price_date = s.price_date;
  if (s.price_timestamp !== undefined) {
    out.price_timestamp = formatChronoSerde(s.price_timestamp);
  }
  if (s.fx_rate !== undefined) out.fx_rate = s.fx_rate;
  if (s.fx_date !== undefined) out.fx_date = s.fx_date;
  if (s.value_in_base !== undefined) out.value_in_base = s.value_in_base;
  if (s.cost_basis !== undefined) out.cost_basis = s.cost_basis;
  if (s.unrealized_gain !== undefined) out.unrealized_gain = s.unrealized_gain;
  if (s.prospective_capital_gains_tax !== undefined) {
    out.prospective_capital_gains_tax = s.prospective_capital_gains_tax;
  }
  if (s.holdings !== undefined) {
    out.holdings = s.holdings.map(serializeHolding);
  }

  return out;
}

/**
 * Serialize an `AccountSummary` to a plain object.
 *
 * `value_in_base` is omitted when undefined.
 */
function serializeAccountSummary(s: AccountSummary): object {
  const out: Record<string, unknown> = {
    account_id: s.account_id,
    account_name: s.account_name,
    connection_name: s.connection_name,
  };

  if (s.value_in_base !== undefined) out.value_in_base = s.value_in_base;

  return out;
}

/**
 * Deep-convert a `PortfolioSnapshot` to a plain JSON-serializable object.
 *
 * - `price_timestamp: Date` fields are formatted via `formatChronoSerde`.
 * - `undefined` fields are omitted by construction (matches Rust `skip_serializing_if`).
 */
export function serializeSnapshot(snapshot: PortfolioSnapshot): object {
  const out: Record<string, unknown> = {
    as_of_date: snapshot.as_of_date,
    currency: snapshot.currency,
    total_value: snapshot.total_value,
  };

  if (snapshot.total_cost_basis !== undefined) out.total_cost_basis = snapshot.total_cost_basis;
  if (snapshot.total_unrealized_gain !== undefined) {
    out.total_unrealized_gain = snapshot.total_unrealized_gain;
  }
  if (snapshot.prospective_capital_gains_tax !== undefined) {
    out.prospective_capital_gains_tax = snapshot.prospective_capital_gains_tax;
  }
  if (snapshot.valuation_scenario !== undefined) {
    const scenario: Record<string, unknown> = {
      equity_multiplier: snapshot.valuation_scenario.equity_multiplier,
      equity_change_percent: snapshot.valuation_scenario.equity_change_percent,
      pre_tax_total_value: snapshot.valuation_scenario.pre_tax_total_value,
      equity_value_before: snapshot.valuation_scenario.equity_value_before,
      equity_value_after: snapshot.valuation_scenario.equity_value_after,
    };
    if (snapshot.valuation_scenario.target_pre_tax_total_value !== undefined) {
      scenario.target_pre_tax_total_value =
        snapshot.valuation_scenario.target_pre_tax_total_value;
    }
    out.valuation_scenario = scenario;
  }
  if (snapshot.by_asset !== undefined) {
    out.by_asset = snapshot.by_asset.map(serializeAssetSummary);
  }
  if (snapshot.by_account !== undefined) {
    out.by_account = snapshot.by_account.map(serializeAccountSummary);
  }

  return out;
}

// ---------------------------------------------------------------------------
// Grouping parser
// ---------------------------------------------------------------------------

function parseGrouping(s: string | undefined): Grouping {
  if (s === undefined) return 'both';
  switch (s.toLowerCase()) {
    case 'asset':
      return 'asset';
    case 'account':
      return 'account';
    case 'both':
      return 'both';
    default:
      return 'both';
  }
}

function parseTaxRateFraction(rate: string, context: string): Decimal {
  try {
    return new Decimal(rate).div(100);
  } catch {
    throw new Error(`Invalid ${context}: ${rate}`);
  }
}

function parseDecimalArg(value: string, context: string): Decimal {
  try {
    return new Decimal(value);
  } catch {
    throw new Error(`Invalid ${context}: ${value}`);
  }
}

function resolveEquityValuationAdjustment(
  equityChangePercent: string | undefined,
  targetPreTaxTotalValue: string | undefined,
): EquityValuationAdjustment | undefined {
  if (equityChangePercent !== undefined && targetPreTaxTotalValue !== undefined) {
    throw new Error(
      '--equity-change-percent and --target-pre-tax-total-value cannot be used together',
    );
  }

  if (equityChangePercent !== undefined) {
    return {
      type: 'percent_change',
      percent: parseDecimalArg(equityChangePercent, 'equity change percent'),
    };
  }

  if (targetPreTaxTotalValue !== undefined) {
    return {
      type: 'target_pre_tax_total_value',
      amount: parseDecimalArg(targetPreTaxTotalValue, 'target pre-tax total value'),
    };
  }

  return undefined;
}

function resolveCapitalGainsTaxRate(
  config: ResolvedConfig,
  cliPercentRate: string | undefined,
): { rate: Decimal | undefined; includeLatentTaxVirtualAccount: boolean } {
  const latentTax = config.portfolio.latent_capital_gains_tax;
  if (cliPercentRate !== undefined) {
    return {
      rate: parseTaxRateFraction(cliPercentRate, 'capital gains tax rate'),
      includeLatentTaxVirtualAccount: latentTax.enabled,
    };
  }

  if (!latentTax.enabled) {
    return { rate: undefined, includeLatentTaxVirtualAccount: false };
  }

  if (latentTax.rate === undefined) {
    throw new Error('portfolio.latent_capital_gains_tax.enabled requires a rate');
  }

  return {
    rate: new Decimal(latentTax.rate),
    includeLatentTaxVirtualAccount: true,
  };
}

function applyLatentTaxVirtualAccount(
  snapshot: PortfolioSnapshot,
  config: ResolvedConfig,
): PortfolioSnapshot {
  if (snapshot.prospective_capital_gains_tax === undefined) return snapshot;

  const tax = new Decimal(snapshot.prospective_capital_gains_tax);
  if (tax.lte(0)) return snapshot;

  const totalValue = new Decimal(snapshot.total_value);
  const next: PortfolioSnapshot = {
    ...snapshot,
    total_value: decStrRounded(totalValue.minus(tax), config.display.currency_decimals),
  };

  if (next.by_account !== undefined) {
    next.by_account = [
      ...next.by_account,
      {
        account_id: 'virtual:latent_capital_gains_tax',
        account_name: config.portfolio.latent_capital_gains_tax.account_name,
        connection_name: 'Virtual',
        value_in_base: decStrRounded(tax.negated(), config.display.currency_decimals),
      },
    ];
  }

  return next;
}

// ---------------------------------------------------------------------------
// Command handler
// ---------------------------------------------------------------------------

export interface PortfolioSnapshotOptions {
  currency?: string;
  date?: string;
  groupBy?: string;
  detail?: boolean;
  capitalGainsTaxRate?: string;
  equityChangePercent?: string;
  targetPreTaxTotalValue?: string;
}

/**
 * Execute the portfolio snapshot command.
 *
 * Creates a store-only `MarketDataService` (no external sources) and a
 * `PortfolioService`, computes the snapshot, and serializes it.
 */
export async function portfolioSnapshot(
  storage: Storage,
  marketDataStore: MarketDataStore,
  config: ResolvedConfig,
  options: PortfolioSnapshotOptions,
  clock?: Clock,
): Promise<object> {
  const effectiveClock = clock ?? new SystemClock();
  const currency = options.currency ?? config.reporting_currency;
  const asOfDate = options.date ?? effectiveClock.today();
  const grouping = parseGrouping(options.groupBy);
  const includeDetail = options.detail ?? false;
  const { rate: capitalGainsTaxRate, includeLatentTaxVirtualAccount } = resolveCapitalGainsTaxRate(
    config,
    options.capitalGainsTaxRate,
  );
  const equityValuationAdjustment = resolveEquityValuationAdjustment(
    options.equityChangePercent,
    options.targetPreTaxTotalValue,
  );

  const marketDataService = new MarketDataService(marketDataStore);
  const portfolioService = new PortfolioService(storage, marketDataService, effectiveClock);

  let snapshot = await portfolioService.calculate({
    as_of_date: asOfDate,
    currency,
    currency_decimals: config.display.currency_decimals,
    grouping,
    include_detail: includeDetail,
    capital_gains_tax_rate: capitalGainsTaxRate,
    equity_valuation_adjustment: equityValuationAdjustment,
  });
  if (includeLatentTaxVirtualAccount) {
    snapshot = applyLatentTaxVirtualAccount(snapshot, config);
  }

  return serializeSnapshot(snapshot);
}

export interface PortfolioTaxImpactOptions {
  currency?: string;
  date?: string;
  capitalGainsTaxRate?: string;
  min?: string;
  max?: string;
  points?: number;
  graph?: boolean;
  output?: string;
  svgOutput?: string;
  title?: string;
  width?: number;
  height?: number;
}

export async function portfolioTaxImpact(
  storage: Storage,
  marketDataStore: MarketDataStore,
  config: ResolvedConfig,
  options: PortfolioTaxImpactOptions,
  clock?: Clock,
): Promise<TaxImpactOutput> {
  const effectiveClock = clock ?? new SystemClock();
  const currency = options.currency ?? config.reporting_currency;
  const asOfDate = options.date ?? effectiveClock.today();
  const { rate } = resolveCapitalGainsTaxRate(config, options.capitalGainsTaxRate);
  if (rate === undefined) {
    throw new Error(
      'portfolio tax-impact requires --capital-gains-tax-rate or enabled portfolio.latent_capital_gains_tax.rate',
    );
  }

  const pointCount = options.points ?? 25;
  if (pointCount < 1) throw new Error('points must be at least 1');

  const marketDataService = new MarketDataService(marketDataStore);
  const portfolioService = new PortfolioService(storage, marketDataService, effectiveClock);
  const baseQuery = {
    as_of_date: asOfDate,
    currency,
    currency_decimals: config.display.currency_decimals,
    grouping: 'asset' as const,
    include_detail: false,
    capital_gains_tax_rate: rate,
  };

  const base = await portfolioService.calculate(baseQuery);
  const currentNominal = parseDecimalArg(base.total_value, 'current nominal net worth');
  const currentTax =
    base.prospective_capital_gains_tax === undefined
      ? new Decimal(0)
      : parseDecimalArg(base.prospective_capital_gains_tax, 'current tax liability');
  const currentAfterTax = currentNominal.minus(currentTax);

  const minValue =
    options.min === undefined
      ? currentNominal.times('0.5')
      : parseDecimalArg(options.min, 'minimum nominal net worth');
  const maxValue =
    options.max === undefined
      ? currentNominal
      : parseDecimalArg(options.max, 'maximum nominal net worth');
  if (minValue.gt(maxValue)) throw new Error('min must be less than or equal to max');

  const targets: Decimal[] = [];
  if (pointCount === 1) {
    targets.push(minValue);
  } else {
    const step = maxValue.minus(minValue).div(pointCount - 1);
    for (let i = 0; i < pointCount; i += 1) {
      targets.push(minValue.plus(step.times(i)));
    }
  }

  const curvePoints: TaxImpactPoint[] = [];
  for (const target of targets) {
    const snapshot = await portfolioService.calculate({
      ...baseQuery,
      equity_valuation_adjustment: {
        type: 'target_pre_tax_total_value',
        amount: target,
      },
    });
    if (snapshot.valuation_scenario === undefined) {
      throw new Error('Missing valuation_scenario for tax impact point');
    }

    const nominal = parseDecimalArg(snapshot.total_value, 'scenario nominal net worth');
    const tax =
      snapshot.prospective_capital_gains_tax === undefined
        ? new Decimal(0)
        : parseDecimalArg(snapshot.prospective_capital_gains_tax, 'scenario tax liability');

    curvePoints.push({
      nominal_net_worth: decStrRounded(nominal, config.display.currency_decimals),
      tax_liability: decStrRounded(tax, config.display.currency_decimals),
      after_tax_net_worth: decStrRounded(nominal.minus(tax), config.display.currency_decimals),
      equity_multiplier: snapshot.valuation_scenario.equity_multiplier,
      equity_change_percent: snapshot.valuation_scenario.equity_change_percent,
    });
  }

  const shouldWriteGraph = options.graph === true || options.output !== undefined || options.svgOutput !== undefined;
  const graphOutput = shouldWriteGraph
    ? await writeTaxImpactGraph(curvePoints, currency, {
        title: options.title ?? 'Keepbook Tax Impact',
        output: options.output ?? 'artifacts/tax-impact.html',
        svgOutput: options.svgOutput,
        width: options.width ?? 1400,
        height: options.height ?? 900,
      })
    : undefined;

  const result: TaxImpactOutput = {
    currency,
    as_of_date: asOfDate,
    capital_gains_tax_rate: decStr(rate),
    current_nominal_net_worth: decStrRounded(currentNominal, config.display.currency_decimals),
    current_tax_liability: decStrRounded(currentTax, config.display.currency_decimals),
    current_after_tax_net_worth: decStrRounded(currentAfterTax, config.display.currency_decimals),
    points: curvePoints,
  };
  if (graphOutput !== undefined) result.graph = graphOutput;
  return result;
}

async function writeTaxImpactGraph(
  points: TaxImpactPoint[],
  currency: string,
  options: { title: string; output: string; svgOutput?: string; width: number; height: number },
): Promise<TaxImpactGraphOutput> {
  if (options.width < 360 || options.height < 240) {
    throw new Error('Graph width and height must be at least 360x240');
  }
  const svgOutput =
    options.svgOutput ??
    path.join(
      path.dirname(options.output),
      `${path.basename(options.output, path.extname(options.output))}.svg`,
    );
  const svg = renderTaxImpactSvg(points, currency, options.title, options.width, options.height);
  const html = renderTaxImpactHtml(options.title, options.width, options.output, svgOutput);

  await mkdir(path.dirname(svgOutput), { recursive: true });
  await mkdir(path.dirname(options.output), { recursive: true });
  await writeFile(svgOutput, svg);
  await writeFile(options.output, html);

  return { html_path: options.output, svg_path: svgOutput };
}

function renderTaxImpactHtml(
  title: string,
  width: number,
  htmlPath: string,
  svgPath: string,
): string {
  const imgSrc =
    path.dirname(htmlPath) === path.dirname(svgPath) ? `./${path.basename(svgPath)}` : svgPath;
  return `<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>${taxGraphEscapeHtml(title)}</title>
    <style>
      body { margin: 0; background: #edf2f7; display: grid; place-items: center; min-height: 100vh; }
      img { width: min(96vw, ${width}px); height: auto; box-shadow: 0 18px 48px rgba(16,42,67,.18); border-radius: 20px; }
    </style>
  </head>
  <body>
    <img src="${taxGraphEscapeHtml(imgSrc)}" alt="${taxGraphEscapeHtml(title)}" />
  </body>
</html>
`;
}

function renderTaxImpactSvg(
  points: TaxImpactPoint[],
  currency: string,
  title: string,
  width: number,
  height: number,
): string {
  if (points.length === 0) throw new Error('Tax impact graph requires at least one point');
  const marginLeft = 126;
  const marginRight = 64;
  const marginTop = 108;
  const marginBottom = 104;
  const plotX = marginLeft;
  const plotY = marginTop;
  const plotW = width - marginLeft - marginRight;
  const plotH = height - marginTop - marginBottom;

  const xs = points.map((point) => Number.parseFloat(point.nominal_net_worth));
  const ys = points.map((point) => Number.parseFloat(point.after_tax_net_worth));
  const xMin = Math.min(...xs);
  const xMax = Math.max(...xs);
  const yMinRaw = Math.min(...ys, xMin);
  const yMaxRaw = Math.max(...ys, xMax);
  const xCollapsed = Math.abs(xMax - xMin) < Number.EPSILON;
  const xSpan = xCollapsed ? Math.max(Math.abs(xMin), 1) * 0.1 : Math.abs(xMax - xMin);
  const ySpan = Math.max(Math.abs(yMaxRaw - yMinRaw), 1);
  const xLo = xCollapsed ? xMin - xSpan : xMin - xSpan * 0.04;
  const xHi = xCollapsed ? xMax + xSpan : xMax + xSpan * 0.04;
  const yLo = yMinRaw - ySpan * 0.06;
  const yHi = yMaxRaw + ySpan * 0.06;

  const scaleX = (value: number): number => plotX + ((value - xLo) / (xHi - xLo)) * plotW;
  const scaleY = (value: number): number => plotY + ((yHi - value) / (yHi - yLo)) * plotH;
  const curvePoints = xs.map((x, index): [number, number] => [scaleX(x), scaleY(ys[index])]);
  const identityPoints: Array<[number, number]> = [
    [scaleX(xMin), scaleY(xMin)],
    [scaleX(xMax), scaleY(xMax)],
  ];

  let svg = `<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}" viewBox="0 0 ${width} ${height}" role="img" aria-labelledby="title desc">
  <title id="title">${taxGraphEscapeHtml(title)}</title>
  <desc id="desc">Tax impact graph from nominal net worth to after-tax net worth in ${taxGraphEscapeHtml(currency)}</desc>
  <rect width="100%" height="100%" fill="#f8fafc"/>
  <text x="${marginLeft}" y="54" font-size="34" fill="#102a43" font-family="ui-sans-serif, system-ui, sans-serif">${taxGraphEscapeHtml(title)}</text>
  <text x="${marginLeft}" y="84" font-size="18" fill="#627d98" font-family="ui-sans-serif, system-ui, sans-serif">Nominal net worth to net worth after expected latent tax - ${taxGraphEscapeHtml(currency)}</text>
  <rect x="${plotX}" y="${plotY}" width="${plotW}" height="${plotH}" fill="#ffffff" stroke="#d9e2ec" rx="8"/>
`;
  for (let i = 0; i <= 5; i += 1) {
    const ratio = i / 5;
    const y = plotY + ratio * plotH;
    const value = yHi - ratio * (yHi - yLo);
    svg += `  <line x1="${plotX}" y1="${y.toFixed(2)}" x2="${plotX + plotW}" y2="${y.toFixed(2)}" stroke="#eef2f7"/>
  <text x="${plotX - 14}" y="${(y + 5).toFixed(2)}" font-size="14" text-anchor="end" fill="#627d98" font-family="ui-sans-serif, system-ui, sans-serif">${taxGraphEscapeHtml(taxGraphFormatCurrencyTick(value, currency))}</text>
`;
  }
  for (let i = 0; i <= 5; i += 1) {
    const ratio = i / 5;
    const x = plotX + ratio * plotW;
    const value = xLo + ratio * (xHi - xLo);
    svg += `  <line x1="${x.toFixed(2)}" y1="${plotY}" x2="${x.toFixed(2)}" y2="${plotY + plotH + 8}" stroke="#e5eaf1"/>
  <text x="${x.toFixed(2)}" y="${plotY + plotH + 34}" font-size="14" text-anchor="middle" fill="#627d98" font-family="ui-sans-serif, system-ui, sans-serif">${taxGraphEscapeHtml(taxGraphFormatCurrencyTick(value, currency))}</text>
`;
  }
  svg += `  <path d="${taxGraphPathFromPoints(identityPoints)}" fill="none" stroke="#94a3b8" stroke-width="2" stroke-dasharray="8 8"/>
  <path d="${taxGraphPathFromPoints(curvePoints)}" fill="none" stroke="#1c7ed6" stroke-width="4" stroke-linejoin="round" stroke-linecap="round"/>
`;
  for (const [x, y] of curvePoints) {
    svg += `  <circle cx="${x.toFixed(2)}" cy="${y.toFixed(2)}" r="4" fill="#0b7285" stroke="#ffffff" stroke-width="2"/>
`;
  }
  svg += `  <text x="${marginLeft}" y="${height - 28}" font-size="16" fill="#829ab1" font-family="ui-sans-serif, system-ui, sans-serif">x: nominal net worth | y: after-tax net worth | dashed: no-tax line</text>
</svg>
`;
  return svg;
}

function taxGraphPathFromPoints(points: Array<[number, number]>): string {
  return points
    .map(([x, y], index) => `${index === 0 ? 'M' : 'L'} ${x.toFixed(2)} ${y.toFixed(2)}`)
    .join(' ');
}

function taxGraphFormatCurrencyTick(value: number, currency: string): string {
  const sign = value < 0 ? '-' : '';
  const abs = Math.abs(value);
  let compact: string;
  if (abs >= 1_000_000_000) compact = `${(abs / 1_000_000_000).toFixed(1)}B`;
  else if (abs >= 1_000_000) compact = `${(abs / 1_000_000).toFixed(1)}M`;
  else if (abs >= 1_000) compact = `${(abs / 1_000).toFixed(1)}K`;
  else compact = abs.toFixed(0);
  return `${sign}${compact} ${currency}`;
}

function taxGraphEscapeHtml(input: string): string {
  return input
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;')
    .replaceAll("'", '&#39;');
}

// ---------------------------------------------------------------------------
// Market Data Price History
// ---------------------------------------------------------------------------

type PriceHistoryInterval = 'daily' | 'weekly' | 'monthly' | 'yearly';

type AssetPriceCache = {
  asset: AssetType;
  asset_id: AssetId;
  prices: Map<string, PricePoint>;
};

type FxCache = Map<string, Map<string, FxRatePoint>>;

type FailureCounter = { count: number };

type FxRateContext = {
  marketData: MarketDataService;
  store: JsonlMarketDataStore;
  fxCache: FxCache;
  stats: PriceHistoryStats;
  failures: PriceHistoryFailure[];
  failureCount: FailureCounter;
  failureLimit: number;
  lookbackDays: number;
};

const MS_PER_DAY = 24 * 60 * 60 * 1000;

const YMD_REGEX = /^(\d{4})-(\d{2})-(\d{2})$/;
const YM_REGEX = /^(\d{4})-(\d{2})$/;
const YEAR_REGEX = /^(\d{4})$/;
const RELATIVE_DATE_REGEX = /^([+-])(\d+)([dwmy])$/i;

export interface PriceHistoryOptions {
  account?: string;
  connection?: string;
  start?: string;
  end?: string;
  interval?: string;
  lookback_days?: number;
  request_delay_ms?: number;
  currency?: string;
  include_fx?: boolean;
}

function emptyPriceHistoryStats(): PriceHistoryStats {
  return { attempted: 0, existing: 0, fetched: 0, lookback: 0, missing: 0 };
}

function parseHistoryInterval(value: string): PriceHistoryInterval {
  switch (value.trim().toLowerCase()) {
    case 'daily':
      return 'daily';
    case 'weekly':
      return 'weekly';
    case 'monthly':
      return 'monthly';
    case 'yearly':
    case 'annual':
    case 'annually':
      return 'yearly';
    default:
      throw new Error(`Invalid interval: ${value}. Use: daily, weekly, monthly, yearly, annual`);
  }
}

function intervalAsString(interval: PriceHistoryInterval): string {
  return interval;
}

function parseYmdOrThrow(value: string, kind: 'start' | 'end'): string {
  const trimmed = value.trim();
  const match = YMD_REGEX.exec(trimmed);
  if (match === null) {
    throw new Error(`Invalid ${kind} date: ${value}`);
  }

  const year = Number.parseInt(match[1], 10);
  const month = Number.parseInt(match[2], 10);
  const day = Number.parseInt(match[3], 10);
  const parsed = new Date(Date.UTC(year, month - 1, day));
  if (
    parsed.getUTCFullYear() !== year ||
    parsed.getUTCMonth() !== month - 1 ||
    parsed.getUTCDate() !== day
  ) {
    throw new Error(`Invalid ${kind} date: ${value}`);
  }

  return formatDateYMD(parsed);
}

function formatYmdParts(year: number, month: number, day: number): string {
  return `${year.toString().padStart(4, '0')}-${month.toString().padStart(2, '0')}-${day
    .toString()
    .padStart(2, '0')}`;
}

function addMonthsClampedYmd(value: string, months: number): string {
  const parsed = parseYmdDate(value);
  const year = parsed.getUTCFullYear();
  const monthIndex = parsed.getUTCMonth();
  const targetMonthIndex = year * 12 + monthIndex + months;
  const targetYear = Math.floor(targetMonthIndex / 12);
  const targetMonth = targetMonthIndex - targetYear * 12 + 1;
  const targetDay = Math.min(parsed.getUTCDate(), daysInMonth(targetYear, targetMonth));
  return formatYmdParts(targetYear, targetMonth, targetDay);
}

function addRelativeDateYmd(value: string, amount: number, unit: string): string {
  switch (unit.toLowerCase()) {
    case 'd':
      return addDaysYmd(value, amount);
    case 'w':
      return addDaysYmd(value, amount * 7);
    case 'm':
      return addMonthsClampedYmd(value, amount);
    case 'y':
      return addMonthsClampedYmd(value, amount * 12);
    default:
      throw new Error(`Unsupported relative date unit: ${unit}`);
  }
}

function invalidDateRangeArg(value: string, kind: 'start' | 'end'): Error {
  return new Error(
    `Invalid ${kind} date: ${value}. Use YYYY-MM-DD, YYYY-MM, YYYY, today, or relative offsets like -1y, -3m, -2w, -10d`,
  );
}

function parseDateBoundOrThrow(value: string, kind: 'start' | 'end', today: string): string {
  const trimmed = value.trim();

  if (trimmed.toLowerCase() === 'today') {
    return today;
  }

  if (YMD_REGEX.test(trimmed)) {
    return parseYmdOrThrow(trimmed, kind);
  }

  const yearMatch = YEAR_REGEX.exec(trimmed);
  if (yearMatch !== null) {
    const year = Number.parseInt(yearMatch[1], 10);
    return kind === 'start' ? formatYmdParts(year, 1, 1) : formatYmdParts(year, 12, 31);
  }

  const monthMatch = YM_REGEX.exec(trimmed);
  if (monthMatch !== null) {
    const year = Number.parseInt(monthMatch[1], 10);
    const month = Number.parseInt(monthMatch[2], 10);
    if (month < 1 || month > 12) {
      throw invalidDateRangeArg(value, kind);
    }
    const day = kind === 'start' ? 1 : daysInMonth(year, month);
    return formatYmdParts(year, month, day);
  }

  const relativeMatch = RELATIVE_DATE_REGEX.exec(trimmed);
  if (relativeMatch !== null) {
    const sign = relativeMatch[1] === '-' ? -1 : 1;
    const amount = Number.parseInt(relativeMatch[2], 10) * sign;
    return addRelativeDateYmd(today, amount, relativeMatch[3]);
  }

  throw invalidDateRangeArg(value, kind);
}

function parseYmdDate(value: string): Date {
  const match = YMD_REGEX.exec(value);
  if (match === null) {
    throw new Error(`Invalid date value: ${value}`);
  }
  const year = Number.parseInt(match[1], 10);
  const month = Number.parseInt(match[2], 10);
  const day = Number.parseInt(match[3], 10);
  return new Date(Date.UTC(year, month - 1, day));
}

function compareYmd(a: string, b: string): number {
  return a.localeCompare(b);
}

function addDaysYmd(value: string, days: number): string {
  const parsed = parseYmdDate(value);
  parsed.setUTCDate(parsed.getUTCDate() + days);
  return formatDateYMD(parsed);
}

function daysInMonth(year: number, month: number): number {
  return new Date(Date.UTC(year, month, 0)).getUTCDate();
}

function monthEnd(date: string): string {
  const parsed = parseYmdDate(date);
  const year = parsed.getUTCFullYear();
  const month = parsed.getUTCMonth() + 1;
  const day = daysInMonth(year, month);
  return `${year.toString().padStart(4, '0')}-${month.toString().padStart(2, '0')}-${day
    .toString()
    .padStart(2, '0')}`;
}

function yearEnd(year: number): string {
  return `${year.toString().padStart(4, '0')}-12-31`;
}

function nextMonthEnd(date: string): string {
  const parsed = parseYmdDate(date);
  let year = parsed.getUTCFullYear();
  let month = parsed.getUTCMonth() + 1;
  if (month === 12) {
    year += 1;
    month = 1;
  } else {
    month += 1;
  }
  const day = daysInMonth(year, month);
  return `${year.toString().padStart(4, '0')}-${month.toString().padStart(2, '0')}-${day
    .toString()
    .padStart(2, '0')}`;
}

function nextYearEnd(date: string): string {
  const parsed = parseYmdDate(date);
  return yearEnd(parsed.getUTCFullYear() + 1);
}

function alignStartDate(date: string, interval: PriceHistoryInterval): string {
  switch (interval) {
    case 'monthly':
      return monthEnd(date);
    case 'yearly':
      return yearEnd(parseYmdDate(date).getUTCFullYear());
    default:
      return date;
  }
}

function advanceIntervalDate(date: string, interval: PriceHistoryInterval): string {
  switch (interval) {
    case 'daily':
      return addDaysYmd(date, 1);
    case 'weekly':
      return addDaysYmd(date, 7);
    case 'monthly':
      return nextMonthEnd(date);
    case 'yearly':
      return nextYearEnd(date);
  }
}

function daysInclusive(startDate: string, endDate: string): number {
  const start = parseYmdDate(startDate).getTime();
  const end = parseYmdDate(endDate).getTime();
  return Math.floor((end - start) / MS_PER_DAY) + 1;
}

function fxKey(base: string, quote: string): string {
  return `${base}|${quote}`;
}

function errorMessage(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

function upsertPriceCache(cache: Map<string, PricePoint>, point: PricePoint): void {
  const existing = cache.get(point.as_of_date);
  if (existing === undefined || existing.timestamp.getTime() < point.timestamp.getTime()) {
    cache.set(point.as_of_date, point);
  }
}

function upsertFxCache(cache: Map<string, FxRatePoint>, point: FxRatePoint): void {
  const existing = cache.get(point.as_of_date);
  if (existing === undefined || existing.timestamp.getTime() < point.timestamp.getTime()) {
    cache.set(point.as_of_date, point);
  }
}

function resolveCachedPrice(
  cache: Map<string, PricePoint>,
  date: string,
  lookbackDays: number,
): { point: PricePoint; exact: boolean } | null {
  const exact = cache.get(date);
  if (exact !== undefined) {
    return { point: exact, exact: true };
  }

  for (let offset = 1; offset <= lookbackDays; offset++) {
    const target = addDaysYmd(date, -offset);
    const point = cache.get(target);
    if (point !== undefined) {
      return { point, exact: false };
    }
  }

  return null;
}

function resolveCachedFx(
  cache: Map<string, FxRatePoint>,
  date: string,
  lookbackDays: number,
): { point: FxRatePoint; exact: boolean } | null {
  const exact = cache.get(date);
  if (exact !== undefined) {
    return { point: exact, exact: true };
  }

  for (let offset = 1; offset <= lookbackDays; offset++) {
    const target = addDaysYmd(date, -offset);
    const point = cache.get(target);
    if (point !== undefined) {
      return { point, exact: false };
    }
  }

  return null;
}

async function loadPriceCache(
  store: MarketDataStore,
  assetId: AssetId,
): Promise<Map<string, PricePoint>> {
  const all = await store.get_all_prices(assetId);
  const cache = new Map<string, PricePoint>();
  for (const point of all) {
    if (point.kind !== 'close') continue;
    upsertPriceCache(cache, point);
  }
  return cache;
}

async function loadFxCache(
  store: MarketDataStore,
  base: string,
  quote: string,
): Promise<Map<string, FxRatePoint>> {
  const all = await store.get_all_fx_rates(base, quote);
  const cache = new Map<string, FxRatePoint>();
  for (const point of all) {
    if (point.kind !== 'close') continue;
    upsertFxCache(cache, point);
  }
  return cache;
}

async function resolvePriceHistoryScope(
  storage: Storage,
  account: string | undefined,
  connection: string | undefined,
): Promise<{ scope: PriceHistoryScopeOutput; accounts: AccountType[] }> {
  if (account !== undefined && connection !== undefined) {
    throw new Error('Specify only one of --account or --connection');
  }

  if (account !== undefined) {
    const found = await findAccount(storage, account);
    if (found === null) {
      throw new Error(`Account not found: ${account}`);
    }
    return {
      scope: { type: 'account', id: found.id.asStr(), name: found.name },
      accounts: [found],
    };
  }

  if (connection !== undefined) {
    const found = await findConnection(storage, connection);
    if (found === null) {
      throw new Error(`Connection not found: ${connection}`);
    }

    const accounts: AccountType[] = [];
    const seenIds = new Set<string>();

    for (const accountId of found.state.account_ids) {
      const accountIdStr = accountId.asStr();
      if (seenIds.has(accountIdStr)) continue;
      seenIds.add(accountIdStr);

      if (!Id.isPathSafe(accountIdStr)) {
        continue;
      }

      const accountFromStorage = await storage.getAccount(accountId);
      if (accountFromStorage === null) continue;
      if (!accountFromStorage.connection_id.equals(found.state.id)) continue;
      accounts.push(accountFromStorage);
    }

    const allAccounts = await storage.listAccounts();
    for (const accountEntry of allAccounts) {
      if (!accountEntry.connection_id.equals(found.state.id)) continue;
      const accountIdStr = accountEntry.id.asStr();
      if (seenIds.has(accountIdStr)) continue;
      seenIds.add(accountIdStr);
      accounts.push(accountEntry);
    }

    if (accounts.length === 0) {
      throw new Error(`No accounts found for connection ${found.config.name}`);
    }

    return {
      scope: { type: 'connection', id: found.state.id.asStr(), name: found.config.name },
      accounts,
    };
  }

  const accounts = await storage.listAccounts();
  if (accounts.length === 0) {
    throw new Error('No accounts found');
  }
  return { scope: { type: 'portfolio' }, accounts };
}

async function ensureFxRate(
  ctx: FxRateContext,
  base: string,
  quote: string,
  date: string,
): Promise<void> {
  ctx.stats.attempted += 1;

  const baseUpper = base.toUpperCase();
  const quoteUpper = quote.toUpperCase();
  const pairKey = fxKey(baseUpper, quoteUpper);

  if (!ctx.fxCache.has(pairKey)) {
    ctx.fxCache.set(pairKey, await loadFxCache(ctx.store, baseUpper, quoteUpper));
  }

  const pairCache = ctx.fxCache.get(pairKey);
  if (pairCache === undefined) {
    return;
  }

  const cached = resolveCachedFx(pairCache, date, ctx.lookbackDays);
  if (cached !== null) {
    if (cached.exact) {
      ctx.stats.existing += 1;
    } else {
      ctx.stats.lookback += 1;
    }
    return;
  }

  try {
    const fetched = await ctx.marketData.fxClose(baseUpper, quoteUpper, date);
    if (fetched.as_of_date === date) {
      ctx.stats.fetched += 1;
    } else {
      ctx.stats.lookback += 1;
    }

    upsertFxCache(pairCache, fetched);
  } catch (err) {
    ctx.stats.missing += 1;
    ctx.failureCount.count += 1;
    if (ctx.failures.length < ctx.failureLimit) {
      ctx.failures.push({
        kind: 'fx',
        date,
        error: errorMessage(err),
        base: baseUpper,
        quote: quoteUpper,
      });
    }
  }
}

/**
 * Fetch historical prices for assets in scope.
 *
 * Mirrors Rust `app::fetch_historical_prices` output shape and semantics.
 */
export async function fetchHistoricalPrices(
  storage: Storage,
  config: ResolvedConfig,
  options: PriceHistoryOptions,
  clock?: Clock,
): Promise<PriceHistoryOutput> {
  const effectiveClock = clock ?? new SystemClock();

  const lookbackDaysRaw = options.lookback_days ?? 7;
  if (!Number.isFinite(lookbackDaysRaw) || lookbackDaysRaw < 0) {
    throw new Error('lookback_days must be a non-negative number');
  }
  const lookbackDays = Math.trunc(lookbackDaysRaw);

  const requestDelayMsRaw = options.request_delay_ms ?? 0;
  if (!Number.isFinite(requestDelayMsRaw) || requestDelayMsRaw < 0) {
    throw new Error('request_delay_ms must be a non-negative number');
  }
  const requestDelayMs = Math.trunc(requestDelayMsRaw);
  const includeFx = options.include_fx ?? true;

  const { scope, accounts } = await resolvePriceHistoryScope(
    storage,
    options.account,
    options.connection,
  );

  const assetsByHash = new Map<string, AssetType>();
  let earliestBalanceDate: string | undefined;

  for (const account of accounts) {
    const snapshots = await storage.getBalanceSnapshots(account.id);
    for (const snapshot of snapshots) {
      const date = formatDateYMD(snapshot.timestamp);
      if (earliestBalanceDate === undefined || compareYmd(date, earliestBalanceDate) < 0) {
        earliestBalanceDate = date;
      }
      for (const balance of snapshot.balances) {
        const normalizedAsset = Asset.normalized(balance.asset);
        assetsByHash.set(Asset.hash(normalizedAsset), normalizedAsset);
      }
    }
  }

  if (assetsByHash.size === 0) {
    throw new Error('No balances found for selected scope');
  }

  const startDate =
    options.start !== undefined
      ? parseYmdOrThrow(options.start, 'start')
      : (earliestBalanceDate ??
        (() => {
          throw new Error('No balances found to infer start date');
        })());
  const endDate =
    options.end !== undefined ? parseYmdOrThrow(options.end, 'end') : effectiveClock.today();

  if (compareYmd(startDate, endDate) > 0) {
    throw new Error('Start date must be on or before end date');
  }

  const interval = parseHistoryInterval(options.interval ?? 'monthly');
  const alignedStart = alignStartDate(startDate, interval);

  const targetCurrency = options.currency ?? config.reporting_currency;
  const targetCurrencyUpper = targetCurrency.toUpperCase();

  const store = new JsonlMarketDataStore(config.data_dir);
  const marketData = new MarketDataService(store).withLookbackDays(lookbackDays);

  const assetCaches: AssetPriceCache[] = [];
  for (const asset of assetsByHash.values()) {
    const assetId = AssetId.fromAsset(asset);
    assetCaches.push({
      asset,
      asset_id: assetId,
      prices: await loadPriceCache(store, assetId),
    });
  }

  assetCaches.sort((a, b) => a.asset_id.asStr().localeCompare(b.asset_id.asStr()));

  const fxCache: FxCache = new Map();
  if (includeFx) {
    for (const assetCache of assetCaches) {
      if (assetCache.asset.type !== 'currency') continue;
      const base = assetCache.asset.iso_code.toUpperCase();
      if (base === targetCurrencyUpper) continue;
      const key = fxKey(base, targetCurrencyUpper);
      if (!fxCache.has(key)) {
        fxCache.set(key, await loadFxCache(store, base, targetCurrencyUpper));
      }
    }
  }

  const prices = emptyPriceHistoryStats();
  const fx = emptyPriceHistoryStats();
  const failures: PriceHistoryFailure[] = [];
  const failureCount: FailureCounter = { count: 0 };
  const failureLimit = 50;
  const shouldDelayRequests = requestDelayMs > 0;

  const fxCtx: FxRateContext = {
    marketData,
    store,
    fxCache,
    stats: fx,
    failures,
    failureCount,
    failureLimit,
    lookbackDays,
  };

  let current = alignedStart;
  let points = 0;
  while (compareYmd(current, endDate) <= 0) {
    points += 1;

    for (const assetCache of assetCaches) {
      let shouldDelay = false;

      switch (assetCache.asset.type) {
        case 'currency': {
          if (includeFx) {
            const base = assetCache.asset.iso_code.toUpperCase();
            if (base !== targetCurrencyUpper) {
              await ensureFxRate(fxCtx, base, targetCurrencyUpper, current);
            }
          }
          break;
        }

        case 'equity':
        case 'crypto': {
          prices.attempted += 1;
          const cached = resolveCachedPrice(assetCache.prices, current, lookbackDays);
          if (cached !== null) {
            if (cached.exact) {
              prices.existing += 1;
            } else {
              prices.lookback += 1;
            }

            if (includeFx && cached.point.quote_currency.toUpperCase() !== targetCurrencyUpper) {
              await ensureFxRate(
                fxCtx,
                cached.point.quote_currency.toUpperCase(),
                targetCurrencyUpper,
                current,
              );
            }
            break;
          }

          try {
            const fetched = await marketData.priceClose(assetCache.asset, current);
            if (fetched.as_of_date === current) {
              prices.fetched += 1;
            } else {
              prices.lookback += 1;
            }
            upsertPriceCache(assetCache.prices, fetched);
            shouldDelay = shouldDelayRequests;

            if (includeFx && fetched.quote_currency.toUpperCase() !== targetCurrencyUpper) {
              await ensureFxRate(
                fxCtx,
                fetched.quote_currency.toUpperCase(),
                targetCurrencyUpper,
                current,
              );
            }
          } catch (err) {
            prices.missing += 1;
            failureCount.count += 1;
            if (failures.length < failureLimit) {
              failures.push({
                kind: 'price',
                date: current,
                error: errorMessage(err),
                asset_id: assetCache.asset_id.asStr(),
                asset: assetCache.asset,
              });
            }
            shouldDelay = shouldDelayRequests;
          }

          break;
        }
      }

      if (shouldDelay) {
        await new Promise<void>((resolve) => {
          setTimeout(resolve, requestDelayMs);
        });
      }
    }

    current = advanceIntervalDate(current, interval);
  }

  if (config.git.auto_commit) {
    try {
      await tryAutoCommit(config.data_dir, 'market data fetch', config.git.auto_push);
    } catch {
      // Keep command behavior best-effort for auto-commit.
    }
  }

  return {
    scope,
    currency: targetCurrency,
    interval: intervalAsString(interval),
    start_date: startDate,
    end_date: endDate,
    earliest_balance_date: earliestBalanceDate,
    days: daysInclusive(startDate, endDate),
    points,
    assets: assetCaches.map((cache) => ({ asset: cache.asset, asset_id: cache.asset_id.asStr() })),
    prices,
    fx: includeFx ? fx : undefined,
    failure_count: failureCount.count,
    failures: failures.length > 0 ? failures : undefined,
  };
}

// ---------------------------------------------------------------------------
// Portfolio History
// ---------------------------------------------------------------------------

export interface PortfolioHistoryOptions {
  currency?: string;
  start?: string;
  end?: string;
  granularity?: string;
  includePrices?: boolean;
}

/**
 * Format a ChangeTrigger into its string representation.
 *
 * - Balance trigger: `"balance:<account_id>:<json_asset>"` (compact JSON, no spaces)
 * - Price trigger: `"price:<asset_id_string>"`
 * - FxRate trigger: `"fx:<base>/<quote>"`
 */
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

interface HistoryCostBasisBackfill {
  unitCostBasis: Decimal;
}

interface HistoryValuation {
  totalValue: Decimal;
  prospectiveCapitalGainsTax: Decimal | undefined;
}

async function collectHistoryCostBasisBackfill(
  storage: Storage,
): Promise<Map<string, HistoryCostBasisBackfill>> {
  const totals = new Map<string, { amount: Decimal; costBasis: Decimal }>();

  for (const [accountId, snapshot] of await storage.getLatestBalances()) {
    if (storage.getAccountConfig(accountId)?.exclude_from_portfolio === true) {
      continue;
    }

    for (const balance of snapshot.balances) {
      if (balance.cost_basis === undefined) continue;

      const amount = new Decimal(balance.amount);
      if (amount.isZero()) continue;

      const assetId = AssetId.fromAsset(balance.asset).asStr();
      const existing = totals.get(assetId) ?? {
        amount: new Decimal(0),
        costBasis: new Decimal(0),
      };
      existing.amount = existing.amount.plus(amount);
      existing.costBasis = existing.costBasis.plus(new Decimal(balance.cost_basis));
      totals.set(assetId, existing);
    }
  }

  const backfill = new Map<string, HistoryCostBasisBackfill>();
  for (const [assetId, total] of totals) {
    if (!total.amount.isZero()) {
      backfill.set(assetId, { unitCostBasis: total.costBasis.div(total.amount) });
    }
  }
  return backfill;
}

function addOptionalDecimal(total: Decimal | undefined, value: Decimal): Decimal {
  return (total ?? new Decimal(0)).plus(value);
}

function computeHistoryValuationWithCarryForward(
  byAsset: AssetSummary[],
  carryForwardUnitValues: Map<string, Decimal>,
  costBasisBackfill: Map<string, HistoryCostBasisBackfill>,
  capitalGainsTaxRate: Decimal | undefined,
): HistoryValuation | undefined {
  try {
    let totalValue = new Decimal(0);
    let prospectiveCapitalGainsTax: Decimal | undefined;

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

      if (summary.prospective_capital_gains_tax !== undefined) {
        const tax = new Decimal(summary.prospective_capital_gains_tax);
        if (tax.gt(0)) {
          prospectiveCapitalGainsTax = addOptionalDecimal(prospectiveCapitalGainsTax, tax);
        }
        continue;
      }

      if (capitalGainsTaxRate === undefined) continue;

      const costBasis =
        summary.cost_basis !== undefined
          ? new Decimal(summary.cost_basis)
          : costBasisBackfill.get(assetId)?.unitCostBasis.times(totalAmount);
      if (costBasis === undefined) continue;

      const gain = assetValue.minus(costBasis);
      if (gain.gt(0)) {
        prospectiveCapitalGainsTax = addOptionalDecimal(
          prospectiveCapitalGainsTax,
          gain.times(capitalGainsTaxRate),
        );
      }
    }

    return { totalValue, prospectiveCapitalGainsTax };
  } catch {
    return undefined;
  }
}

function applyLatentTaxToHistoryTotalValue(
  totalValue: string,
  prospectiveCapitalGainsTax: Decimal | undefined,
  snapshot: PortfolioSnapshot,
  config: ResolvedConfig,
  includeLatentTaxAdjustment: boolean,
): string {
  if (!includeLatentTaxAdjustment) {
    return totalValue;
  }

  const tax =
    prospectiveCapitalGainsTax ??
    (snapshot.prospective_capital_gains_tax === undefined
      ? undefined
      : new Decimal(snapshot.prospective_capital_gains_tax));
  if (tax === undefined) return totalValue;
  if (tax.lte(0)) return totalValue;

  return decStrRounded(new Decimal(totalValue).minus(tax), config.display.currency_decimals);
}

/**
 * Execute the portfolio history command.
 *
 * Collects change points from storage and market data, calculates portfolio
 * value at each point, and returns the history with optional summary statistics.
 */
export async function portfolioHistory(
  storage: Storage,
  marketDataStore: MarketDataStore,
  config: ResolvedConfig,
  options: PortfolioHistoryOptions,
  clock?: Clock,
): Promise<HistoryOutput> {
  const effectiveClock = clock ?? new SystemClock();
  const currency = options.currency ?? config.reporting_currency;
  const resolvedGranularity = options.granularity ?? config.history.portfolio_granularity;
  const granularity = parseGranularity(resolvedGranularity);
  const today = effectiveClock.today();
  const startDate =
    options.start !== undefined ? parseDateBoundOrThrow(options.start, 'start', today) : undefined;
  const endDate =
    options.end !== undefined ? parseDateBoundOrThrow(options.end, 'end', today) : undefined;

  const marketDataService = new MarketDataService(marketDataStore);
  if (config.history.lookback_days !== undefined) {
    marketDataService.withLookbackDays(config.history.lookback_days);
  }
  marketDataService.withFutureProjection(config.history.allow_future_projection);

  // Collect change points
  const allPoints = await collectChangePoints(storage, marketDataStore, {
    includePrices: options.includePrices ?? config.history.include_prices,
  });

  // Filter by date range
  const dateFiltered = filterByDateRange(allPoints, startDate, endDate);

  // Filter by granularity
  const points = filterByGranularity(dateFiltered, granularity, 'last');

  // Calculate portfolio value at each change point
  const historyPoints: HistoryPoint[] = [];
  let previousTotalValue: Decimal | undefined;
  const carryForwardUnitValues = new Map<string, Decimal>();
  const costBasisBackfill = await collectHistoryCostBasisBackfill(storage);
  const { rate: capitalGainsTaxRate, includeLatentTaxVirtualAccount: includeLatentTaxAdjustment } =
    resolveCapitalGainsTaxRate(config, undefined);
  for (const point of points) {
    const portfolioService = new PortfolioService(storage, marketDataService, effectiveClock);

    const snapshot = await portfolioService.calculate({
      as_of_date: formatDateYMD(point.timestamp),
      currency,
      currency_decimals: config.display.currency_decimals,
      grouping: 'both',
      include_detail: false,
      capital_gains_tax_rate: capitalGainsTaxRate,
    });

    const historyValuation =
      snapshot.by_asset !== undefined
        ? computeHistoryValuationWithCarryForward(
            snapshot.by_asset,
            carryForwardUnitValues,
            costBasisBackfill,
            capitalGainsTaxRate,
          )
        : undefined;
    const baseTotalValue =
      historyValuation !== undefined
        ? decStrRounded(historyValuation.totalValue, config.display.currency_decimals)
        : snapshot.total_value;
    const totalValue = applyLatentTaxToHistoryTotalValue(
      baseTotalValue,
      historyValuation?.prospectiveCapitalGainsTax,
      snapshot,
      config,
      includeLatentTaxAdjustment,
    );
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
    start_date: startDate ?? null,
    end_date: endDate ?? null,
    granularity: resolvedGranularity,
    points: historyPoints,
    summary,
  };
}

// ---------------------------------------------------------------------------
// Portfolio Change Points
// ---------------------------------------------------------------------------

/**
 * Serialize a `ChangeTrigger` to its JSON-serializable form.
 *
 * - Balance: `{type: 'balance', account_id: "<id>", asset: <asset>}`
 * - Price: `{type: 'price', asset_id: "<asset_id_string>"}`
 * - FxRate: `{type: 'fx_rate', base: "<base>", quote: "<quote>"}`
 */
export function serializeChangeTrigger(trigger: ChangeTrigger): SerializedChangeTrigger {
  switch (trigger.type) {
    case 'balance':
      return { type: 'balance', account_id: trigger.account_id.asStr(), asset: trigger.asset };
    case 'price':
      return { type: 'price', asset_id: trigger.asset_id.asStr() };
    case 'fx_rate':
      return { type: 'fx_rate', base: trigger.base, quote: trigger.quote };
  }
}

/**
 * Serialize a `ChangePoint` to its JSON-serializable form.
 *
 * Uses `formatChronoSerde` (Z suffix) because Rust's `ChangePoint.timestamp`
 * is serialized via chrono serde derive, not manual `to_rfc3339`.
 */
export function serializeChangePoint(point: ChangePoint): SerializedChangePoint {
  return {
    timestamp:
      point.timestamp_nanos !== undefined
        ? formatChronoSerdeFromEpochNanos(point.timestamp_nanos)
        : formatChronoSerde(point.timestamp),
    triggers: point.triggers.map(serializeChangeTrigger),
  };
}

export interface PortfolioChangePointsOptions {
  start?: string;
  end?: string;
  granularity?: string;
  includePrices?: boolean;
}

/**
 * Execute the portfolio change-points command.
 *
 * Collects change points from storage and market data, filters by date range
 * and granularity, serializes each point, and returns the output.
 */
export async function portfolioChangePoints(
  storage: Storage,
  marketDataStore: MarketDataStore,
  config: ResolvedConfig,
  options: PortfolioChangePointsOptions,
  clock?: Clock,
): Promise<ChangePointsOutput> {
  const effectiveClock = clock ?? new SystemClock();
  const resolvedGranularity = options.granularity ?? config.history.change_points_granularity;
  const granularity = parseGranularity(resolvedGranularity);
  const today = effectiveClock.today();
  const startDate =
    options.start !== undefined ? parseDateBoundOrThrow(options.start, 'start', today) : undefined;
  const endDate =
    options.end !== undefined ? parseDateBoundOrThrow(options.end, 'end', today) : undefined;

  // Collect change points
  const allPoints = await collectChangePoints(storage, marketDataStore, {
    includePrices: options.includePrices ?? config.history.include_prices,
  });

  // Filter by date range
  const dateFiltered = filterByDateRange(allPoints, startDate, endDate);

  // Filter by granularity
  const points = filterByGranularity(dateFiltered, granularity, 'last');

  // Serialize each change point
  const serialized = points.map(serializeChangePoint);

  return {
    start_date: startDate ?? null,
    end_date: endDate ?? null,
    granularity: resolvedGranularity,
    include_prices: options.includePrices ?? config.history.include_prices,
    points: serialized,
  };
}
