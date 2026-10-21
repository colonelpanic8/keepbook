import type { ReactNode } from "react";

export function SummaryCard({ children }: { children: ReactNode }) {
  return <section className="summary-card" aria-label="Account overview">{children}</section>;
}
