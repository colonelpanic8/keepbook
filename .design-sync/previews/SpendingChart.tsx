import { SpendingChart } from "keepbook-design";

const tags = ["Housing", "Groceries", "Travel", "Utilities", "Dining", "Subscriptions"];
const buckets = [
  { label: "2026-04", segments: [{ tag: "Housing", value: 2400 }, { tag: "Groceries", value: 312.4 }, { tag: "Utilities", value: 118.2 }, { tag: "Dining", value: 64.1 }, { tag: "Subscriptions", value: 15.99 }] },
  { label: "2026-05", segments: [{ tag: "Housing", value: 2400 }, { tag: "Groceries", value: 170.41 }, { tag: "Utilities", value: 118.63 }, { tag: "Dining", value: 42.5 }, { tag: "Subscriptions", value: 15.99 }] },
  { label: "2026-06", segments: [{ tag: "Housing", value: 2400 }, { tag: "Groceries", value: 171.44 }, { tag: "Travel", value: 330 }, { tag: "Utilities", value: 124.09 }, { tag: "Dining", value: 61.75 }, { tag: "Subscriptions", value: 15.99 }] },
  { label: "2026-07", segments: [{ tag: "Housing", value: 2400 }, { tag: "Groceries", value: 88.42 }, { tag: "Dining", value: 38.2 }, { tag: "Subscriptions", value: 15.99 }] },
];

export const Monthly = () => (
  <SpendingChart buckets={buckets} tags={tags} total="$8,361.43" bucket="Monthly" hover={{ bucket: 2, tag: "Travel" }} />
);

export const FocusedTag = () => (
  <SpendingChart buckets={buckets} tags={tags} total="$742.67" bucket="Monthly" selectedTag="Groceries" />
);
