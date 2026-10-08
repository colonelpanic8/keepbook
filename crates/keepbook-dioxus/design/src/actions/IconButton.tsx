import type { MouseEventHandler } from "react";
import { cx } from "../cx";

export interface IconButtonProps {
  /** Tooltip and accessible name; required because the button shows only a glyph. */
  label: string;
  /** A single glyph: `›` expand or next, `‹` previous, `×` close, `+` add. */
  glyph: string;
  disabled?: boolean;
  className?: string;
  onClick?: MouseEventHandler<HTMLButtonElement>;
}

/**
 * A transparent 28px button for a single glyph: expand, paginate, or close.
 *
 * Mirrors `components/actions/icon_button.rs`.
 */
export function IconButton({ label, glyph, disabled, className, onClick }: IconButtonProps) {
  return (
    <button
      className={cx("icon-button", className)}
      type="button"
      title={label}
      aria-label={label}
      disabled={disabled}
      onClick={onClick}
    >
      {glyph}
    </button>
  );
}
