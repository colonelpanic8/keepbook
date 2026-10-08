import { useState, type ReactNode } from "react";
import { cx } from "../cx";
import { ControlButton } from "./ControlButton";
import { renderIcon, type IconName } from "./icons";

/** A less common variant of the split button's main action. */
export interface MenuAction {
  value: string;
  label: string;
  /** One line explaining how this variant differs. */
  detail: string;
}

export interface SplitButtonProps {
  /** Label of the main action. */
  children: ReactNode;
  primary?: boolean;
  icon?: IconName;
  title?: string;
  busy?: boolean;
  disabled?: boolean;
  /** Accessible label for the chevron. */
  menuLabel: string;
  actions: MenuAction[];
  /** Start with the menu open (for static designs). */
  defaultOpen?: boolean;
  onClick?: () => void;
  onSelect?: (value: string) => void;
}

/**
 * A main action plus a chevron menu of its less common variants.
 *
 * For example, "Refresh all prices" with "Refresh stale prices" and "Resync
 * data" in the menu. Use it instead of a checkbox that modifies an action, and
 * to keep a toolbar down to its essential buttons. Mirrors `SplitButton` in
 * `views/shared.rs`.
 */
export function SplitButton({
  children,
  primary,
  icon,
  title,
  busy,
  disabled,
  menuLabel,
  actions,
  defaultOpen = false,
  onClick,
  onSelect,
}: SplitButtonProps) {
  const [open, setOpen] = useState(defaultOpen);
  return (
    <div className="split-button" onKeyDown={(event) => event.key === "Escape" && setOpen(false)}>
      <ControlButton primary={primary} icon={icon} title={title} busy={busy} disabled={disabled} onClick={onClick}>
        {children}
      </ControlButton>
      <button
        className={cx("control-button", primary && "primary", "split-button-caret")}
        aria-label={menuLabel}
        aria-expanded={open}
        disabled={disabled || busy}
        onClick={() => setOpen(!open)}
      >
        {renderIcon("chevron-down")}
      </button>
      {open && (
        <>
          <div className="menu-backdrop" onClick={() => setOpen(false)} />
          <div className="menu">
            {actions.map((action) => (
              <button
                key={action.value}
                className="menu-item"
                onClick={() => {
                  setOpen(false);
                  onSelect?.(action.value);
                }}
              >
                <strong>{action.label}</strong>
                <small>{action.detail}</small>
              </button>
            ))}
          </div>
        </>
      )}
    </div>
  );
}
