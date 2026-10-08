import type { MouseEventHandler } from "react";

export interface IconButtonProps {
  /** Accessible name; required because the button shows only a glyph. */
  label: string;
  /** A single glyph: `›` expand or next, `‹` previous, `×` close, `+` add. */
  glyph: string;
  disabled?: boolean;
  onClick?: MouseEventHandler<HTMLButtonElement>;
}

/**
 * A transparent 28px button for a single glyph: expand, paginate, or close.
 */
export function IconButton({ label, glyph, disabled, onClick }: IconButtonProps) {
  return (
    <button className="icon-button" aria-label={label} title={label} disabled={disabled} onClick={onClick}>
      {glyph}
    </button>
  );
}
