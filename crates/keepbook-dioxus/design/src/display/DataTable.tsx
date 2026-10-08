import type { CSSProperties, ReactNode } from "react";
import { cx } from "../cx";

export interface DataTableColumn {
  label: string;
  /** CSS grid track, e.g. `minmax(200px, 1.2fr)` or `120px`. Defaults to an equal share. */
  width?: string;
  /** Right-align numeric columns such as amounts. */
  align?: "start" | "end";
}

export interface DataTableProps {
  columns: DataTableColumn[];
  /** One array of cells per row. Wrap emphasized values such as amounts in `<strong>`. */
  rows: ReactNode[][];
  /** Indexes of rows to mute as ignored or excluded. */
  mutedRows?: number[];
  /** Makes rows clickable buttons. */
  onRowClick?: (index: number) => void;
}

/**
 * keepbook's data table: uppercase header, hairline rows, stacked cards on narrow screens.
 *
 * It scrolls horizontally on desktop and turns each row into a stacked card
 * below 1200px wide. Inside a `TreeGroup` it drops its own border. Put amounts
 * in end-aligned columns and show missing values as an em dash (—).
 */
export function DataTable({ columns, rows, mutedRows = [], onRowClick }: DataTableProps) {
  const grid: CSSProperties = { gridTemplateColumns: columns.map((c) => c.width ?? "minmax(0, 1fr)").join(" ") };
  const align = (index: number): CSSProperties | undefined =>
    columns[index]?.align === "end" ? { textAlign: "right" } : undefined;
  const cells = (row: ReactNode[]) =>
    row.map((content, index) => (
      <span key={index} style={align(index)}>
        {content}
      </span>
    ));
  return (
    <div className="data-table">
      <div className="table-head" style={grid}>
        {columns.map((column, index) => (
          <span key={column.label} style={align(index)}>
            {column.label}
          </span>
        ))}
      </div>
      {rows.map((row, index) => {
        const className = cx("table-row", mutedRows.includes(index) && "ignored-account-row");
        return onRowClick ? (
          <button key={index} className={className} style={grid} onClick={() => onRowClick(index)}>
            {cells(row)}
          </button>
        ) : (
          <div key={index} className={className} style={grid}>
            {cells(row)}
          </div>
        );
      })}
    </div>
  );
}
