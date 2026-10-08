## Building with Keepbook

Keepbook is a local-first personal finance app: net worth, accounts, assets,
spending, and recurring costs. Its UI is dense and quiet, built for reading
numbers. These components mirror the app's real Dioxus components, and
`styles.css` is the app's real stylesheet.

**Setup.** No provider or wrapper is needed: load `styles.css` and the bundle,
and every component is styled. Start full screens with `AppShell`, which draws
the sidebar and the scrolling workspace. Pass page content as its children, in
this order: a `PageToolbar`, the toolbar's `OperationStatus` lines, a
`SummaryGrid` of three `MetricCard`s, then `Panel`s. For the dark theme, set
`data-theme="dark"` on `<html>`, or wrap any section in
`<Theme mode="dark">` to show it in the dark theme.

**Styling.** Compose with the components and don't restyle them. For your own
layout glue, use inline styles with the design tokens and never raw values:

| Need | Tokens |
|---|---|
| Space | `--sp-2`, `--sp-4`, `--sp-8`, `--sp-12`, `--sp-16`, `--sp-24` |
| Surfaces | `--color-bg`, `--color-surface`, `--color-surface-subtle`, `--color-surface-inset` |
| Text | `--color-text`, `--color-text-muted` |
| Lines | `--color-divider`, `--color-border`, `--color-border-strong` |
| Brand | `--color-primary`, `--color-accent-bg`, `--color-accent-fg` |
| Status | `--color-positive`, `--color-negative`, `--color-warning` (each also has `-bg`) |
| Type | `--fs-caption`, `--fs-small`, `--fs-body`, `--fs-title`, `--fs-display`; `--fw-medium`, `--fw-bold` |
| Shape | `--radius-sm`, `--radius-md`, `--radius-pill` |
| Data | `--series-1` … `--series-10`, assigned by position |

Wrap gains and losses in `<strong className="change-positive">` or
`<strong className="change-negative">`.

**Rules the app follows:**
- At most one `primary` button per toolbar.
- Use `SplitButton` for an action's less common variants, never a checkbox.
- Use `SegmentedControl` for every one-of-N choice.
- Show empty data with `EmptyState`.
- Chart money with `NetWorthChart`, `SpendingChart` (stacked by tag over time), and `SpendingBreakdown` (donut and tag rows).
- Show missing values as an em dash (—).
- Write copy in sentence case: "Refresh prices", not "Refresh Prices".
- Format money as `-$1,436.20` and dates as `2026-10-08`.
- Status messages use a present participle while working ("Refreshing prices…"), then the past tense with a count ("Refreshed 14 prices.").

**Where the truth lives:**
- `guidelines/README.md`: the full design guide, covering content, color roles, layout, and charts.
- `guidelines/interaction-states.md`: loading and failure behavior.
- Each component's `.prompt.md`: its examples.

**Example:**

```jsx
const { AppShell, PageToolbar, ControlButton, SplitButton, OperationStatus, SummaryGrid, MetricCard, Panel, NetWorthChart } = window.Keepbook;

<AppShell active="Net Worth">
  <PageToolbar>
    <ControlButton icon="git-branch">Git sync</ControlButton>
    <SplitButton primary icon="refresh" menuLabel="More refresh options"
      actions={[{ value: "stale", label: "Refresh stale prices", detail: "Skip prices that are still fresh" }]}>
      Refresh all prices
    </SplitButton>
  </PageToolbar>
  <OperationStatus message="Refreshed 14 prices." />
  <SummaryGrid>
    <MetricCard label="Net worth" value="$6,814.24" detail="2026-10-08" />
    <MetricCard label="Accounts" value="3" detail="3 total" />
    <MetricCard label="Connections" value="1" detail="Configured sources" />
  </SummaryGrid>
  <Panel title="Net worth" subtitle="1Y / Weekly">
    <NetWorthChart points={[{ date: "2026-01-08", value: 5920.4 }, { date: "2026-10-08", value: 6814.24 }]}
      change="+$893.84 (15.1%)" changeTone="positive" />
  </Panel>
</AppShell>
```
