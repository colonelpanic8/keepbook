export interface OperationStatusProps {
  /** Busy: a present participle and an ellipsis ("Refreshing prices…"). Done: past tense with a count ("Refreshed 14 prices."). */
  message: string;
  /** Work in progress: shows a spinner on the tinted treatment. */
  busy?: boolean;
}

/**
 * Inline feedback for work the user started, shown next to its controls.
 *
 * It never blocks the page or replaces loaded content. Mirrors
 * `components/feedback/operation_status.rs`.
 */
export function OperationStatus({ message, busy }: OperationStatusProps) {
  return (
    <div className={busy ? "notice busy" : "notice"} role="status" aria-live="polite" aria-busy={busy || undefined}>
      {busy && <span className="activity-spinner" aria-hidden="true" />}
      <span>{message}</span>
    </div>
  );
}
