import type { ReactNode } from "react";
import { cx } from "../cx";

export interface BadgeProps {
  /** `neutral` (default) for plain state; `positive` for active or verified; `negative` for dismissed or failed; `warning` for liabilities. */
  tone?: "neutral" | "positive" | "negative" | "warning";
  children: ReactNode;
}

/** A short status label in a pill ("Active", "Liability", "Not counted"). Use sentence case, never uppercase. */
export function Badge({ tone = "neutral", children }: BadgeProps) {
  return <span className={cx("badge", tone !== "neutral" && tone)}>{children}</span>;
}
