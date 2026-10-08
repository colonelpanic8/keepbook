import { cx } from "../cx";
import { compactMoney, money } from "./money";

export interface NetWorthPoint {
  /** ISO date, `YYYY-MM-DD`. */
  date: string;
  value: number;
}

export interface NetWorthChartProps {
  /** History points in date order, from the app's portfolio history output. */
  points: NetWorthPoint[];
  /** Reporting currency; USD shows as `$`, others as a code prefix. */
  currency?: string;
  /** App-formatted change across the range, e.g. "+$1,204.10 (21.5%)". */
  change?: string;
  /** Sign of the change; colors the change text. */
  changeTone?: "positive" | "negative";
  /** Index of a point to show as hovered, with its tooltip. */
  hoverIndex?: number;
}

const WIDTH = 720;
const HEIGHT = 260;
const PAD = { left: 68, right: 20, top: 18, bottom: 38 };

/**
 * The net worth line chart with current value, range change, and hover tooltip.
 *
 * Mirrors `NetWorthChart` in `views/charts/net_worth.rs`, including its
 * geometry and compact axis labels. Pass values from the portfolio history
 * output; never compute totals in the UI.
 */
export function NetWorthChart({ points, currency = "USD", change, changeTone, hoverIndex }: NetWorthChartProps) {
  if (points.length === 0) {
    return (
      <div className="chart-empty">
        <strong>No net worth history</strong>
        <small>Refresh balances to populate the graph.</small>
      </div>
    );
  }
  const plotWidth = WIDTH - PAD.left - PAD.right;
  const plotHeight = HEIGHT - PAD.top - PAD.bottom;
  const values = points.map((p) => p.value);
  const min = Math.min(...values);
  const max = Math.max(...values);
  const range = Math.abs(max - min);
  const padding = range === 0 ? Math.max(Math.abs(max) * 0.05, 1) : range * 0.08;
  const yMin = min - padding;
  const yMax = max + padding;
  const yRange = Math.max(yMax - yMin, 1);
  const plotted = points.map((p, index) => ({
    ...p,
    x: points.length <= 1 ? PAD.left + plotWidth / 2 : PAD.left + (index / (points.length - 1)) * plotWidth,
    y: PAD.top + ((yMax - p.value) / yRange) * plotHeight,
  }));
  const line = plotted.map((p, i) => `${i === 0 ? "M" : "L"} ${p.x.toFixed(2)} ${p.y.toFixed(2)}`).join(" ");
  const first = plotted[0];
  const last = plotted[plotted.length - 1];
  const area = `${line} L ${last.x.toFixed(2)} ${PAD.top + plotHeight} L ${first.x.toFixed(2)} ${PAD.top + plotHeight} Z`;
  const hover = hoverIndex != null ? plotted[hoverIndex] : undefined;
  const tip = hover && {
    x: Math.max(hover.x + 196 > WIDTH - PAD.right ? hover.x - 196 : hover.x + 12, 8),
    y: Math.max(hover.y - 60 < PAD.top ? hover.y + 12 : hover.y - 60, 8),
  };
  return (
    <div className="chart-card">
      <div className="chart-meta">
        <div>
          <span className="metric-label">Current</span>
          <strong>{compactMoney(last.value, currency)}</strong>
        </div>
        {change && (
          <div>
            <span className="metric-label">Range change</span>
            <strong className={cx(changeTone === "positive" && "change-positive", changeTone === "negative" && "change-negative")}>
              {change}
            </strong>
          </div>
        )}
      </div>
      <svg className="net-worth-chart" viewBox={`0 0 ${WIDTH} ${HEIGHT}`} role="img" aria-label="Net worth over time">
        <line className="chart-grid" x1={PAD.left} x2={WIDTH - PAD.right} y1={PAD.top} y2={PAD.top} />
        <line className="chart-grid" x1={PAD.left} x2={WIDTH - PAD.right} y1={PAD.top + plotHeight / 2} y2={PAD.top + plotHeight / 2} />
        <line className="chart-grid axis" x1={PAD.left} x2={WIDTH - PAD.right} y1={PAD.top + plotHeight} y2={PAD.top + plotHeight} />
        <text className="chart-axis-label" x={8} y={PAD.top + 4}>
          {compactMoney(yMax, currency)}
        </text>
        <text className="chart-axis-label" x={8} y={PAD.top + plotHeight / 2 + 4}>
          {compactMoney(yMin + yRange / 2, currency)}
        </text>
        <text className="chart-axis-label" x={8} y={PAD.top + plotHeight + 4}>
          {compactMoney(yMin, currency)}
        </text>
        <text className="chart-axis-label date-label" x={PAD.left} y={HEIGHT - 10}>
          {first.date}
        </text>
        <text className="chart-axis-label date-label end" x={WIDTH - PAD.right} y={HEIGHT - 10}>
          {last.date}
        </text>
        {plotted.length > 1 && (
          <>
            <path className="chart-area" d={area} />
            <path className="chart-line" d={line} />
          </>
        )}
        {plotted.map((p) => (
          <circle key={p.date} className="chart-point" cx={p.x} cy={p.y} r={3.4} />
        ))}
        {hover && tip && (
          <g>
            <line className="chart-hover-line" x1={hover.x} x2={hover.x} y1={PAD.top} y2={hover.y} />
            <circle className="chart-hover-point" cx={hover.x} cy={hover.y} r={6} />
            <rect className="chart-tooltip" x={tip.x} y={tip.y} width={184} height={50} rx={6} />
            <text className="chart-tooltip-date" x={tip.x + 10} y={tip.y + 20}>
              {hover.date}
            </text>
            <text className="chart-tooltip-value" x={tip.x + 10} y={tip.y + 38}>
              {money(hover.value, currency, 2)}
            </text>
          </g>
        )}
      </svg>
    </div>
  );
}
