export interface OperationStatusProps {
  /** Busy: a present participle and an ellipsis ("Refreshing prices…"). Done: past tense with a count ("Refreshed 14 prices."). */
  message: string;
  /** Work in progress: shows a spinner on the tinted treatment. */
  busy?: boolean;
}

/**
 * A line that holds a region while its data first loads.
 *
 * Feedback on work the user started belongs on the control that started it
 * (a button's `feedback`, a panel's `status`), not here. Mirrors
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
