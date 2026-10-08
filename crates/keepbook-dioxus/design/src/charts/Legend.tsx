import { seriesColor } from "../cx";

export interface LegendProps {
  /** Series labels, colored by position from the shared series palette. */
  items: string[];
}

/** A wrapping row of swatch + label pairs under a chart. Series colors are assigned by position, matching the chart. */
export function Legend({ items }: LegendProps) {
  return (
    <div className="stacked-legend">
      {items.map((label, index) => (
        <span key={label} className="stacked-legend-item">
          <span className="stacked-legend-swatch" style={{ background: seriesColor(index) }} />
          <span>{label}</span>
        </span>
      ))}
    </div>
  );
}
