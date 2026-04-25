import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import toml from 'toml';

import type { ResolvedConfig } from '../config.js';
import type { MarketDataStore } from '../market-data/store.js';
import type { Storage } from '../storage/storage.js';
import { portfolioHistory, type PortfolioHistoryOptions } from './portfolio.js';
import type { HistoryOutput, HistoryPoint } from './types.js';

const DEFAULT_WIDTH = 1400;
const DEFAULT_HEIGHT = 900;

type GraphConfigFile = {
  start?: string;
  end?: string;
  currency?: string;
  granularity?: string;
  include_prices?: boolean;
  output?: string;
  svg_output?: string;
  title?: string;
  subtitle?: string;
  width?: number;
  height?: number;
  min_value?: number;
  max_value?: number;
};

export type PortfolioGraphOptions = {
  graphConfig?: string;
  start?: string;
  end?: string;
  currency?: string;
  granularity?: string;
  includePrices?: boolean;
  output?: string;
  svgOutput?: string;
  title?: string;
  subtitle?: string;
  width?: number;
  height?: number;
  minValue?: number;
  maxValue?: number;
};

export type ResolvedGraphOptions = {
  start?: string;
  end?: string;
  currency?: string;
  granularity: string;
  includePrices: boolean;
  output: string;
  svgOutput: string;
  title: string;
  subtitle?: string;
  width: number;
  height: number;
  minValue?: number;
  maxValue?: number;
};

export interface PortfolioGraphOutput {
  html_path: string;
  svg_path: string;
  currency: string;
  start_date: string | null;
  end_date: string | null;
  granularity: string;
  point_count: number;
}

export async function portfolioGraph(
  storage: Storage,
  marketDataStore: MarketDataStore,
  config: ResolvedConfig,
  options: PortfolioGraphOptions,
): Promise<PortfolioGraphOutput> {
  const resolved = await resolveGraphOptions(options);
  const historyOptions: PortfolioHistoryOptions = {
    currency: resolved.currency,
    start: resolved.start,
    end: resolved.end,
    granularity: resolved.granularity,
    includePrices: resolved.includePrices,
  };
  const history = await portfolioHistory(storage, marketDataStore, config, historyOptions);

  const svg = renderNetWorthSvg(history, resolved);
  const html = renderGraphHtml(resolved, history);

  await mkdir(path.dirname(resolved.svgOutput), { recursive: true });
  await mkdir(path.dirname(resolved.output), { recursive: true });
  await writeFile(resolved.svgOutput, svg);
  await writeFile(resolved.output, html);

  return {
    html_path: resolved.output,
    svg_path: resolved.svgOutput,
    currency: history.currency,
    start_date: history.start_date,
    end_date: history.end_date,
    granularity: history.granularity,
    point_count: history.points.length,
  };
}

async function resolveGraphOptions(options: PortfolioGraphOptions): Promise<ResolvedGraphOptions> {
  const fileOptions = options.graphConfig ? await readGraphConfig(options.graphConfig) : {};

  const start = options.start ?? fileOptions.start;
  const end = options.end ?? fileOptions.end;
  const currency = options.currency ?? fileOptions.currency;
  const granularity = options.granularity ?? fileOptions.granularity ?? 'daily';
  const includePrices = options.includePrices ?? fileOptions.include_prices ?? true;
  const title = options.title ?? fileOptions.title ?? 'Keepbook Net Worth';
  const subtitle = options.subtitle ?? fileOptions.subtitle;
  const width = options.width ?? fileOptions.width ?? DEFAULT_WIDTH;
  const height = options.height ?? fileOptions.height ?? DEFAULT_HEIGHT;
  const minValue = options.minValue ?? fileOptions.min_value;
  const maxValue = options.maxValue ?? fileOptions.max_value;

  if (width < 360 || height < 240) {
    throw new Error('Graph width and height must be at least 360x240');
  }
  if (minValue !== undefined && maxValue !== undefined && minValue >= maxValue) {
    throw new Error('Graph min-value must be less than max-value');
  }

  const output = options.output ?? fileOptions.output ?? defaultGraphOutputPath(start, end);
  const svgOutput =
    options.svgOutput ??
    fileOptions.svg_output ??
    path.join(path.dirname(output), `${path.basename(output, path.extname(output))}.svg`);

  return {
    start,
    end,
    currency,
    granularity,
    includePrices,
    output,
    svgOutput,
    title,
    subtitle,
    width,
    height,
    minValue,
    maxValue,
  };
}

async function readGraphConfig(filePath: string): Promise<GraphConfigFile> {
  const raw = await readFile(filePath, 'utf8');
  const parsed = toml.parse(raw) as Record<string, unknown>;
  const result: GraphConfigFile = {};

  for (const key of [
    'start',
    'end',
    'currency',
    'granularity',
    'output',
    'svg_output',
    'title',
    'subtitle',
  ]) {
    const value = parsed[key];
    if (typeof value === 'string' && value.trim().length > 0) {
      result[key as keyof GraphConfigFile] = value.trim() as never;
    }
  }

  if (typeof parsed.include_prices === 'boolean') result.include_prices = parsed.include_prices;
  if (isFiniteNumber(parsed.width)) result.width = parsed.width;
  if (isFiniteNumber(parsed.height)) result.height = parsed.height;
  if (isFiniteNumber(parsed.min_value)) result.min_value = parsed.min_value;
  if (isFiniteNumber(parsed.max_value)) result.max_value = parsed.max_value;

  return result;
}

function isFiniteNumber(value: unknown): value is number {
  return typeof value === 'number' && Number.isFinite(value);
}

function defaultGraphOutputPath(start?: string, end?: string): string {
  let name: string;
  if (start !== undefined && end !== undefined) {
    name = `net-worth-${start}-to-${end}.html`;
  } else if (start !== undefined) {
    name = `net-worth-since-${start}.html`;
  } else if (end !== undefined) {
    name = `net-worth-through-${end}.html`;
  } else {
    name = 'net-worth.html';
  }
  return path.join('artifacts', name);
}

function renderGraphHtml(options: ResolvedGraphOptions, history: HistoryOutput): string {
  const imgSrc = imageSrcForHtml(options.output, options.svgOutput);
  const alt = `${options.title} graph with ${history.points.length} points`;
  return `<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>${escapeHtml(options.title)}</title>
    <style>
      body { margin: 0; background: #edf2f7; display: grid; place-items: center; min-height: 100vh; }
      img { width: min(96vw, ${options.width}px); height: auto; box-shadow: 0 18px 48px rgba(16,42,67,.18); border-radius: 20px; }
    </style>
  </head>
  <body>
    <img src="${escapeHtml(imgSrc)}" alt="${escapeHtml(alt)}" />
  </body>
</html>
`;
}

function imageSrcForHtml(htmlPath: string, svgPath: string): string {
  if (path.dirname(htmlPath) === path.dirname(svgPath)) {
    return `./${path.basename(svgPath)}`;
  }
  return svgPath;
}

export function renderNetWorthSvg(history: HistoryOutput, options: ResolvedGraphOptions): string {
  const width = options.width;
  const height = options.height;
  const marginLeft = 120;
  const marginRight = 56;
  const marginTop = 104;
  const marginBottom = 96;
  const plotX = marginLeft;
  const plotY = marginTop;
  const plotW = width - marginLeft - marginRight;
  const plotH = height - marginTop - marginBottom;

  const values = history.points.map((point) => parseHistoryValue(point));
  const dates = history.points.map((point) => parseDate(point.date));

  let yMin: number;
  let yMax: number;
  if (values.length === 0) {
    yMin = 0;
    yMax = 1;
  } else {
    const min = Math.min(...values);
    const max = Math.max(...values);
    const span = Math.abs(max - min);
    const pad = span === 0 ? Math.max(Math.abs(max), 1) * 0.05 : span * 0.08;
    yMin = min - pad;
    yMax = max + pad;
  }
  if (options.minValue !== undefined) yMin = options.minValue;
  if (options.maxValue !== undefined) yMax = options.maxValue;
  if (yMin >= yMax) {
    throw new Error('Graph value range is invalid after applying min/max bounds');
  }

  const minDay = dates[0] !== undefined ? dateDays(dates[0]) : 0;
  const maxDay = dates[dates.length - 1] !== undefined ? dateDays(dates[dates.length - 1]) : minDay + 1;
  const daySpan = Math.max(maxDay - minDay, 1);

  const points = values.map((value, index): [number, number] => {
    const x =
      values.length === 1
        ? plotX + plotW / 2
        : plotX + ((dateDays(dates[index]) - minDay) / daySpan) * plotW;
    const y = plotY + ((yMax - value) / (yMax - yMin)) * plotH;
    return [x, y];
  });

  const linePath = pathFromPoints(points);
  const areaPath = areaPathFromPoints(points, plotY + plotH);
  const startLabel = history.points[0]?.date ?? 'no data';
  const endLabel = history.points[history.points.length - 1]?.date ?? 'no data';
  const subtitle = options.subtitle ?? `${startLabel} to ${endLabel} - ${history.currency}`;

  let svg = `<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}" viewBox="0 0 ${width} ${height}" role="img" aria-labelledby="title desc">
  <title id="title">${escapeHtml(options.title)}</title>
  <desc id="desc">Net worth graph from ${escapeHtml(startLabel)} to ${escapeHtml(endLabel)} in ${escapeHtml(history.currency)}</desc>
  <rect width="100%" height="100%" fill="#f8fafc"/>
  <text x="${marginLeft}" y="54" font-size="34" fill="#102a43" font-family="ui-sans-serif, system-ui, sans-serif">${escapeHtml(options.title)}</text>
  <text x="${marginLeft}" y="84" font-size="18" fill="#627d98" font-family="ui-sans-serif, system-ui, sans-serif">${escapeHtml(subtitle)}</text>
  <rect x="${plotX}" y="${plotY}" width="${plotW}" height="${plotH}" fill="#ffffff" stroke="#d9e2ec" rx="8"/>
`;

  for (let i = 0; i <= 5; i += 1) {
    const ratio = i / 5;
    const y = plotY + ratio * plotH;
    const value = yMax - ratio * (yMax - yMin);
    svg += `  <line x1="${plotX}" y1="${y.toFixed(2)}" x2="${plotX + plotW}" y2="${y.toFixed(2)}" stroke="#eef2f7"/>
  <text x="${plotX - 14}" y="${(y + 5).toFixed(2)}" font-size="14" text-anchor="end" fill="#627d98" font-family="ui-sans-serif, system-ui, sans-serif">${escapeHtml(formatCurrencyTick(value, history.currency))}</text>
`;
  }

  for (const [date, x] of xTicks(dates, plotX, plotW)) {
    const label = formatDate(date);
    svg += `  <line x1="${x.toFixed(2)}" y1="${plotY}" x2="${x.toFixed(2)}" y2="${plotY + plotH + 8}" stroke="#e5eaf1"/>
  <text x="${x.toFixed(2)}" y="${plotY + plotH + 34}" font-size="14" text-anchor="middle" fill="#627d98" font-family="ui-sans-serif, system-ui, sans-serif">${escapeHtml(label)}</text>
`;
  }

  if (areaPath.length > 0) {
    svg += `  <path d="${areaPath}" fill="#2f80ed" opacity="0.14"/>
  <path d="${linePath}" fill="none" stroke="#1c7ed6" stroke-width="4" stroke-linejoin="round" stroke-linecap="round"/>
`;
  } else {
    svg += `  <text x="${plotX + plotW / 2}" y="${plotY + plotH / 2}" font-size="24" text-anchor="middle" fill="#627d98" font-family="ui-sans-serif, system-ui, sans-serif">No history points</text>
`;
  }

  const lastPoint = points[points.length - 1];
  const lastValue = values[values.length - 1];
  if (lastPoint !== undefined && lastValue !== undefined) {
    svg += `  <circle cx="${lastPoint[0].toFixed(2)}" cy="${lastPoint[1].toFixed(2)}" r="6" fill="#0b7285" stroke="#ffffff" stroke-width="3"/>
  <text x="${(lastPoint[0] + 12).toFixed(2)}" y="${(lastPoint[1] - 12).toFixed(2)}" font-size="18" fill="#102a43" font-family="ui-sans-serif, system-ui, sans-serif">${escapeHtml(formatCurrencyTick(lastValue, history.currency))}</text>
`;
  }

  svg += `  <text x="${marginLeft}" y="${height - 28}" font-size="16" fill="#829ab1" font-family="ui-sans-serif, system-ui, sans-serif">Source: keepbook portfolio graph --start ${history.start_date ?? 'earliest'} --end ${history.end_date ?? 'today'} --granularity ${escapeHtml(history.granularity)}</text>
</svg>
`;

  return svg;
}

function parseHistoryValue(point: HistoryPoint): number {
  const value = Number.parseFloat(point.total_value);
  if (!Number.isFinite(value)) {
    throw new Error(`Invalid history value ${point.total_value}`);
  }
  return value;
}

function parseDate(value: string): Date {
  const date = new Date(`${value}T00:00:00.000Z`);
  if (Number.isNaN(date.getTime())) {
    throw new Error(`Invalid history date ${value}`);
  }
  return date;
}

function dateDays(date: Date): number {
  return Math.floor(date.getTime() / 86_400_000);
}

function formatDate(date: Date): string {
  return date.toISOString().slice(0, 10);
}

function pathFromPoints(points: Array<[number, number]>): string {
  return points
    .map(([x, y], index) => `${index === 0 ? 'M' : 'L'} ${x.toFixed(2)} ${y.toFixed(2)}`)
    .join(' ');
}

function areaPathFromPoints(points: Array<[number, number]>, baselineY: number): string {
  const first = points[0];
  const last = points[points.length - 1];
  if (first === undefined || last === undefined) return '';
  return `${pathFromPoints(points)} L ${last[0].toFixed(2)} ${baselineY.toFixed(2)} L ${first[0].toFixed(2)} ${baselineY.toFixed(2)} Z`;
}

function xTicks(dates: Date[], plotX: number, plotW: number): Array<[Date, number]> {
  if (dates.length === 0) return [];
  const count = Math.min(dates.length, 6);
  if (count === 1) return [[dates[0], plotX + plotW / 2]];

  const ticks: Array<[Date, number]> = [];
  for (let i = 0; i < count; i += 1) {
    const index = Math.round((i * (dates.length - 1)) / (count - 1));
    const date = dates[index];
    const x = plotX + (i / (count - 1)) * plotW;
    if (date !== undefined && ticks[ticks.length - 1]?.[0].getTime() !== date.getTime()) {
      ticks.push([date, x]);
    }
  }
  return ticks;
}

function formatCurrencyTick(value: number, currency: string): string {
  const sign = value < 0 ? '-' : '';
  const abs = Math.abs(value);
  let compact: string;
  if (abs >= 1_000_000_000) {
    compact = `${(abs / 1_000_000_000).toFixed(1)}B`;
  } else if (abs >= 1_000_000) {
    compact = `${(abs / 1_000_000).toFixed(1)}M`;
  } else if (abs >= 1_000) {
    compact = `${(abs / 1_000).toFixed(1)}K`;
  } else {
    compact = abs.toFixed(0);
  }
  return `${sign}${compact} ${currency}`;
}

function escapeHtml(input: string): string {
  return input
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;')
    .replaceAll("'", '&#39;');
}
