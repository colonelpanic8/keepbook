import { PageToolbar, SplitButton } from "keepbook-design";

const actions = [
  { value: "stale", label: "Refresh stale prices", detail: "Skip prices that are still fresh" },
  { value: "resync", label: "Resync data", detail: "Reload keepbook data from disk" },
  { value: "git", label: "Git sync", detail: "Pull and push the data repository" },
];

export const AccountsActions = () => (
  <PageToolbar>
    <SplitButton primary icon="refresh" menuLabel="More refresh options" actions={actions}>
      Refresh all prices
    </SplitButton>
  </PageToolbar>
);

export const AfterRefresh = () => (
  <PageToolbar>
    <SplitButton
      primary
      icon="refresh"
      menuLabel="More refresh options"
      actions={actions}
      feedback={{ tone: "done", label: "Prices refreshed", detail: "Refreshed 14 prices." }}
    >
      Refresh all prices
    </SplitButton>
  </PageToolbar>
);
