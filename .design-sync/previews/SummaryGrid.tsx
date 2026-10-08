import { MetricCard, SummaryGrid } from "keepbook-design";

export const AccountsSummary = () => (
  <SummaryGrid>
    <MetricCard label="Net worth" value="$6,814.24" detail="2026-10-08" />
    <MetricCard label="Accounts" value="3" detail="3 total" />
    <MetricCard label="Connections" value="1" detail="Configured sources" />
  </SummaryGrid>
);
