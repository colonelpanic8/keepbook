import { useState, type ReactNode } from "react";
import logo from "../../../../../assets/keepbook-icon.svg";
import { renderIcon, type IconName } from "../actions/icons";
import { cx } from "../cx";
import { Spinner } from "../feedback/Spinner";
import { toSelectOption, type SelectOption } from "../forms/Select";

/** A sidebar destination: its label and icon. */
export interface NavItem {
  label: string;
  icon: IconName;
}

/** The app's navigation views, in sidebar order, with their icons. */
export const NAV_ITEMS: NavItem[] = [
  { label: "Accounts", icon: "wallet" },
  { label: "Assets", icon: "landmark" },
  { label: "Spending", icon: "receipt" },
  { label: "Recurring", icon: "repeat" },
  { label: "Net Worth", icon: "trending-up" },
  { label: "Net Worth Breakdown", icon: "layers" },
  { label: "Connections", icon: "plug" },
  { label: "Proposed Edits", icon: "file-pen-line" },
  { label: "Settings", icon: "settings" },
];

export interface AppShellProps {
  /** App name beside the logo. */
  title?: string;
  /** Reporting currency shown under the app name. */
  currency?: string;
  /** Repositories for the switcher; no switcher when empty. */
  repositories?: (string | SelectOption)[];
  /** Selected repository value. */
  repository?: string;
  repositoryBusy?: boolean;
  /** A status line under the switcher, e.g. "Switching to Parents…". */
  repositoryStatus?: string;
  /** Loaded data is revalidating: fades a spinner badge in over the logo without taking space. */
  refreshing?: boolean;
  /** Navigation items; defaults to the app's views. */
  navItems?: NavItem[];
  /** Selected navigation item. */
  active?: string;
  onRepositoryChange?: (value: string) => void;
  onNavigate?: (item: string) => void;
  /** Page content: usually a `PageToolbar`, `SummaryGrid`, and `Panel`s. */
  children?: ReactNode;
}

/**
 * The full keepbook app frame: sidebar navigation beside a scrolling workspace.
 *
 * Holds the logo, repository switcher, and navigation. `refreshing` badges the
 * logo while loaded data revalidates. Below 1200px wide the sidebar becomes a
 * sticky compact header whose hamburger opens a drawer.
 * Every full-screen design starts here. Mirrors `components/layout/app_shell.rs`.
 */
export function AppShell({
  title = "Keepbook",
  currency = "USD",
  repositories = ["Personal"],
  repository,
  repositoryBusy = false,
  repositoryStatus,
  refreshing = false,
  navItems = NAV_ITEMS,
  active = navItems[0]?.label,
  onRepositoryChange,
  onNavigate,
  children,
}: AppShellProps) {
  const [open, setOpen] = useState(false);
  const options = repositories.map(toSelectOption);
  return (
    <div className="app-shell">
      <aside className={cx("app-nav", open && "open")}>
        <div className="nav-header">
          <div className="nav-title">
            <div className="nav-logo-slot">
              <div className="nav-logo" dangerouslySetInnerHTML={{ __html: logo }} />
              <span
                className={cx("nav-refresh-badge", refreshing && "active")}
                role="progressbar"
                aria-label="Refreshing app data"
                aria-hidden={refreshing ? undefined : "true"}
              >
                <Spinner size="small" />
              </span>
            </div>
            <div className="nav-title-text">
              <strong>{title}</strong>
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
          {options.length > 0 && (
            <label className="repository-switcher">
              <span>Repository</span>
              <select
                className="control-input"
                aria-label="Repository"
                disabled={repositoryBusy}
                defaultValue={repository ?? options[0].value}
                onChange={(event) => onRepositoryChange?.(event.target.value)}
              >
                {options.map((option) => (
                  <option key={option.value} value={option.value} disabled={option.disabled}>
                    {option.label}
                  </option>
                ))}
              </select>
            </label>
          )}
        </div>
        {repositoryStatus && (
          <div className="repository-switch-status" aria-live="polite">
            {repositoryBusy && <Spinner />}
            <small>{repositoryStatus}</small>
          </div>
        )}
        <nav>
          {navItems.map((item) => (
            <button
              key={item.label}
              className={cx("nav-button", item.label === active && "selected")}
              onClick={() => {
                setOpen(false);
                onNavigate?.(item.label);
              }}
            >
              {renderIcon(item.icon)}
              <span>{item.label}</span>
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
      <div className="workspace">{children}</div>
    </div>
  );
}
