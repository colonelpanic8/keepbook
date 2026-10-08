import type { MouseEventHandler } from "react";
import { cx } from "../cx";

export interface TagPillProps {
  /** The spending tag. */
  tag: string;
  /** Shows a remove mark; clicking the pill removes the tag. */
  removable?: boolean;
  /** An unapplied suggestion; clicking applies it. */
  suggestion?: boolean;
  onClick?: MouseEventHandler<HTMLButtonElement>;
}

/**
 * A spending tag pill: applied, removable, or an outlined suggestion.
 *
 * Tag colors come from the swatch in tag lists and charts, never from the pill
 * itself.
 */
export function TagPill({ tag, removable, suggestion, onClick }: TagPillProps) {
  if (suggestion) {
    return (
      <button className="tag-suggestion-pill" onClick={onClick}>
        + {tag}
      </button>
    );
  }
  return (
    <button className={cx("tag-pill", removable ? "removable" : "readonly")} title={removable ? "Remove tag" : undefined} onClick={onClick}>
      <span>{tag}</span>
      {removable && <span className="tag-pill-remove">x</span>}
    </button>
  );
}
