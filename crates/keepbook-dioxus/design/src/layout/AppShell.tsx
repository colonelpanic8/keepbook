import { useState, type ReactNode } from "react";
import logo from "../../../../../assets/keepbook-icon.svg";
import { cx } from "../cx";

/** The app's navigation views, in sidebar order. */
export const NAV_ITEMS = [
  "Accounts",
  "Assets",
  "Spending",
  "Recurring",
  "Net Worth",
  "Net Worth Breakdown",
  "Connections",
  "Proposed Edits",
  "Settings",
];

export interface AppShellProps {
  /** Selected navigation item. */
  active?: string;
  /** Reporting currency shown under the app name. */
  currency?: string;
  /** Repository names for the switcher; the first is selected. */
  repositories?: string[];
  /** Navigation items; defaults to the app's views. */
  navItems?: string[];
  onNavigate?: (item: string) => void;
  /** Page content: usually a `PageToolbar`, `SummaryGrid`, and `Panel`s. */
  children?: ReactNode;
}

/**
 * The full keepbook app frame: sidebar navigation beside a scrolling workspace.
 *
 * Holds the logo, repository switcher, and navigation. Below 1200px wide the
 * sidebar becomes a sticky compact header whose hamburger opens a drawer.
 * Every full-screen design starts here.
 */
export function AppShell({
  active = "Accounts",
  currency = "USD",
  repositories = ["Personal"],
  navItems = NAV_ITEMS,
  onNavigate,
  children,
}: AppShellProps) {
  const [open, setOpen] = useState(false);
  return (
    <main className="shell">
      <div className="app-shell">
        <aside className={cx("app-nav", open && "open")}>
          <div className="nav-header">
            <div className="nav-title">
              <div className="nav-logo" dangerouslySetInnerHTML={{ __html: logo }} />
              <div className="nav-title-text">
                <strong>Keepbook</strong>
                <small>{currency}</small>
              </div>
            </div>
            <button
              className="mobile-nav-toggle"
              type="button"
              aria-label="Toggle navigation"
              aria-expanded={open}
              onClick={() => setOpen(!open)}
            >
              <span aria-hidden="true" />
              <span aria-hidden="true" />
              <span aria-hidden="true" />
            </button>
            <label className="repository-switcher">
              <span>Repository</span>
              <select className="control-input" aria-label="Repository">
                {repositories.map((name) => (
                  <option key={name}>{name}</option>
                ))}
              </select>
            </label>
          </div>
          <nav>
            {navItems.map((item) => (
              <button
                key={item}
                className={cx("nav-button", item === active && "selected")}
                onClick={() => {
                  setOpen(false);
                  onNavigate?.(item);
                }}
              >
                {item}
              </button>
            ))}
          </nav>
        </aside>
        <button
          className={cx("nav-backdrop", open && "open")}
          type="button"
          aria-label="Close navigation"
          onClick={() => setOpen(false)}
        />
        <div className="workspace">
          <div>{children}</div>
        </div>
      </div>
    </main>
  );
}
