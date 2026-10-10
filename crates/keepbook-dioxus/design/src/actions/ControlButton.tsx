import type { MouseEventHandler, ReactNode } from "react";
import { cx } from "../cx";
import { Spinner } from "../feedback/Spinner";
import { renderIcon, type IconName } from "./icons";

/** The state of the work a button started, shown on the button itself. */
export interface ButtonFeedback {
  tone: "busy" | "done" | "failed";
  /** A short label that fits the button: "Refreshing…", "Prices refreshed". Empty shows only the icon. */
  label: string;
  /** The full message, shown as the tooltip. */
  detail?: string;
}

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
  /** Lays the state of the work it started over the label, keeping the button's width. Cleared once settled. */
  feedback?: ButtonFeedback;
  disabled?: boolean;
  /** Tooltip; use it to explain how sibling variants differ. */
  title?: string;
  className?: string;
  onClick?: MouseEventHandler<HTMLButtonElement>;
}

/**
 * keepbook's one button: quiet by default, filled with the brand color when `primary`.
 *
 * While `busy`, a spinner takes the icon's slot. `feedback` reports the work
 * the button started on the button itself: its own label stays in place,
 * hidden, so the width never changes, and a longer feedback label is
 * ellipsized. Mirrors `components/actions/control_button.rs`.
 */
export function ControlButton({
  children,
  primary,
  selected,
  danger,
  small,
  icon,
  busy,
  feedback,
  disabled,
  title,
  className,
  onClick,
}: ControlButtonProps) {
  const isBusy = busy || feedback?.tone === "busy";
  const classes = cx(
    "control-button",
    selected && "selected",
    primary && "primary",
    danger && "danger",
    small && "small",
    className,
  );
  if (feedback) {
    return (
      <button
        className={cx(classes, "has-feedback", `feedback-${feedback.tone}`)}
        title={feedback.detail ?? title}
        disabled={disabled || isBusy}
        aria-busy={isBusy || undefined}
        onClick={onClick}
      >
        <span className="button-face" aria-hidden="true">
          {icon && renderIcon(icon)}
          {children}
        </span>
        <span className="button-feedback" role="status" aria-live="polite">
          {feedback.tone === "busy" ? (
            <Spinner size="small" />
          ) : (
            renderIcon(feedback.tone === "done" ? "check" : "circle-alert")
          )}
          <span className="button-feedback-label">{feedback.label}</span>
        </span>
      </button>
    );
  }
  return (
    <button className={classes} title={title} disabled={disabled || isBusy} aria-busy={isBusy || undefined} onClick={onClick}>
      {busy ? (
        <Spinner size="small" />
      ) : (
        icon && renderIcon(icon)
      )}
      {children}
    </button>
  );
}
