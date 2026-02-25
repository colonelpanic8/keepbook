import React from "react";
import { StyleSheet } from "react-native";
import { CartesianChart, Bar, useChartPressState } from "victory-native";
import { useFont } from "@shopify/react-native-skia";

import { Text, View } from "@/components/Themed";
import { chartColors, chartDefaults } from "./chart-colors";

// eslint-disable-next-line @typescript-eslint/no-var-requires
const spaceMono = require("../../assets/fonts/SpaceMono-Regular.ttf");

export interface SpendingDataPoint {
  label: string; // period label (e.g. "Jan", "Week 3")
  total: number; // spending total
}

export interface SpendingChartProps {
  data: SpendingDataPoint[];
  height?: number;
}

type ChartDatum = { x: string; total: number };

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

export function SpendingChart({
  data,
  height = chartDefaults.height,
}: SpendingChartProps) {
  const font = useFont(spaceMono, 11);

  const { state } = useChartPressState({
    x: "" as string,
    y: { total: 0 },
  });

  if (data.length === 0) {
    return (
      <View style={[styles.empty, { height }]}>
        <Text style={styles.emptyText}>No data available</Text>
      </View>
    );
  }

  const chartData: ChartDatum[] = data.map((d) => ({
    x: d.label,
    total: d.total,
  }));

  const xTickCount = Math.min(data.length, 8);

  return (
    <View style={{ height }}>
      <CartesianChart
        data={chartData}
        xKey="x"
        yKeys={["total"]}
        domainPadding={{ ...chartDefaults.domainPadding, left: 20, right: 20 }}
        chartPressState={state}
        axisOptions={{
          font,
          tickCount: { x: xTickCount, y: 4 },
          lineColor: "transparent",
          labelColor: chartColors.label,
          formatXLabel: (label) => String(label),
          formatYLabel: (label) => formatDollarValue(Number(label)),
        }}
      >
        {({ points, chartBounds }) => (
          <Bar
            points={points.total}
            chartBounds={chartBounds}
            color={chartColors.bar}
            roundedCorners={{ topLeft: 4, topRight: 4 }}
            animate={{ type: "timing", duration: 500 }}
          />
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
