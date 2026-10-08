import type { ReactNode } from "react";
import { cx } from "../cx";

export interface SummaryGridProps {
  className?: string;
  /** Usually three `MetricCard`s. */
  children: ReactNode;
}

/**
 * A row of three headline metrics at the top of a view.
 *
 * The columns stack below 1200px wide. Mirrors `components/layout/summary_grid.rs`.
 */
export function SummaryGrid({ className, children }: SummaryGridProps) {
  return <section className={cx("summary-grid", className)}>{children}</section>;
}
