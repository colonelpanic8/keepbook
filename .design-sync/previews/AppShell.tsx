import {
  AppShell, Badge, Checkbox, ControlButton, DataTable, MetricCard, OperationStatus,
  PageToolbar, Panel, SplitButton, SummaryGrid, TreeGroup,
} from "keepbook-design";

const accounts = [
  { name: "Demo Checking", balance: "$8,250.44", tags: "cash", included: true },
  { name: "Demo Credit Card", balance: "-$1,436.20", tags: "card", included: true },
  { name: "Old Savings", balance: "$0.00", tags: "", included: false },
];

export const AccountsScreen = () => (
  <AppShell active="Accounts" repositories={["Personal", "Parents"]}>
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
    <OperationStatus busy message="Refreshing prices…" />
    <SummaryGrid>
      <MetricCard label="Net worth" value="$6,814.24" detail="2026-10-08" />
      <MetricCard label="Accounts" value="3" detail="3 total" />
      <MetricCard label="Connections" value="1" detail="Configured sources" />
    </SummaryGrid>
    <Panel title="Accounts" subtitle="3">
      <TreeGroup title="Demo Bank" subtitle="manual" aside={<Badge tone="positive">2/3 active</Badge>}>
        <DataTable className="account-table" columns={["Account", "Balance (USD)", "Status", "Tags", "Include"]}>
          {accounts.map((a) => (
            <div
              key={a.name}
              className={a.included ? "table-row account-row-with-toggle" : "table-row ignored-account-row account-row-with-toggle"}
            >
              <button className="account-row-main account-click-row" title="View graph">
                <strong>{a.name}</strong>
              </button>
              <span>{a.balance}</span>
              {a.included ? <Badge tone="positive">Active</Badge> : <Badge>Ignored</Badge>}
              <small>{a.tags}</small>
              <div className="account-override-cell">
                <Checkbox label="Include" className="account-include-toggle" checked={a.included} />
              </div>
            </div>
          ))}
        </DataTable>
      </TreeGroup>
    </Panel>
  </AppShell>
);
