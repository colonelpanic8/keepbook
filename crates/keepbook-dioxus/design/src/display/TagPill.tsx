import type { MouseEventHandler } from "react";

export interface TagPillProps {
  /** The spending tag. */
  tag: string;
  /** Shows a remove mark; clicking the pill removes the tag. */
  removable?: boolean;
  /** An unapplied suggestion; clicking applies it. */
  suggestion?: boolean;
  disabled?: boolean;
  onClick?: MouseEventHandler<HTMLButtonElement>;
}

/**
 * A spending tag pill: read-only, removable, or an outlined suggestion.
 *
 * Tag colors come from the swatch in tag lists and charts, never from the pill
 * itself. Mirrors `components/display/tag_pill.rs`.
 */
export function TagPill({ tag, removable, suggestion, disabled, onClick }: TagPillProps) {
  if (suggestion) {
    return (
      <button className="tag-suggestion-pill" disabled={disabled} onClick={onClick}>
        {tag}
      </button>
    );
  }
  if (removable) {
    return (
      <button className="tag-pill removable" title="Remove tag" disabled={disabled} onClick={onClick}>
        <span>{tag}</span>
        <span className="tag-pill-remove">x</span>
      </button>
    );
  }
  return <span className="tag-pill readonly">{tag}</span>;
}
