import type { ReactNode } from "react";

export interface SummaryGridProps {
  /** Usually three `MetricCard`s. */
  children: ReactNode;
}

/**
 * A row of three headline metrics at the top of a view.
 *
 * The columns stack below 1200px wide.
 */
export function SummaryGrid({ children }: SummaryGridProps) {
  return <section className="summary-grid">{children}</section>;
}
