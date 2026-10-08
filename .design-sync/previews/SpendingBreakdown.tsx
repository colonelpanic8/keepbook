import { SpendingBreakdown } from "keepbook-design";

const tags = [
  { key: "Housing", total: "-7200", transactionCount: 3 },
  { key: "Groceries", total: "-430.27", transactionCount: 5 },
  { key: "Travel", total: "-330", transactionCount: 1 },
  { key: "Utilities", total: "-242.72", transactionCount: 2 },
  { key: "Dining", total: "-142.45", transactionCount: 3 },
  { key: "Subscriptions", total: "-15.99", transactionCount: 1 },
];
const total = { label: "Total", value: "$8,361.43", detail: "15 transactions / 2025-10-08 to 2026-10-08" };

export const ByTag = () => <SpendingBreakdown tags={tags} totals={[total]} />;

export const FocusedPeriod = () => (
  <SpendingBreakdown
    tags={tags}
    totals={[total, { label: "2026-06", value: "$3,103.27", detail: "8 transactions", highlighted: true }]}
    selected="Groceries"
  />
);
