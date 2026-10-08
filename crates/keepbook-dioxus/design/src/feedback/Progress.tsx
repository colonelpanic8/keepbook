export interface ProgressProps {
  /** What is in progress, e.g. "Cloning keepbook-data…". */
  label: string;
}

/** An indeterminate progress bar with a spinner and label, for long operations such as a Git clone. */
export function Progress({ label }: ProgressProps) {
  return (
    <div className="clone-progress" role="status" aria-live="polite">
      <span className="activity-spinner large" aria-hidden="true" />
      <div className="clone-progress-copy">
        <p>{label}</p>
        <div className="indeterminate-progress">
          <span />
        </div>
      </div>
    </div>
  );
}
