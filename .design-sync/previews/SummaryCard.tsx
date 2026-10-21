import { SummaryCard, MetricCard } from "keepbook-design";

export const Overview = () => (
  <SummaryCard>
    <MetricCard label="Net worth" value="$6,814.24" detail="2026-10-08" />
    <MetricCard label="Accounts" value="3" detail="2 active" />
    <MetricCard label="Connections" value="1" detail="" />
  </SummaryCard>
);
