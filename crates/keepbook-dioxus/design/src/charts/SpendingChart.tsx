import { cx, seriesColor } from "../cx";
import { EmptyState } from "../feedback/EmptyState";
import { Legend } from "./Legend";
import { compactMoney, money } from "./money";

export interface SpendingBucket {
  /** Bucket label, e.g. `2026-06`. */
  label: string;
  startDate: string;
  endDate: string;
  /** Spending in the bucket, as a positive amount. */
  total: number;
  transactionCount: number;
  /** Spending per tag in the bucket, as positive amounts. */
  segments: { key: string; value: number; transactionCount: number }[];
}

export interface SpendingChartProps {
  /** Buckets in date order, from the app's spending output. */
  buckets: SpendingBucket[];
  /** Tags in legend order; this order assigns the series colors. */
  series: string[];
  /** App-formatted total for the range, e.g. "$8,361.43". */
  total: string;
  /** `Daily`, `Weekly`, `Monthly`, `Quarterly`, or `Yearly`. */
  bucketLabel?: string;
  currency?: string;
  /** Tag colors by key; defaults to the series palette by position. */
  colors?: Record<string, string>;
  /** A focused tag: the bars narrow to it and the legend highlights it. */
  selected?: string;
  /** A segment pinned by a click: it is outlined and shows its tooltip. */
  pinned?: { bucket: number; key: string };
}

const WIDTH = 720;
const HEIGHT = 300;
const PAD = { left: 68, right: 20, top: 18, bottom: 44 };
const PERIOD_NOUN: Record<string, string> = {
  Daily: "day",
  Weekly: "week",
  Monthly: "month",
  Quarterly: "quarter",
  Yearly: "year",
};

/**
 * The spending-over-time stacked bar chart: one bar per bucket, stacked by tag.
 *
 * Shows the range total and bucket size above the bars and a tag legend below.
 * Clicking a segment pins a tooltip with the tag's amount and the bucket total. Mirrors
 * `components/charts/spending_chart.rs`, geometry and copy included.
 */
export function SpendingChart({
  buckets,
  series,
  total,
  bucketLabel = "Monthly",
  currency = "USD",
  colors = {},
  selected,
  pinned,
}: SpendingChartProps) {
  const color = (key: string) => colors[key] ?? seriesColor(Math.max(series.indexOf(key), 0));
  const shown = selected
    ? buckets.map((b) => {
        const segments = b.segments.filter((s) => s.key === selected);
        return {
          ...b,
          segments,
          total: segments.reduce((sum, s) => sum + s.value, 0),
          transactionCount: segments.reduce((sum, s) => sum + s.transactionCount, 0),
        };
      })
    : buckets;
  if (shown.length === 0 || series.length === 0) {
    return <EmptyState title="No spending over time" detail="Refresh transactions or adjust the range." className="spending-over-time-empty" />;
  }
  const plotWidth = WIDTH - PAD.left - PAD.right;
  const plotHeight = HEIGHT - PAD.top - PAD.bottom;
  const yMax = Math.max(...shown.map((b) => b.total), 1) * 1.08;
  const slot = plotWidth / shown.length;
  const gap = Math.min(Math.max(slot * 0.18, 3), 14);
  const barWidth = Math.max(slot - gap, 2);
  const rects = shown.flatMap((b, index) => {
    const x = PAD.left + index * slot + gap / 2;
    let cumulative = 0;
    return series.flatMap((key) => {
      const segment = b.segments.find((s) => s.key === key);
      if (!segment) return [];
      const y0 = PAD.top + ((yMax - cumulative) / yMax) * plotHeight;
      cumulative += segment.value;
      const y1 = PAD.top + ((yMax - cumulative) / yMax) * plotHeight;
      return [{ bucket: b, index, key, segment, x, y: y1, height: Math.max(y0 - y1, 0.6) }];
    });
  });
  const hovered = pinned && rects.find((r) => r.index === pinned.bucket && r.key === pinned.key);
  const tooltip = hovered && {
    title: `${hovered.bucket.label} · ${hovered.key}`,
    detail: `${money(hovered.segment.value, currency, 2)} category · ${money(hovered.bucket.total, currency, 2)} ${PERIOD_NOUN[bucketLabel] ?? "period"} total`,
  };
  const tipWidth =
    tooltip && Math.min(Math.max(Math.max(tooltip.title.length * 8.5, tooltip.detail.length * 7.5) + 28, 184), Math.max(WIDTH - 16, 184));
  const tipX = hovered && tipWidth && Math.min(Math.max(hovered.x + barWidth / 2, 8 + tipWidth / 2), WIDTH - 8 - tipWidth / 2);
  return (
    <div className="chart-card spending-over-time-card">
      <div className="chart-meta">
        <div>
          <span className="metric-label">{selected ? `Over Time · ${selected}` : "Over Time"}</span>
          <strong>{total}</strong>
        </div>
        <div>
          <span className="metric-label">Bucket</span>
          <strong>{bucketLabel}</strong>
        </div>
      </div>
      <svg className="net-worth-chart spending-bar-chart" viewBox={`0 0 ${WIDTH} ${HEIGHT}`} role="img">
        <line className="chart-grid" x1={PAD.left} x2={WIDTH - PAD.right} y1={PAD.top} y2={PAD.top} />
        <line className="chart-grid" x1={PAD.left} x2={WIDTH - PAD.right} y1={PAD.top + plotHeight / 2} y2={PAD.top + plotHeight / 2} />
        <line className="chart-grid axis" x1={PAD.left} x2={WIDTH - PAD.right} y1={PAD.top + plotHeight} y2={PAD.top + plotHeight} />
        <text className="chart-axis-label" x={8} y={PAD.top + 4}>
          {compactMoney(yMax, currency)}
        </text>
        <text className="chart-axis-label" x={8} y={PAD.top + plotHeight / 2 + 4}>
          {compactMoney(yMax / 2, currency)}
        </text>
        <text className="chart-axis-label" x={8} y={PAD.top + plotHeight + 4}>
          $0
        </text>
        <text className="chart-axis-label date-label" x={PAD.left} y={HEIGHT - 10}>
          {shown[0].label}
        </text>
        <text className="chart-axis-label date-label end" x={WIDTH - PAD.right} y={HEIGHT - 10}>
          {shown[shown.length - 1].label}
        </text>
        <g className="spending-bar-hit-layer">
          {shown.map((b, index) => (
            <rect key={b.label} className="spending-bar-hit-zone" x={PAD.left + index * slot} y={PAD.top} width={slot} height={plotHeight}>
              <title>{`${b.label}: ${money(b.total, currency, 2)} / ${b.transactionCount} transactions / ${b.startDate} to ${b.endDate}`}</title>
            </rect>
          ))}
        </g>
        {rects.map((r) => (
          <rect
            key={`${r.index}-${r.key}`}
            className={cx("spending-bar-segment", r === hovered && "pinned")}
            x={r.x}
            y={r.y}
            width={barWidth}
            height={r.height}
            style={{ fill: color(r.key) }}
          >
            <title>{`${r.bucket.label}: ${money(r.bucket.total, currency, 2)} total / ${r.key}: ${money(r.segment.value, currency, 2)} (${r.segment.transactionCount} tx)`}</title>
          </rect>
        ))}
        {tooltip && tipWidth && tipX && (
          <g className="spending-chart-tooltip" transform={`translate(${tipX}, ${PAD.top + 14})`}>
            <rect x={-tipWidth / 2} y={-11} width={tipWidth} height={42} rx={5} />
            <text className="spending-tooltip-title" x={0} y={3}>
              {tooltip.title}
            </text>
            <text className="spending-tooltip-detail" x={0} y={19}>
              {tooltip.detail}
            </text>
          </g>
        )}
      </svg>
      <Legend
        className="spending-bar-legend"
        items={series.map((key) => ({ label: key, color: color(key) }))}
        selected={selected}
        onSelect={() => {}}
      />
    </div>
  );
}
