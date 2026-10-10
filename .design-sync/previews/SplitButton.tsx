import { SplitButton } from "keepbook-design";

const actions = [
  { value: "stale", label: "Refresh stale prices", detail: "Skip prices that are still fresh" },
  { value: "resync", label: "Resync data", detail: "Reload keepbook data from disk" },
  { value: "git", label: "Git sync", detail: "Pull and push the data repository" },
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
  <SplitButton primary icon="refresh" menuLabel="More refresh options" actions={actions} feedback={{ tone: "busy", label: "Syncing…" }}>
    Refresh all prices
  </SplitButton>
);

export const Failed = () => (
  <SplitButton
    primary
    icon="refresh"
    menuLabel="More refresh options"
    actions={actions}
    feedback={{ tone: "failed", label: "Sync failed", detail: "Git sync failed: remote rejected the push." }}
  >
    Refresh all prices
  </SplitButton>
);
