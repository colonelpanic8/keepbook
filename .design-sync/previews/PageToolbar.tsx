import { ControlButton, OperationStatus, PageToolbar, SplitButton } from "keepbook-design";

export const AccountsActions = () => (
  <div>
    <PageToolbar>
      <ControlButton icon="git-branch">Git sync</ControlButton>
      <SplitButton
        primary
        icon="refresh"
        menuLabel="More refresh options"
        actions={[
          { value: "stale", label: "Refresh stale prices", detail: "Skip prices that are still fresh" },
          { value: "resync", label: "Resync data", detail: "Reload keepbook data from disk" },
        ]}
      >
        Refresh all prices
      </SplitButton>
    </PageToolbar>
    <OperationStatus message="Refreshed 14 prices." />
  </div>
);
