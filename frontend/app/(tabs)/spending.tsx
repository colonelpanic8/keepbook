import React, { useCallback, useMemo, useState } from 'react';
import { Pressable, ScrollView, StyleSheet } from 'react-native';
import AsyncStorage from '@react-native-async-storage/async-storage';
import { useFocusEffect } from '@react-navigation/native';
import { Text, View } from '@/components/Themed';
import { SpendingChart, type SpendingDataPoint } from '@/components/charts/SpendingChart';
import { TimeRangeSelector, TimeRange, timeRangeToQuery } from '@/components/charts/TimeRangeSelector';
import { ChartContainer } from '@/components/charts/ChartContainer';
import { useColorScheme } from '@/components/useColorScheme';
import KeepbookNative from '@/modules/keepbook-native';

type GroupBy = 'none' | 'category' | 'merchant' | 'account' | 'tag';
type SpendingPeriod = 'daily' | 'weekly' | 'monthly' | 'quarterly' | 'yearly' | 'range';
type SpendingDirection = 'outflow' | 'inflow' | 'net';

const GROUP_OPTIONS: { label: string; value: GroupBy }[] = [
  { label: 'None', value: 'none' },
  { label: 'Category', value: 'category' },
  { label: 'Merchant', value: 'merchant' },
  { label: 'Account', value: 'account' },
  { label: 'Tag', value: 'tag' },
];

const PERIOD_OPTIONS: { label: string; value: SpendingPeriod }[] = [
  { label: 'Day', value: 'daily' },
  { label: 'Week', value: 'weekly' },
  { label: 'Month', value: 'monthly' },
  { label: 'Quarter', value: 'quarterly' },
  { label: 'Year', value: 'yearly' },
  { label: 'Range', value: 'range' },
];

const DIRECTION_OPTIONS: { label: string; value: SpendingDirection }[] = [
  { label: 'Spent', value: 'outflow' },
  { label: 'Inflow', value: 'inflow' },
  { label: 'Net', value: 'net' },
];

interface BreakdownRow {
  key: string;
  total: number;
  transactionCount: number;
}

interface ScreenColors {
  muted: string;
  subtle: string;
  surface: string;
  border: string;
}

function bucketSizeForPeriod(period: SpendingPeriod): number {
  switch (period) {
    case 'daily':
      return 1;
    case 'weekly':
      return 7;
    case 'quarterly':
      return 91;
    case 'yearly':
      return 365;
    case 'range':
      return 365;
    case 'monthly':
    default:
      return 30;
  }
}

function periodNoun(period: SpendingPeriod): string {
  switch (period) {
    case 'daily':
      return 'day';
    case 'weekly':
      return 'week';
    case 'monthly':
      return 'month';
    case 'quarterly':
      return 'quarter';
    case 'yearly':
      return 'year';
    case 'range':
      return 'range';
  }
}

function generateMockSpendingData(lookbackDays: number | null, period: SpendingPeriod): SpendingDataPoint[] {
  const days = lookbackDays ?? 365;
  const bucketSize = bucketSizeForPeriod(period);
  const numBuckets = Math.ceil(days / bucketSize);
  const now = new Date();
  return Array.from({ length: numBuckets }, (_, i) => {
    const d = new Date(now);
    d.setDate(d.getDate() - (numBuckets - 1 - i) * bucketSize);
    const end = new Date(d);
    end.setDate(end.getDate() + bucketSize - 1);
    return {
      label: d.toLocaleDateString('en-US', { month: 'short', day: 'numeric' }),
      total: 500 + Math.random() * 3000,
      startDate: d.toISOString().slice(0, 10),
      endDate: end.toISOString().slice(0, 10),
      transactionCount: 0,
    };
  });
}

function formatPeriodLabel(startDate: string, period: SpendingPeriod): string {
  try {
    const d = new Date(startDate + 'T00:00:00Z');
    if (period === 'yearly') {
      return d.toLocaleDateString('en-US', { year: 'numeric', timeZone: 'UTC' });
    }
    if (period === 'monthly') {
      return d.toLocaleDateString('en-US', { month: 'short', year: '2-digit', timeZone: 'UTC' });
    }
    if (period === 'quarterly') {
      const quarter = Math.floor(d.getUTCMonth() / 3) + 1;
      return `Q${quarter} '${String(d.getUTCFullYear()).slice(2)}`;
    }
    return d.toLocaleDateString('en-US', { month: 'short', day: 'numeric', timeZone: 'UTC' });
  } catch {
    return startDate;
  }
}

function formatDateRange(start: string | null, end: string | null): string {
  if (!start && !end) return 'All dates';
  const fmt = (value: string) => {
    const d = new Date(`${value}T00:00:00Z`);
    if (Number.isNaN(d.getTime())) return value;
    return d.toLocaleDateString('en-US', {
      month: 'short',
      day: 'numeric',
      year: 'numeric',
      timeZone: 'UTC',
    });
  };
  if (start && end) return `${fmt(start)} - ${fmt(end)}`;
  if (start) return `Since ${fmt(start)}`;
  return `Through ${fmt(end!)}`;
}

function formatMoney(value: number, currency = 'USD'): string {
  const prefix = currency === 'USD' ? '$' : `${currency} `;
  return `${prefix}${value.toLocaleString('en-US', {
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
  })}`;
}

export default function SpendingScreen() {
  const isDark = useColorScheme() === 'dark';
  const colors: ScreenColors = {
    muted: isDark ? 'rgba(255, 255, 255, 0.58)' : 'rgba(17, 24, 39, 0.62)',
    subtle: isDark ? 'rgba(255, 255, 255, 0.42)' : 'rgba(17, 24, 39, 0.46)',
    surface: isDark ? 'rgba(255, 255, 255, 0.08)' : 'rgba(17, 24, 39, 0.07)',
    border: isDark ? 'rgba(255, 255, 255, 0.16)' : 'rgba(17, 24, 39, 0.14)',
  };
  const [timeRange, setTimeRange] = useState(TimeRange.SIX_MONTHS);
  const [period, setPeriod] = useState<SpendingPeriod>('monthly');
  const [groupBy, setGroupBy] = useState<GroupBy>('none');
  const [direction, setDirection] = useState<SpendingDirection>('outflow');
  const [data, setData] = useState<SpendingDataPoint[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [usingMock, setUsingMock] = useState(false);
  const [totalSpending, setTotalSpending] = useState(0);
  const [txCount, setTxCount] = useState(0);
  const [currency, setCurrency] = useState('USD');
  const [dateRangeLabel, setDateRangeLabel] = useState('Loading range');
  const [breakdown, setBreakdown] = useState<BreakdownRow[]>([]);

  const query = timeRangeToQuery(timeRange);
  const mockData = useMemo(
    () => generateMockSpendingData(query.lookbackDays, period),
    [query.lookbackDays, period],
  );

  const fetchData = useCallback(async () => {
    setLoading(true);
    setError(null);
    setUsingMock(false);

    try {
      const dataDir = (await AsyncStorage.getItem('keepbook.data_dir')) || 'git';
      const start = query.lookbackDays
        ? new Date(Date.now() - query.lookbackDays * 86400000).toISOString().slice(0, 10)
        : null;
      const end = new Date().toISOString().slice(0, 10);
      setDateRangeLabel(formatDateRange(start, end));

      const json = await KeepbookNative.spending(
        dataDir,
        start,
        end,
        period,
        groupBy,
        direction,
      );
      const result = JSON.parse(json);

      if (result.error) {
        setData(mockData);
        setUsingMock(true);
        setTotalSpending(mockData.reduce((sum, d) => sum + d.total, 0));
        setTxCount(0);
        setBreakdown([]);
        setLoading(false);
        return;
      }

      setCurrency(result.currency ?? 'USD');
      if (result.periods && result.periods.length > 0) {
        const points: SpendingDataPoint[] = result.periods.map((p: any) => ({
          label: formatPeriodLabel(p.start_date, period),
          total: Math.abs(parseFloat(p.total)),
          startDate: p.start_date,
          endDate: p.end_date,
          transactionCount: p.transaction_count ?? 0,
        }));
        setData(points);
        setTotalSpending(Math.abs(parseFloat(result.total ?? '0')));
        setTxCount(result.transaction_count ?? 0);
        const totalsByKey = new Map<string, BreakdownRow>();
        for (const p of result.periods) {
          for (const entry of p.breakdown ?? []) {
            const current = totalsByKey.get(entry.key) ?? {
              key: entry.key,
              total: 0,
              transactionCount: 0,
            };
            current.total += Math.abs(parseFloat(entry.total ?? '0'));
            current.transactionCount += entry.transaction_count ?? 0;
            totalsByKey.set(entry.key, current);
          }
        }
        setBreakdown(
          [...totalsByKey.values()]
            .sort((a, b) => b.total - a.total)
            .slice(0, 6),
        );
      } else {
        setData(mockData);
        setUsingMock(true);
        setTotalSpending(mockData.reduce((sum, d) => sum + d.total, 0));
        setTxCount(0);
        setBreakdown([]);
      }
      setLoading(false);
    } catch (err) {
      setData(mockData);
      setUsingMock(true);
      setTotalSpending(mockData.reduce((sum, d) => sum + d.total, 0));
      setTxCount(0);
      setBreakdown([]);
      setDateRangeLabel('Sample range');
      setLoading(false);
    }
  }, [query.lookbackDays, period, groupBy, direction, mockData]);

  useFocusEffect(
    useCallback(() => {
      void fetchData();
    }, [fetchData]),
  );

  const avgPerPeriod = data.length > 0 ? totalSpending / data.length : 0;
  const periodLabel = periodNoun(period);

  return (
    <ScrollView style={styles.container}>
      <View style={styles.header}>
        <Text style={styles.title}>Spending</Text>
        <Text style={[styles.subtitle, { color: colors.muted }]}>{dateRangeLabel}</Text>
      </View>
      <ControlSection label="Date range" colors={colors}>
        <TimeRangeSelector selected={timeRange} onSelect={setTimeRange} />
      </ControlSection>
      <ControlSection label="Bucket" colors={colors}>
        <SegmentedControl
          options={PERIOD_OPTIONS}
          value={period}
          onChange={setPeriod}
          colors={colors}
        />
      </ControlSection>
      <View style={styles.metaRow}>
        <Metric label="Total" value={formatMoney(totalSpending, currency)} colors={colors} />
        <Metric
          label={`Avg / ${periodLabel}`}
          value={formatMoney(avgPerPeriod, currency)}
          colors={colors}
        />
        <Metric label="Transactions" value={String(txCount)} colors={colors} />
      </View>
      <ChartContainer loading={loading} error={error}>
        <SpendingChart
          data={data}
          currency={currency}
          periodLabel={periodLabel}
        />
      </ChartContainer>
      <View style={styles.summary}>
        <Text style={[styles.summaryText, { color: colors.muted }]}>
          {data.length} {periodLabel}{data.length === 1 ? '' : 's'} shown
        </Text>
        {usingMock && (
          <Text style={[styles.mockLabel, { color: colors.subtle }]}>Sample data - sync from Settings</Text>
        )}
      </View>
      <ControlSection label="Direction" colors={colors}>
        <SegmentedControl
          options={DIRECTION_OPTIONS}
          value={direction}
          onChange={setDirection}
          colors={colors}
        />
      </ControlSection>
      <ControlSection label="Breakdown" colors={colors}>
        <SegmentedControl
          options={GROUP_OPTIONS}
          value={groupBy}
          onChange={setGroupBy}
          colors={colors}
        />
      </ControlSection>
      {breakdown.length > 0 && (
        <View style={[styles.breakdownList, { borderTopColor: colors.border }]}>
          <Text style={[styles.breakdownTitle, { color: colors.muted }]}>Top {groupBy}</Text>
          {breakdown.map((row) => (
            <View key={row.key} style={[styles.breakdownRow, { borderBottomColor: colors.border }]}>
              <Text style={styles.breakdownKey} numberOfLines={1}>{row.key}</Text>
              <Text style={[styles.breakdownValue, { color: colors.muted }]}>
                {formatMoney(row.total, currency)} · {row.transactionCount} txns
              </Text>
            </View>
          ))}
        </View>
      )}
    </ScrollView>
  );
}

interface ControlSectionProps {
  label: string;
  colors: ScreenColors;
  children: React.ReactNode;
}

function ControlSection({ label, colors, children }: ControlSectionProps) {
  return (
    <View style={styles.controlSection}>
      <Text style={[styles.controlLabel, { color: colors.muted }]}>{label}</Text>
      {children}
    </View>
  );
}

interface SegmentedOption<T extends string> {
  label: string;
  value: T;
}

interface SegmentedControlProps<T extends string> {
  options: SegmentedOption<T>[];
  value: T;
  onChange: (value: T) => void;
  colors: ScreenColors;
}

function SegmentedControl<T extends string>({
  options,
  value,
  onChange,
  colors,
}: SegmentedControlProps<T>) {
  return (
    <View style={styles.segmentedRow}>
      {options.map((option) => {
        const isActive = option.value === value;
        return (
          <Pressable
            key={option.value}
            onPress={() => onChange(option.value)}
            style={[
              styles.segmentButton,
              { backgroundColor: colors.surface },
              isActive && styles.segmentButtonActive,
            ]}
          >
            <Text
              style={[
                styles.segmentLabel,
                { color: colors.muted },
                isActive && styles.segmentLabelActive,
              ]}
            >
              {option.label}
            </Text>
          </Pressable>
        );
      })}
    </View>
  );
}

function Metric({ label, value, colors }: { label: string; value: string; colors: ScreenColors }) {
  return (
    <View style={[styles.metric, { backgroundColor: colors.surface }]}>
      <Text style={[styles.metricLabel, { color: colors.subtle }]}>{label}</Text>
      <Text style={styles.metricValue} numberOfLines={1}>{value}</Text>
    </View>
  );
}

const styles = StyleSheet.create({
  container: { flex: 1 },
  header: {
    paddingHorizontal: 20,
    paddingTop: 16,
    paddingBottom: 6,
    gap: 4,
  },
  title: { fontSize: 26, fontWeight: '700' },
  subtitle: { fontSize: 13 },
  controlSection: {
    paddingHorizontal: 14,
    paddingTop: 8,
    gap: 6,
  },
  controlLabel: {
    paddingHorizontal: 6,
    fontSize: 12,
    fontWeight: '700',
    textTransform: 'uppercase',
  },
  segmentedRow: {
    flexDirection: 'row',
    flexWrap: 'wrap',
    justifyContent: 'center',
    gap: 6,
  },
  segmentButton: {
    minWidth: 62,
    paddingHorizontal: 10,
    paddingVertical: 7,
    borderRadius: 6,
    alignItems: 'center',
  },
  segmentButtonActive: { backgroundColor: 'rgba(52, 152, 219, 0.9)' },
  segmentLabel: {
    fontSize: 12,
    fontWeight: '700',
  },
  segmentLabelActive: { color: '#fff' },
  metaRow: {
    flexDirection: 'row',
    gap: 8,
    paddingHorizontal: 16,
    paddingTop: 14,
    paddingBottom: 4,
  },
  metric: {
    flex: 1,
    minHeight: 62,
    justifyContent: 'center',
    paddingHorizontal: 10,
    paddingVertical: 8,
    borderRadius: 6,
  },
  metricLabel: {
    fontSize: 11,
    fontWeight: '700',
    textTransform: 'uppercase',
  },
  metricValue: {
    marginTop: 5,
    fontSize: 16,
    fontWeight: '700',
  },
  summary: { paddingHorizontal: 20, paddingVertical: 8, alignItems: 'center', gap: 4 },
  summaryText: { fontSize: 13 },
  mockLabel: { fontSize: 12, marginTop: 4 },
  breakdownList: {
    marginTop: 12,
    marginHorizontal: 16,
    marginBottom: 28,
    paddingVertical: 8,
    borderTopWidth: StyleSheet.hairlineWidth,
  },
  breakdownTitle: {
    paddingVertical: 8,
    fontSize: 12,
    fontWeight: '700',
    textTransform: 'uppercase',
  },
  breakdownRow: {
    minHeight: 36,
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'space-between',
    gap: 10,
    borderBottomWidth: StyleSheet.hairlineWidth,
  },
  breakdownKey: { flex: 1, fontSize: 14, fontWeight: '600' },
  breakdownValue: {
    flexShrink: 0,
    fontSize: 13,
  },
});
