import { cx, seriesColor } from "../cx";

export interface LegendItem {
  label: string;
  /** A CSS color; defaults to the series palette by position. */
  color?: string;
  /** De-emphasized, e.g. an asset series beside account series. */
  muted?: boolean;
}

export interface LegendProps {
  /** Entries; a plain string is a label colored by position. */
  items: (string | LegendItem)[];
  className?: string;
  /** Highlights the entry with this label. */
  selected?: string;
  /** Makes entries buttons that report the clicked label. */
  onSelect?: (label: string) => void;
}

/**
 * A wrapping row of swatch and label pairs under a chart.
 *
 * Series colors are assigned by position, matching the chart. Mirrors
 * `components/charts/legend.rs`.
 */
export function Legend({ items, className, selected, onSelect }: LegendProps) {
  return (
    <div className={cx("stacked-legend", className)}>
      {items.map((raw, index) => {
        const item: LegendItem = typeof raw === "string" ? { label: raw } : raw;
        const classes = cx("stacked-legend-item", item.muted && "asset", item.label === selected && "selected");
        const content = (
          <>
            <span className="stacked-legend-swatch" style={{ background: item.color ?? seriesColor(index) }} />
            <span>{item.label}</span>
          </>
        );
        return onSelect ? (
          <button key={item.label} className={classes} onClick={() => onSelect(item.label)}>
            {content}
          </button>
        ) : (
          <span key={item.label} className={classes}>
            {content}
          </span>
        );
      })}
    </div>
  );
}
