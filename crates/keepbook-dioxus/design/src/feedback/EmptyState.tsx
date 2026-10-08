import { cx } from "../cx";
import { Spinner } from "./Spinner";

export interface EmptyStateProps {
  title: string;
  /** How to get content here, e.g. "Refresh balances to populate the graph." */
  detail: string;
  /** Shows a large spinner: the content is loading for the first time. */
  loading?: boolean;
  /** For lists and tables: a fixed minimum height instead of a chart's aspect ratio. */
  compact?: boolean;
  className?: string;
}

/**
 * A dashed placeholder that keeps the footprint of the content it replaces.
 *
 * The page doesn't jump when data arrives. Mirrors
 * `components/feedback/empty_state.rs`.
 */
export function EmptyState({ title, detail, loading, compact, className }: EmptyStateProps) {
  if (loading) {
    return (
      <div className={cx("chart-loading", compact && "compact", className)} role="status" aria-live="polite">
        <Spinner size="large" />
        <strong>{title}</strong>
        <span>{detail}</span>
      </div>
    );
  }
  return (
    <div className={cx("chart-empty", compact && "compact", className)}>
      <strong>{title}</strong>
      <small>{detail}</small>
    </div>
  );
}
