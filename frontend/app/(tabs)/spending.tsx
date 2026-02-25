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

function generateMockSpendingData(lookbackDays: number | null, period: string): SpendingDataPoint[] {
  const days = lookbackDays ?? 365;
  let bucketSize: number;
  switch (period) {
    case 'daily': bucketSize = 1; break;
    case 'weekly': bucketSize = 7; break;
    default: bucketSize = 30;
  }
  const numBuckets = Math.ceil(days / bucketSize);
  const now = new Date();
  return Array.from({ length: numBuckets }, (_, i) => {
    const d = new Date(now);
    d.setDate(d.getDate() - (numBuckets - 1 - i) * bucketSize);
    return {
      label: d.toLocaleDateString('en-US', { month: 'short', day: 'numeric' }),
      total: 500 + Math.random() * 3000,
    };
  });
}

export default function SpendingScreen() {
  const [timeRange, setTimeRange] = useState(TimeRange.SIX_MONTHS);
  const [groupBy, setGroupBy] = useState<GroupBy>('none');
  const query = timeRangeToQuery(timeRange);
  const data = useMemo(() => generateMockSpendingData(query.lookbackDays, query.period), [query.lookbackDays, query.period]);

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
          <Pressable key={value} onPress={() => setGroupBy(value)}
            style={[styles.groupButton, value === groupBy && styles.groupButtonActive]}>
            <Text style={[styles.groupLabel, value === groupBy && styles.groupLabelActive]}>{label}</Text>
          </Pressable>
        ))}
      </View>
    </ScrollView>
  );
}

const styles = StyleSheet.create({
  container: { flex: 1 },
  stats: { paddingHorizontal: 20, paddingVertical: 16, alignItems: 'center', gap: 4 },
  totalLabel: { fontSize: 14, color: 'rgba(255, 255, 255, 0.5)' },
  totalValue: { fontSize: 28, fontWeight: 'bold' },
  avgText: { fontSize: 14, color: 'rgba(255, 255, 255, 0.5)' },
  groupByRow: { flexDirection: 'row', justifyContent: 'center', gap: 8, paddingVertical: 12, paddingHorizontal: 16 },
  groupButton: { paddingHorizontal: 12, paddingVertical: 6, borderRadius: 6, backgroundColor: 'rgba(255, 255, 255, 0.08)' },
  groupButtonActive: { backgroundColor: 'rgba(52, 152, 219, 0.9)' },
  groupLabel: { fontSize: 12, fontWeight: '600', color: 'rgba(255, 255, 255, 0.5)' },
  groupLabelActive: { color: '#fff' },
});
