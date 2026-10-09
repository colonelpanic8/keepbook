import type { ReactNode } from "react";

export interface StatusStackProps {
  /** Called with `true` while the pointer is over the stack, so settled notices can wait. */
  onHoverChange?: (hovered: boolean) => void;
  /** `OperationStatus` notices, oldest first. */
  children?: ReactNode;
}

/**
 * A floating column of `OperationStatus` notices pinned to the bottom of the viewport.
 *
 * Feedback for work the user started goes here, so starting, finishing, or
 * dismissing it never shifts the page. Mirrors `components/feedback/status_stack.rs`.
 */
export function StatusStack({ onHoverChange, children }: StatusStackProps) {
  return (
    <div
      className="status-stack"
      onMouseEnter={onHoverChange && (() => onHoverChange(true))}
      onMouseLeave={onHoverChange && (() => onHoverChange(false))}
    >
      {children}
    </div>
  );
}
