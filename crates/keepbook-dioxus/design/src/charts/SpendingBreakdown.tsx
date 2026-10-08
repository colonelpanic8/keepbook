import { cx, seriesColor } from "../cx";
import { money } from "./money";

export interface SpendingTag {
  tag: string;
  /** Spending for the range, as a positive amount. */
  total: number;
  /** Number of transactions. */
  count: number;
}

export interface SpendingBreakdownProps {
  /** Tags in descending order of spending; this order assigns the series colors. */
  tags: SpendingTag[];
  /** App-formatted total for the range, e.g. "$8,361.43". */
  total: string;
  /** Context under the total, e.g. "15 transactions / 2025-10-08 to 2026-10-08". */
  detail: string;
  currency?: string;
  /** A focused tag: its slice and row are highlighted. */
  selectedTag?: string;
}

function slicePath(start: number, end: number): string {
  const [cx, cy, r] = [130, 130, 104];
  const point = (angle: number) => `${(cx + r * Math.cos(angle)).toFixed(2)} ${(cy + r * Math.sin(angle)).toFixed(2)}`;
  return `M ${cx} ${cy} L ${point(start)} A ${r} ${r} 0 ${end - start > Math.PI ? 1 : 0} 1 ${point(end)} Z`;
}

/**
 * Spending by tag: a donut chart beside the range total and one row per tag.
 *
 * Each row shows the tag's swatch, amount, and transaction count. Below 1200px
 * wide the donut stacks above the list. Mirrors `SpendingPieChart` and
 * `TagRow` in `views/spending/pie.rs`.
 */
export function SpendingBreakdown({ tags, total, detail, currency = "USD", selectedTag }: SpendingBreakdownProps) {
  const sum = tags.reduce((acc, t) => acc + Math.abs(t.total), 0);
  let cursor = -Math.PI / 2;
  const slices = tags.map((t, index) => {
    const start = cursor;
    cursor += sum > 0 ? (Math.abs(t.total) / sum) * Math.PI * 2 : 0;
    return { tag: t.tag, d: slicePath(start, cursor), color: seriesColor(index) };
  });
  return (
    <div className="spending-layout">
      <div className="spending-chart-area">
        {sum > 0 ? (
          <svg className="spending-pie" viewBox="0 0 260 260" role="img" aria-label="Spending by tag">
            {slices.map((s) => (
              <path key={s.tag} className={cx("pie-slice", s.tag === selectedTag && "selected")} d={s.d} style={{ fill: s.color }} />
            ))}
            <circle className="pie-hole" cx={130} cy={130} r={56} />
            <text className="pie-center-label" x={130} y={124}>
              Spend
            </text>
            <text className="pie-center-value" x={130} y={145}>
              {tags.length}
            </text>
          </svg>
        ) : (
          <div className="chart-empty spending-empty">
            <strong>No spending in range</strong>
            <small>Refresh transactions or adjust the range.</small>
          </div>
        )}
      </div>
      <div className="tag-list">
        <div className="spending-total">
          <span className="metric-label">Total</span>
          <strong>{total}</strong>
          <small>{detail}</small>
        </div>
        {tags.map((t, index) => (
          <button key={t.tag} className={cx("tag-row", t.tag === selectedTag && "selected")}>
            <span className="tag-swatch" style={{ background: seriesColor(index) }} aria-hidden="true" />
            <span className="tag-name">{t.tag}</span>
            <strong>{money(t.total, currency, 2)}</strong>
            <small>{t.count} tx</small>
          </button>
        ))}
      </div>
    </div>
  );
}
