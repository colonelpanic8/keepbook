import React from "react";
import { Pressable, StyleSheet, View as RNView } from "react-native";

import { Text } from "@/components/Themed";

export enum TimeRange {
  WEEK = "WEEK",
  MONTH = "MONTH",
  THREE_MONTHS = "THREE_MONTHS",
  SIX_MONTHS = "SIX_MONTHS",
  YEAR = "YEAR",
  ALL = "ALL",
}

const RANGE_LABELS: Record<TimeRange, string> = {
  [TimeRange.WEEK]: "W",
  [TimeRange.MONTH]: "M",
  [TimeRange.THREE_MONTHS]: "3M",
  [TimeRange.SIX_MONTHS]: "6M",
  [TimeRange.YEAR]: "Y",
  [TimeRange.ALL]: "ALL",
};

const RANGES = [
  TimeRange.WEEK,
  TimeRange.MONTH,
  TimeRange.THREE_MONTHS,
  TimeRange.SIX_MONTHS,
  TimeRange.YEAR,
  TimeRange.ALL,
] as const;

export interface TimeRangeQuery {
  lookbackDays: number | null;
  granularity: "daily" | "weekly" | "monthly";
  period: "daily" | "weekly" | "monthly";
}

export function timeRangeToQuery(range: TimeRange): TimeRangeQuery {
  switch (range) {
    case TimeRange.WEEK:
      return { lookbackDays: 7, granularity: "daily", period: "daily" };
    case TimeRange.MONTH:
      return { lookbackDays: 30, granularity: "daily", period: "daily" };
    case TimeRange.THREE_MONTHS:
      return { lookbackDays: 90, granularity: "weekly", period: "weekly" };
    case TimeRange.SIX_MONTHS:
      return { lookbackDays: 180, granularity: "weekly", period: "monthly" };
    case TimeRange.YEAR:
      return { lookbackDays: 365, granularity: "monthly", period: "monthly" };
    case TimeRange.ALL:
      return { lookbackDays: null, granularity: "monthly", period: "monthly" };
  }
}

interface TimeRangeSelectorProps {
  selected: TimeRange;
  onSelect: (range: TimeRange) => void;
}

export function TimeRangeSelector({
  selected,
  onSelect,
}: TimeRangeSelectorProps) {
  return (
    <RNView style={styles.container}>
      {RANGES.map((range) => {
        const isActive = range === selected;
        return (
          <Pressable
            key={range}
            onPress={() => onSelect(range)}
            style={[styles.pill, isActive && styles.pillActive]}
          >
            <Text
              style={[styles.pillText, isActive && styles.pillTextActive]}
            >
              {RANGE_LABELS[range]}
            </Text>
          </Pressable>
        );
      })}
    </RNView>
  );
}

const ACTIVE_BG = "#2f95dc"; // matches tintColorLight from Colors.ts

const styles = StyleSheet.create({
  container: {
    flexDirection: "row",
    justifyContent: "center",
    alignItems: "center",
    gap: 6,
    paddingVertical: 8,
    paddingHorizontal: 12,
  },
  pill: {
    paddingHorizontal: 14,
    paddingVertical: 6,
    borderRadius: 16,
    backgroundColor: "rgba(255, 255, 255, 0.08)",
  },
  pillActive: {
    backgroundColor: ACTIVE_BG,
  },
  pillText: {
    fontSize: 13,
    fontWeight: "600",
    color: "rgba(255, 255, 255, 0.5)",
  },
  pillTextActive: {
    color: "#fff",
  },
});
