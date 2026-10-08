import { SpendingChart } from "keepbook-design";

const series = ["Housing", "Groceries", "Travel", "Utilities", "Dining", "Subscriptions"];
const month = (label: string, end: string, segments: [string, number, number][]) => ({
  label,
  startDate: `${label}-01`,
  endDate: `${label}-${end}`,
  total: segments.reduce((sum, [, value]) => sum + value, 0),
  transactionCount: segments.reduce((sum, [, , count]) => sum + count, 0),
  segments: segments.map(([key, value, transactionCount]) => ({ key, value, transactionCount })),
});
const buckets = [
  month("2026-04", "30", [["Housing", 2400, 1], ["Groceries", 312.4, 4], ["Utilities", 118.2, 1], ["Dining", 64.1, 2], ["Subscriptions", 15.99, 1]]),
  month("2026-05", "31", [["Housing", 2400, 1], ["Groceries", 170.41, 2], ["Utilities", 118.63, 1], ["Dining", 42.5, 1], ["Subscriptions", 15.99, 1]]),
  month("2026-06", "30", [["Housing", 2400, 1], ["Groceries", 171.44, 2], ["Travel", 330, 1], ["Utilities", 124.09, 1], ["Dining", 61.75, 2], ["Subscriptions", 15.99, 1]]),
  month("2026-07", "31", [["Housing", 2400, 1], ["Groceries", 88.42, 1], ["Dining", 38.2, 1], ["Subscriptions", 15.99, 1]]),
];

export const Monthly = () => (
  <SpendingChart buckets={buckets} series={series} total="$11,304.10" bucketLabel="Monthly" pinned={{ bucket: 2, key: "Travel" }} />
);

export const FocusedTag = () => (
  <SpendingChart buckets={buckets} series={series} total="$742.67" bucketLabel="Monthly" selected="Groceries" />
);
