import {
  AppShell, Badge, Checkbox, ControlButton, DataTable, MetricCard, OperationStatus,
  PageToolbar, Panel, SplitButton, SummaryGrid, TreeGroup,
} from "keepbook-design";

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
        <DataTable
          columns={[
            { label: "Account", width: "minmax(200px, 1.2fr)" },
            { label: "Balance (USD)", width: "minmax(150px, 0.7fr)" },
            { label: "Status", width: "minmax(110px, 0.4fr)" },
            { label: "Tags", width: "minmax(180px, 1fr)" },
            { label: "Include", width: "minmax(132px, 0.6fr)" },
          ]}
          rows={[
            [<strong>Demo Checking</strong>, <strong>$8,250.44</strong>, <Badge tone="positive">Active</Badge>, <small>cash</small>, <Checkbox label="Included" checked />],
            [<strong>Demo Credit Card</strong>, <strong>-$1,436.20</strong>, <Badge tone="positive">Active</Badge>, <small>card</small>, <Checkbox label="Included" checked />],
            [<strong>Old Savings</strong>, <strong>$0.00</strong>, <Badge>Ignored</Badge>, <small>—</small>, <Checkbox label="Included" />],
          ]}
          mutedRows={[2]}
        />
      </TreeGroup>
    </Panel>
  </AppShell>
);
