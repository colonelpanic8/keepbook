import { cx, seriesColor } from "../cx";
import { EmptyState } from "../feedback/EmptyState";
import { money } from "./money";

export interface SpendingTag {
  key: string;
  /** App decimal text for the tag's spending, e.g. "-430.27". */
  total: string;
  transactionCount: number;
}

export interface SpendingTotal {
  label: string;
  /** App-formatted value, e.g. "$8,361.43". */
  value: string;
  detail: string;
  /** A focused period or selection. */
  highlighted?: boolean;
}

export interface SpendingBreakdownProps {
  /** Tags in descending order of spending; this order assigns the series colors. */
  tags: SpendingTag[];
  /** Total cards beside the donut: the range total, then any focused period or selection. */
  totals: SpendingTotal[];
  currency?: string;
  /** Tag colors by key; defaults to the series palette by position. */
  colors?: Record<string, string>;
  /** A focused tag: its slice and row are highlighted. */
  selected?: string;
}

const fixed = (n: number) => n.toFixed(2);

function slicePath(start: number, end: number): string {
  const [cx, cy, r] = [130, 130, 104];
  return `M ${fixed(cx)} ${fixed(cy)} L ${fixed(cx + r * Math.cos(start))} ${fixed(cy + r * Math.sin(start))} A ${fixed(r)} ${fixed(r)} 0 ${end - start > Math.PI ? 1 : 0} 1 ${fixed(cx + r * Math.cos(end))} ${fixed(cy + r * Math.sin(end))} Z`;
}

/**
 * Spending by tag: a donut chart beside the totals and one row per tag.
 *
 * Each row shows the tag's swatch, amount, and transaction count. Below 1200px
 * wide the donut stacks above the list. Mirrors
 * `components/charts/spending_breakdown.rs`.
 */
export function SpendingBreakdown({ tags, totals, currency = "USD", colors = {}, selected }: SpendingBreakdownProps) {
  const color = (key: string, index: number) => colors[key] ?? seriesColor(index);
  const values = tags.map((t) => Math.abs(Number(t.total)) || 0);
  const sum = values.reduce((a, b) => a + b, 0);
  let cursor = -Math.PI / 2;
  const slices =
    sum > 0
      ? tags.flatMap((t, index) => {
          if (values[index] <= 0) return [];
          const start = cursor;
          cursor += (values[index] / sum) * Math.PI * 2;
          return [{ key: t.key, total: values[index], d: slicePath(start, cursor), color: color(t.key, index) }];
        })
      : [];
  return (
    <div className="spending-layout">
      <div className="spending-chart-area">
        {slices.length === 0 ? (
          <EmptyState compact title="No spending in range" detail="Refresh transactions or adjust the range." />
        ) : (
          <svg className="spending-pie" viewBox="0 0 260 260" role="img">
            {slices.map((s) => (
              <path key={s.key} className={cx("pie-slice", s.key === selected && "selected")} d={s.d} style={{ fill: s.color }}>
                <title>{`${s.key}: ${money(s.total, currency, 2)}`}</title>
              </path>
            ))}
            <circle className="pie-hole" cx={130} cy={130} r={56} />
            <text className="pie-center-label" x={130} y={124}>
              Spend
            </text>
            <text className="pie-center-value" x={130} y={145}>
              {tags.length}
            </text>
          </svg>
        )}
      </div>
      <div className="tag-list">
        {totals.map((t) => (
          <div key={t.label} className={cx("spending-total", t.highlighted && "selected-total")}>
            <span className="metric-label">{t.label}</span>
            <strong>{t.value}</strong>
            <small>{t.detail}</small>
          </div>
        ))}
        {tags.map((t, index) => (
          <button key={t.key} className={cx("tag-row", t.key === selected && "selected")}>
            <span className="tag-swatch" style={{ background: color(t.key, index) }} aria-hidden="true" />
            <span className="tag-name">{t.key}</span>
            <strong>{money(Number(t.total), currency, 2)}</strong>
            <small>{`${t.transactionCount} tx`}</small>
          </button>
        ))}
      </div>
    </div>
  );
}
