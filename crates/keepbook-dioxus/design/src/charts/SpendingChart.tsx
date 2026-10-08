import { cx, seriesColor } from "../cx";
import { compactMoney, money } from "./money";

export interface SpendingBucket {
  /** Bucket label, e.g. `2026-06`. */
  label: string;
  /** Spending per tag in this bucket, as positive amounts. */
  segments: { tag: string; value: number }[];
}

export interface SpendingChartProps {
  /** Buckets in date order, from the app's spending output. */
  buckets: SpendingBucket[];
  /** Tags in legend order; this order assigns the series colors. */
  tags: string[];
  /** App-formatted total for the range, e.g. "$8,361.43". */
  total: string;
  /** `Daily`, `Weekly`, `Monthly`, `Quarterly`, or `Yearly`. */
  bucket?: string;
  currency?: string;
  /** A segment to show as hovered, with its tooltip. */
  hover?: { bucket: number; tag: string };
  /** A focused tag: the legend highlights it and the meta names it. */
  selectedTag?: string;
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
 * Hovering a segment shows the tag's amount and the bucket total. Mirrors
 * `SpendingOverTimeChart` in `views/spending/over_time.rs`, including its
 * geometry and tooltip copy.
 */
export function SpendingChart({
  buckets,
  tags,
  total,
  bucket = "Monthly",
  currency = "USD",
  hover,
  selectedTag,
}: SpendingChartProps) {
  // A focused tag narrows every bar to that tag, as the app does.
  const shown = selectedTag
    ? buckets.map((b) => ({ ...b, segments: b.segments.filter((s) => s.tag === selectedTag) }))
    : buckets;
  if (shown.length === 0) {
    return (
      <div className="chart-empty spending-over-time-empty">
        <strong>No spending in range</strong>
        <small>Refresh transactions or adjust the range.</small>
      </div>
    );
  }
  const plotWidth = WIDTH - PAD.left - PAD.right;
  const plotHeight = HEIGHT - PAD.top - PAD.bottom;
  const bucketTotal = (b: SpendingBucket) => b.segments.reduce((sum, s) => sum + s.value, 0);
  const yMax = Math.max(...shown.map(bucketTotal), 1) * 1.08;
  const slot = plotWidth / shown.length;
  const gap = Math.min(Math.max(slot * 0.18, 3), 14);
  const barWidth = Math.max(slot - gap, 2);
  const rects = shown.flatMap((b, index) => {
    const x = PAD.left + index * slot + gap / 2;
    let cumulative = 0;
    return tags.flatMap((tag, series) => {
      const segment = b.segments.find((s) => s.tag === tag);
      if (!segment) return [];
      const y0 = PAD.top + ((yMax - cumulative) / yMax) * plotHeight;
      cumulative += segment.value;
      const y1 = PAD.top + ((yMax - cumulative) / yMax) * plotHeight;
      return [{ index, tag, value: segment.value, x, y: y1, height: Math.max(y0 - y1, 0.6), color: seriesColor(series) }];
    });
  });
  const hovered = hover && rects.find((r) => r.index === hover.bucket && r.tag === hover.tag);
  const tooltip = hovered && {
    title: `${shown[hovered.index].label} · ${hovered.tag}`,
    detail: `${money(hovered.value, currency, 2)} category · ${money(bucketTotal(shown[hovered.index]), currency, 2)} ${PERIOD_NOUN[bucket] ?? "period"} total`,
  };
  const tipWidth =
    tooltip && Math.min(Math.max(Math.max(tooltip.title.length * 8.5, tooltip.detail.length * 7.5) + 28, 184), WIDTH - 16);
  const tipX = hovered && tipWidth && Math.min(Math.max(hovered.x + barWidth / 2, 8 + tipWidth / 2), WIDTH - 8 - tipWidth / 2);
  return (
    <div className="chart-card spending-over-time-card">
      <div className="chart-meta">
        <div>
          <span className="metric-label">{selectedTag ? `Over Time · ${selectedTag}` : "Over Time"}</span>
          <strong>{total}</strong>
        </div>
        <div>
          <span className="metric-label">Bucket</span>
          <strong>{bucket}</strong>
        </div>
      </div>
      <svg className="net-worth-chart spending-bar-chart" viewBox={`0 0 ${WIDTH} ${HEIGHT}`} role="img" aria-label="Spending over time">
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
        {rects.map((r) => (
          <rect
            key={`${r.index}-${r.tag}`}
            className={cx("spending-bar-segment", r === hovered && "pinned")}
            x={r.x}
            y={r.y}
            width={barWidth}
            height={r.height}
            style={{ fill: r.color }}
          />
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
      <div className="stacked-legend spending-bar-legend">
        {tags.map((tag, index) => (
          <button key={tag} className={cx("stacked-legend-item", tag === selectedTag && "selected")}>
            <span className="stacked-legend-swatch" style={{ background: seriesColor(index) }} />
            <span>{tag}</span>
          </button>
        ))}
      </div>
    </div>
  );
}
