import React, { useCallback, useEffect, useMemo, useState } from 'react';
import { ScrollView, StyleSheet } from 'react-native';
import AsyncStorage from '@react-native-async-storage/async-storage';
import { Text, View } from '@/components/Themed';
import { NetWorthChart, type NetWorthDataPoint } from '@/components/charts/NetWorthChart';
import { TimeRangeSelector, TimeRange, timeRangeToQuery } from '@/components/charts/TimeRangeSelector';
import { ChartContainer } from '@/components/charts/ChartContainer';
import KeepbookNative from '@/modules/keepbook-native';

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
  const [data, setData] = useState<NetWorthDataPoint[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [usingMock, setUsingMock] = useState(false);

  const query = timeRangeToQuery(timeRange);
  const mockData = useMemo(() => generateMockNetWorthData(query.lookbackDays), [query.lookbackDays]);

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

      const json = await KeepbookNative.portfolioHistory(
        dataDir,
        start,
        end,
        query.granularity ?? 'daily',
      );
      const result = JSON.parse(json);

      if (result.error) {
        // Fall back to mock data
        setData(mockData);
        setUsingMock(true);
        setLoading(false);
        return;
      }

      if (result.points && result.points.length > 0) {
        const points: NetWorthDataPoint[] = result.points.map((p: any) => ({
          date: p.date,
          value: parseFloat(p.total_value),
        }));
        setData(points);
      } else {
        // No data -- use mock
        setData(mockData);
        setUsingMock(true);
      }
      setLoading(false);
    } catch (err) {
      // Fall back to mock data on any error
      setData(mockData);
      setUsingMock(true);
      setLoading(false);
    }
  }, [query.lookbackDays, query.granularity, mockData]);

  useEffect(() => {
    void fetchData();
  }, [fetchData]);

  const currentValue = data.length > 0 ? data[data.length - 1].value : 0;
  const startValue = data.length > 0 ? data[0].value : 0;
  const absoluteChange = currentValue - startValue;
  const percentChange = startValue !== 0 ? (absoluteChange / startValue) * 100 : 0;
  const isPositive = absoluteChange >= 0;

  return (
    <ScrollView style={styles.container}>
      <TimeRangeSelector selected={timeRange} onSelect={setTimeRange} />
      <ChartContainer loading={loading} error={error}>
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
        {usingMock && (
          <Text style={styles.mockLabel}>Sample data -- sync from Settings</Text>
        )}
      </View>
    </ScrollView>
  );
}

const styles = StyleSheet.create({
  container: { flex: 1 },
  stats: { paddingHorizontal: 20, paddingVertical: 16, alignItems: 'center', gap: 4 },
  totalValue: { fontSize: 28, fontWeight: 'bold' },
  change: { fontSize: 16, fontWeight: '500' },
  mockLabel: { fontSize: 12, color: 'rgba(255, 255, 255, 0.35)', marginTop: 4 },
});
