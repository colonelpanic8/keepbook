import type { ReactNode } from "react";

export interface PageToolbarProps {
  /** `ControlButton`s and `SplitButton`s; at most one `primary`. */
  children: ReactNode;
}

/**
 * Page-level actions (refresh, resync, sync), right-aligned at the top of a view.
 *
 * Place it above the summary metrics, never in a panel header, and put the
 * actions' `OperationStatus` lines directly after it.
 */
export function PageToolbar({ children }: PageToolbarProps) {
  return <div className="page-toolbar">{children}</div>;
}
