import React, { useMemo, useState } from 'react';
import { ScrollView, StyleSheet } from 'react-native';
import { Text, View } from '@/components/Themed';
import { NetWorthChart, type NetWorthDataPoint } from '@/components/charts/NetWorthChart';
import { TimeRangeSelector, TimeRange, timeRangeToQuery } from '@/components/charts/TimeRangeSelector';
import { ChartContainer } from '@/components/charts/ChartContainer';

function generateMockNetWorthData(lookbackDays: number | null): NetWorthDataPoint[] {
  const days = lookbackDays ?? 365;
  const points: NetWorthDataPoint[] = [];
  let value = 100000;
  const now = new Date();
  for (let i = days; i >= 0; i--) {
    const d = new Date(now);
    d.setDate(d.getDate() - i);
    value += (Math.random() - 0.45) * 1000;
    points.push({ date: d.toISOString().slice(0, 10), value: Math.max(value, 0) });
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
  container: { flex: 1 },
  stats: { paddingHorizontal: 20, paddingVertical: 16, alignItems: 'center', gap: 4 },
  totalValue: { fontSize: 28, fontWeight: 'bold' },
  change: { fontSize: 16, fontWeight: '500' },
});
