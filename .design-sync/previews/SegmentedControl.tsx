import { SegmentedControl } from "keepbook-design";

export const Range = () => (
  <SegmentedControl
    label="Range"
    selected="1y"
    options={[
      { value: "30d", label: "30D" },
      { value: "90d", label: "90D" },
      { value: "6m", label: "6M" },
      { value: "1y", label: "1Y" },
      { value: "max", label: "Max" },
    ]}
  />
);

export const Bucket = () => (
  <SegmentedControl
    label="Bucket"
    selected="monthly"
    options={[
      { value: "daily", label: "Daily" },
      { value: "weekly", label: "Weekly" },
      { value: "monthly", label: "Monthly" },
      { value: "quarterly", label: "Quarterly" },
      { value: "yearly", label: "Yearly" },
    ]}
  />
);

export const TwoOptions = () => (
  <SegmentedControl
    label="View"
    selected="tags"
    options={[
      { value: "tags", label: "Tags" },
      { value: "matches", label: "String matches" },
    ]}
  />
);
