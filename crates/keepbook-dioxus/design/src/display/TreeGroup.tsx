import type { ReactNode } from "react";

export interface TreeGroupProps {
  /** The parent, e.g. a connection name. */
  title: string;
  /** Muted detail under the title, e.g. the synchronizer. */
  subtitle?: string;
  /** Right-aligned summary, usually a `Badge`. */
  aside?: ReactNode;
  /** Usually an `embedded` `DataTable` of the parent's children. */
  children: ReactNode;
}

/** A titled group of rows, such as a connection and its accounts. Stack several in a `Panel`. */
export function TreeGroup({ title, subtitle, aside, children }: TreeGroupProps) {
  return (
    <section className="tree-group">
      <div className="tree-parent">
        <div>
          <strong>{title}</strong>
          {subtitle && <small>{subtitle}</small>}
        </div>
        {aside}
      </div>
      {children}
    </section>
  );
}
