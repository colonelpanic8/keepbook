import { cx } from "../cx";

export interface SpinnerProps {
  /** `small` inside buttons, `medium` beside text, `large` in placeholders. */
  size?: "small" | "medium" | "large";
}

/** An indeterminate activity spinner. Pair it with text saying what is happening. */
export function Spinner({ size = "medium" }: SpinnerProps) {
  return (
    <span
      className={cx("activity-spinner", size === "small" && "control-spinner", size === "large" && "large")}
      aria-hidden="true"
    />
  );
}
