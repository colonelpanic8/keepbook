import { SpendingBreakdown } from "keepbook-design";

const tags = [
  { tag: "Housing", total: 7200, count: 3 },
  { tag: "Groceries", total: 430.27, count: 5 },
  { tag: "Travel", total: 330, count: 1 },
  { tag: "Utilities", total: 242.72, count: 2 },
  { tag: "Dining", total: 142.45, count: 3 },
  { tag: "Subscriptions", total: 15.99, count: 1 },
];

export const ByTag = () => (
  <SpendingBreakdown tags={tags} total="$8,361.43" detail="15 transactions / 2025-10-08 to 2026-10-08" />
);

export const SelectedTag = () => (
  <SpendingBreakdown tags={tags} total="$8,361.43" detail="15 transactions / 2025-10-08 to 2026-10-08" selectedTag="Groceries" />
);
