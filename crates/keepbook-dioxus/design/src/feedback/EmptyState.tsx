import { cx } from "../cx";

export interface EmptyStateProps {
  title: string;
  /** How to get content here, e.g. "Refresh balances to populate the graph." */
  detail: string;
  /** Shows a large spinner: the content is loading for the first time. */
  loading?: boolean;
  /** For lists and tables: a fixed minimum height instead of a chart's aspect ratio. */
  compact?: boolean;
}

/**
 * A dashed placeholder that keeps the footprint of the content it replaces.
 *
 * The page doesn't jump when data arrives. Mirrors `GraphLoadingPanel` and the
 * views' `chart-empty` blocks.
 */
export function EmptyState({ title, detail, loading, compact }: EmptyStateProps) {
  if (loading) {
    return (
      <div className={cx("chart-loading", compact && "spending-empty")} role="status" aria-live="polite">
        <span className="activity-spinner large" />
        <strong>{title}</strong>
        <span>{detail}</span>
      </div>
    );
  }
  return (
    <div className={cx("chart-empty", compact && "spending-empty")}>
      <strong>{title}</strong>
      <small>{detail}</small>
    </div>
  );
}
