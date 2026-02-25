# Frontend Charts Redesign

## Overview

Redesign the keepbook frontend from scratch around financial charts and insights. Replace the current placeholder screens with a charts-first experience featuring net worth and spending time-series graphs with configurable time scales.

## Technology Stack

- **Framework**: React Native + Expo (existing)
- **Charting**: Victory Native + @shopify/react-native-skia
- **Platforms**: Android, iOS, Web (all equally)
- **Animations**: react-native-reanimated (existing)
- **Touch**: react-native-gesture-handler

## Navigation

4 bottom tabs:

| Tab | Purpose |
|-----|---------|
| **Net Worth** | Portfolio value over time (line chart with area fill) |
| **Spending** | Spending trends over time (bar chart, optional grouping) |
| **Accounts** | Account list grouped by connection |
| **Settings** | Git sync config, data directory selection |

## Screen Designs

### Net Worth Tab (primary screen)

**Layout (top to bottom):**
1. Time range selector: `[W] [M] [3M] [6M] [Y] [ALL]`
2. Line chart with area fill showing portfolio total value over time
3. Summary stats: current total, absolute change, percentage change for selected period

**Data source:** `portfolioHistory(dataDir, {start, end, granularity})`

**Time range → query mapping:**

| Range | Lookback | Granularity |
|-------|----------|-------------|
| W | 7 days | daily |
| M | 1 month | daily |
| 3M | 3 months | weekly |
| 6M | 6 months | weekly |
| Y | 1 year | monthly |
| ALL | all data | monthly |

**Interactions:**
- Press on chart shows tooltip with exact value and date
- Time range switch animates chart transition

### Spending Tab

**Layout (top to bottom):**
1. Time range selector (same shared component)
2. Bar chart: spending per time bucket, stacked when grouped
3. Summary stats: total spending, average per period, transaction count
4. Group-by toggle: `[None] [Category] [Merchant] [Account]`

**Data source:** `spending(dataDir, {start, end, period, groupBy, direction: "outflow"})`

**Time range → period mapping:**

| Range | Period |
|-------|--------|
| W | daily |
| M | daily |
| 3M | weekly |
| 6M | monthly |
| Y | monthly |
| ALL | monthly |

**Interactions:**
- Tap a bar to see period breakdown
- Group-by toggle re-renders with stacked colors + legend

### Accounts Tab

- List of accounts grouped by connection
- Each row: name, latest balance, asset type, last sync time
- Tap account to see transaction history (scrollable list)

### Settings Tab

- Git sync configuration (host, repo, branch, token)
- Data directory selection (demo vs git)
- Sync button and status display
- Migrated from current Tab 2

## Shared Components

### TimeRangeSelector
Horizontal button strip following Railbird's `DateRangeEnum` pattern. Highlighted button for active range. Returns selected range enum value.

### ChartContainer
Wrapper handling loading spinner, error state, and empty state for any chart. Based on Railbird's `ChartView` pattern.

### Chart color theme
Centralized in `chart-colors.ts` with accent/secondary colors and opacity presets for fills, lines, and highlighted states.

## Data Layer

Extend `KeepbookNativeBackend` with new methods that call the keepbook TS library directly:

```typescript
portfolioHistory(dataDir: string, opts: {start?: string, end?: string, granularity?: string}): Promise<string>
spending(dataDir: string, opts: {start?: string, end?: string, period?: string, groupBy?: string, direction?: string}): Promise<string>
portfolioSnapshot(dataDir: string, opts: {date?: string}): Promise<string>
```

Each returns JSON string matching the CLI output format. The frontend parses and transforms to Victory data format.

## File Structure

```
frontend/
  app/(tabs)/
    index.tsx          -> Net Worth tab
    spending.tsx       -> Spending tab
    accounts.tsx       -> Accounts tab
    settings.tsx       -> Settings tab
    _layout.tsx        -> Updated 4-tab navigator
  app/
    account/[id].tsx   -> Account detail / transactions
  components/
    charts/
      TimeRangeSelector.tsx
      ChartContainer.tsx
      NetWorthChart.tsx
      SpendingChart.tsx
      chart-colors.ts
      tooltip.tsx
  modules/keepbook-native/
    src/KeepbookNativeBackend.ts  -> Extended with portfolio/spending methods
```

## New Dependencies

- `victory-native` ~41.x
- `@shopify/react-native-skia` ~2.x
- `react-native-gesture-handler` (if not already present)

## Implementation Order

1. Add charting dependencies (victory-native, skia)
2. Build shared components (TimeRangeSelector, ChartContainer, chart-colors)
3. Extend KeepbookNativeBackend with portfolio/spending methods
4. Build Net Worth tab with line chart
5. Build Spending tab with bar chart
6. Rebuild Accounts tab as grouped list
7. Migrate Settings tab from current Tab 2
8. Update tab navigator layout
9. Test on Android, iOS, Web
