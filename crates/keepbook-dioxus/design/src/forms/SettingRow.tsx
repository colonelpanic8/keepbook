import type { ReactNode } from "react";
import { cx } from "../cx";

export interface SettingRowProps {
  title: string;
  description: string;
  /** Set when the control is a `Select` or `SegmentedControl`: it stacks under the copy below 520px. */
  stacked?: boolean;
  /** Exactly one control: a `Switch`, `Select`, or `SegmentedControl`. */
  children: ReactNode;
}

/** One setting: a title and description on the left, a single control on the right. */
export function SettingRow({ title, description, stacked, children }: SettingRowProps) {
  return (
    <article className={cx("setting-row", stacked && "setting-row-stacked")}>
      <div className="setting-copy">
        <strong>{title}</strong>
        <small>{description}</small>
      </div>
      {children}
    </article>
  );
}
