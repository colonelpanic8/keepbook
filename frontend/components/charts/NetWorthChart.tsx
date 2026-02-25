import React from "react";
import { StyleSheet } from "react-native";
import { CartesianChart, Line, Area, useChartPressState } from "victory-native";
import { useFont } from "@shopify/react-native-skia";

import { Text, View } from "@/components/Themed";
import { chartColors, chartDefaults } from "./chart-colors";

// eslint-disable-next-line @typescript-eslint/no-var-requires
const spaceMono = require("../../assets/fonts/SpaceMono-Regular.ttf");

export interface NetWorthDataPoint {
  date: string; // YYYY-MM-DD
  value: number; // portfolio total value
}

export interface NetWorthChartProps {
  data: NetWorthDataPoint[];
  height?: number;
}

type ChartDatum = { x: string; value: number };

function formatDate(dateStr: string): string {
  const months = [
    "Jan",
    "Feb",
    "Mar",
    "Apr",
    "May",
    "Jun",
    "Jul",
    "Aug",
    "Sep",
    "Oct",
    "Nov",
    "Dec",
  ];
  const [, m, d] = dateStr.split("-");
  const monthIdx = parseInt(m, 10) - 1;
  const day = parseInt(d, 10);
  return `${months[monthIdx]} ${day}`;
}

function formatDollarValue(value: number): string {
  const abs = Math.abs(value);
  const sign = value < 0 ? "-" : "";
  if (abs >= 1_000_000) {
    const m = abs / 1_000_000;
    return `${sign}$${m.toFixed(1)}M`;
  }
  if (abs >= 1_000) {
    const k = abs / 1_000;
    return `${sign}$${k.toFixed(0)}K`;
  }
  return `${sign}$${abs.toFixed(0)}`;
}

export function NetWorthChart({
  data,
  height = chartDefaults.height,
}: NetWorthChartProps) {
  const font = useFont(spaceMono, 11);

  const { state } = useChartPressState({
    x: "" as string,
    y: { value: 0 },
  });

  if (data.length === 0) {
    return (
      <View style={[styles.empty, { height }]}>
        <Text style={styles.emptyText}>No data available</Text>
      </View>
    );
  }

  const chartData: ChartDatum[] = data.map((d) => ({
    x: d.date,
    value: d.value,
  }));

  return (
    <View style={{ height }}>
      <CartesianChart
        data={chartData}
        xKey="x"
        yKeys={["value"]}
        domainPadding={chartDefaults.domainPadding}
        chartPressState={state}
        axisOptions={{
          font,
          tickCount: { x: 5, y: 4 },
          lineColor: "transparent",
          labelColor: chartColors.label,
          formatXLabel: (label) => formatDate(String(label)),
          formatYLabel: (label) => formatDollarValue(Number(label)),
        }}
      >
        {({ points, chartBounds }) => (
          <>
            <Area
              points={points.value}
              y0={chartBounds.bottom}
              color={chartColors.accentFill}
              animate={{ type: "timing", duration: 500 }}
            />
            <Line
              points={points.value}
              color={chartColors.accent}
              strokeWidth={2}
              animate={{ type: "timing", duration: 500 }}
            />
          </>
        )}
      </CartesianChart>
    </View>
  );
}

const styles = StyleSheet.create({
  empty: {
    justifyContent: "center",
    alignItems: "center",
  },
  emptyText: {
    color: "rgba(255, 255, 255, 0.5)",
    fontSize: 14,
  },
});
