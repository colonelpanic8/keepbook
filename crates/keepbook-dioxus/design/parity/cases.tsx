// The React side of each markup parity case. Every entry renders the mirror
// with the same inputs as the Dioxus case of the same name in
// `tests/unit/parity_tests.rs`.
import type { ComponentType } from "react";
import {
  AppShell,
  Badge,
  Checkbox,
  ControlButton,
  DataTable,
  EmptyState,
  FilterChip,
  IconButton,
  InlineStatus,
  Legend,
  MetricCard,
  Modal,
  NetWorthChart,
  OperationStatus,
  PageToolbar,
  Panel,
  Progress,
  SegmentedControl,
  Select,
  SettingRow,
  SpendingBreakdown,
  SpendingChart,
  StatusStack,
  Spinner,
  SplitButton,
  SummaryGrid,
  Switch,
  TagPill,
  TextInput,
  ThemeOptions,
  ThemePicker,
  TreeGroup,
} from "../src";
import { renderIcon } from "../src/actions/icons";

export const cases: Record<string, ComponentType> = {
  "AppShell.basic": () => (
    <AppShell
      title="Keepbook"
      currency="USD"
      repositories={[
        { value: "personal", label: "Personal" },
        { value: "parents", label: "Parents", disabled: true },
      ]}
      repository="personal"
      navItems={[
        { label: "Accounts", icon: "wallet" },
        { label: "Spending", icon: "receipt" },
      ]}
      active="Accounts"
    >
      <p>Content</p>
    </AppShell>
  ),
  "Modal.basic": () => (
    <Modal
      title="Clone repository"
      headerActions={<IconButton label="Close" glyph="×" />}
      actions={<ControlButton danger>Cancel</ControlButton>}
    >
      <Progress label="Cloning…" />
    </Modal>
  ),
  "Modal.wide": () => (
    <Modal title="Group edit" wide>
      <p>Body</p>
    </Modal>
  ),
  "PageToolbar.basic": () => (
    <PageToolbar>
      <ControlButton icon="git-branch">Git sync</ControlButton>
      <ControlButton primary icon="refresh">
        Refresh prices
      </ControlButton>
    </PageToolbar>
  ),
  "Panel.basic": () => (
    <Panel title="Accounts" subtitle="2" actions={<ControlButton>Refresh</ControlButton>}>
      <p>Body</p>
    </Panel>
  ),
  "Panel.title_only": () => (
    <Panel title="Accounts" className="assets-panel">
      <p>Body</p>
    </Panel>
  ),
  "SummaryGrid.basic": () => (
    <SummaryGrid>
      <MetricCard label="Net worth" value="$6,814.24" detail="2026-10-08" />
      <MetricCard label="Accounts" value="3" detail="3 total" />
    </SummaryGrid>
  ),
  "ThemePicker.basic": () => <ThemePicker />,
  "ThemeOptions.fern_system": () => <ThemeOptions settings={{ palette: "fern", mode: "system" }} />,
  "ThemeOptions.dynamic_seed": () => (
    <ThemeOptions settings={{ palette: "dynamic", mode: "dark", seed: "#6750a4" }} />
  ),
  "ThemeOptions.dynamic_wallpaper": () => <ThemeOptions settings={{ palette: "dynamic", mode: "system" }} wallpaper />,
  "IconSvg.refresh": () => renderIcon("refresh"),
  "ControlButton.primary_icon": () => (
    <ControlButton primary icon="refresh" title="Fetch prices">
      Refresh prices
    </ControlButton>
  ),
  "ControlButton.busy": () => (
    <ControlButton busy icon="refresh">
      Refreshing
    </ControlButton>
  ),
  "ControlButton.danger_small": () => (
    <ControlButton danger small disabled>
      Dismiss
    </ControlButton>
  ),
  "ControlButton.selected": () => <ControlButton selected>Monthly</ControlButton>,
  "SplitButton.closed": () => (
    <SplitButton
      primary
      icon="refresh"
      menuLabel="More refresh options"
      actions={[{ value: "stale", label: "Refresh stale prices", detail: "Skip fresh prices" }]}
    >
      Refresh all prices
    </SplitButton>
  ),
  "IconButton.close": () => <IconButton label="Close" glyph="×" />,
  "IconButton.toggle": () => <IconButton label="Expand to edit" glyph="›" className="transaction-expand-toggle" disabled />,
  "SegmentedControl.range": () => (
    <SegmentedControl
      label="Range"
      options={[
        { value: "30d", label: "30D" },
        { value: "1y", label: "1Y" },
      ]}
      selected="1y"
    />
  ),
  "FilterChip.basic": () => <FilterChip label="Focused: Groceries" title="Clear the focused tag" />,
  "TextInput.labeled_number": () => <TextInput label="Min" kind="number" value="0.70" />,
  "TextInput.search": () => <TextInput kind="search" className="transaction-search-input" value="" placeholder="Filter titles" />,
  "TextInput.date_small": () => <TextInput label="Start" kind="date" small value="2026-07-10" min="2026-01-01" max="2026-12-31" />,
  "TextInput.multiline": () => <TextInput multiline value="" placeholder="Ask for a rule" disabled />,
  "Select.labeled": () => (
    <Select
      label="Sort"
      options={[
        { value: "cost", label: "Annual cost" },
        { value: "name", label: "Name" },
      ]}
      value="name"
    />
  ),
  "Select.bare": () => <Select options={[{ value: "auto", label: "Auto" }]} value="auto" disabled />,
  "Checkbox.checked": () => <Checkbox label="Include" className="account-include-toggle" checked />,
  "Checkbox.disabled": () => <Checkbox label="Stale only" checked={false} disabled />,
  "Switch.on": () => <Switch label="Start minimized to tray" checked />,
  "SettingRow.stacked": () => (
    <SettingRow title="Window decorations" description="Auto hides the title bar" stacked>
      <p>Control</p>
    </SettingRow>
  ),
  "Badge.neutral": () => <Badge>Not counted</Badge>,
  "Badge.positive": () => <Badge tone="positive">Active</Badge>,
  "Badge.negative": () => <Badge tone="negative">Dismissed</Badge>,
  "Badge.warning": () => <Badge tone="warning">Liability</Badge>,
  "TagPill.readonly": () => <TagPill tag="Groceries" />,
  "TagPill.removable": () => <TagPill tag="Dining" removable />,
  "TagPill.suggestion": () => <TagPill tag="Travel" suggestion disabled />,
  "MetricCard.basic": () => <MetricCard label="Net worth" value="$6,814.24" detail="2026-10-08" />,
  "DataTable.basic": () => (
    <DataTable className="account-table" columns={["Account", "Balance"]}>
      <div className="table-row">
        <span>
          <strong>Checking</strong>
        </span>
        <span>$8,250.44</span>
      </div>
    </DataTable>
  ),
  "TreeGroup.basic": () => (
    <TreeGroup title="Demo Bank" subtitle="manual" aside={<Badge tone="positive">2/2 active</Badge>}>
      <p>Rows</p>
    </TreeGroup>
  ),
  "OperationStatus.busy": () => <OperationStatus message="Refreshing prices…" busy />,
  "OperationStatus.done": () => <OperationStatus message="Refreshed 14 prices." busy={false} />,
  "OperationStatus.dismissible": () => <OperationStatus message="Data resynced." busy={false} onDismiss={() => {}} />,
  "StatusStack.basic": () => (
    <StatusStack>
      <OperationStatus message="Refreshing prices…" busy />
      <OperationStatus message="Refreshed 14 prices." busy={false} onDismiss={() => {}} />
    </StatusStack>
  ),
  "InlineStatus.basic": () => <InlineStatus title="Couldn't load" message="Try again." />,
  "EmptyState.basic": () => <EmptyState title="No assets" detail="Refresh balances." />,
  "EmptyState.loading": () => <EmptyState title="Updating graph" detail="1Y / Weekly" loading />,
  "EmptyState.compact": () => (
    <EmptyState title="No pending edits" detail="Nothing queued." compact className="spending-over-time-empty" />
  ),
  "Spinner.small": () => <Spinner size="small" />,
  "Spinner.medium": () => <Spinner />,
  "Spinner.large": () => <Spinner size="large" />,
  "Progress.busy": () => <Progress label="Cloning keepbook-data…" />,
  "Progress.done": () => <Progress label="Clone complete." busy={false} />,
  "Legend.static": () => (
    <Legend
      items={[
        { label: "Checking", color: "var(--series-1)" },
        { label: "Brokerage", color: "var(--series-2)", muted: true },
      ]}
    />
  ),
  "Legend.selectable": () => (
    <Legend
      className="spending-bar-legend"
      items={[
        { label: "Housing", color: "var(--series-1)" },
        { label: "Dining", color: "var(--series-2)" },
      ]}
      selected="Dining"
      onSelect={() => {}}
    />
  ),
  "NetWorthChart.basic": () => (
    <NetWorthChart
      data={[
        { date: "2026-01-01", value: 5000 },
        { date: "2026-02-01", value: 5600.5 },
        { date: "2026-03-01", value: 5400 },
      ]}
      currency="USD"
      emptyTitle="No net worth history"
      emptyDetail="Refresh balances."
      currentValueText="5400"
      changeText="+$400.00 (8.0%)"
    />
  ),
  "NetWorthChart.empty": () => (
    <NetWorthChart data={[]} currency="USD" emptyTitle="No net worth history" emptyDetail="Refresh balances." />
  ),
  "SpendingChart.basic": () => (
    <SpendingChart
      buckets={[
        {
          label: "2026-04",
          startDate: "2026-04-01",
          endDate: "2026-04-30",
          total: 700,
          transactionCount: 2,
          segments: [
            { key: "Housing", value: 600, transactionCount: 1 },
            { key: "Dining", value: 100, transactionCount: 1 },
          ],
        },
        {
          label: "2026-05",
          startDate: "2026-05-01",
          endDate: "2026-05-31",
          total: 250,
          transactionCount: 1,
          segments: [{ key: "Dining", value: 250, transactionCount: 1 }],
        },
      ]}
      series={["Housing", "Dining"]}
      total="$950.00"
      bucketLabel="Monthly"
    />
  ),
  "SpendingBreakdown.basic": () => (
    <SpendingBreakdown
      tags={[
        { key: "Housing", total: "-600", transactionCount: 1 },
        { key: "Dining", total: "-350", transactionCount: 2 },
      ]}
      currency="USD"
      totals={[{ label: "Total", value: "$950.00", detail: "3 transactions" }]}
      selected="Dining"
    />
  ),
};
