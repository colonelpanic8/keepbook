import { EmptyState } from "../feedback/EmptyState";
import { compactMoney, money } from "./money";

export interface NetWorthPoint {
  /** ISO date, `YYYY-MM-DD`. */
  date: string;
  value: number;
}

export interface NetWorthChartProps {
  /** History points in date order, from the app's portfolio history output. */
  data: NetWorthPoint[];
  /** Reporting currency; USD shows as `$`, others as a code prefix. */
  currency?: string;
  /** Fixed `[min, max]` value range; padded from the data when omitted. */
  yDomain?: [number, number];
  emptyTitle?: string;
  emptyDetail?: string;
  /** App decimal text for the latest value, e.g. "6814.24". */
  currentValueText?: string;
  /** App-formatted change across the range, e.g. "+$1,204.10 (21.5%)". */
  changeText?: string;
  /** Index of a point to show as hovered, with its tooltip (designs only). */
  hoverIndex?: number;
}

const WIDTH = 720;
const HEIGHT = 260;
const PAD = { left: 68, right: 20, top: 18, bottom: 38 };

/**
 * The net worth line chart: current value and range change above an SVG line with a tinted area.
 *
 * Hovering a point shows its date and value. Pass values from the portfolio
 * history output; never compute totals in the UI. Mirrors
 * `components/charts/net_worth_chart.rs`, geometry included.
 */
export function NetWorthChart({
  data,
  currency = "USD",
  yDomain,
  emptyTitle = "No net worth history",
  emptyDetail = "Refresh balances to populate the chart.",
  currentValueText,
  changeText = "",
  hoverIndex,
}: NetWorthChartProps) {
  if (data.length === 0) return <EmptyState title={emptyTitle} detail={emptyDetail} />;
  const plotWidth = WIDTH - PAD.left - PAD.right;
  const plotHeight = HEIGHT - PAD.top - PAD.bottom;
  const values = data.map((p) => p.value);
  const [yMin, yMax] =
    yDomain ??
    (() => {
      const min = Math.min(...values);
      const max = Math.max(...values);
      const range = Math.abs(max - min);
      const padding = range === 0 ? Math.max(Math.abs(max) * 0.05, 1) : range * 0.08;
      return [min - padding, max + padding];
    })();
  const yRange = Math.max(yMax - yMin, 1);
  const points = data.map((p, index) => ({
    ...p,
    index,
    x: data.length <= 1 ? PAD.left + plotWidth / 2 : PAD.left + (index / (data.length - 1)) * plotWidth,
    y: PAD.top + ((yMax - p.value) / yRange) * plotHeight,
  }));
  const line = points.map((p, i) => `${i === 0 ? "M" : "L"} ${p.x.toFixed(2)} ${p.y.toFixed(2)}`).join(" ");
  const first = points[0];
  const last = points[points.length - 1];
  const bottom = (PAD.top + plotHeight).toFixed(2);
  const area = `${line} L ${last.x.toFixed(2)} ${bottom} L ${first.x.toFixed(2)} ${bottom} Z`;
  const hits = points.map((p, i) => {
    const previous = i === 0 ? PAD.left : (points[i - 1].x + p.x) / 2;
    const next = i + 1 === points.length ? WIDTH - PAD.right : (p.x + points[i + 1].x) / 2;
    return { x: previous, width: Math.max(next - previous, 1) };
  });
  const rules = points
    .map((p) => `.chart-hit-zone-${p.index}:hover ~ .chart-hover-detail-${p.index} { display: block; }`)
    .concat(hoverIndex != null ? [`.chart-hover-detail-${hoverIndex} { display: block; }`] : [])
    .join("\n");
  const current = currentValueText != null ? compactMoney(Number(currentValueText), currency) : compactMoney(last.value, currency);
  return (
    <div className="chart-card">
      <div className="chart-meta">
        <div>
          <span className="metric-label">Current</span>
          <strong>{current}</strong>
        </div>
        <div>
          <span className="metric-label">Range change</span>
          <strong>{changeText}</strong>
        </div>
      </div>
      <svg className="net-worth-chart" viewBox={`0 0 ${WIDTH} ${HEIGHT}`} role="img">
        <style>{rules}</style>
        <defs>
          <linearGradient id="chart-area-fill" x1="0" y1="0" x2="0" y2="1">
            <stop className="chart-area-stop-top" offset="0" />
            <stop className="chart-area-stop-bottom" offset="1" />
          </linearGradient>
        </defs>
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
        {points.length > 1 && (
          <>
            <path className="chart-area" d={area} />
            <path className="chart-line" d={line} />
          </>
        )}
        {points.map((p) => (
          <circle key={p.index} className="chart-point" cx={p.x} cy={p.y} r={3.4}>
            <title>{`${p.date}: ${money(p.value, currency, 2)}`}</title>
          </circle>
        ))}
        <g className="chart-hover-layer">
          {points.map((p, i) => (
            <rect
              key={`hit-${p.index}`}
              className={`chart-hit-zone chart-hit-zone-${p.index}`}
              x={hits[i].x}
              y={PAD.top}
              width={hits[i].width}
              height={plotHeight}
            />
          ))}
          {points.map((p) => {
            const tipX = Math.max(p.x + 184 + 12 > WIDTH - PAD.right ? p.x - 184 - 12 : p.x + 12, 8);
            const tipY = Math.max(p.y - 50 - 10 < PAD.top ? p.y + 12 : p.y - 50 - 10, 8);
            return (
              <g key={`detail-${p.index}`} className={`chart-hover-detail chart-hover-detail-${p.index}`}>
                <line className="chart-hover-line" x1={p.x} x2={p.x} y1={PAD.top} y2={p.y} />
                <circle className="chart-hover-point" cx={p.x} cy={p.y} r={6} />
                <rect className="chart-tooltip" x={tipX} y={tipY} width={184} height={50} rx={6} />
                <text className="chart-tooltip-date" x={tipX + 10} y={tipY + 20}>
                  {p.date}
                </text>
                <text className="chart-tooltip-value" x={tipX + 10} y={tipY + 38}>
                  {money(p.value, currency, 2)}
                </text>
              </g>
            );
          })}
        </g>
      </svg>
    </div>
  );
}
