import type { ReactNode } from "react";
import { cx } from "../cx";

export interface DataTableProps {
  /** The table's own class, e.g. `account-table`, whose stylesheet rule sets the column tracks. */
  className?: string;
  /** Column headings. */
  columns: string[];
  /**
   * Rows: one `<div className="table-row">` each, with one child per column.
   * Add `ignored-account-row` to mute an ignored or excluded row.
   */
  children?: ReactNode;
}

/**
 * keepbook's data table: uppercase header, hairline rows, stacked cards on narrow screens.
 *
 * Column tracks come from the table's class: `account-table`,
 * `connection-table`, `proposed-edits-table`, or `recurring-occurrence-table`.
 * It scrolls horizontally on desktop and turns each row into a stacked card
 * below 1200px wide. Inside a `TreeGroup` it drops its own border. Show
 * missing values as an em dash (—). Mirrors `components/display/data_table.rs`.
 */
export function DataTable({ className, columns, children }: DataTableProps) {
  return (
    <div className={cx("data-table", className)}>
      <div className="table-head">
        {columns.map((column) => (
          <span key={column}>{column}</span>
        ))}
      </div>
      {children}
    </div>
  );
}
