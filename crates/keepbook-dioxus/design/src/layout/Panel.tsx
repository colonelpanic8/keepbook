import type { ReactNode } from "react";
import { cx } from "../cx";

export interface PanelProps {
  title: string;
  /** One supporting fact, shown muted under the title. */
  subtitle?: string;
  /** Panel-scoped controls on the right of the header. */
  actions?: ReactNode;
  className?: string;
  children?: ReactNode;
}

/**
 * The unit of page content: a bordered surface with a titled header.
 *
 * A panel is a single column that never grows wider than the workspace, so
 * long values wrap. Mirrors `components/layout/panel.rs`.
 */
export function Panel({ title, subtitle, actions, className, children }: PanelProps) {
  return (
    <section className={cx("panel", className)}>
      <div className="panel-header">
        <div className="panel-title">
          <h2>{title}</h2>
          {subtitle && <span>{subtitle}</span>}
        </div>
        {actions}
      </div>
      {children}
    </section>
  );
}
