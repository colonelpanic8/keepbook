import { SplitButton } from "keepbook-design";

const actions = [
  { value: "stale", label: "Refresh stale prices", detail: "Skip prices that are still fresh" },
  { value: "resync", label: "Resync data", detail: "Reload keepbook data from disk" },
];

export const MenuOpen = () => (
  // Toolbars are right-aligned, so the menu opens toward the left.
  <div style={{ display: "flex", justifyContent: "flex-end", alignItems: "flex-start", minHeight: 180 }}>
    <SplitButton primary icon="refresh" menuLabel="More refresh options" actions={actions} defaultOpen>
      Refresh all prices
    </SplitButton>
  </div>
);

export const Closed = () => (
  <SplitButton primary icon="refresh" menuLabel="More refresh options" actions={actions}>
    Refresh all prices
  </SplitButton>
);

export const Busy = () => (
  <SplitButton primary busy menuLabel="More refresh options" actions={actions}>
    Refreshing
  </SplitButton>
);
