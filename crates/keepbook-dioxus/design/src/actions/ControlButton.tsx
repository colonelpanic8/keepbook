import type { MouseEventHandler, ReactNode } from "react";
import { cx } from "../cx";
import { Spinner } from "../feedback/Spinner";
import { renderIcon, type IconName } from "./icons";

export interface ControlButtonProps {
  children: ReactNode;
  /** The emphasized action. Use at most one per toolbar. */
  primary?: boolean;
  /** Toggled-on state, e.g. an active filter. Not for emphasis. */
  selected?: boolean;
  /** Destructive actions: dismiss, remove, reject. */
  danger?: boolean;
  /** Compact size for dense rows such as tag editors. */
  small?: boolean;
  /** Leading 14px stroke icon. */
  icon?: IconName;
  /** Work in progress: shows a spinner in place of the icon and disables the button. */
  busy?: boolean;
  disabled?: boolean;
  /** Tooltip; use it to explain how sibling variants differ. */
  title?: string;
  className?: string;
  onClick?: MouseEventHandler<HTMLButtonElement>;
}

/**
 * keepbook's one button: quiet by default, filled with the brand color when `primary`.
 *
 * While `busy`, a spinner takes the icon's slot so the label doesn't shift.
 * Use a present participle label while busy ("Refreshing"). Mirrors
 * `components/actions/control_button.rs`.
 */
export function ControlButton({
  children,
  primary,
  selected,
  danger,
  small,
  icon,
  busy,
  disabled,
  title,
  className,
  onClick,
}: ControlButtonProps) {
  return (
    <button
      className={cx(
        "control-button",
        selected && "selected",
        primary && "primary",
        danger && "danger",
        small && "small",
        className,
      )}
      title={title}
      disabled={disabled || busy}
      aria-busy={busy || undefined}
      onClick={onClick}
    >
      {busy ? (
        <Spinner size="small" />
      ) : (
        icon && renderIcon(icon)
      )}
      {children}
    </button>
  );
}
