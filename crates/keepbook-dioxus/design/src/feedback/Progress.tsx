import { Spinner } from "./Spinner";

export interface ProgressProps {
  /** What is in progress, e.g. "Cloning keepbook-data…". */
  label: string;
  /** While false, only the label shows, for the operation's final message. Defaults to true. */
  busy?: boolean;
}

/** An indeterminate progress bar with a spinner and label, for long operations. Mirrors `components/feedback/progress.rs`. */
export function Progress({ label, busy = true }: ProgressProps) {
  return (
    <div className="clone-progress" role="status" aria-live="polite">
      {busy && <Spinner size="large" />}
      <div className="clone-progress-copy">
        <p>{label}</p>
        {busy && (
          <div className="indeterminate-progress">
            <span />
          </div>
        )}
      </div>
    </div>
  );
}
