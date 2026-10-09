import { IconButton } from "../actions/IconButton";

export interface OperationStatusProps {
  /** Busy: a present participle and an ellipsis ("Refreshing prices…"). Done: past tense with a count ("Refreshed 14 prices."). */
  message: string;
  /** Work in progress: shows a spinner on the tinted treatment. */
  busy?: boolean;
  /** Adds a close button to a settled message. Used inside a `StatusStack`. */
  onDismiss?: () => void;
}

/**
 * Non-modal feedback for an operation: a busy line while work runs, then its result.
 *
 * Inline, it holds a region's first load. Feedback for work the user started
 * goes in a `StatusStack`, so it never moves the page. Mirrors
 * `components/feedback/operation_status.rs`.
 */
export function OperationStatus({ message, busy, onDismiss }: OperationStatusProps) {
  return (
    <div className={busy ? "notice busy" : "notice"} role="status" aria-live="polite" aria-busy={busy || undefined}>
      {busy && <span className="activity-spinner" aria-hidden="true" />}
      <span>{message}</span>
      {onDismiss && !busy && <IconButton label="Dismiss" glyph="×" className="notice-dismiss" onClick={() => onDismiss()} />}
    </div>
  );
}
