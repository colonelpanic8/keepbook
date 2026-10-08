import { EmptyState } from "keepbook-design";

export const NoData = () => (
  <EmptyState title="No net worth breakdown" detail="Refresh balances to populate the stacked graph." />
);

export const Loading = () => <EmptyState loading title="Updating graph" detail="1Y / Weekly" />;

export const EmptyList = () => (
  <EmptyState compact title="No predictable recurring costs" detail="Include borderline patterns or lower confidence to widen the scan." />
);
