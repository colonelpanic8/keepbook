# Frontend Charts Redesign Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Redesign the keepbook mobile/web frontend around financial charts — net worth and spending time-series graphs with configurable time scales.

**Architecture:** Replace the current 2-tab placeholder app with a 4-tab charts-first experience (Net Worth, Spending, Accounts, Settings). Use Victory Native + Skia for cross-platform charting. Extend git sync to fetch full financial data from GitHub, build AsyncStorage-backed storage adapters that implement the keepbook TS library's Storage/MarketDataStore interfaces, and call the existing business logic functions (portfolioHistory, spendingReport) directly. This avoids duplicating calculation code.

**Tech Stack:** React Native/Expo, Victory Native, @shopify/react-native-skia, react-native-reanimated, TypeScript

**Working directory:** `/home/imalison/Projects/keepbook/frontend`

**TS library directory:** `/home/imalison/Projects/keepbook/ts/src`

---

## Phase 1: Chart Infrastructure

### Task 1: Install Charting Dependencies

**Files:**
- Modify: `frontend/package.json`

**Step 1: Install victory-native and skia**

```bash
cd /home/imalison/Projects/keepbook/frontend
yarn add victory-native @shopify/react-native-skia react-native-gesture-handler
```

Note: react-native-reanimated is already installed. react-native-gesture-handler is a transitive dep of expo-router but we need it as a direct dependency for Victory.

**Step 2: Install TS library deps needed by business logic**

```bash
cd /home/imalison/Projects/keepbook/frontend
yarn add decimal.js date-fns uuid
yarn add -D @types/uuid
```

These are needed to import the keepbook TS library's portfolio/spending functions.

**Step 3: Verify app still starts**

```bash
cd /home/imalison/Projects/keepbook/frontend
yarn web
```

Expected: App launches without errors (press Ctrl+C to stop after confirming).

**Step 4: Commit**

```bash
git add frontend/package.json frontend/yarn.lock
git commit -m "feat(frontend): add charting dependencies (victory-native, skia)"
```

---

### Task 2: Configure Metro to Resolve TS Library Imports

The keepbook TS library at `../ts/src/` contains the business logic we need (portfolioHistory, spendingReport). We need Metro bundler to resolve imports from that directory.

**Files:**
- Create: `frontend/metro.config.js`
- Modify: `frontend/tsconfig.json` (add path alias)

**Step 1: Create Metro config with watchFolders**

Create `frontend/metro.config.js`:

```javascript
const { getDefaultConfig } = require('expo/metro-config');
const path = require('path');

const config = getDefaultConfig(__dirname);

// Allow importing from the TS library source
const tsLibRoot = path.resolve(__dirname, '../ts/src');

config.watchFolders = [tsLibRoot];

// Resolve .ts/.tsx files in the TS library (imports use .js extensions)
const originalResolveRequest = config.resolver.resolveRequest;
config.resolver.resolveRequest = (context, moduleName, platform) => {
  // Redirect @keepbook/* imports to the TS library source
  if (moduleName.startsWith('@keepbook/')) {
    const relPath = moduleName.replace('@keepbook/', '');
    const resolved = path.resolve(tsLibRoot, relPath);
    return context.resolveRequest(context, resolved, platform);
  }
  if (originalResolveRequest) {
    return originalResolveRequest(context, moduleName, platform);
  }
  return context.resolveRequest(context, moduleName, platform);
};

// Ensure node_modules from the main project are available to TS library imports
config.resolver.nodeModulesPaths = [
  path.resolve(__dirname, 'node_modules'),
];

module.exports = config;
```

**Step 2: Add path alias to tsconfig.json**

Add to `compilerOptions.paths`:

```json
{
  "compilerOptions": {
    "paths": {
      "@/*": ["./*"],
      "@keepbook/*": ["../ts/src/*"]
    }
  }
}
```

**Step 3: Verify a simple import works**

Create a quick test by importing a type from the TS library in any component and checking the app still builds. If the `.js` extension imports cause issues, add `resolver.sourceExts` to include `ts` and `tsx` if not already present, and configure the resolver to strip `.js` from import specifiers when resolving within the TS library directory.

**Step 4: Commit**

```bash
git add frontend/metro.config.js frontend/tsconfig.json
git commit -m "feat(frontend): configure Metro to resolve keepbook TS library imports"
```

---

### Task 3: Create Chart Theme and Shared Components

**Files:**
- Create: `frontend/components/charts/chart-colors.ts`
- Create: `frontend/components/charts/TimeRangeSelector.tsx`
- Create: `frontend/components/charts/ChartContainer.tsx`

**Step 1: Create chart color constants**

Create `frontend/components/charts/chart-colors.ts`:

```typescript
// Chart accent color (blue-green for net worth growth)
export const CHART_ACCENT_RGB = { r: 46, g: 204, b: 113 };  // Emerald green
export const CHART_NEGATIVE_RGB = { r: 231, g: 76, b: 60 };  // Red for negative
export const CHART_BAR_RGB = { r: 52, g: 152, b: 219 };      // Blue for spending bars

function rgba(c: { r: number; g: number; b: number }, a: number): string {
  return `rgba(${c.r}, ${c.g}, ${c.b}, ${a})`;
}

export const chartColors = {
  accent: rgba(CHART_ACCENT_RGB, 1),
  accentDim: rgba(CHART_ACCENT_RGB, 0.3),
  accentFill: rgba(CHART_ACCENT_RGB, 0.15),
  negative: rgba(CHART_NEGATIVE_RGB, 1),
  negativeDim: rgba(CHART_NEGATIVE_RGB, 0.3),
  bar: rgba(CHART_BAR_RGB, 1),
  barDim: rgba(CHART_BAR_RGB, 0.6),
  gridLine: 'rgba(255, 255, 255, 0.1)',
  label: 'rgba(255, 255, 255, 0.6)',
};

export const chartDefaults = {
  height: 300,
  domainPadding: { top: 30, bottom: 10, left: 10, right: 10 },
};
```

**Step 2: Create TimeRangeSelector component**

Create `frontend/components/charts/TimeRangeSelector.tsx` following Railbird's pattern:

```tsx
import React from 'react';
import { Pressable, StyleSheet, View } from 'react-native';
import { Text } from '@/components/Themed';

export enum TimeRange {
  WEEK = 'week',
  MONTH = 'month',
  THREE_MONTHS = 'threeMonths',
  SIX_MONTHS = 'sixMonths',
  YEAR = 'year',
  ALL = 'all',
}

const RANGES: { label: string; value: TimeRange }[] = [
  { label: 'W', value: TimeRange.WEEK },
  { label: 'M', value: TimeRange.MONTH },
  { label: '3M', value: TimeRange.THREE_MONTHS },
  { label: '6M', value: TimeRange.SIX_MONTHS },
  { label: 'Y', value: TimeRange.YEAR },
  { label: 'ALL', value: TimeRange.ALL },
];

interface TimeRangeSelectorProps {
  selected: TimeRange;
  onSelect: (range: TimeRange) => void;
}

export function TimeRangeSelector({ selected, onSelect }: TimeRangeSelectorProps) {
  return (
    <View style={styles.container}>
      {RANGES.map(({ label, value }) => (
        <Pressable
          key={value}
          onPress={() => onSelect(value)}
          style={[styles.button, value === selected && styles.buttonActive]}
        >
          <Text style={[styles.label, value === selected && styles.labelActive]}>
            {label}
          </Text>
        </Pressable>
      ))}
    </View>
  );
}

/** Map TimeRange to {lookbackDays, granularity, period} for backend queries */
export function timeRangeToQuery(range: TimeRange): {
  lookbackDays: number | null;
  granularity: string;
  period: string;
} {
  switch (range) {
    case TimeRange.WEEK:
      return { lookbackDays: 7, granularity: 'daily', period: 'daily' };
    case TimeRange.MONTH:
      return { lookbackDays: 30, granularity: 'daily', period: 'daily' };
    case TimeRange.THREE_MONTHS:
      return { lookbackDays: 90, granularity: 'weekly', period: 'weekly' };
    case TimeRange.SIX_MONTHS:
      return { lookbackDays: 180, granularity: 'weekly', period: 'monthly' };
    case TimeRange.YEAR:
      return { lookbackDays: 365, granularity: 'monthly', period: 'monthly' };
    case TimeRange.ALL:
      return { lookbackDays: null, granularity: 'monthly', period: 'monthly' };
  }
}

const styles = StyleSheet.create({
  container: {
    flexDirection: 'row',
    justifyContent: 'center',
    gap: 8,
    paddingVertical: 12,
    paddingHorizontal: 16,
  },
  button: {
    paddingHorizontal: 14,
    paddingVertical: 8,
    borderRadius: 8,
    backgroundColor: 'rgba(255, 255, 255, 0.08)',
  },
  buttonActive: {
    backgroundColor: 'rgba(52, 152, 219, 0.9)',
  },
  label: {
    fontSize: 13,
    fontWeight: '600',
    color: 'rgba(255, 255, 255, 0.5)',
  },
  labelActive: {
    color: '#fff',
  },
});
```

**Step 3: Create ChartContainer component**

Create `frontend/components/charts/ChartContainer.tsx`:

```tsx
import React from 'react';
import { ActivityIndicator, StyleSheet } from 'react-native';
import { Text, View } from '@/components/Themed';

interface ChartContainerProps {
  loading: boolean;
  error: string | null;
  height?: number;
  children: React.ReactNode;
}

export function ChartContainer({ loading, error, height = 300, children }: ChartContainerProps) {
  if (loading) {
    return (
      <View style={[styles.center, { height }]}>
        <ActivityIndicator size="large" />
      </View>
    );
  }

  if (error) {
    return (
      <View style={[styles.center, { height }]}>
        <Text style={styles.errorText}>{error}</Text>
      </View>
    );
  }

  return <>{children}</>;
}

const styles = StyleSheet.create({
  center: {
    justifyContent: 'center',
    alignItems: 'center',
  },
  errorText: {
    color: '#e74c3c',
    textAlign: 'center',
    paddingHorizontal: 24,
  },
});
```

**Step 4: Commit**

```bash
git add frontend/components/charts/
git commit -m "feat(frontend): add chart theme, TimeRangeSelector, and ChartContainer components"
```

---

## Phase 2: Chart Screens (Mock Data First)

### Task 4: Build Net Worth Chart Component

**Files:**
- Create: `frontend/components/charts/NetWorthChart.tsx`

Build the Victory Native line chart with area fill. Start with mock data so the chart renders immediately.

**Step 1: Create NetWorthChart with mock data**

Create `frontend/components/charts/NetWorthChart.tsx`:

```tsx
import React from 'react';
import { StyleSheet, View as RNView } from 'react-native';
import { CartesianChart, Line, Area, useChartPressState } from 'victory-native';
import { useFont } from '@shopify/react-native-skia';
import { Text } from '@/components/Themed';
import { chartColors, chartDefaults } from './chart-colors';

export interface NetWorthDataPoint {
  date: string;    // YYYY-MM-DD
  value: number;   // portfolio total value
}

interface NetWorthChartProps {
  data: NetWorthDataPoint[];
  height?: number;
}

export function NetWorthChart({ data, height = chartDefaults.height }: NetWorthChartProps) {
  const font = useFont(require('../../assets/fonts/SpaceMono-Regular.ttf'), 11);
  const { state, isActive } = useChartPressState({ x: '', y: { value: 0 } });

  if (data.length === 0) {
    return (
      <RNView style={[styles.empty, { height }]}>
        <Text style={styles.emptyText}>No data available</Text>
      </RNView>
    );
  }

  const chartData = data.map((d) => ({
    x: d.date,
    value: d.value,
  }));

  const formatYLabel = (v: number) => {
    if (v >= 1_000_000) return `$${(v / 1_000_000).toFixed(1)}M`;
    if (v >= 1_000) return `$${(v / 1_000).toFixed(0)}K`;
    return `$${v.toFixed(0)}`;
  };

  const formatXLabel = (label: string) => {
    // Show abbreviated date: "Jan 15", "Feb 3", etc.
    const d = new Date(label + 'T00:00:00Z');
    return d.toLocaleDateString('en-US', { month: 'short', day: 'numeric', timeZone: 'UTC' });
  };

  return (
    <RNView style={{ height }}>
      <CartesianChart
        data={chartData}
        xKey="x"
        yKeys={['value']}
        domainPadding={chartDefaults.domainPadding}
        chartPressState={state}
        xAxis={{
          font,
          labelColor: chartColors.label,
          lineColor: 'transparent',
          tickCount: 5,
          formatXLabel,
        }}
        yAxis={[{
          font,
          labelColor: chartColors.label,
          lineColor: chartColors.gridLine,
          formatYLabel,
          tickCount: 4,
        }]}
      >
        {({ points, chartBounds }) => (
          <>
            <Area
              points={points.value}
              y0={chartBounds.bottom}
              color={chartColors.accentFill}
              animate={{ type: 'timing', duration: 500 }}
            />
            <Line
              points={points.value}
              color={chartColors.accent}
              strokeWidth={2}
              animate={{ type: 'timing', duration: 500 }}
            />
          </>
        )}
      </CartesianChart>
    </RNView>
  );
}

const styles = StyleSheet.create({
  empty: {
    justifyContent: 'center',
    alignItems: 'center',
  },
  emptyText: {
    color: 'rgba(255, 255, 255, 0.4)',
  },
});
```

**Step 2: Verify it renders**

This will be tested when we build the Net Worth tab screen (Task 6).

**Step 3: Commit**

```bash
git add frontend/components/charts/NetWorthChart.tsx
git commit -m "feat(frontend): add NetWorthChart component with Victory Native line chart"
```

---

### Task 5: Build Spending Chart Component

**Files:**
- Create: `frontend/components/charts/SpendingChart.tsx`

**Step 1: Create SpendingChart with bar rendering**

Create `frontend/components/charts/SpendingChart.tsx`:

```tsx
import React from 'react';
import { StyleSheet, View as RNView } from 'react-native';
import { CartesianChart, Bar, useChartPressState } from 'victory-native';
import { useFont } from '@shopify/react-native-skia';
import { Text } from '@/components/Themed';
import { chartColors, chartDefaults } from './chart-colors';

export interface SpendingDataPoint {
  label: string;           // period label (e.g. "Jan", "Week 3")
  total: number;           // spending total for the period
}

interface SpendingChartProps {
  data: SpendingDataPoint[];
  height?: number;
}

export function SpendingChart({ data, height = chartDefaults.height }: SpendingChartProps) {
  const font = useFont(require('../../assets/fonts/SpaceMono-Regular.ttf'), 11);
  const { state } = useChartPressState({ x: '', y: { total: 0 } });

  if (data.length === 0) {
    return (
      <RNView style={[styles.empty, { height }]}>
        <Text style={styles.emptyText}>No spending data</Text>
      </RNView>
    );
  }

  const chartData = data.map((d) => ({
    x: d.label,
    total: d.total,
  }));

  const formatYLabel = (v: number) => {
    if (v >= 1_000_000) return `$${(v / 1_000_000).toFixed(1)}M`;
    if (v >= 1_000) return `$${(v / 1_000).toFixed(0)}K`;
    return `$${v.toFixed(0)}`;
  };

  return (
    <RNView style={{ height }}>
      <CartesianChart
        data={chartData}
        xKey="x"
        yKeys={['total']}
        domainPadding={{ ...chartDefaults.domainPadding, left: 20, right: 20 }}
        chartPressState={state}
        xAxis={{
          font,
          labelColor: chartColors.label,
          lineColor: 'transparent',
          tickCount: Math.min(data.length, 8),
        }}
        yAxis={[{
          font,
          labelColor: chartColors.label,
          lineColor: chartColors.gridLine,
          formatYLabel,
          tickCount: 4,
        }]}
      >
        {({ points, chartBounds }) => (
          <Bar
            points={points.total}
            chartBounds={chartBounds}
            color={chartColors.bar}
            roundedCorners={{ topLeft: 4, topRight: 4 }}
            animate={{ type: 'timing', duration: 500 }}
          />
        )}
      </CartesianChart>
    </RNView>
  );
}

const styles = StyleSheet.create({
  empty: {
    justifyContent: 'center',
    alignItems: 'center',
  },
  emptyText: {
    color: 'rgba(255, 255, 255, 0.4)',
  },
});
```

**Step 2: Commit**

```bash
git add frontend/components/charts/SpendingChart.tsx
git commit -m "feat(frontend): add SpendingChart component with Victory Native bar chart"
```

---

### Task 6: Build Net Worth Tab Screen

**Files:**
- Rewrite: `frontend/app/(tabs)/index.tsx` (replace current home screen)

**Step 1: Rewrite index.tsx as Net Worth screen**

Replace the current connections/accounts list with the Net Worth chart screen. Use mock data initially — the data layer will be wired up in Phase 3.

```tsx
import React, { useMemo, useState } from 'react';
import { ScrollView, StyleSheet } from 'react-native';
import { Text, View } from '@/components/Themed';
import { NetWorthChart, type NetWorthDataPoint } from '@/components/charts/NetWorthChart';
import { TimeRangeSelector, TimeRange, timeRangeToQuery } from '@/components/charts/TimeRangeSelector';
import { ChartContainer } from '@/components/charts/ChartContainer';

// Mock data generator — will be replaced with real data in Phase 3
function generateMockNetWorthData(lookbackDays: number | null): NetWorthDataPoint[] {
  const days = lookbackDays ?? 365;
  const points: NetWorthDataPoint[] = [];
  let value = 100000;
  const now = new Date();
  for (let i = days; i >= 0; i--) {
    const d = new Date(now);
    d.setDate(d.getDate() - i);
    value += (Math.random() - 0.45) * 1000; // slight upward trend
    points.push({
      date: d.toISOString().slice(0, 10),
      value: Math.max(value, 0),
    });
  }
  return points;
}

export default function NetWorthScreen() {
  const [timeRange, setTimeRange] = useState(TimeRange.THREE_MONTHS);
  const query = timeRangeToQuery(timeRange);

  const data = useMemo(() => generateMockNetWorthData(query.lookbackDays), [query.lookbackDays]);

  const currentValue = data.length > 0 ? data[data.length - 1].value : 0;
  const startValue = data.length > 0 ? data[0].value : 0;
  const absoluteChange = currentValue - startValue;
  const percentChange = startValue !== 0 ? (absoluteChange / startValue) * 100 : 0;
  const isPositive = absoluteChange >= 0;

  return (
    <ScrollView style={styles.container}>
      <TimeRangeSelector selected={timeRange} onSelect={setTimeRange} />

      <ChartContainer loading={false} error={null}>
        <NetWorthChart data={data} />
      </ChartContainer>

      <View style={styles.stats}>
        <Text style={styles.totalValue}>
          ${currentValue.toLocaleString('en-US', { minimumFractionDigits: 2, maximumFractionDigits: 2 })}
        </Text>
        <Text style={[styles.change, { color: isPositive ? '#2ecc71' : '#e74c3c' }]}>
          {isPositive ? '+' : ''}${absoluteChange.toLocaleString('en-US', { minimumFractionDigits: 2, maximumFractionDigits: 2 })}
          {' '}({isPositive ? '+' : ''}{percentChange.toFixed(2)}%)
        </Text>
      </View>
    </ScrollView>
  );
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
  },
  stats: {
    paddingHorizontal: 20,
    paddingVertical: 16,
    alignItems: 'center',
    gap: 4,
  },
  totalValue: {
    fontSize: 28,
    fontWeight: 'bold',
  },
  change: {
    fontSize: 16,
    fontWeight: '500',
  },
});
```

**Step 2: Verify the screen renders**

```bash
cd /home/imalison/Projects/keepbook/frontend && yarn web
```

Navigate to the app and verify the Net Worth tab shows a line chart with mock data and the time range selector works.

**Step 3: Commit**

```bash
git add frontend/app/\(tabs\)/index.tsx
git commit -m "feat(frontend): build Net Worth tab with line chart and time range selector"
```

---

### Task 7: Build Spending Tab Screen

**Files:**
- Create: `frontend/app/(tabs)/spending.tsx`

**Step 1: Create spending.tsx**

```tsx
import React, { useMemo, useState } from 'react';
import { Pressable, ScrollView, StyleSheet } from 'react-native';
import { Text, View } from '@/components/Themed';
import { SpendingChart, type SpendingDataPoint } from '@/components/charts/SpendingChart';
import { TimeRangeSelector, TimeRange, timeRangeToQuery } from '@/components/charts/TimeRangeSelector';
import { ChartContainer } from '@/components/charts/ChartContainer';

type GroupBy = 'none' | 'category' | 'merchant' | 'account';

const GROUP_OPTIONS: { label: string; value: GroupBy }[] = [
  { label: 'None', value: 'none' },
  { label: 'Category', value: 'category' },
  { label: 'Merchant', value: 'merchant' },
  { label: 'Account', value: 'account' },
];

// Mock data — will be replaced in Phase 3
function generateMockSpendingData(lookbackDays: number | null, period: string): SpendingDataPoint[] {
  const days = lookbackDays ?? 365;
  const buckets: SpendingDataPoint[] = [];
  let bucketSize: number;
  switch (period) {
    case 'daily': bucketSize = 1; break;
    case 'weekly': bucketSize = 7; break;
    case 'monthly': bucketSize = 30; break;
    default: bucketSize = 30;
  }
  const numBuckets = Math.ceil(days / bucketSize);
  const now = new Date();
  for (let i = numBuckets - 1; i >= 0; i--) {
    const d = new Date(now);
    d.setDate(d.getDate() - i * bucketSize);
    const label = d.toLocaleDateString('en-US', { month: 'short', day: 'numeric' });
    buckets.push({
      label,
      total: 500 + Math.random() * 3000,
    });
  }
  return buckets;
}

export default function SpendingScreen() {
  const [timeRange, setTimeRange] = useState(TimeRange.SIX_MONTHS);
  const [groupBy, setGroupBy] = useState<GroupBy>('none');
  const query = timeRangeToQuery(timeRange);

  const data = useMemo(
    () => generateMockSpendingData(query.lookbackDays, query.period),
    [query.lookbackDays, query.period],
  );

  const totalSpending = data.reduce((sum, d) => sum + d.total, 0);
  const avgPerPeriod = data.length > 0 ? totalSpending / data.length : 0;

  return (
    <ScrollView style={styles.container}>
      <TimeRangeSelector selected={timeRange} onSelect={setTimeRange} />

      <ChartContainer loading={false} error={null}>
        <SpendingChart data={data} />
      </ChartContainer>

      <View style={styles.stats}>
        <Text style={styles.totalLabel}>Total Spending</Text>
        <Text style={styles.totalValue}>
          ${totalSpending.toLocaleString('en-US', { minimumFractionDigits: 2, maximumFractionDigits: 2 })}
        </Text>
        <Text style={styles.avgText}>
          Avg ${avgPerPeriod.toLocaleString('en-US', { minimumFractionDigits: 2, maximumFractionDigits: 2 })} / {query.period.replace('ly', '')}
        </Text>
      </View>

      <View style={styles.groupByRow}>
        {GROUP_OPTIONS.map(({ label, value }) => (
          <Pressable
            key={value}
            onPress={() => setGroupBy(value)}
            style={[styles.groupButton, value === groupBy && styles.groupButtonActive]}
          >
            <Text style={[styles.groupLabel, value === groupBy && styles.groupLabelActive]}>
              {label}
            </Text>
          </Pressable>
        ))}
      </View>
    </ScrollView>
  );
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
  },
  stats: {
    paddingHorizontal: 20,
    paddingVertical: 16,
    alignItems: 'center',
    gap: 4,
  },
  totalLabel: {
    fontSize: 14,
    color: 'rgba(255, 255, 255, 0.5)',
  },
  totalValue: {
    fontSize: 28,
    fontWeight: 'bold',
  },
  avgText: {
    fontSize: 14,
    color: 'rgba(255, 255, 255, 0.5)',
  },
  groupByRow: {
    flexDirection: 'row',
    justifyContent: 'center',
    gap: 8,
    paddingVertical: 12,
    paddingHorizontal: 16,
  },
  groupButton: {
    paddingHorizontal: 12,
    paddingVertical: 6,
    borderRadius: 6,
    backgroundColor: 'rgba(255, 255, 255, 0.08)',
  },
  groupButtonActive: {
    backgroundColor: 'rgba(52, 152, 219, 0.9)',
  },
  groupLabel: {
    fontSize: 12,
    fontWeight: '600',
    color: 'rgba(255, 255, 255, 0.5)',
  },
  groupLabelActive: {
    color: '#fff',
  },
});
```

**Step 2: Commit**

```bash
git add frontend/app/\(tabs\)/spending.tsx
git commit -m "feat(frontend): build Spending tab with bar chart and group-by toggle"
```

---

### Task 8: Update Tab Navigator and Build Accounts/Settings Tabs

**Files:**
- Rewrite: `frontend/app/(tabs)/_layout.tsx`
- Create: `frontend/app/(tabs)/accounts.tsx`
- Rename/rewrite: `frontend/app/(tabs)/two.tsx` → `frontend/app/(tabs)/settings.tsx`
- Delete: `frontend/app/modal.tsx` (no longer needed)
- Delete: `frontend/components/EditScreenInfo.tsx` (no longer needed)

**Step 1: Create accounts.tsx**

Create `frontend/app/(tabs)/accounts.tsx`:

```tsx
import React, { useCallback, useEffect, useState } from 'react';
import { ScrollView, StyleSheet } from 'react-native';
import AsyncStorage from '@react-native-async-storage/async-storage';
import { useFocusEffect } from 'expo-router';
import { Text, View } from '@/components/Themed';
import KeepbookNative from '@/modules/keepbook-native';

type AccountSummary = {
  id: string;
  name: string;
  connection_id: string;
  created_at: string;
  active: boolean;
};

type ConnectionSummary = {
  id: string;
  name: string;
  synchronizer: string;
  status: string;
};

export default function AccountsScreen() {
  const [accounts, setAccounts] = useState<AccountSummary[]>([]);
  const [connections, setConnections] = useState<ConnectionSummary[]>([]);

  const refresh = useCallback(async () => {
    const saved = await AsyncStorage.getItem('keepbook.data_dir');
    const dataDir = saved || KeepbookNative.demoDataDir();
    const [acctJson, connJson] = await Promise.all([
      KeepbookNative.listAccounts(dataDir),
      KeepbookNative.listConnections(dataDir),
    ]);
    try { setAccounts(JSON.parse(acctJson)); } catch { setAccounts([]); }
    try { setConnections(JSON.parse(connJson)); } catch { setConnections([]); }
  }, []);

  useFocusEffect(useCallback(() => { void refresh(); }, [refresh]));

  const connMap = new Map(connections.map((c) => [c.id, c]));

  // Group accounts by connection
  const grouped = new Map<string, { connection: ConnectionSummary | null; accounts: AccountSummary[] }>();
  for (const acct of accounts) {
    const key = acct.connection_id;
    if (!grouped.has(key)) {
      grouped.set(key, { connection: connMap.get(key) ?? null, accounts: [] });
    }
    grouped.get(key)!.accounts.push(acct);
  }

  return (
    <ScrollView style={styles.container} contentContainerStyle={styles.content}>
      {grouped.size === 0 && (
        <Text style={styles.muted}>No accounts. Sync data from Settings.</Text>
      )}
      {[...grouped.entries()].map(([connId, { connection, accounts: accts }]) => (
        <View key={connId} style={styles.group}>
          <Text style={styles.groupTitle}>
            {connection?.name ?? connId}
          </Text>
          {accts.map((a) => (
            <View key={a.id} style={styles.card}>
              <Text style={styles.cardTitle}>
                {a.name} {a.active ? '' : '(inactive)'}
              </Text>
              <Text style={styles.cardMeta}>id: {a.id}</Text>
            </View>
          ))}
        </View>
      ))}
    </ScrollView>
  );
}

const styles = StyleSheet.create({
  container: { flex: 1 },
  content: { padding: 16, gap: 16 },
  group: { gap: 8 },
  groupTitle: { fontSize: 16, fontWeight: 'bold', marginBottom: 4 },
  card: {
    borderWidth: 1,
    borderColor: 'rgba(255,255,255,0.15)',
    borderRadius: 10,
    padding: 12,
    gap: 4,
  },
  cardTitle: { fontWeight: '600' },
  cardMeta: { color: 'rgba(255,255,255,0.5)', fontSize: 13 },
  muted: { color: 'rgba(255,255,255,0.4)', textAlign: 'center', marginTop: 40 },
});
```

**Step 2: Create settings.tsx from current two.tsx**

Rename `two.tsx` to `settings.tsx`. The content stays the same — it's the git sync config screen. Remove the `EditScreenInfo` import and usage.

**Step 3: Update _layout.tsx with 4 tabs**

Rewrite `frontend/app/(tabs)/_layout.tsx`:

```tsx
import React from 'react';
import FontAwesome from '@expo/vector-icons/FontAwesome';
import { Tabs } from 'expo-router';

import Colors from '@/constants/Colors';
import { useColorScheme } from '@/components/useColorScheme';
import { useClientOnlyValue } from '@/components/useClientOnlyValue';

function TabBarIcon(props: {
  name: React.ComponentProps<typeof FontAwesome>['name'];
  color: string;
}) {
  return <FontAwesome size={24} style={{ marginBottom: -3 }} {...props} />;
}

export default function TabLayout() {
  const colorScheme = useColorScheme();

  return (
    <Tabs
      screenOptions={{
        tabBarActiveTintColor: Colors[colorScheme ?? 'light'].tint,
        headerShown: useClientOnlyValue(false, true),
      }}
    >
      <Tabs.Screen
        name="index"
        options={{
          title: 'Net Worth',
          tabBarIcon: ({ color }) => <TabBarIcon name="line-chart" color={color} />,
        }}
      />
      <Tabs.Screen
        name="spending"
        options={{
          title: 'Spending',
          tabBarIcon: ({ color }) => <TabBarIcon name="bar-chart" color={color} />,
        }}
      />
      <Tabs.Screen
        name="accounts"
        options={{
          title: 'Accounts',
          tabBarIcon: ({ color }) => <TabBarIcon name="bank" color={color} />,
        }}
      />
      <Tabs.Screen
        name="settings"
        options={{
          title: 'Settings',
          tabBarIcon: ({ color }) => <TabBarIcon name="cog" color={color} />,
        }}
      />
    </Tabs>
  );
}
```

**Step 4: Delete modal.tsx and EditScreenInfo.tsx**

```bash
rm frontend/app/modal.tsx
rm frontend/components/EditScreenInfo.tsx
```

Also remove the modal route from `frontend/app/_layout.tsx` — simplify the Stack to just `(tabs)`.

**Step 5: Verify all 4 tabs render**

```bash
cd /home/imalison/Projects/keepbook/frontend && yarn web
```

**Step 6: Commit**

```bash
git add -A frontend/app/ frontend/components/
git commit -m "feat(frontend): 4-tab layout with Net Worth, Spending, Accounts, Settings"
```

---

## Phase 3: Real Data Integration

### Task 9: Extend Git Sync to Fetch Full Financial Data

The current git sync only fetches connections and accounts. We need to also fetch balance snapshots, transactions, transaction annotations, and market data (prices, FX rates) so portfolio/spending calculations can run.

**Files:**
- Modify: `frontend/modules/keepbook-native/src/KeepbookNativeBackend.ts`

**Step 1: Extend gitSync to download balance/transaction/price data**

After the existing connection and account fetching, add:

1. For each account, fetch `balances.jsonl`, `transactions.jsonl`, and `transaction_annotations.jsonl` from GitHub and store in AsyncStorage keyed by path (e.g., `keepbook.file.git.data/accounts/{id}/balances.jsonl`).

2. Walk the GitHub tree for `data/prices/` and `data/fx/` directories. For each JSONL file found, download and store in AsyncStorage keyed by path.

The storage key pattern: `keepbook.file.{dataDir}.{relativePath}`

This enables the AsyncStorage-backed Storage adapter (Task 10) to find data by path.

Implementation: Add a helper `fetchAndStoreFile(owner, name, branch, path, dataDir, authToken)` that downloads a file from GitHub and stores it in AsyncStorage. Then iterate over the tree entries matching the file patterns.

**Step 2: Store a manifest of downloaded files**

Store `keepbook.manifest.{dataDir}` as a JSON array of all downloaded relative paths. This lets the storage adapter enumerate available files without scanning all AsyncStorage keys.

**Step 3: Test by syncing and verifying data appears in AsyncStorage**

```bash
cd /home/imalison/Projects/keepbook/frontend && yarn web
```

Navigate to Settings, configure GitHub sync, press Sync, then check that the data is stored (can add a debug log temporarily).

**Step 4: Commit**

```bash
git add frontend/modules/keepbook-native/src/KeepbookNativeBackend.ts
git commit -m "feat(frontend): extend git sync to fetch balances, transactions, and market data"
```

---

### Task 10: Build AsyncStorage-Backed Storage Adapter

Create a read-only Storage implementation that reads from AsyncStorage where git sync stored the data. This implements the keepbook TS library's `Storage` interface so we can pass it to `portfolioHistory()` and `spendingReport()`.

**Files:**
- Create: `frontend/modules/keepbook-native/src/AsyncStorageStorage.ts`

**Step 1: Implement the Storage interface**

The adapter reads JSONL content from AsyncStorage keys like `keepbook.file.{dataDir}.data/accounts/{id}/balances.jsonl` and parses them into the model types.

Key methods to implement:
- `listConnections()` — read manifest, find connection dirs, parse TOML config + JSON state
- `listAccounts()` — read manifest, find account dirs, parse JSON
- `getBalanceSnapshots(accountId)` — read balances.jsonl from AsyncStorage
- `getTransactions(accountId)` — read transactions.jsonl, dedupe
- `getTransactionAnnotationPatches(accountId)` — read annotations.jsonl
- `getLatestBalanceSnapshot(accountId)` — from getBalanceSnapshots, find latest
- `getLatestBalances()` — iterate all accounts
- `getConnection(id)`, `getAccount(id)` — look up individual entities
- `getCredentialStore()` — return null (no credentials on mobile)
- `getAccountConfig()` — return null (no account configs on mobile)
- Write methods (`save*`, `delete*`, `append*`) — throw Error('read-only')

Import model types from the keepbook TS library:
```typescript
import type { Storage } from '@keepbook/storage/storage';
import { Account } from '@keepbook/models/account';
import { BalanceSnapshot } from '@keepbook/models/balance';
import { ConnectionState } from '@keepbook/models/connection';
import { Transaction } from '@keepbook/models/transaction';
import { TransactionAnnotationPatch } from '@keepbook/models/transaction-annotation';
import { Id } from '@keepbook/models/id';
```

If the Metro import resolution from Task 2 doesn't work cleanly, an alternative is to copy/vendor just the model type files and the Storage interface into the frontend. The key thing is that the adapter implements the same interface shape so it can be passed to the business logic functions.

**Step 2: Test by instantiating the adapter and calling listAccounts**

**Step 3: Commit**

```bash
git add frontend/modules/keepbook-native/src/AsyncStorageStorage.ts
git commit -m "feat(frontend): AsyncStorage-backed read-only Storage adapter"
```

---

### Task 11: Build AsyncStorage-Backed MarketDataStore Adapter

**Files:**
- Create: `frontend/modules/keepbook-native/src/AsyncStorageMarketDataStore.ts`

**Step 1: Implement MarketDataStore interface**

Read price and FX JSONL files from AsyncStorage (stored by git sync in Task 9).

Key methods:
- `get_price(assetId, date, kind)` — read yearly JSONL for the asset, find matching entry
- `get_all_prices(assetId)` — read all yearly JSONL files for the asset
- `get_fx_rate(base, quote, date, kind)` — read FX JSONL, find matching entry
- `get_all_fx_rates(base, quote)` — read all FX JSONL files
- Write methods — no-op (read-only)
- `get_asset_entry()` — return null
- `upsert_asset_entry()` — no-op

Storage keys follow the pattern from git sync: `keepbook.file.{dataDir}.data/prices/{asset_id}/{year}.jsonl` and `keepbook.file.{dataDir}.data/fx/{BASE}-{QUOTE}/{year}.jsonl`.

**Step 2: Commit**

```bash
git add frontend/modules/keepbook-native/src/AsyncStorageMarketDataStore.ts
git commit -m "feat(frontend): AsyncStorage-backed read-only MarketDataStore adapter"
```

---

### Task 12: Wire Net Worth Chart to Real Data

**Files:**
- Modify: `frontend/app/(tabs)/index.tsx`
- Modify: `frontend/modules/keepbook-native/src/KeepbookNativeBackend.ts` (add portfolioHistory method)

**Step 1: Add portfolioHistory to backend**

Add a method to KeepbookNativeBackend that:
1. Creates AsyncStorageStorage and AsyncStorageMarketDataStore for the current dataDir
2. Constructs a minimal ResolvedConfig (reporting_currency from settings or default "USD")
3. Calls the keepbook TS library's `portfolioHistory()` function
4. Returns the result as JSON string

```typescript
async portfolioHistory(
  dataDir: string,
  opts: { start?: string; end?: string; granularity?: string },
): Promise<string> {
  const storage = new AsyncStorageStorage(dataDir);
  const marketDataStore = new AsyncStorageMarketDataStore(dataDir);
  const config = buildMobileConfig(dataDir);
  const result = await portfolioHistoryFn(storage, marketDataStore, config, {
    start: opts.start,
    end: opts.end,
    granularity: opts.granularity,
    includePrices: true,
  });
  return JSON.stringify(result);
}
```

**Step 2: Update Net Worth screen to use real data**

Replace the mock data generation with a call to `KeepbookNative.portfolioHistory()`. Parse the JSON response and map `HistoryPoint[]` to `NetWorthDataPoint[]`.

```typescript
const data = useMemo(async () => {
  const json = await KeepbookNative.portfolioHistory(dataDir, {
    start: startDate,
    end: endDate,
    granularity: query.granularity,
  });
  const history: HistoryOutput = JSON.parse(json);
  return history.points.map(p => ({
    date: p.date,
    value: parseFloat(p.total_value),
  }));
}, [dataDir, startDate, endDate, query.granularity]);
```

Use a proper `useEffect` + state pattern instead of async useMemo. Set loading/error states for ChartContainer.

**Step 3: Verify with real data**

Sync from GitHub, then navigate to Net Worth tab. Verify the chart shows real portfolio history.

**Step 4: Commit**

```bash
git add frontend/app/\(tabs\)/index.tsx frontend/modules/keepbook-native/src/KeepbookNativeBackend.ts
git commit -m "feat(frontend): wire Net Worth chart to real portfolio history data"
```

---

### Task 13: Wire Spending Chart to Real Data

**Files:**
- Modify: `frontend/app/(tabs)/spending.tsx`
- Modify: `frontend/modules/keepbook-native/src/KeepbookNativeBackend.ts` (add spending method)

**Step 1: Add spending method to backend**

Similar to Task 12 — create adapters, call `spendingReport()` from the TS library.

```typescript
async spending(
  dataDir: string,
  opts: { start?: string; end?: string; period?: string; groupBy?: string; direction?: string },
): Promise<string> {
  const storage = new AsyncStorageStorage(dataDir);
  const marketDataStore = new AsyncStorageMarketDataStore(dataDir);
  const config = buildMobileConfig(dataDir);
  const result = await spendingReportFn(storage, marketDataStore, config, {
    start: opts.start,
    end: opts.end,
    period: opts.period ?? 'monthly',
    direction: opts.direction ?? 'outflow',
    group_by: opts.groupBy ?? 'none',
  });
  return JSON.stringify(result);
}
```

**Step 2: Update Spending screen to use real data**

Replace mock data with call to `KeepbookNative.spending()`. Map `SpendingPeriodOutput[]` to `SpendingDataPoint[]`:

```typescript
const mapped = spending.periods.map(p => ({
  label: formatPeriodLabel(p.start_date, query.period),
  total: parseFloat(p.total),
}));
```

Wire the group-by toggle to re-fetch with different groupBy parameter.

**Step 3: Verify with real data**

**Step 4: Commit**

```bash
git add frontend/app/\(tabs\)/spending.tsx frontend/modules/keepbook-native/src/KeepbookNativeBackend.ts
git commit -m "feat(frontend): wire Spending chart to real spending report data"
```

---

### Task 14: Polish and Test Across Platforms

**Step 1: Test on web**

```bash
cd /home/imalison/Projects/keepbook/frontend && yarn web
```

Verify all 4 tabs work, charts render, time range selector changes data.

**Step 2: Test on Android**

```bash
cd /home/imalison/Projects/keepbook/frontend && yarn android
```

Verify charts render on Android emulator/device.

**Step 3: Fix platform-specific issues**

Common issues to watch for:
- Skia web rendering requires canvaskit-wasm — may need Metro/webpack config
- Font loading differences between platforms
- AsyncStorage size limits (default 6MB on Android — may need to increase for large datasets)
- Touch/gesture handling differences

**Step 4: Final commit**

```bash
git add -A frontend/
git commit -m "fix(frontend): platform-specific fixes for charts"
```

---

## Summary of Deliverables

| Task | Deliverable |
|------|------------|
| 1 | Charting deps installed |
| 2 | Metro configured to import TS library |
| 3 | TimeRangeSelector, ChartContainer, chart-colors |
| 4 | NetWorthChart component |
| 5 | SpendingChart component |
| 6 | Net Worth tab screen (mock data) |
| 7 | Spending tab screen (mock data) |
| 8 | 4-tab navigator, Accounts tab, Settings tab |
| 9 | Git sync fetches full financial data |
| 10 | AsyncStorage-backed Storage adapter |
| 11 | AsyncStorage-backed MarketDataStore adapter |
| 12 | Net Worth chart with real data |
| 13 | Spending chart with real data |
| 14 | Cross-platform testing and polish |
