import React, { useCallback, useMemo, useState } from 'react';
import { Platform, Pressable, ScrollView, StyleSheet, TextInput } from 'react-native';
import AsyncStorage from '@react-native-async-storage/async-storage';
import { useFocusEffect } from 'expo-router';
import { Text, View } from '@/components/Themed';
import { NetWorthChart, type NetWorthDataPoint } from '@/components/charts/NetWorthChart';
import { ChartContainer } from '@/components/charts/ChartContainer';
import { useColorScheme } from '@/components/useColorScheme';
import KeepbookNative from '@/modules/keepbook-native';

const DAY_MS = 86400000;

function parseDateInput(value: string): number | null {
  if (!/^\d{4}-\d{2}-\d{2}$/.test(value)) return null;
  const timestamp = Date.parse(`${value}T00:00:00.000Z`);
  if (!Number.isFinite(timestamp)) return null;
  return new Date(timestamp).toISOString().slice(0, 10) === value ? timestamp : null;
}

function offsetDate(date: string, days: number): string {
  const timestamp = parseDateInput(date);
  if (timestamp == null) return date;
  return new Date(timestamp + days * DAY_MS).toISOString().slice(0, 10);
}

function parseMoneyInput(value: string): number | null {
  const cleaned = value.replace(/[$,\s]/g, '');
  if (cleaned === '') return null;
  const parsed = Number(cleaned);
  return Number.isFinite(parsed) ? parsed : null;
}

function formatInputNumber(value: number): string {
  return value.toFixed(2).replace(/\.00$/, '').replace(/(\.\d*[1-9])0+$/, '$1');
}

function formatMoney(value: number): string {
  return `$${value.toLocaleString('en-US', {
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
  })}`;
}

function dataDateBounds(points: NetWorthDataPoint[]): { min: string; max: string } | null {
  if (points.length === 0) return null;
  return { min: points[0].date, max: points[points.length - 1].date };
}

function filterDataByDateRange(
  points: NetWorthDataPoint[],
  startDate: string,
  endDate: string,
): NetWorthDataPoint[] {
  const start = parseDateInput(startDate);
  const end = parseDateInput(endDate);
  if (start == null || end == null || start > end) return [];

  return points.filter((point) => {
    const timestamp = parseDateInput(point.date);
    return timestamp != null && timestamp >= start && timestamp <= end;
  });
}

function dataValueBounds(points: NetWorthDataPoint[]): [number, number] | null {
  if (points.length === 0) return null;
  let min = points[0].value;
  let max = points[0].value;
  for (const point of points) {
    min = Math.min(min, point.value);
    max = Math.max(max, point.value);
  }
  return min === max ? [min - 1, max + 1] : [min, max];
}

async function describeMissingHistory(dataDir: string): Promise<string> {
  try {
    const accountsJson = await KeepbookNative.listAccounts(dataDir);
    const accounts = JSON.parse(accountsJson);
    if (!Array.isArray(accounts) || accounts.length === 0) {
      return `No accounts found in data dir "${dataDir}". Sync from Settings, then refresh this tab.`;
    }
    return `Found ${accounts.length} account${accounts.length === 1 ? '' : 's'} in data dir "${dataDir}", but no net worth history points. Check that synced accounts have balance snapshots.`;
  } catch {
    return `No net worth history found in data dir "${dataDir}". Sync from Settings, then refresh this tab.`;
  }
}

type RangePreset = '1Y' | '2Y' | 'MAX';

const RANGE_PRESETS: Array<{ label: string; value: RangePreset }> = [
  { label: '1Y', value: '1Y' },
  { label: '2Y', value: '2Y' },
  { label: 'Max', value: 'MAX' },
];

interface DateRangeInputProps {
  label: string;
  value: string;
  min?: string;
  max?: string;
  colors: ControlColors;
  onChange: (value: string) => void;
}

interface ControlColors {
  text: string;
  mutedText: string;
  inputBackground: string;
  inputBorder: string;
  pillBackground: string;
  pillText: string;
}

function DateRangeInput({ label, value, min, max, colors, onChange }: DateRangeInputProps) {
  if (Platform.OS === 'web') {
    return (
      <View style={styles.field}>
        <Text style={[styles.label, { color: colors.mutedText }]}>{label}</Text>
        {React.createElement('input', {
          type: 'date',
          value,
          min,
          max,
          onChange: (event: React.ChangeEvent<HTMLInputElement>) => onChange(event.target.value),
          style: {
            ...styles.webDateInput,
            color: colors.text,
            backgroundColor: colors.inputBackground,
            borderColor: colors.inputBorder,
          } as React.CSSProperties,
        })}
      </View>
    );
  }

  return (
    <View style={styles.field}>
      <Text style={[styles.label, { color: colors.mutedText }]}>{label}</Text>
      <TextInput
        style={[
          styles.input,
          {
            color: colors.text,
            backgroundColor: colors.inputBackground,
            borderColor: colors.inputBorder,
          },
        ]}
        value={value}
        onChangeText={onChange}
        placeholder="YYYY-MM-DD"
        autoCapitalize="none"
        autoCorrect={false}
        keyboardType="numbers-and-punctuation"
        maxLength={10}
      />
    </View>
  );
}

export default function NetWorthScreen() {
  const colorScheme = useColorScheme();
  const isDark = colorScheme === 'dark';
  const colors: ControlColors = {
    text: isDark ? '#fff' : '#111827',
    mutedText: isDark ? 'rgba(255, 255, 255, 0.55)' : '#4b5563',
    inputBackground: isDark ? 'rgba(255, 255, 255, 0.06)' : '#fff',
    inputBorder: isDark ? 'rgba(255, 255, 255, 0.14)' : '#d1d5db',
    pillBackground: isDark ? 'rgba(255, 255, 255, 0.08)' : '#eaf4fd',
    pillText: isDark ? '#fff' : '#1769aa',
  };
  const [allData, setAllData] = useState<NetWorthDataPoint[]>([]);
  const [startDate, setStartDate] = useState('');
  const [endDate, setEndDate] = useState('');
  const [yMinInput, setYMinInput] = useState('');
  const [yMaxInput, setYMaxInput] = useState('');
  const [dataDir, setDataDir] = useState('');
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const visibleData = useMemo(() => {
    return filterDataByDateRange(allData, startDate, endDate);
  }, [allData, startDate, endDate]);

  const yDomain = useMemo<[number, number] | undefined>(() => {
    const min = parseMoneyInput(yMinInput);
    const max = parseMoneyInput(yMaxInput);
    if (min == null || max == null) return undefined;
    return min < max ? [min, max] : undefined;
  }, [yMinInput, yMaxInput]);

  const dataBounds = useMemo(() => dataDateBounds(allData), [allData]);
  const visibleValueBounds = useMemo(() => dataValueBounds(visibleData), [visibleData]);
  const displayedAxisRange =
    yDomain !== undefined
      ? `${formatMoney(yDomain[0])} - ${formatMoney(yDomain[1])}`
      : 'Auto';
  const displayedDataRange =
    visibleValueBounds !== null
      ? `${formatMoney(visibleValueBounds[0])} - ${formatMoney(visibleValueBounds[1])}`
      : 'No visible data';
  const hasDateRangeError =
    startDate !== '' &&
    endDate !== '' &&
    (parseDateInput(startDate) == null ||
      parseDateInput(endDate) == null ||
      (parseDateInput(startDate) ?? 0) > (parseDateInput(endDate) ?? 0));
  const hasYRangeError =
    yMinInput !== '' &&
    yMaxInput !== '' &&
    (parseMoneyInput(yMinInput) == null ||
      parseMoneyInput(yMaxInput) == null ||
      (parseMoneyInput(yMinInput) ?? 0) >= (parseMoneyInput(yMaxInput) ?? 0));

  const setYRangeToData = useCallback((points: NetWorthDataPoint[]) => {
    const bounds = dataValueBounds(points);
    if (!bounds) {
      setYMinInput('');
      setYMaxInput('');
      return;
    }
    setYMinInput(formatInputNumber(bounds[0]));
    setYMaxInput(formatInputNumber(bounds[1]));
  }, []);

  const applyInitialRanges = useCallback(
    (points: NetWorthDataPoint[]) => {
      const bounds = dataDateBounds(points);
      if (!bounds) return;
      const nextEnd = bounds.max;
      const nextStart = offsetDate(nextEnd, -365);
      const clampedStart = nextStart < bounds.min ? bounds.min : nextStart;

      setStartDate(clampedStart);
      setEndDate(nextEnd);
      setYRangeToData(filterDataByDateRange(points, clampedStart, nextEnd));
    },
    [setYRangeToData],
  );

  const applyRangePresetAndFitY = useCallback(
    (preset: RangePreset) => {
      const bounds = dataDateBounds(allData);
      if (!bounds) return;

      const nextEnd = bounds.max;
      const nextStart =
        preset === 'MAX'
          ? bounds.min
          : offsetDate(nextEnd, preset === '1Y' ? -365 : -365 * 2);
      const clampedStart = nextStart < bounds.min ? bounds.min : nextStart;

      setStartDate(clampedStart);
      setEndDate(nextEnd);
      setYRangeToData(filterDataByDateRange(allData, clampedStart, nextEnd));
    },
    [allData, setYRangeToData],
  );

  const fetchData = useCallback(async () => {
    setLoading(true);
    setError(null);

    try {
      const savedDataDir = (await AsyncStorage.getItem('keepbook.data_dir')) || KeepbookNative.gitDataDir();
      setDataDir(savedDataDir);
      const end = new Date().toISOString().slice(0, 10);

      const json = await KeepbookNative.portfolioHistory(
        savedDataDir,
        null,
        end,
        'daily',
      );
      const result = JSON.parse(json);

      if (result.error) {
        setAllData([]);
        setStartDate('');
        setEndDate('');
        setYMinInput('');
        setYMaxInput('');
        setError(`Unable to load net worth history: ${String(result.error)}`);
        setLoading(false);
        return;
      }

      if (result.points && result.points.length > 0) {
        const points: NetWorthDataPoint[] = result.points.map((p: any) => ({
          date: p.date,
          value: parseFloat(p.total_value),
        })).sort((a: NetWorthDataPoint, b: NetWorthDataPoint) => a.date.localeCompare(b.date));
        setAllData(points);
        applyInitialRanges(points);
      } else {
        setAllData([]);
        setStartDate('');
        setEndDate('');
        setYMinInput('');
        setYMaxInput('');
        setError(await describeMissingHistory(savedDataDir));
      }
      setLoading(false);
    } catch (err) {
      setAllData([]);
      setStartDate('');
      setEndDate('');
      setYMinInput('');
      setYMaxInput('');
      setError(`Unable to load net worth history: ${String(err)}`);
      setLoading(false);
    }
  }, [applyInitialRanges]);

  useFocusEffect(
    useCallback(() => {
      void fetchData();
    }, [fetchData]),
  );

  const hasVisibleData = visibleData.length > 0;
  const currentValue = hasVisibleData ? visibleData[visibleData.length - 1].value : null;
  const startValue = hasVisibleData ? visibleData[0].value : null;
  const absoluteChange = currentValue !== null && startValue !== null ? currentValue - startValue : 0;
  const percentChange = startValue !== null && startValue !== 0 ? (absoluteChange / startValue) * 100 : 0;
  const isPositive = absoluteChange >= 0;

  return (
    <ScrollView style={styles.container}>
      <View style={styles.controls}>
        <View style={styles.presetRow}>
          {RANGE_PRESETS.map((preset) => (
            <Pressable
              key={preset.value}
              onPress={() => applyRangePresetAndFitY(preset.value)}
              style={[styles.pill, { backgroundColor: colors.pillBackground }]}
            >
              <Text style={[styles.pillText, { color: colors.pillText }]}>{preset.label}</Text>
            </Pressable>
          ))}
          <Pressable
            onPress={() => setYRangeToData(visibleData)}
            style={[styles.pill, styles.fitPill]}
          >
            <Text style={styles.pillText}>Fit Y</Text>
          </Pressable>
          <Pressable
            onPress={() => void fetchData()}
            style={[styles.pill, styles.refreshPill]}
          >
            <Text style={styles.pillText}>Refresh</Text>
          </Pressable>
        </View>

        <View style={styles.controlGrid}>
          <DateRangeInput
            label="Start"
            value={startDate}
            min={dataBounds?.min}
            max={endDate || dataBounds?.max}
            colors={colors}
            onChange={setStartDate}
          />
          <DateRangeInput
            label="End"
            value={endDate}
            min={startDate || dataBounds?.min}
            max={dataBounds?.max}
            colors={colors}
            onChange={setEndDate}
          />
          <View style={styles.field}>
            <Text style={[styles.label, { color: colors.mutedText }]}>Min</Text>
            <TextInput
              style={[
                styles.input,
                {
                  color: colors.text,
                  backgroundColor: colors.inputBackground,
                  borderColor: colors.inputBorder,
                },
              ]}
              value={yMinInput}
              onChangeText={setYMinInput}
              keyboardType="decimal-pad"
              inputMode="decimal"
            />
          </View>
          <View style={styles.field}>
            <Text style={[styles.label, { color: colors.mutedText }]}>Max</Text>
            <TextInput
              style={[
                styles.input,
                {
                  color: colors.text,
                  backgroundColor: colors.inputBackground,
                  borderColor: colors.inputBorder,
                },
              ]}
              value={yMaxInput}
              onChangeText={setYMaxInput}
              keyboardType="decimal-pad"
              inputMode="decimal"
            />
          </View>
        </View>
        {hasDateRangeError && <Text style={styles.validation}>Use a valid start date before end date.</Text>}
        {hasYRangeError && <Text style={styles.validation}>Y min must be less than Y max.</Text>}
        <View style={styles.rangeSummary}>
          <Text style={[styles.rangeSummaryText, { color: colors.mutedText }]}>
            X {startDate || '-'} - {endDate || '-'}
          </Text>
          <Text style={[styles.rangeSummaryText, { color: colors.mutedText }]}>
            Data Y {displayedDataRange}
          </Text>
          <Text style={[styles.rangeSummaryText, { color: colors.mutedText }]}>
            Axis Y {displayedAxisRange}
          </Text>
          {dataDir !== '' && (
            <Text style={[styles.rangeSummaryText, { color: colors.mutedText }]}>
              Data dir {dataDir}
            </Text>
          )}
        </View>
      </View>
      <ChartContainer loading={loading} error={error}>
        <NetWorthChart data={visibleData} yDomain={yDomain} />
      </ChartContainer>
      {hasVisibleData && currentValue !== null && (
        <View style={styles.stats}>
          <Text style={styles.totalValue}>
            ${currentValue.toLocaleString('en-US', { minimumFractionDigits: 2, maximumFractionDigits: 2 })}
          </Text>
          <Text style={[styles.change, { color: isPositive ? '#2ecc71' : '#e74c3c' }]}>
            {isPositive ? '+' : ''}${absoluteChange.toLocaleString('en-US', { minimumFractionDigits: 2, maximumFractionDigits: 2 })}
            {' '}({isPositive ? '+' : ''}{percentChange.toFixed(2)}%)
          </Text>
        </View>
      )}
    </ScrollView>
  );
}

const styles = StyleSheet.create({
  container: { flex: 1 },
  controls: {
    paddingHorizontal: 16,
    paddingTop: 10,
    paddingBottom: 8,
    gap: 10,
  },
  presetRow: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 8,
    flexWrap: 'wrap',
  },
  pill: {
    minWidth: 54,
    paddingHorizontal: 12,
    paddingVertical: 7,
    borderRadius: 16,
    alignItems: 'center',
  },
  fitPill: {
    backgroundColor: '#2f95dc',
  },
  refreshPill: {
    backgroundColor: '#374151',
  },
  pillText: {
    fontSize: 13,
    fontWeight: '600',
  },
  controlGrid: {
    flexDirection: 'row',
    flexWrap: 'wrap',
    gap: 10,
  },
  field: {
    flexGrow: 1,
    flexBasis: 148,
    gap: 4,
  },
  label: {
    fontSize: 12,
    fontWeight: '600',
  },
  input: {
    minHeight: 40,
    borderWidth: 1,
    borderRadius: 8,
    paddingHorizontal: 10,
    paddingVertical: 8,
    fontSize: 14,
  },
  webDateInput: {
    minHeight: 40,
    borderWidth: 1,
    borderRadius: 8,
    paddingLeft: 10,
    paddingRight: 10,
    fontSize: 14,
    boxSizing: 'border-box',
    width: '100%',
  },
  validation: {
    color: '#e74c3c',
    fontSize: 12,
  },
  rangeSummary: {
    flexDirection: 'row',
    flexWrap: 'wrap',
    gap: 10,
  },
  rangeSummaryText: {
    fontSize: 12,
    fontWeight: '500',
  },
  stats: { paddingHorizontal: 20, paddingVertical: 16, alignItems: 'center', gap: 4 },
  totalValue: { fontSize: 28, fontWeight: 'bold' },
  change: { fontSize: 16, fontWeight: '500' },
  mockLabel: { fontSize: 12, color: 'rgba(255, 255, 255, 0.35)', marginTop: 4 },
});
