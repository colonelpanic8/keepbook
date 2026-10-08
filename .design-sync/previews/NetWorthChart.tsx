import { NetWorthChart } from "keepbook-design";

const points = [
  { date: "2025-10-08", value: 5610.14 },
  { date: "2025-11-08", value: 5702.3 },
  { date: "2025-12-08", value: 5655.91 },
  { date: "2026-01-08", value: 5920.4 },
  { date: "2026-02-08", value: 6011.77 },
  { date: "2026-03-08", value: 6088.02 },
  { date: "2026-04-08", value: 6240.65 },
  { date: "2026-05-08", value: 6402.18 },
  { date: "2026-06-08", value: 6512.8 },
  { date: "2026-07-08", value: 6630.55 },
  { date: "2026-08-08", value: 6702.0 },
  { date: "2026-10-08", value: 6814.24 },
];

export const OneYear = () => (
  <NetWorthChart points={points} change="+$1,204.10 (21.5%)" changeTone="positive" hoverIndex={4} />
);

export const Empty = () => <NetWorthChart points={[]} />;
